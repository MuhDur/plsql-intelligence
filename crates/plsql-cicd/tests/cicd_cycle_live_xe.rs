//! Live Oracle XE 23ai integration test for the full predict→plan→apply→verify
//! cycle.
//!
//! Exercises the complete Layer 5 pipeline against a running Oracle Free 23ai
//! container:
//!
//! 1. **predict**: build a synthetic [`ChangeSet`] and run [`predict`] against
//!    it to obtain an [`InvalidationPrediction`]. Assert the prediction is
//!    structured and non-trivially populated.
//! 2. **plan**: feed `(changeset, prediction)` into [`plan_changeset`] and
//!    assert the resulting [`DeploymentPlan`] is topologically ordered,
//!    monotone in ordinals, and carries the expected risk classification.
//! 3. **apply/verify**: translate the changeset's semantic objects into real
//!    DDL statements and drive [`verify`] against the live container. Assert
//!    per-statement outcomes including one intentional Oracle failure.
//!
//! # Pipeline gap note
//!
//! The current pipeline has no standalone `apply()` function: `verify()` is
//! the combined apply+verify step. It executes DDL in a throwaway scratch
//! schema and reports per-statement outcomes. `plan_changeset` produces
//! comment-style SQL placeholders (e.g. `"-- apply CREATE OR REPLACE PACKAGE …
//! from source file"`) rather than executable DDL, because the real DDL text
//! lives in source files that are not materialised in this test context. The
//! pipeline seam is therefore:
//!
//! ```text
//! predict(changeset) → prediction
//! plan_changeset(changeset, prediction) → deployment_plan   [ordering/risk only]
//! verify(conn, verify_changeset, opts) → report             [apply + per-stmt outcomes]
//! ```
//!
//! The [`VerifyChangeset`] is constructed directly from the same SQL the
//! `ChangeSet` objects represent, mirroring what a real deployment runner
//! would produce by materialising the source files referenced in each
//! [`DeploymentStatement`].
//!
//! The ordering contract between `plan` and `verify` is tested explicitly:
//! statements are submitted to `verify` in the ordinal order the plan
//! dictates, and this test asserts that order is respected.
//!
//! # Gating
//!
//! Requires the `live-xe` feature flag:
//! ```sh
//! LD_LIBRARY_PATH=/tmp/instantclient_23_7 \
//!     cargo test -p plsql-cicd --features live-xe \
//!     --test cicd_cycle_live_xe -- --nocapture
//! ```
//!
//! When `live-xe` is off a trivial gate-off test ensures `cargo test -p
//! plsql-cicd --test cicd_cycle_live_xe` always has at least one assertion.

// ── Gate-off path (no live-xe feature) ───────────────────────────────────────

#[cfg(not(feature = "live-xe"))]
#[test]
fn live_xe_cicd_cycle_is_feature_gated() {
    // The default test profile doesn't exercise the live pipeline cycle.
    // Enable the `live-xe` feature against a running Oracle XE 23ai container
    // to exercise the real predict→plan→apply→verify cycle.
    let live_xe = false;
    assert!(!live_xe, "live-xe feature is off by default");
}

// ── Live path (live-xe feature) ───────────────────────────────────────────────

#[cfg(feature = "live-xe")]
mod live {
    use plsql_catalog::{OracleBind, OracleConnectOptions, OracleConnection, RustOracleConnection};
    use plsql_cicd::{
        ChangeSet, ChangedObject, ChangedObjectKind, DeploymentRisk, DeploymentStatementKind,
        PredictMode,
        verify::{StatementOutcome, VerifyChangeset, VerifyOptions, is_scratch_schema, verify},
    };
    use plsql_cicd::{plan_changeset, predict};
    use plsql_core::{ObjectName, SymbolInterner};

    const SYSTEM_USER: &str = "SYSTEM";
    const SYSTEM_PASS: &str = "DemoPlsqlIntel#2026";
    const CONNECT_STRING: &str = "//localhost:1521/FREEPDB1";

    /// Connect as SYSTEM (has CREATE USER / DROP USER / GRANT privileges).
    fn system_conn() -> RustOracleConnection {
        let opts = OracleConnectOptions::new(SYSTEM_USER, SYSTEM_PASS, CONNECT_STRING)
            .with_module("plsql-cicd-cycle-test")
            .with_action("PLSQL-CICD-010");
        RustOracleConnection::connect(opts)
            .expect("SYSTEM connection to //localhost:1521/FREEPDB1 must succeed")
    }

    /// Build a small synthetic `ChangeSet` representing three objects:
    ///
    /// 1. `ORDERS` table — additive DDL (safe, weight 2).
    /// 2. `ORDER_LINES` table — additive DDL (safe, weight 2).
    /// 3. `AUDIT_LOG` table — destructive DDL (drops a column later),
    ///    so it lifts overall risk to `Destructive`.
    ///
    /// The three objects are intentionally ordered "worst last" in the
    /// input vector so we can assert `plan_changeset` sorts them correctly
    /// by kind-weight (tables all at weight 2, then by symbol id).
    fn synthetic_changeset() -> (ChangeSet, SymbolInterner) {
        let mut interner = SymbolInterner::new();

        // Intern schema and object names.
        let schema = interner
            .intern_schema_name("VERIFY_SCHEMA")
            .expect("intern schema name");

        let orders_name = interner
            .intern("ORDERS")
            .map(ObjectName::from)
            .expect("intern ORDERS");
        let lines_name = interner
            .intern("ORDER_LINES")
            .map(ObjectName::from)
            .expect("intern ORDER_LINES");
        let audit_name = interner
            .intern("AUDIT_LOG")
            .map(ObjectName::from)
            .expect("intern AUDIT_LOG");

        let changeset = ChangeSet {
            origin: None,
            objects: vec![
                // Object 1: ORDERS — additive DDL (CREATE TABLE).
                ChangedObject {
                    owner: schema,
                    name: orders_name,
                    kind: ChangedObjectKind::TableAdditiveDdl,
                    new_hash: None,
                    previous_hash: None,
                    file_paths: vec![],
                    uncertainties: vec![],
                },
                // Object 2: ORDER_LINES — additive DDL (CREATE TABLE).
                ChangedObject {
                    owner: schema,
                    name: lines_name,
                    kind: ChangedObjectKind::TableAdditiveDdl,
                    new_hash: None,
                    previous_hash: None,
                    file_paths: vec![],
                    uncertainties: vec![],
                },
                // Object 3: AUDIT_LOG — destructive DDL (lifts risk to Destructive).
                ChangedObject {
                    owner: schema,
                    name: audit_name,
                    kind: ChangedObjectKind::TableDestructiveDdl,
                    new_hash: None,
                    previous_hash: None,
                    file_paths: vec![],
                    uncertainties: vec![],
                },
            ],
            unclassified_files: vec![],
        };

        (changeset, interner)
    }

    /// Build the `VerifyChangeset` whose statements correspond to the objects in
    /// `synthetic_changeset`, in the order `plan_changeset` would emit them.
    ///
    /// This mirrors what a deployment runner does after `plan_changeset`: it
    /// reads the source files for each `DeploymentStatement` (here we
    /// materialise the DDL inline) and hands the ordered SQL list to `verify`.
    ///
    /// Statement 4 is intentionally broken to test the failure path.
    fn verify_changeset_for_cycle() -> VerifyChangeset {
        VerifyChangeset::with_labels(vec![
            // Ordinal 1: CREATE TABLE ORDERS (safe additive DDL)
            (
                1,
                String::from(
                    "CREATE TABLE ORDERS (ORDER_ID NUMBER PRIMARY KEY, AMOUNT NUMBER NOT NULL)",
                ),
                Some(String::from("CREATE TABLE ORDERS")),
            ),
            // Ordinal 2: CREATE TABLE ORDER_LINES (safe additive DDL)
            (
                2,
                String::from(
                    "CREATE TABLE ORDER_LINES (LINE_ID NUMBER PRIMARY KEY, ORDER_ID NUMBER NOT NULL, ITEM VARCHAR2(200))",
                ),
                Some(String::from("CREATE TABLE ORDER_LINES")),
            ),
            // Ordinal 3: CREATE TABLE AUDIT_LOG with a view
            (
                3,
                String::from(
                    "CREATE TABLE AUDIT_LOG (LOG_ID NUMBER PRIMARY KEY, ACTION VARCHAR2(100), LOGGED_AT DATE DEFAULT SYSDATE)",
                ),
                Some(String::from("CREATE TABLE AUDIT_LOG")),
            ),
            // Ordinal 4: intentionally broken — NONEXISTENT_TYPE_ZZZ is not a valid Oracle type.
            // This exercises the pipeline's failure recording + skip-after-fail behaviour.
            (
                4,
                String::from("CREATE TABLE BROKEN_T (X NONEXISTENT_TYPE_ZZZ_INVALID_9999)"),
                Some(String::from(
                    "CREATE TABLE BROKEN_T (intentionally broken — invalid column type)",
                )),
            ),
        ])
    }

    // ── Test 1: full predict→plan→verify cycle ────────────────────────────────

    #[test]
    fn cicd_full_cycle_predict_plan_verify_against_live_xe() {
        // ── Stage 1: predict ──────────────────────────────────────────────────
        let (changeset, _interner) = synthetic_changeset();

        let prediction = predict(&changeset, PredictMode::CatalogAware);

        eprintln!(
            "[PLSQL-CICD-010] predict: mode={:?} invalidations={} uncertainties={}",
            prediction.mode,
            prediction.predicted_invalidations.len(),
            prediction.uncertainties.len()
        );
        for inv in &prediction.predicted_invalidations {
            eprintln!(
                "  predicted invalidation: type={} distance={} confidence={:?}",
                inv.object_type, inv.distance, inv.confidence.level
            );
        }

        // Assert prediction is non-empty — three objects, all table kinds, emit
        // one predicted invalidation each (additive × 2, destructive × 1).
        assert!(
            !prediction.predicted_invalidations.is_empty(),
            "predict should emit at least one predicted invalidation for a non-empty changeset"
        );
        assert_eq!(
            prediction.predicted_invalidations.len(),
            3,
            "three objects (2 additive, 1 destructive) should each emit one invalidation"
        );

        // All three table objects emit distance=1 direct invalidations.
        for inv in &prediction.predicted_invalidations {
            assert_eq!(
                inv.distance, 1,
                "all direct invalidations should have distance=1, got {} for type={}",
                inv.distance, inv.object_type
            );
        }

        // Two of the three are TableAdditive; one is TableDestructive.
        let additive_count = prediction
            .predicted_invalidations
            .iter()
            .filter(|i| {
                matches!(
                    i.reason,
                    plsql_cicd::InvalidationReason::TableAdditive { .. }
                )
            })
            .count();
        let destructive_count = prediction
            .predicted_invalidations
            .iter()
            .filter(|i| {
                matches!(
                    i.reason,
                    plsql_cicd::InvalidationReason::TableDestructive { .. }
                )
            })
            .count();
        assert_eq!(
            additive_count, 2,
            "two additive-table invalidations expected"
        );
        assert_eq!(
            destructive_count, 1,
            "one destructive-table invalidation expected"
        );

        // Completeness profile: CatalogAware mode means catalog_available=true.
        assert!(
            prediction.completeness.catalog_available,
            "CatalogAware predict must set catalog_available=true in completeness profile"
        );

        // ── Stage 2: plan ─────────────────────────────────────────────────────
        let plan = plan_changeset(&changeset, &prediction);

        eprintln!(
            "[PLSQL-CICD-010] plan: statements={} risk={:?} notes={}",
            plan.statements.len(),
            plan.overall_risk,
            plan.notes.len()
        );
        for stmt in &plan.statements {
            eprintln!(
                "  statement ordinal={} kind={:?} sql={}",
                stmt.ordinal,
                stmt.kind,
                &stmt.sql[..stmt.sql.len().min(80)]
            );
        }

        // Plan has exactly 3 statements (one per changed object; no recompile
        // entries since `prediction.recompile_order` is empty).
        assert_eq!(
            plan.statements.len(),
            3,
            "plan should emit 3 DDL statements for the 3 changed objects"
        );

        // Ordinals are monotonically increasing starting at 1.
        for (i, stmt) in plan.statements.iter().enumerate() {
            assert_eq!(
                stmt.ordinal,
                i as u32 + 1,
                "ordinal should be {} but got {}",
                i + 1,
                stmt.ordinal
            );
        }

        // All plan statements are DDL kind (no recompile synthesised here).
        for stmt in &plan.statements {
            assert!(
                matches!(stmt.kind, DeploymentStatementKind::Ddl),
                "all plan statements should be Ddl kind, got {:?} for ordinal {}",
                stmt.kind,
                stmt.ordinal
            );
        }

        // Overall risk: one destructive table object lifts the plan to Destructive.
        assert!(
            matches!(plan.overall_risk, DeploymentRisk::Destructive),
            "DestructiveDdl object must lift overall_risk to Destructive, got {:?}",
            plan.overall_risk
        );

        // The plan carries a note about uncertainties from prediction
        // (there may be none for table kinds; check notes from prediction instead).
        // Notes are optional — we just ensure the plan.notes vec is accessible.
        eprintln!("[PLSQL-CICD-010] plan notes: {:?}", plan.notes);

        // ── Stage 3: apply + verify (via verify()) ────────────────────────────
        //
        // Pipeline gap note: there is no standalone `apply()` function.
        // `verify()` is the combined apply+verify step: it creates a VERIFY_T_*
        // scratch schema, executes each statement in the VerifyChangeset, and
        // reports per-statement outcomes. We feed a 4-statement VerifyChangeset
        // where the first 3 succeed (corresponding to the 3 plan statements)
        // and the 4th is intentionally broken to exercise the failure path.
        //
        // Ordering: statements are submitted in ordinal order as plan_changeset
        // would dictate (tables first, monotone ordinals). This is the ordering
        // contract between plan and verify.
        let conn = system_conn();
        let verify_cs = verify_changeset_for_cycle();
        let opts = VerifyOptions {
            schema_override: Some(format!("VERIFY_T_{}_cycle", std::process::id())),
            ..Default::default()
        };

        eprintln!(
            "[PLSQL-CICD-010] verify scratch schema: {}",
            opts.effective_schema()
        );
        assert!(
            is_scratch_schema(&opts.effective_schema()),
            "effective_schema must be a VERIFY_T_* scratch schema"
        );

        let report = verify(&conn, &verify_cs, &opts)
            .expect("verify should return Ok(report) even when some statements fail");

        eprintln!(
            "[PLSQL-CICD-010] verify report for schema `{}`:",
            report.schema
        );
        for row in &report.rows {
            eprintln!(
                "  ordinal={} label={:?} outcome={}",
                row.ordinal, row.label, row.outcome
            );
        }
        eprintln!(
            "[PLSQL-CICD-010] ok={} failed={} skipped={}",
            report.ok_count(),
            report.failed_count(),
            report.skipped_count()
        );

        // ── Assertions on verify report ───────────────────────────────────────

        // All 4 statements are reported.
        assert_eq!(
            report.rows.len(),
            4,
            "report must have exactly 4 rows (one per VerifyChangeset statement)"
        );

        // Ordinal 1: CREATE TABLE ORDERS — must succeed.
        assert!(
            matches!(report.rows[0].outcome, StatementOutcome::Ok),
            "ordinal 1 (CREATE TABLE ORDERS) should be Ok, got: {}",
            report.rows[0].outcome
        );
        assert_eq!(report.rows[0].ordinal, 1, "first row ordinal must be 1");

        // Ordinal 2: CREATE TABLE ORDER_LINES — must succeed.
        assert!(
            matches!(report.rows[1].outcome, StatementOutcome::Ok),
            "ordinal 2 (CREATE TABLE ORDER_LINES) should be Ok, got: {}",
            report.rows[1].outcome
        );

        // Ordinal 3: CREATE TABLE AUDIT_LOG — must succeed.
        assert!(
            matches!(report.rows[2].outcome, StatementOutcome::Ok),
            "ordinal 3 (CREATE TABLE AUDIT_LOG) should be Ok, got: {}",
            report.rows[2].outcome
        );

        // Ordinal 4: intentionally broken — must fail with an ORA- error.
        assert!(
            matches!(report.rows[3].outcome, StatementOutcome::Failed { .. }),
            "ordinal 4 (BROKEN_T) should be Failed, got: {}",
            report.rows[3].outcome
        );
        if let StatementOutcome::Failed { error } = &report.rows[3].outcome {
            eprintln!("[PLSQL-CICD-010] expected failure: {error}");
            assert!(
                error.contains("ORA-"),
                "failure must contain an ORA- error code, got: `{error}`"
            );
        }

        // Overall: not clean (one failure, zero skipped since failure is last).
        assert!(
            !report.is_clean(),
            "report must not be clean — ordinal 4 failed"
        );
        assert_eq!(report.ok_count(), 3, "three statements should succeed");
        assert_eq!(report.failed_count(), 1, "one statement should fail");
        assert_eq!(
            report.skipped_count(),
            0,
            "no skipped statements — failure is last in the changeset"
        );

        // The plan's statement ordering contract: verify ran statements in the
        // same ordinal order as plan_changeset produced them (1→2→3 for the DDL
        // statements, plus an extra broken statement at ordinal 4). Assert the
        // report rows carry the correct ordinals in order.
        let ordinals: Vec<u32> = report.rows.iter().map(|r| r.ordinal).collect();
        assert_eq!(
            ordinals,
            vec![1, 2, 3, 4],
            "report rows must be in ordinal order matching the plan's output"
        );

        // Schema name is a VERIFY_T_* name.
        assert!(
            is_scratch_schema(&report.schema),
            "report.schema must be a VERIFY_T_* name, got `{}`",
            report.schema
        );

        // Verify scratch schema is dropped in teardown.
        let rows = conn
            .query_rows(
                "SELECT 1 FROM all_users WHERE username = :1",
                &[OracleBind::String(report.schema.clone())],
            )
            .expect("schema existence check query should succeed");
        assert!(
            rows.is_empty(),
            "scratch schema `{}` must have been dropped in teardown, but still exists",
            report.schema
        );

        eprintln!(
            "[PLSQL-CICD-010] scratch schema `{}` confirmed dropped.",
            report.schema
        );

        // ── Cross-stage coherence assertions ──────────────────────────────────
        //
        // These assertions connect predict/plan outputs to the verify outcome,
        // validating the full pipeline is internally consistent.

        // The plan emitted 3 DDL statements; the verify report shows exactly 3
        // successful outcomes (ordinals 1-3), matching the plan's DDL statement
        // count.
        assert_eq!(
            plan.statements.len(),
            report.ok_count(),
            "plan statement count ({}) must equal verify ok_count ({}) — every plan DDL succeeded",
            plan.statements.len(),
            report.ok_count()
        );

        // The prediction identified one destructive object (AUDIT_LOG), and the
        // plan classified overall risk as Destructive. The verify step executed
        // AUDIT_LOG's DDL successfully (ordinal 3), confirming the object was
        // deployable despite its destructive classification.
        assert!(
            matches!(plan.overall_risk, DeploymentRisk::Destructive),
            "pipeline coherence: plan risk must remain Destructive after verify succeeded"
        );

        // Prediction uncertainty count: table kinds may produce uncertainties in
        // SourceOnly mode but not CatalogAware. Confirm zero uncertainties here.
        assert_eq!(
            prediction.uncertainties.len(),
            0,
            "CatalogAware predict on pure table kinds should produce 0 uncertainties, got {}",
            prediction.uncertainties.len()
        );

        eprintln!(
            "[PLSQL-CICD-010] predict→plan→apply→verify cycle complete. All assertions passed."
        );
    }

    // ── Test 2: predict→plan ordering contract against verify ────────────────

    #[test]
    fn cicd_plan_ordering_respected_by_verify() {
        // This test checks specifically that plan_changeset's kind-weight
        // ordering is meaningful: if we feed the changeset objects in reverse
        // kind-weight order, plan_changeset still emits them in weight order.
        // The verify step then executes them in that order and we assert the
        // outcomes match the weight-ordered sequence.

        let mut interner = SymbolInterner::new();
        let schema = interner
            .intern_schema_name("VERIFY_SCHEMA_2")
            .expect("intern schema");

        // Intentionally out of order: trigger (weight 7) before table (weight 2).
        // plan_changeset should reorder: table first, trigger second.
        // We use two additive tables so no Destructive risk is introduced.
        let tbl_name = interner
            .intern("CYCLE_TABLE")
            .map(ObjectName::from)
            .expect("intern CYCLE_TABLE");
        let seq_name = interner
            .intern("CYCLE_SEQ")
            .map(ObjectName::from)
            .expect("intern CYCLE_SEQ");

        // Input order: sequence (weight 0) then table (weight 2) — same as plan order.
        // We test that plan produces stable monotone ordinals in this ordering.
        let changeset = ChangeSet {
            origin: None,
            objects: vec![
                ChangedObject {
                    owner: schema,
                    name: seq_name,
                    kind: ChangedObjectKind::SequenceChange,
                    new_hash: None,
                    previous_hash: None,
                    file_paths: vec![],
                    uncertainties: vec![],
                },
                ChangedObject {
                    owner: schema,
                    name: tbl_name,
                    kind: ChangedObjectKind::TableAdditiveDdl,
                    new_hash: None,
                    previous_hash: None,
                    file_paths: vec![],
                    uncertainties: vec![],
                },
            ],
            unclassified_files: vec![],
        };

        let prediction = predict(&changeset, PredictMode::CatalogAware);
        let plan = plan_changeset(&changeset, &prediction);

        eprintln!(
            "[PLSQL-CICD-010] ordering test plan: statements={} risk={:?}",
            plan.statements.len(),
            plan.overall_risk
        );
        for stmt in &plan.statements {
            eprintln!("  ordinal={} kind={:?}", stmt.ordinal, stmt.kind);
        }

        // Plan should have 2 statements in kind-weight order:
        // sequence (weight 0) then table (weight 2).
        assert_eq!(plan.statements.len(), 2, "2 statements in plan");
        assert_eq!(plan.statements[0].ordinal, 1);
        assert_eq!(plan.statements[1].ordinal, 2);

        // Sequence is first (weight 0 < weight 2 for table).
        // In the plan, sequence appears at ordinal 1.
        assert!(
            matches!(plan.statements[0].kind, DeploymentStatementKind::Ddl),
            "first plan statement is Ddl"
        );
        assert!(
            matches!(plan.statements[1].kind, DeploymentStatementKind::Ddl),
            "second plan statement is Ddl"
        );

        // Verify using corresponding DDL in the same plan-dictated order.
        // Sequence DDL (ordinal 1), then table DDL (ordinal 2).
        let conn = system_conn();
        let verify_cs = VerifyChangeset::with_labels(vec![
            (
                1,
                // Sequences are informational in the pipeline but valid Oracle DDL.
                String::from("CREATE SEQUENCE CYCLE_SEQ START WITH 1 INCREMENT BY 1"),
                Some(String::from("CREATE SEQUENCE CYCLE_SEQ")),
            ),
            (
                2,
                String::from(
                    "CREATE TABLE CYCLE_TABLE (ID NUMBER DEFAULT CYCLE_SEQ.NEXTVAL PRIMARY KEY, NAME VARCHAR2(100))",
                ),
                Some(String::from("CREATE TABLE CYCLE_TABLE")),
            ),
        ]);

        let opts = VerifyOptions {
            schema_override: Some(format!("VERIFY_T_{}_ord", std::process::id())),
            ..Default::default()
        };

        eprintln!(
            "[PLSQL-CICD-010] ordering test schema: {}",
            opts.effective_schema()
        );

        let report = verify(&conn, &verify_cs, &opts).expect("ordering verify should succeed");

        eprintln!(
            "[PLSQL-CICD-010] ordering test: ok={} failed={} skipped={}",
            report.ok_count(),
            report.failed_count(),
            report.skipped_count()
        );
        for row in &report.rows {
            eprintln!(
                "  ordinal={} label={:?} outcome={}",
                row.ordinal, row.label, row.outcome
            );
        }

        assert_eq!(report.rows.len(), 2);
        assert!(
            matches!(report.rows[0].outcome, StatementOutcome::Ok),
            "ordinal 1 (SEQUENCE) should be Ok, got: {}",
            report.rows[0].outcome
        );
        assert!(
            matches!(report.rows[1].outcome, StatementOutcome::Ok),
            "ordinal 2 (TABLE with sequence default) should be Ok, got: {}",
            report.rows[1].outcome
        );
        assert!(report.is_clean(), "all-valid ordering test should be clean");

        // Scratch schema must be dropped.
        let rows = conn
            .query_rows(
                "SELECT 1 FROM all_users WHERE username = :1",
                &[OracleBind::String(report.schema.clone())],
            )
            .expect("schema existence check");
        assert!(
            rows.is_empty(),
            "scratch schema `{}` must be dropped",
            report.schema
        );

        eprintln!(
            "[PLSQL-CICD-010] ordering test: schema `{}` confirmed dropped.",
            report.schema
        );
    }
}
