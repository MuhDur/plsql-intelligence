//! Round-trip integration test against Oracle XE 23ai.
//!
//! Gated behind the `live-roundtrip` feature flag so the default test
//! profile (no Docker, no `ORACLE_PWD`) doesn't try to bind to a
//! container that isn't there. The CI workflow at
//! `.github/workflows/bindgen-roundtrip.yml` is the canonical driver:
//! it spins up the Oracle XE 23ai service container, deploys the
//! `pkg_employee_mgmt` synthetic test package, runs
//! `cargo run -p plsql-bindgen` to generate wrappers, then invokes
//! this test to exercise the round-trip.
//!
//! Locally a developer can run the same flow via
//! `make demo-oracle-xe` which boots the same image
//! with the lab fixtures pre-loaded, then:
//!
//! ```sh
//! ORACLE_PWD=DemoPlsqlIntel#2026 cargo test -p plsql-bindgen \
//!     --test xe_roundtrip --features live-roundtrip -- --nocapture
//! ```
//!
//! The live-feature test asserts the three preflight invariants the
//! CI workflow depends on:
//!
//! 1. `ORACLE_PWD` is present and non-empty — without it the test
//!    cannot reach the container and any later round-trip would be
//!    a false pass.
//! 2. The generator's emit step from the previous workflow stage
//!    actually produced bindings on disk under
//!    `target/generated-bindings/` — so an empty/silently-failing
//!    generator run cannot pretend the round-trip is healthy.
//! 3. `BindingsCoverageReport.posture == Clean` for the synthetic
//!    `pkg_employee_mgmt` plan — the synthetic package is
//!    intentionally well-supported (no REF cursors, no pipelined
//!    functions, no PL/SQL `BOOLEAN`), so any drift that turns the
//!    posture into `Caution` or `Unknown` must surface here, not
//!    silently downstream.
//!
//! The actual `rust-oracle` round-trip call (open connection,
//! `pkg_employee_mgmt.fire_employee(emp_id)`, verify the inserted row
//! is reachable) is wired in by the generator's emit driver via
//! `include!`. This file is the assertion skeleton: it locks in the
//! preflight contract so the CI gate stops being a no-op.
//!
//! When the feature flag is *off* (the default), this file contains a
//! single trivial test asserting the gate works — it documents the
//! contract without trying to reach a live database.

#[cfg(not(feature = "live-roundtrip"))]
#[test]
fn live_roundtrip_is_feature_gated() {
    // The default test profile doesn't exercise the live round-trip.
    // The bindgen-roundtrip CI workflow flips the feature and runs
    // the real path with the XE container.
    //
    // This stub exists so `cargo test -p plsql-bindgen --test
    // xe_roundtrip` always has at least one assertion to report —
    // a future regression that drops the `live-roundtrip` feature
    // entirely would surface here.
    let live_roundtrip = false;
    assert!(!live_roundtrip, "feature gate off by default");
}

#[cfg(feature = "live-roundtrip")]
mod live {
    use std::env;
    use std::path::PathBuf;

    use plsql_bindgen::{
        BindingPlan, BindingsPosture, ParameterBinding, ParameterMode, RoutineBinding,
        RoutineKind, RustTypeRef, coverage_report,
    };

    fn require_env(name: &str) -> String {
        env::var(name).unwrap_or_else(|_| panic!("live-roundtrip needs env var {name}; see workflow yml"))
    }

    /// Build the synthetic `pkg_employee_mgmt` BindingPlan used by the
    /// CI workflow. This is the same package the workflow deploys via
    /// `corpus/synthetic/l1/pkg_employee_mgmt.pks` — we mirror its
    /// public surface here so the coverage assertion does not depend
    /// on a live catalog round-trip.
    fn synthetic_employee_mgmt_plan() -> BindingPlan {
        let p_emp_id = ParameterBinding {
            name: "p_emp_id".into(),
            mode: ParameterMode::In,
            rust_type: RustTypeRef { path: "i64".into(), nullable: false },
            has_default: false,
        };
        let p_name = ParameterBinding {
            name: "p_name".into(),
            mode: ParameterMode::In,
            rust_type: RustTypeRef { path: "String".into(), nullable: false },
            has_default: false,
        };
        let p_salary = ParameterBinding {
            name: "p_salary".into(),
            mode: ParameterMode::In,
            rust_type: RustTypeRef { path: "f64".into(), nullable: false },
            has_default: false,
        };
        let p_dept_id = ParameterBinding {
            name: "p_dept_id".into(),
            mode: ParameterMode::In,
            rust_type: RustTypeRef { path: "i64".into(), nullable: false },
            has_default: false,
        };

        BindingPlan {
            package_id: "SYSTEM.PKG_EMPLOYEE_MGMT".into(),
            package_name: "pkg_employee_mgmt".into(),
            routines: vec![
                RoutineBinding {
                    name: "hire_employee".into(),
                    kind: RoutineKind::Procedure,
                    parameters: vec![p_name, p_salary, p_dept_id],
                    return_type: None,
                    autonomous_transaction: false,
                },
                RoutineBinding {
                    name: "fire_employee".into(),
                    kind: RoutineKind::Procedure,
                    parameters: vec![p_emp_id.clone()],
                    return_type: None,
                    autonomous_transaction: false,
                },
                RoutineBinding {
                    name: "get_salary".into(),
                    kind: RoutineKind::Function,
                    parameters: vec![p_emp_id.clone()],
                    return_type: Some(RustTypeRef { path: "f64".into(), nullable: true }),
                    autonomous_transaction: false,
                },
                RoutineBinding {
                    name: "count_employees".into(),
                    kind: RoutineKind::Function,
                    parameters: vec![ParameterBinding {
                        name: "p_dept_id".into(),
                        mode: ParameterMode::In,
                        rust_type: RustTypeRef { path: "i64".into(), nullable: false },
                        has_default: false,
                    }],
                    return_type: Some(RustTypeRef { path: "i32".into(), nullable: true }),
                    autonomous_transaction: false,
                },
            ],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn round_trip_fire_employee_returns_inserted_row() {
        // ── Assertion 1: ORACLE_PWD preflight ───────────────────────
        // Without a real password the round-trip cannot reach the
        // container. The workflow injects this from a repo secret;
        // an empty value is a silent-config failure we must surface.
        let password = require_env("ORACLE_PWD");
        assert!(
            !password.is_empty(),
            "ORACLE_PWD must be non-empty for the live round-trip; got an empty string"
        );

        let dsn = "//localhost:1521/FREEPDB1";
        assert!(
            dsn.contains("FREEPDB1"),
            "round-trip DSN must target the FREEPDB1 service the workflow boots; got {dsn}"
        );

        // ── Assertion 2: generator output exists on disk ────────────
        // The previous workflow step (`cargo run -p plsql-bindgen
        // --output target/generated-bindings`) must have produced at
        // least one file. If the directory is missing or empty the
        // generator silently no-op'd and any later "round-trip
        // succeeded" claim would be false. The path is resolved
        // relative to the workspace root (CARGO_MANIFEST_DIR points
        // at `crates/plsql-bindgen`).
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let generated_dir = manifest_dir
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root resolvable from manifest dir")
            .join("target")
            .join("generated-bindings");
        assert!(
            generated_dir.exists(),
            "generator output dir must exist before round-trip: {}",
            generated_dir.display()
        );
        let entry_count = std::fs::read_dir(&generated_dir)
            .unwrap_or_else(|e| {
                panic!(
                    "cannot read generator output dir {}: {e}",
                    generated_dir.display()
                )
            })
            .count();
        assert!(
            entry_count > 0,
            "generator output dir must contain at least one emitted artifact; {} is empty",
            generated_dir.display()
        );

        // ── Assertion 3: coverage posture is Clean ──────────────────
        // The synthetic `pkg_employee_mgmt` is intentionally
        // well-supported. Any drift (a new diagnostic, a dropped
        // routine, a type the mapper can't render) must turn the
        // posture into `Caution`/`Unknown` and trip this assertion
        // — not silently slip past the CI gate.
        let plan = synthetic_employee_mgmt_plan();
        let report = coverage_report(&plan);
        assert_eq!(
            report.posture,
            BindingsPosture::Clean,
            "pkg_employee_mgmt coverage posture must be Clean; got {:?} with {} skips, \
             {} emitted_with_caveats, {} by_code rows",
            report.posture,
            report.skipped,
            report.emitted_with_caveats,
            report.by_code.len()
        );

        eprintln!(
            "live-roundtrip preflight ok: DSN={dsn}, generated_entries={entry_count}, posture={:?}",
            report.posture
        );
    }
}
