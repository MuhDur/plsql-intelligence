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
//! - For guarded writes, append a signed, hash-chained out-of-band
//!   `oraclemcp-audit` record and fsync it before Oracle execution.
//! - Optionally mirror non-proof metadata into an audit table when
//!   `audit_table` is configured.
//! - Doctor subcommand verifies the audit posture and reports it.
//!
//! This module exposes the helpers concrete live-DB tools call before
//! issuing SQL. It is transport- and connection-library-agnostic, so
//! per-tool code can plug it into whichever connection abstraction it
//! chooses.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oraclemcp_audit::{
    AuditDecision, AuditEntryDraft, AuditOutcome, AuditRecord, Auditor as UpstreamAuditor,
    FileAuditSink, SigningKey,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

/// Environment variable naming the append-only JSONL audit file used by
/// guarded writes.
pub const GUARDED_AUDIT_FILE_ENV: &str = "PLSQL_MCP_AUDIT_FILE";
/// Environment variable carrying the HMAC key bytes for guarded-write audit
/// records. The value is read as UTF-8 bytes; operators can provide a random
/// high-entropy string from their secret manager.
pub const GUARDED_AUDIT_KEY_ENV: &str = "PLSQL_MCP_AUDIT_KEY";
/// Optional key id recorded with each HMAC signature.
pub const GUARDED_AUDIT_KEY_ID_ENV: &str = "PLSQL_MCP_AUDIT_KEY_ID";

/// Errors while installing or appending the durable guarded-write audit sink.
#[derive(Debug, Error)]
pub enum GuardedAuditError {
    #[error(
        "{GUARDED_AUDIT_FILE_ENV} is set but {GUARDED_AUDIT_KEY_ENV} is missing; guarded writes require both"
    )]
    MissingKeyForFile,
    #[error(
        "{GUARDED_AUDIT_KEY_ENV} is set but {GUARDED_AUDIT_FILE_ENV} is missing; guarded writes require both"
    )]
    MissingFileForKey,
    #[error("guarded audit sink error: {0}")]
    Sink(#[from] oraclemcp_audit::AuditError),
}

/// Signed, out-of-band audit writer for guarded writes and privilege
/// escalations.
///
/// This is deliberately a narrow wrapper around `oraclemcp-audit`: it keeps
/// `plsql-mcp`'s dispatch code focused on tool semantics while preserving the
/// upstream fsync-before-execute contract for `durable=true` appends.
#[derive(Clone)]
pub struct GuardedAudit {
    auditor: Arc<UpstreamAuditor>,
}

/// Local draft for one guarded-write audit append.
pub struct GuardedAuditDraft<'a> {
    pub client: &'a AuditClient,
    pub tool_name: &'a str,
    pub sql: &'a str,
    pub danger_level: &'a str,
    pub decision: AuditDecision,
    pub outcome: AuditOutcome,
    pub rows_affected: Option<u64>,
}

impl GuardedAudit {
    /// Build a signed guarded-write auditor over an append-only file.
    ///
    /// The file is JSONL, one `oraclemcp-audit` [`AuditRecord`] per line.
    pub fn file(
        path: impl AsRef<Path>,
        key_id: impl Into<String>,
        key_bytes: impl Into<Vec<u8>>,
    ) -> Result<Self, GuardedAuditError> {
        let sink = FileAuditSink::open(path)?;
        Ok(Self {
            auditor: Arc::new(UpstreamAuditor::new(
                Box::new(sink),
                SigningKey::new(key_id, key_bytes),
            )),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_sink_for_test(
        sink: Box<dyn oraclemcp_audit::AuditSink>,
        key_id: impl Into<String>,
        key_bytes: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            auditor: Arc::new(UpstreamAuditor::new(
                sink,
                SigningKey::new(key_id, key_bytes),
            )),
        }
    }

    /// Build from the environment, returning `Ok(None)` when audit is simply
    /// not configured. Supplying only one of file/key is a hard configuration
    /// error because it would make guarded writes look auditable when they are
    /// not.
    pub fn from_env() -> Result<Option<Self>, GuardedAuditError> {
        let file = std::env::var(GUARDED_AUDIT_FILE_ENV).ok();
        let key = std::env::var(GUARDED_AUDIT_KEY_ENV).ok();
        match (file, key) {
            (None, None) => Ok(None),
            (Some(_), None) => Err(GuardedAuditError::MissingKeyForFile),
            (None, Some(_)) => Err(GuardedAuditError::MissingFileForKey),
            (Some(path), Some(key)) => {
                let key_id = std::env::var(GUARDED_AUDIT_KEY_ID_ENV)
                    .unwrap_or_else(|_| String::from("plsql-mcp-env"));
                Ok(Some(Self::file(path, key_id, key.into_bytes())?))
            }
        }
    }

    /// Append a signed, durable audit record. `durable=true` means the record
    /// has been flushed and fsynced before this returns.
    pub fn append(&self, draft: GuardedAuditDraft<'_>) -> Result<AuditRecord, GuardedAuditError> {
        let upstream = AuditEntryDraft {
            agent_identity: agent_identity(draft.client),
            tool: draft.tool_name.to_string(),
            sql: draft.sql.to_string(),
            danger_level: draft.danger_level.to_string(),
            decision: draft.decision,
            rows_affected: draft.rows_affected,
            outcome: draft.outcome,
        };
        Ok(self.auditor.append(&upstream, audit_timestamp(), true)?)
    }
}

impl std::fmt::Debug for GuardedAudit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardedAudit").finish_non_exhaustive()
    }
}

fn agent_identity(client: &AuditClient) -> String {
    format!(
        "program={} model={} session={}",
        blank_as_unknown(&client.program),
        blank_as_unknown(&client.model),
        blank_as_unknown(&client.session_id)
    )
}

fn blank_as_unknown(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "unknown"
    } else {
        trimmed
    }
}

fn audit_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    format!("unix:{secs}")
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
            parts.next().is_none() && is_simple_sql_name(first) && is_simple_sql_name(second)
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
    if !bytes.first().is_some_and(u8::is_ascii_alphabetic) {
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
            let plan =
                AuditPlan::for_tool(fixture_client(), "describe_table").with_audit_table(name);
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
            let plan =
                AuditPlan::for_tool(fixture_client(), "describe_table").with_audit_table(attack);
            assert!(
                plan.is_none(),
                "malicious table name {attack:?} must be rejected"
            );
        }
    }

    #[test]
    fn guarded_audit_appends_signed_durable_record() {
        use oraclemcp_audit::{
            AuditOutcome as UpstreamOutcome, BrokenReason, MemoryAuditSink, VerifyOutcome,
            verify_records,
        };
        use std::sync::Arc;

        struct SharedSink(Arc<MemoryAuditSink>);
        impl oraclemcp_audit::AuditSink for SharedSink {
            fn append(
                &self,
                record: &oraclemcp_audit::AuditRecord,
            ) -> Result<(), oraclemcp_audit::AuditError> {
                self.0.append(record)
            }

            fn flush(&self) -> Result<(), oraclemcp_audit::AuditError> {
                self.0.flush()
            }
        }

        let sink = Arc::new(MemoryAuditSink::new());
        let audit = GuardedAudit::from_sink_for_test(
            Box::new(SharedSink(Arc::clone(&sink))),
            "k-test",
            b"guarded-test-key".to_vec(),
        );
        let record = audit
            .append(GuardedAuditDraft {
                client: &fixture_client(),
                tool_name: "execute_approved",
                sql: "CREATE OR REPLACE VIEW V AS SELECT 1 FROM DUAL",
                danger_level: "DDL",
                decision: AuditDecision::Allowed,
                outcome: UpstreamOutcome::Pending,
                rows_affected: None,
            })
            .expect("append");

        assert_eq!(sink.flush_count(), 1, "guarded append must fsync");
        assert_eq!(record.seq, 1);
        assert_eq!(record.tool, "execute_approved");
        assert_eq!(record.key_id.as_deref(), Some("k-test"));
        assert_eq!(
            verify_records(
                &sink.records(),
                &[SigningKey::new("k-test", b"guarded-test-key".to_vec())],
            ),
            VerifyOutcome::Ok { records: 1 }
        );

        let mut tampered = sink.records();
        tampered
            .first_mut()
            .expect("one audit record")
            .sql_preview
            .push_str(" -- tampered");
        assert_eq!(
            verify_records(
                &tampered,
                &[SigningKey::new("k-test", b"guarded-test-key".to_vec())],
            ),
            VerifyOutcome::Broken {
                seq: 1,
                index: 0,
                reason: BrokenReason::HashMismatch,
            }
        );
    }
}
