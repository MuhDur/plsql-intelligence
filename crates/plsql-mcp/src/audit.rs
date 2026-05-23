//! Audit baseline for the live-DB tool surface.
//!
//! Per plan §13A.3, every live-DB tool call must:
//!
//! - Tag the Oracle session via
//!   `DBMS_APPLICATION_INFO.SET_MODULE('plsql-mcp', $tool_name)`.
//! - Set `V$SESSION.ACTION` to the agent model name (surfaced via the MCP
//!   `_meta.session.client_info` field).
//! - Embed `/* plsql-mcp $tool $session-id $agent-model */` as a comment
//!   on every emitted SQL statement.
//! - Optionally append to an audit table when `audit_table` is configured.
//! - Doctor subcommand verifies the audit posture and reports it.
//!
//! This module exposes the helpers concrete live-DB tools call before
//! issuing SQL. It is transport- and connection-library-agnostic, so
//! per-tool code can plug it into whichever connection abstraction it
//! chooses.

use serde::{Deserialize, Serialize};

/// Stable module name reported to Oracle via `DBMS_APPLICATION_INFO`.
/// Matches the SQLcl MCP convention (`MODULE='SQLcl-MCP'`) so DBAs see a
/// consistent vendor marker across MCP servers.
pub const APPLICATION_MODULE: &str = "plsql-mcp";

/// MCP client identification — propagates the calling agent's model
/// (`Cursor`, `Claude Code`, `Codex`, ...) plus an opaque session id.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditClient {
    /// Free-form agent program (e.g. `"claude-code"`).
    pub program: String,
    /// Agent model identifier (e.g. `"claude-opus-4-7"`).
    pub model: String,
    /// Opaque session identifier the MCP server assigned for this run.
    pub session_id: String,
}

impl AuditClient {
    #[must_use]
    pub fn new(
        program: impl Into<String>,
        model: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            program: program.into(),
            model: model.into(),
            session_id: session_id.into(),
        }
    }
}

/// Optional sink that mirrors audit records into a customer-owned table.
/// `audit_table = "PLSQL_MCP_AUDIT"` is the configured default name when
/// the operator opts in.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditSink {
    /// Owner-qualified audit table name (e.g. `"OPS_AUDIT.MCP_CALLS"`).
    /// [`AuditPlan::with_audit_table`] is the only sanctioned way to set
    /// this field: it rejects anything that is not a strict `OWNER.NAME`
    /// or `NAME` identifier, so a value reaching `audit_insert_sql`
    /// cannot carry a statement terminator, extra columns, a `@DBLINK`
    /// suffix, or an embedded subquery.
    pub table_name: Option<String>,
}

impl AuditSink {
    /// `AuditSink` is configured iff a non-empty `table_name` is set.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.table_name
            .as_deref()
            .map(|name| !name.trim().is_empty())
            .unwrap_or(false)
    }
}

/// A planned audit envelope for a single tool call. Concrete tool
/// implementations build one of these per call, run the
/// [`AuditPlan::set_module_sql`] / [`AuditPlan::comment_marker`] pre-flight
/// against the connection, then issue their own SQL with the marker
/// appended.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditPlan {
    pub client: AuditClient,
    pub tool_name: String,
    pub sink: AuditSink,
}

impl AuditPlan {
    /// Build a plan for `tool_name` against `client`. The audit sink can be
    /// changed in place via [`AuditPlan::with_audit_table`] before the plan
    /// is committed.
    #[must_use]
    pub fn for_tool(client: AuditClient, tool_name: impl Into<String>) -> Self {
        Self {
            client,
            tool_name: tool_name.into(),
            sink: AuditSink::default(),
        }
    }

    /// Configure an audit table sink.
    ///
    /// The audit-row INSERT splices the table name verbatim into SQL —
    /// identifier positions cannot be bind-parameterised in Oracle — so
    /// the name is validated here against a strict `OWNER.NAME` / `NAME`
    /// shape before it can ever reach [`AuditPlan::audit_insert_sql`].
    /// Returns `None` when `table_name` is not a valid identifier, so
    /// a config value carrying a statement terminator,
    /// extra columns, a `@DBLINK` suffix, an embedded subquery, or a
    /// comment cannot escape the intended INSERT shape.
    #[must_use]
    pub fn with_audit_table(mut self, table_name: impl Into<String>) -> Option<Self> {
        let name = table_name.into();
        if !is_valid_audit_table_name(name.trim()) {
            return None;
        }
        self.sink.table_name = Some(name);
        Some(self)
    }

    /// PL/SQL anonymous block to run before any tool SQL — sets module +
    /// action so DBAs reviewing `V$SESSION` see a consistent marker.
    /// Returns `(sql, [module, action])` so the caller can bind parameters
    /// rather than interpolating identifiers into the statement text.
    #[must_use]
    pub fn set_module_sql(&self) -> (&'static str, [String; 2]) {
        (
            "begin dbms_application_info.set_module(:1, :2); end;",
            [String::from(APPLICATION_MODULE), self.tool_name.clone()],
        )
    }

    /// PL/SQL anonymous block to run to set `V$SESSION.ACTION` to the
    /// agent's model identifier. Returns `(sql, [action])`.
    #[must_use]
    pub fn set_action_sql(&self) -> (&'static str, [String; 1]) {
        (
            "begin dbms_application_info.set_action(:1); end;",
            [self.client.model.clone()],
        )
    }

    /// Marker comment to append to every SQL statement issued by the tool.
    /// Matches the shape called out in plan §13A.3 verbatim.
    #[must_use]
    pub fn comment_marker(&self) -> String {
        format!(
            "/* plsql-mcp {} {} {} */",
            self.tool_name, self.client.session_id, self.client.model
        )
    }

    /// Append `comment_marker` to `sql`. Idempotent — re-applying does not
    /// double-tag the statement.
    #[must_use]
    pub fn annotate(&self, sql: &str) -> String {
        let marker = self.comment_marker();
        let trimmed = sql.trim_end();
        if trimmed.ends_with(&marker) {
            return String::from(trimmed);
        }
        format!("{trimmed} {marker}")
    }

    /// SQL that appends one audit row to the configured audit table.
    /// Returns `None` when no sink is configured.
    ///
    /// The statement is a positional `INSERT` with five columns:
    /// `tool_name`, `agent_program`, `agent_model`, `session_id`,
    /// `recorded_at`. The first four are positional binds (`:1`..`:4`);
    /// the fifth (`recorded_at`) is set to `SYSTIMESTAMP` by the
    /// statement itself. The audit table (or a view with that
    /// projection) must define those five columns in that order; a
    /// `NUMBER` primary key and any house-keeping columns are the
    /// integrator's choice. `with_audit_table` validates that the
    /// supplied table name is a plain `OWNER.NAME` / `NAME` identifier
    /// before this method ever produces a statement, so identifier
    /// injection is impossible.
    #[must_use]
    pub fn audit_insert_sql(&self) -> Option<String> {
        let table = self.sink.table_name.as_deref()?.trim();
        if table.is_empty() {
            return None;
        }
        Some(format!(
            "insert into {table} (tool_name, agent_program, agent_model, session_id, recorded_at) \
             values (:1, :2, :3, :4, systimestamp)"
        ))
    }
}

/// Validate an audit table name as a strict `OWNER.NAME` or bare
/// `NAME` identifier.
///
/// Each segment must be an unquoted Oracle simple SQL name: a letter
/// followed by letters, digits, `_`, `$`, or `#`, capped at 128 bytes.
/// At most one `.` separator is allowed and both halves must be
/// non-empty. Anything else — whitespace, statement terminators,
/// `@DBLINK` suffixes, commas, parentheses, a leading/trailing/double
/// dot — is rejected, so the name cannot escape the `insert into
/// {table} …` shape it is interpolated into.
#[must_use]
fn is_valid_audit_table_name(name: &str) -> bool {
    let mut parts = name.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    match parts.next() {
        // `OWNER.NAME` — exactly one dot, both halves valid, no third part.
        Some(second) => {
            parts.next().is_none()
                && is_simple_sql_name(first)
                && is_simple_sql_name(second)
        }
        // Bare `NAME` — no dot at all.
        None => is_simple_sql_name(first),
    }
}

/// Bare-bones `DBMS_ASSERT.SIMPLE_SQL_NAME` check for one identifier
/// segment: a letter followed by letters, digits, `_`, `$`, or `#`,
/// 1..=128 bytes. Mirrors `patch.rs::is_simple_sql_name`.
#[must_use]
fn is_simple_sql_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 {
        return false;
    }
    if !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    bytes
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_client() -> AuditClient {
        AuditClient::new("claude-code", "claude-opus-4-7", "sess-xyz")
    }

    #[test]
    fn comment_marker_matches_plan_shape() {
        let plan = AuditPlan::for_tool(fixture_client(), "describe_table");
        assert_eq!(
            plan.comment_marker(),
            "/* plsql-mcp describe_table sess-xyz claude-opus-4-7 */"
        );
    }

    #[test]
    fn annotate_appends_marker_and_is_idempotent() {
        let plan = AuditPlan::for_tool(fixture_client(), "describe_table");
        let annotated = plan.annotate("SELECT * FROM DUAL");
        assert!(annotated.contains("plsql-mcp describe_table sess-xyz claude-opus-4-7"));
        // Re-applying should not stack markers.
        let twice = plan.annotate(&annotated);
        let count = twice.matches("plsql-mcp describe_table").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn set_module_sql_carries_application_module_constant() {
        let plan = AuditPlan::for_tool(fixture_client(), "list_objects");
        let (sql, params) = plan.set_module_sql();
        assert!(sql.contains("dbms_application_info.set_module"));
        assert_eq!(params[0], APPLICATION_MODULE);
        assert_eq!(params[1], "list_objects");
    }

    #[test]
    fn set_action_sql_carries_agent_model() {
        let plan = AuditPlan::for_tool(fixture_client(), "describe_table");
        let (sql, params) = plan.set_action_sql();
        assert!(sql.contains("dbms_application_info.set_action"));
        assert_eq!(params[0], "claude-opus-4-7");
    }

    #[test]
    fn audit_insert_sql_returns_none_when_sink_not_configured() {
        let plan = AuditPlan::for_tool(fixture_client(), "describe_table");
        assert!(plan.audit_insert_sql().is_none());
    }

    #[test]
    fn audit_insert_sql_uses_configured_table() {
        let plan = AuditPlan::for_tool(fixture_client(), "describe_table")
            .with_audit_table("OPS_AUDIT.MCP_CALLS")
            .expect("valid audit table name");
        let sql = plan.audit_insert_sql().expect("sql");
        assert!(sql.contains("insert into OPS_AUDIT.MCP_CALLS"));
        assert!(sql.contains(":1") && sql.contains(":4"));
        assert!(sql.contains("systimestamp"));
    }

    #[test]
    fn audit_sink_is_configured_only_for_non_empty_name() {
        let mut sink = AuditSink::default();
        assert!(!sink.is_configured());
        sink.table_name = Some(String::from("   "));
        assert!(!sink.is_configured());
        sink.table_name = Some(String::from("OPS_AUDIT.MCP_CALLS"));
        assert!(sink.is_configured());
    }

    #[test]
    fn with_audit_table_accepts_valid_identifiers() {
        // oracle-c1e2: a strict OWNER.NAME / NAME shape is accepted.
        for name in ["MCP_CALLS", "OPS_AUDIT.MCP_CALLS", "PKG$WITH#SIGILS"] {
            let plan = AuditPlan::for_tool(fixture_client(), "describe_table")
                .with_audit_table(name);
            assert!(
                plan.is_some(),
                "valid table name {name:?} should be accepted"
            );
            let plan = plan.unwrap();
            assert!(plan.audit_insert_sql().is_some());
        }
    }

    #[test]
    fn with_audit_table_rejects_identifier_injection() {
        // oracle-c1e2: anything that is not a strict OWNER.NAME / NAME
        // identifier must be rejected — no statement terminators, no
        // extra columns, no DBLINK suffix, no embedded subquery, no
        // comment, no whitespace, no double-dot.
        let attacks = [
            "MCP_CALLS (x) VALUES (1); DROP TABLE T --",
            "MCP_CALLS@SOMELINK",
            "MCP_CALLS, OTHER",
            "OPS_AUDIT..MCP_CALLS",
            ".MCP_CALLS",
            "MCP_CALLS.",
            "MCP CALLS",
            "1BAD",
            "OPS-AUDIT.MCP_CALLS",
            "MCP_CALLS;",
            "(SELECT 1 FROM DUAL)",
            "",
            "   ",
        ];
        for attack in attacks {
            let plan = AuditPlan::for_tool(fixture_client(), "describe_table")
                .with_audit_table(attack);
            assert!(
                plan.is_none(),
                "malicious table name {attack:?} must be rejected"
            );
        }
    }
}
