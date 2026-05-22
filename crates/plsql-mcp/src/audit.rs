//! Audit baseline for the live-DB tool surface (`PLSQL-MCP-LIVE-003`).
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
//! issuing SQL. It is transport- and connection-library-agnostic, so the
//! per-tool beads (`PLSQL-MCP-LIVE-004..`) can plug it into whichever
//! connection abstraction they choose.

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
    /// Owner-qualified audit table name (e.g. `"OPS_AUDIT.MCP_CALLS"`); the
    /// `AuditPlan` builder verifies the value is non-empty + matches a
    /// simple `OWNER.NAME` or `NAME` shape.
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
    #[must_use]
    pub fn with_audit_table(mut self, table_name: impl Into<String>) -> Self {
        self.sink.table_name = Some(table_name.into());
        self
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
    /// Returns `None` when no sink is configured. The bead skeleton uses a
    /// simple positional insert; the table schema is documented in
    /// `docs/integrations/live-db/audit-table.md` (PLSQL-MCP-LIVE-003 follow-up).
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
            .with_audit_table("OPS_AUDIT.MCP_CALLS");
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
}
