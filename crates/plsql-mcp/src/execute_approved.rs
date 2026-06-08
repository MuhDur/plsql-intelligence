//! `execute_approved` + `deploy_ddl`.
//!
//! Two execution surfaces share this module:
//!
//! * `execute_approved` — runs a single, previously-previewed DDL
//!   statement under its approval token. The function does *not*
//!   touch a live Oracle handle; it composes the byte-for-byte
//!   verification (via [`PreviewRegistry::verify_byte_for_byte`])
//!   with the [`crate::cross_schema::require_cross_schema_confirmation`]
//!   guard and yields an [`ApprovedExecutionPlan`] the live-DB
//!   executor consumes. This keeps the policy pure and unit-testable.
//!
//! * `deploy_ddl` — lock-free deployment via `DBMS_SCHEDULER`,
//!   adapted from a private-estate one-shot scheduler-job DDL pattern.
//!   Because Oracle DDL implicitly commits and acquires library-
//!   cache locks, the safe deployment shape is to package the DDL
//!   as a one-shot scheduler job, fire-and-forget, then poll
//!   `USER_SCHEDULER_JOB_RUN_DETAILS` for the outcome. This module
//!   emits the canonical anonymous-PL/SQL block + the polling
//!   SELECT; the executor is responsible for the round trips.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::create_or_replace::{CreateOrReplaceError, parse_target_schema};
use crate::cross_schema::{
    CrossSchemaConfirmation, CrossSchemaError, require_cross_schema_confirmation,
};
use crate::preview::{PreviewError, PreviewRegistry};

/// Input to [`run_execute_approved`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecuteApprovedRequest {
    pub connection: String,
    /// Approval token minted by a prior dry-run (patch_package /
    /// patch_view / create_or_replace).
    pub token: String,
    /// The same DDL bytes the operator approved. Verified
    /// byte-for-byte against the previewed payload.
    pub ddl_bytes: String,
    /// Connected principal (the active session's schema). Compared
    /// against `target_schema` to decide whether the cross-schema
    /// confirmation step is needed.
    pub principal_schema: String,
    /// Target schema for the DDL, as the caller understands it.
    ///
    /// This field is **not trusted** for the cross-schema guard:
    /// `run_execute_approved` derives the real target from the
    /// schema named in the byte-verified `ddl_bytes` header.
    /// The field is still validated for agreement — if it disagrees
    /// with the parsed DDL header the request is rejected outright.
    /// An empty string means "caller did not specify"; it is accepted
    /// only when it agrees with (or is silent about) the DDL.
    pub target_schema: String,
    /// What the operator typed when prompted for the cross-schema
    /// confirmation. `None` is acceptable for same-schema writes;
    /// cross-schema writes require this to match `target_schema`.
    pub operator_typed_schema: Option<String>,
}

/// Output of [`run_execute_approved`] — the live-DB adapter
/// receives this and runs the DDL. The plan carries every detail
/// the audit log will need; the executor does not have to dig back
/// into the preview registry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovedExecutionPlan {
    pub connection: String,
    pub token: String,
    /// The verified DDL bytes — guaranteed to equal the previewed
    /// payload byte-for-byte.
    pub ddl_bytes: String,
    /// `sha256:<hex>` of `ddl_bytes`.
    pub ddl_sha256: String,
    /// Same-schema or confirmed cross-schema, with the typed string
    /// recorded for the audit trail.
    pub cross_schema: CrossSchemaConfirmation,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecuteApprovedError {
    #[error("execute_approved refused: connection name is empty")]
    EmptyConnection,
    #[error("execute_approved refused: ddl_bytes is empty")]
    EmptyDdl,
    #[error("execute_approved refused: token is empty")]
    EmptyToken,
    #[error("execute_approved preview registry error: {0}")]
    Preview(#[from] PreviewError),
    #[error("execute_approved cross-schema check failed: {0}")]
    CrossSchema(#[from] CrossSchemaError),
    #[error(
        "execute_approved refused: caller-supplied target_schema {caller:?} disagrees with the schema {ddl:?} named in the verified DDL header"
    )]
    TargetSchemaMismatch { caller: String, ddl: String },
    #[error(
        "execute_approved refused: verified DDL is not a recognised CREATE OR REPLACE shape: {0}"
    )]
    DdlShape(#[from] CreateOrReplaceError),
}

/// Compose preview verification + cross-schema confirmation into
/// one approval plan. Pure — does not touch the database.
pub fn run_execute_approved(
    registry: &mut PreviewRegistry,
    req: ExecuteApprovedRequest,
) -> Result<ApprovedExecutionPlan, ExecuteApprovedError> {
    if req.connection.trim().is_empty() {
        return Err(ExecuteApprovedError::EmptyConnection);
    }
    if req.token.trim().is_empty() {
        return Err(ExecuteApprovedError::EmptyToken);
    }
    if req.ddl_bytes.trim().is_empty() {
        return Err(ExecuteApprovedError::EmptyDdl);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let verified =
        registry.verify_byte_for_byte(&req.token, &req.connection, &req.ddl_bytes, now)?;

    // oracle-jy0w: derive the cross-schema guard's target from the
    // schema named in the *byte-verified* DDL header — never from the
    // caller-supplied `target_schema` field, which an agent could set
    // to the principal schema to slip a cross-schema write past the
    // operator-typed confirmation. An unqualified DDL header targets
    // the current schema, so the effective target is the principal.
    let parsed_schema = parse_target_schema(&verified.ddl_bytes)?;
    let effective_target = parsed_schema
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| req.principal_schema.trim().to_ascii_uppercase());

    // The caller's `target_schema` is still validated: if it names a
    // schema, it must agree with the one the verified DDL actually
    // targets. An empty field means "caller did not specify".
    let caller_target = req.target_schema.trim();
    if !caller_target.is_empty() && caller_target.to_ascii_uppercase() != effective_target {
        return Err(ExecuteApprovedError::TargetSchemaMismatch {
            caller: caller_target.to_string(),
            ddl: effective_target,
        });
    }

    let cross_schema = require_cross_schema_confirmation(
        &req.principal_schema,
        &effective_target,
        req.operator_typed_schema.as_deref(),
    )?;

    Ok(ApprovedExecutionPlan {
        connection: verified.connection.clone(),
        token: verified.token.clone(),
        ddl_bytes: verified.ddl_bytes.clone(),
        ddl_sha256: verified.ddl_sha256.clone(),
        cross_schema,
    })
}

/// Mark the preview as consumed once the live-DB executor reports
/// success. Idempotent — safe to call twice (the registry drops on
/// first call). Surfaced as a separate function so the executor
/// controls when to retire the token: only after the DDL actually
/// committed.
pub fn consume_approved(registry: &mut PreviewRegistry, plan: &ApprovedExecutionPlan) {
    registry.consume(&plan.connection);
}

// ---------------------------------------------------------------------------
// deploy_ddl — lock-free DBMS_SCHEDULER one-shot job
// ---------------------------------------------------------------------------

/// Wire shape returned to the live-DB executor. The PL/SQL block
/// in `submit_block` is fire-and-forget; the `poll_sql` query
/// returns either a `SUCCEEDED`, `FAILED`, or `STOPPED` status
/// plus the error stack when present. The caller polls every
/// `poll_interval_seconds` until status is non-`null` or the
/// deadline elapses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeployDdlPlan {
    pub job_name: String,
    /// Anonymous PL/SQL block — submit via `EXECUTE IMMEDIATE`. The
    /// block creates the job, enables it, and returns without
    /// blocking on the DDL.
    pub submit_block: String,
    /// SQL that fetches the latest run record for `job_name`.
    /// Returns one row: (status, additional_info, run_duration).
    pub poll_sql: String,
    pub poll_interval_seconds: u32,
}

/// Build the `DBMS_SCHEDULER`-based deployment plan for a previously
/// approved DDL statement. `job_name` is the unique handle used to
/// match the run record back to the operation; the caller is
/// responsible for picking a value that does not collide with a
/// concurrent job (a UUID-derived name is the typical choice).
///
/// The pattern is adapted from a private-estate one-shot scheduler-job
/// DDL pattern: wrap the DDL in `EXECUTE IMMEDIATE`, submit as a `PLSQL_BLOCK`
/// scheduler job, and poll `USER_SCHEDULER_JOB_RUN_DETAILS`. This
/// avoids library-cache pile-ups: the operator's session never
/// holds the lock that blocks dependent sessions, because the DDL
/// runs in the scheduler's pool.
#[must_use]
pub fn build_deploy_plan(job_name: &str, ddl_bytes: &str) -> DeployDdlPlan {
    // The DDL is carried at two SQL string-literal nesting levels: the
    // scheduler job runs it via `EXECUTE IMMEDIATE '<ddl>'` (inner
    // literal), and that whole PL/SQL block is itself the
    // `job_action => '<...>'` argument (outer literal). Standard
    // ''-doubling applied once per level is collision-free.
    //
    // An earlier version wrapped the DDL in Oracle alternative quoting
    // (`q'[...]'`); estate DDL containing the closing sequence `]'`
    // closed that literal early and injected arbitrary PL/SQL into the
    // job (security: oracle-tx8d). ''-doubling has no such delimiter.
    let inner = ddl_bytes.replace('\'', "''");
    let job_action = format!("BEGIN EXECUTE IMMEDIATE '{inner}'; END;");
    let job_action_literal = job_action.replace('\'', "''");
    let job = job_name.replace('\'', "''");
    let submit_block = format!(
        "BEGIN\n  DBMS_SCHEDULER.CREATE_JOB(\n    job_name        => '{job}',\n    job_type        => 'PLSQL_BLOCK',\n    job_action      => '{job_action_literal}',\n    start_date      => SYSTIMESTAMP,\n    enabled         => TRUE,\n    auto_drop       => TRUE\n  );\nEND;\n",
        job = job,
        job_action_literal = job_action_literal,
    );
    // oracle-rwjl.11: interpolate the already-''-escaped `job`, not the
    // raw `job_name`. A job_name carrying a single quote would otherwise
    // close the poll literal early (`WHERE job_name = 'x'y'`) — a malformed
    // poll query while submit_block stayed balanced. Keep the escaping
    // symmetric with submit_block so both literals are always closeable
    // only by their own emitted delimiter.
    let poll_sql = format!(
        "SELECT status, additional_info, run_duration\n  FROM USER_SCHEDULER_JOB_RUN_DETAILS\n WHERE job_name = '{job}'\n ORDER BY log_date DESC\n FETCH FIRST 1 ROWS ONLY",
        job = job,
    );
    DeployDdlPlan {
        job_name: job_name.to_string(),
        submit_block,
        poll_sql,
        poll_interval_seconds: 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::{PackagePart, PatchMode, PatchPackageRequest, run_patch_package};

    fn fixed(t: &'static str) -> impl FnOnce() -> String {
        move || t.to_string()
    }

    fn approved_request(token: &str) -> ExecuteApprovedRequest {
        ExecuteApprovedRequest {
            connection: "billing-dev".into(),
            token: token.into(),
            ddl_bytes: "CREATE OR REPLACE PACKAGE BODY BILLING.INVOICE_PKG AS\nBEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            principal_schema: "BILLING".into(),
            target_schema: "BILLING".into(),
            operator_typed_schema: None,
        }
    }

    fn mint_token(registry: &mut PreviewRegistry, token: &'static str) {
        let req = PatchPackageRequest {
            connection: "billing-dev".into(),
            schema: "BILLING".into(),
            package: "INVOICE_PKG".into(),
            part: PackagePart::Body,
            source: "BEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            mode: PatchMode::DryRun,
        };
        run_patch_package(registry, req, fixed(token)).unwrap();
    }

    #[test]
    fn execute_approved_returns_plan_on_match() {
        let mut registry = PreviewRegistry::new();
        mint_token(&mut registry, "tok-x");
        let plan = run_execute_approved(&mut registry, approved_request("tok-x")).unwrap();
        assert_eq!(plan.connection, "billing-dev");
        assert!(plan.ddl_bytes.contains("BILLING.INVOICE_PKG"));
        assert!(plan.ddl_sha256.starts_with("sha256:"));
        assert!(plan.cross_schema.confirmed);
    }

    #[test]
    fn execute_approved_rejects_drift() {
        let mut registry = PreviewRegistry::new();
        mint_token(&mut registry, "tok-d");
        let mut req = approved_request("tok-d");
        req.ddl_bytes.push_str(" -- drift");
        let err = run_execute_approved(&mut registry, req).unwrap_err();
        assert!(matches!(
            err,
            ExecuteApprovedError::Preview(PreviewError::DdlMismatch { .. })
        ));
    }

    #[test]
    fn execute_approved_requires_typed_schema_for_cross_schema() {
        // A genuine cross-schema write: the verified DDL names ANALYTICS
        // while the principal is BILLING. Without an operator-typed
        // confirmation the request must be refused. The target is now
        // derived from the verified DDL header, so the request's
        // `target_schema` field is set to match it (oracle-jy0w).
        let mut registry = PreviewRegistry::new();
        let mint = PatchPackageRequest {
            connection: "analytics-dev".into(),
            schema: "ANALYTICS".into(),
            package: "INVOICE_PKG".into(),
            part: PackagePart::Body,
            source: "BEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            mode: PatchMode::DryRun,
        };
        run_patch_package(&mut registry, mint, fixed("tok-c")).unwrap();

        let req = ExecuteApprovedRequest {
            connection: "analytics-dev".into(),
            token: "tok-c".into(),
            ddl_bytes: "CREATE OR REPLACE PACKAGE BODY ANALYTICS.INVOICE_PKG AS\nBEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            principal_schema: "BILLING".into(),
            target_schema: "ANALYTICS".into(),
            operator_typed_schema: None,
        };
        let err = run_execute_approved(&mut registry, req).unwrap_err();
        assert!(matches!(
            err,
            ExecuteApprovedError::CrossSchema(CrossSchemaError::ConfirmationMissing { .. })
        ));
    }

    #[test]
    fn execute_approved_accepts_typed_schema_for_cross_schema() {
        let mut registry = PreviewRegistry::new();
        // Mint for ANALYTICS-targeting DDL so the byte-verify succeeds.
        let req = PatchPackageRequest {
            connection: "analytics-dev".into(),
            schema: "ANALYTICS".into(),
            package: "INVOICE_PKG".into(),
            part: PackagePart::Body,
            source: "BEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            mode: PatchMode::DryRun,
        };
        run_patch_package(&mut registry, req, fixed("tok-cs")).unwrap();

        let req = ExecuteApprovedRequest {
            connection: "analytics-dev".into(),
            token: "tok-cs".into(),
            ddl_bytes: "CREATE OR REPLACE PACKAGE BODY ANALYTICS.INVOICE_PKG AS\nBEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            principal_schema: "BILLING".into(),
            target_schema: "ANALYTICS".into(),
            operator_typed_schema: Some("ANALYTICS".into()),
        };
        let plan = run_execute_approved(&mut registry, req).unwrap();
        assert!(plan.cross_schema.confirmed);
    }

    #[test]
    fn execute_approved_derives_target_schema_from_verified_ddl() {
        // oracle-jy0w: a caller that submits ANALYTICS-targeting DDL
        // but lies in `target_schema` (claiming the principal schema)
        // must NOT slip past the cross-schema confirmation. The guard
        // keys off the schema named in the verified ddl_bytes, not the
        // unverified field.
        let mut registry = PreviewRegistry::new();
        let req = PatchPackageRequest {
            connection: "analytics-dev".into(),
            schema: "ANALYTICS".into(),
            package: "INVOICE_PKG".into(),
            part: PackagePart::Body,
            source: "BEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            mode: PatchMode::DryRun,
        };
        run_patch_package(&mut registry, req, fixed("tok-lie")).unwrap();

        let req = ExecuteApprovedRequest {
            connection: "analytics-dev".into(),
            token: "tok-lie".into(),
            ddl_bytes: "CREATE OR REPLACE PACKAGE BODY ANALYTICS.INVOICE_PKG AS\nBEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            principal_schema: "BILLING".into(),
            // The lie: claims BILLING (== principal) so the old code
            // would take the SameSchema branch and skip confirmation.
            target_schema: "BILLING".into(),
            operator_typed_schema: None,
        };
        let err = run_execute_approved(&mut registry, req).unwrap_err();
        assert!(
            matches!(
                err,
                ExecuteApprovedError::TargetSchemaMismatch { .. }
                    | ExecuteApprovedError::CrossSchema(
                        CrossSchemaError::ConfirmationMissing { .. }
                    )
            ),
            "lying target_schema must be rejected, got {err:?}"
        );
    }

    #[test]
    fn execute_approved_unqualified_ddl_defaults_to_principal() {
        // A DDL header with no explicit schema targets the current
        // schema — unqualified ⇒ principal, which is same-schema.
        // Minted via create_or_replace so the previewed bytes equal the
        // unqualified DDL verbatim (the patch builder always emits a
        // schema-qualified header).
        use crate::create_or_replace::{
            CreateOrReplaceMode, CreateOrReplaceRequest, run_create_or_replace,
        };
        let unqualified_ddl =
            "CREATE OR REPLACE PACKAGE BODY INVOICE_PKG AS\nBEGIN\n  NULL;\nEND INVOICE_PKG;\n";
        let mut registry = PreviewRegistry::new();
        run_create_or_replace(
            &mut registry,
            CreateOrReplaceRequest {
                connection: "billing-dev".into(),
                operation_summary: "replace unqualified package body".into(),
                ddl_bytes: unqualified_ddl.into(),
                mode: CreateOrReplaceMode::DryRun,
            },
            fixed("tok-unq"),
        )
        .unwrap();

        let req = ExecuteApprovedRequest {
            connection: "billing-dev".into(),
            token: "tok-unq".into(),
            ddl_bytes: unqualified_ddl.into(),
            principal_schema: "BILLING".into(),
            target_schema: "BILLING".into(),
            operator_typed_schema: None,
        };
        let plan = run_execute_approved(&mut registry, req).unwrap();
        assert!(plan.cross_schema.confirmed);
        assert!(matches!(
            plan.cross_schema.decision,
            crate::cross_schema::CrossSchemaDecision::SameSchema { .. }
        ));
    }

    #[test]
    fn execute_approved_rejects_caller_target_schema_disagreement() {
        // Even an honest cross-schema write is rejected when the
        // caller's target_schema field disagrees with the DDL header.
        let mut registry = PreviewRegistry::new();
        let req = PatchPackageRequest {
            connection: "analytics-dev".into(),
            schema: "ANALYTICS".into(),
            package: "INVOICE_PKG".into(),
            part: PackagePart::Body,
            source: "BEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            mode: PatchMode::DryRun,
        };
        run_patch_package(&mut registry, req, fixed("tok-dis")).unwrap();

        let req = ExecuteApprovedRequest {
            connection: "analytics-dev".into(),
            token: "tok-dis".into(),
            ddl_bytes: "CREATE OR REPLACE PACKAGE BODY ANALYTICS.INVOICE_PKG AS\nBEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            principal_schema: "BILLING".into(),
            // Disagrees with the DDL header (ANALYTICS).
            target_schema: "REPORTING".into(),
            operator_typed_schema: Some("REPORTING".into()),
        };
        let err = run_execute_approved(&mut registry, req).unwrap_err();
        assert!(
            matches!(err, ExecuteApprovedError::TargetSchemaMismatch { .. }),
            "disagreeing target_schema must be rejected, got {err:?}"
        );
    }

    #[test]
    fn execute_approved_spaced_qualifier_is_not_same_schema() {
        // oracle-rwjl.8: a BILLING principal deploying a header with a
        // whitespace-spaced qualifier (`ANALYTICS . PKG`) targets schema
        // ANALYTICS, so it must NOT be classified same-schema and must demand
        // the operator-typed destination. Previously parse_target_schema saw a
        // bare `ANALYTICS` token (no dot) and returned None, defaulting the
        // effective target to the BILLING principal and waving the write
        // through with no confirmation.
        use crate::create_or_replace::{
            CreateOrReplaceMode, CreateOrReplaceRequest, run_create_or_replace,
        };
        let spaced_ddl = "CREATE OR REPLACE PACKAGE BODY ANALYTICS . PKG AS BEGIN NULL; END;";
        let mut registry = PreviewRegistry::new();
        run_create_or_replace(
            &mut registry,
            CreateOrReplaceRequest {
                connection: "billing-dev".into(),
                operation_summary: "replace spaced-qualifier package body".into(),
                ddl_bytes: spaced_ddl.into(),
                mode: CreateOrReplaceMode::DryRun,
            },
            fixed("tok-sp"),
        )
        .unwrap();

        // No operator-typed confirmation ⇒ refused as a cross-schema write.
        let req = ExecuteApprovedRequest {
            connection: "billing-dev".into(),
            token: "tok-sp".into(),
            ddl_bytes: spaced_ddl.into(),
            principal_schema: "BILLING".into(),
            target_schema: String::new(),
            operator_typed_schema: None,
        };
        let err = run_execute_approved(&mut registry, req).unwrap_err();
        assert!(
            matches!(
                err,
                ExecuteApprovedError::CrossSchema(CrossSchemaError::ConfirmationMissing { .. })
            ),
            "spaced cross-schema header must require typed confirmation, got {err:?}"
        );

        // With the typed destination it is accepted as a confirmed
        // cross-schema write (re-mint: the dry-run token is single-use).
        run_create_or_replace(
            &mut registry,
            CreateOrReplaceRequest {
                connection: "billing-dev".into(),
                operation_summary: "replace spaced-qualifier package body".into(),
                ddl_bytes: spaced_ddl.into(),
                mode: CreateOrReplaceMode::DryRun,
            },
            fixed("tok-sp2"),
        )
        .unwrap();
        let req_ok = ExecuteApprovedRequest {
            connection: "billing-dev".into(),
            token: "tok-sp2".into(),
            ddl_bytes: spaced_ddl.into(),
            principal_schema: "BILLING".into(),
            target_schema: "ANALYTICS".into(),
            operator_typed_schema: Some("ANALYTICS".into()),
        };
        let plan = run_execute_approved(&mut registry, req_ok).unwrap();
        assert!(plan.cross_schema.confirmed);
        assert!(
            matches!(
                plan.cross_schema.decision,
                crate::cross_schema::CrossSchemaDecision::CrossSchemaConfirmed { .. }
            ),
            "spaced qualifier must classify as cross-schema: {:?}",
            plan.cross_schema.decision
        );
    }

    #[test]
    fn empty_inputs_rejected() {
        let mut registry = PreviewRegistry::new();
        let mut req = approved_request("tok");
        req.connection = "  ".into();
        assert_eq!(
            run_execute_approved(&mut registry, req).unwrap_err(),
            ExecuteApprovedError::EmptyConnection
        );

        let mut req = approved_request("tok");
        req.token = "".into();
        assert_eq!(
            run_execute_approved(&mut registry, req).unwrap_err(),
            ExecuteApprovedError::EmptyToken
        );

        let mut req = approved_request("tok");
        req.ddl_bytes = "  \n".into();
        assert_eq!(
            run_execute_approved(&mut registry, req).unwrap_err(),
            ExecuteApprovedError::EmptyDdl
        );
    }

    #[test]
    fn consume_approved_removes_registry_entry() {
        let mut registry = PreviewRegistry::new();
        mint_token(&mut registry, "tok-r");
        let plan = run_execute_approved(&mut registry, approved_request("tok-r")).unwrap();
        consume_approved(&mut registry, &plan);
        assert!(registry.is_empty());
        // Idempotent: second call is a no-op.
        consume_approved(&mut registry, &plan);
    }

    #[test]
    fn build_deploy_plan_emits_scheduler_block_and_poll_sql() {
        let plan = build_deploy_plan(
            "patch_invoice_pkg_42",
            "CREATE OR REPLACE PACKAGE BODY BILLING.X AS BEGIN NULL; END;",
        );
        assert_eq!(plan.job_name, "patch_invoice_pkg_42");
        assert!(plan.submit_block.contains("DBMS_SCHEDULER.CREATE_JOB"));
        assert!(plan.submit_block.contains("PLSQL_BLOCK"));
        assert!(plan.submit_block.contains("EXECUTE IMMEDIATE"));
        assert!(plan.submit_block.contains("BILLING.X"));
        assert!(plan.poll_sql.contains("USER_SCHEDULER_JOB_RUN_DETAILS"));
        assert!(plan.poll_sql.contains("patch_invoice_pkg_42"));
        assert!(plan.poll_interval_seconds > 0);
    }

    #[test]
    fn build_deploy_plan_escapes_single_quotes() {
        let plan = build_deploy_plan("patch_with_quote", "INSERT INTO X VALUES ('a''b')");
        // The DDL is carried at two nesting levels (the inner
        // `EXECUTE IMMEDIATE '<ddl>'` literal and the outer
        // `job_action => '<...>'` literal); standard ''-doubling at each
        // level keeps every literal balanced and closeable only by its
        // own emitted delimiter.
        assert_eq!(plan.submit_block.matches('\'').count() % 2, 0);
        assert!(plan.submit_block.contains("EXECUTE IMMEDIATE"));
        // oracle-rwjl.11: the poll_sql job_name literal must also be
        // ''-balanced. A job_name containing a single quote previously
        // produced `WHERE job_name = 'x'y'` (odd quote count, literal
        // closes early) because poll_sql interpolated the raw name.
        let plan = build_deploy_plan("x'y", "CREATE TABLE t (a NUMBER)");
        assert_eq!(
            plan.submit_block.matches('\'').count() % 2,
            0,
            "submit_block must stay balanced for a quoted job_name"
        );
        assert_eq!(
            plan.poll_sql.matches('\'').count() % 2,
            0,
            "poll_sql must stay balanced for a quoted job_name: {}",
            plan.poll_sql
        );
        // The escaped name appears doubled in the poll predicate.
        assert!(
            plan.poll_sql.contains("job_name = 'x''y'"),
            "poll_sql must use the ''-escaped job_name: {}",
            plan.poll_sql
        );
    }

    #[test]
    fn build_deploy_plan_resists_qquote_breakout() {
        // A DDL body containing the sequence `]'` would have closed an
        // inner Oracle `q'[...]'` literal early and injected arbitrary
        // PL/SQL into the scheduler job (security: oracle-tx8d). The plan
        // must not depend on alternative-quoting at all — its delimiter
        // can collide with estate DDL.
        let adversarial = "BEGIN NULL; END; ]'; EXECUTE IMMEDIATE 'DROP TABLE t'; --";
        let plan = build_deploy_plan("job_adv", adversarial);
        assert!(
            !plan.submit_block.contains("q'"),
            "must not use Oracle alternative-quoting: {}",
            plan.submit_block
        );
        assert_eq!(plan.submit_block.matches('\'').count() % 2, 0);
    }
}
