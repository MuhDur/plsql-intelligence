//! The six `CompletenessReport` gap metrics are measured from the run
//! (pvlqq.10.1.4): real parsed source through `analyze_project`, exact
//! counts, and a gap-free project that reads `Measured(0)`, never
//! `Unmeasured`. A `t@link` table access targets the remote object
//! (oraclemcp-5e2d5). Fixtures are synthetic.

use std::path::PathBuf;

use plsql_core::Measured;
use plsql_depgraph::EdgeKind;
use plsql_engine::{AnalysisRequest, AnalysisRun, analyze_project};

const GAPS: &str = "\
-- CREATE OR REPLACE PROCEDURE omcp_cm_comment wrapped
CREATE OR REPLACE PACKAGE omcp_cm_pkg AS
  PROCEDURE run_all(p_tab VARCHAR2);
END omcp_cm_pkg;
/
CREATE OR REPLACE PACKAGE BODY omcp_cm_pkg AS
  PROCEDURE run_all(p_tab VARCHAR2) IS
    l_n NUMBER;
    l_c INTEGER;
    l_rc SYS_REFCURSOR;
    l_mail VARCHAR2(40) := 'ops@omcp_cm_link';
  BEGIN
    SELECT COUNT(*) INTO l_n FROM omcp_cm_remote_orders@omcp_cm_link;
    EXECUTE IMMEDIATE 'TRUNCATE TABLE omcp_cm_stage';
    EXECUTE IMMEDIATE 'DELETE FROM ' || p_tab;
    omcp_cm_missing_proc(l_n);
    omcp_cm_remote_proc@omcp_cm_link(l_n);
    IF l_n > 0 THEN
      EXECUTE IMMEDIATE p_tab;
    END IF;
    l_c := DBMS_SQL.OPEN_CURSOR;
    DBMS_SQL.PARSE(l_c, 'SELECT 1 FROM dual', DBMS_SQL.NATIVE);
    OPEN l_rc FOR 'SELECT * FROM ' || p_tab;
  END run_all;
END omcp_cm_pkg;
/
CREATE OR REPLACE PACKAGE omcp_cm_orphan AS
  PROCEDURE lonely;
END omcp_cm_orphan;
/
CREATE OR REPLACE PACKAGE omcp_cm_sealed AS
  PROCEDURE hidden;
END omcp_cm_sealed;
/
CREATE OR REPLACE PACKAGE BODY omcp_cm_sealed wrapped
a000000
1
abcd
b
2f 4d
Ujk2Bl8zXyMQ4g==
/
CREATE OR REPLACE PROCEDURE omcp_cm_hidden wrapped
a000000
1
abcd
7
2f 4d
Ujk2Bl8zXyMQ4g==
/
";

const CLEAN: &str = "\
CREATE OR REPLACE PACKAGE omcp_cm_clean AS
  FUNCTION total RETURN NUMBER;
  PROCEDURE touch;
END omcp_cm_clean;
/
CREATE OR REPLACE PACKAGE BODY omcp_cm_clean AS
  FUNCTION total RETURN NUMBER IS
    l_n NUMBER;
  BEGIN
    SELECT COUNT(*) INTO l_n FROM omcp_cm_orders;
    RETURN l_n;
  END total;
  PROCEDURE touch IS
  BEGIN
    UPDATE omcp_cm_orders SET status = 'X' WHERE id = total;
  END touch;
END omcp_cm_clean;
/
CREATE OR REPLACE PACKAGE omcp_cm_types AS
  SUBTYPE flag IS VARCHAR2(1);
END omcp_cm_types;
/
";

fn analyze(name: &str, source: &str) -> AnalysisRun {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&root).expect("fixture dir");
    std::fs::write(root.join("fixture.sql"), source).expect("write fixture");
    analyze_project(AnalysisRequest {
        project_root: root,
        ..AnalysisRequest::default()
    })
    .expect("analysis runs")
}

#[test]
fn gap_metrics_count_what_the_source_contains() {
    let run = analyze("completeness_gaps", GAPS);
    let c = &run.completeness;
    // omcp_cm_sealed's body and omcp_cm_hidden; the commented header is trivia.
    assert_eq!(c.wrapped_units, Measured::Measured(2));
    // omcp_cm_orphan only: omcp_cm_sealed's body exists, wrapped.
    assert_eq!(c.missing_package_bodies, Measured::Measured(1));
    // Three EXECUTE IMMEDIATE, one DBMS_SQL.PARSE, one OPEN ... FOR <string>.
    assert_eq!(c.dynamic_sql_sites, Measured::Measured(5));
    // Concatenated or variable SQL text: two EXECUTE IMMEDIATE and the OPEN.
    assert_eq!(c.opaque_dynamic_sql_sites, Measured::Measured(3));
    // The remote table and the remote procedure; the e-mail literal is not a link.
    assert_eq!(c.db_link_edges, Measured::Measured(2));
    // omcp_cm_missing_proc and DBMS_SQL.PARSE resolve to no project object.
    assert_eq!(c.unresolved_references, Measured::Measured(2));

    let mut remote: Vec<String> = run
        .dep_graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::DbLink)
        .map(|e| run.dep_graph.nodes[&e.to].logical_id.as_str().to_string())
        .collect();
    remote.sort();
    assert_eq!(
        remote,
        [
            "OMCP_CM_REMOTE_ORDERS@OMCP_CM_LINK",
            "OMCP_CM_REMOTE_PROC@OMCP_CM_LINK"
        ]
    );
}

/// oraclemcp-5e2d5: `SELECT ... FROM t@link` reads the remote object, the
/// node the DbLink edge targets. It must not mint a Reads edge to local `T`.
#[test]
fn db_link_table_access_targets_the_remote_object() {
    let run = analyze("completeness_remote_access", GAPS);
    let graph = &run.dep_graph;
    let logical = |id| graph.nodes[id].logical_id.as_str().to_string();
    let reads: Vec<String> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Reads)
        .map(|e| logical(&e.to))
        .collect();
    assert_eq!(reads, ["OMCP_CM_REMOTE_ORDERS@OMCP_CM_LINK"]);
    assert!(
        graph
            .nodes
            .values()
            .all(|n| n.logical_id.as_str() != "OMCP_CM_REMOTE_ORDERS"),
        "no local node for the remote table"
    );
    let read = graph
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::Reads)
        .expect("remote read edge");
    let link = graph
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::DbLink && logical(&e.to) == reads[0])
        .expect("db-link edge to the remote table");
    assert_eq!((read.from, read.to), (link.from, link.to));
    assert_eq!(
        graph.provenance[&read.id].resolution_strategy,
        plsql_depgraph::ResolutionStrategy::DbLinkBoundary
    );
}

#[test]
fn gap_free_project_reads_measured_zero() {
    let run = analyze("completeness_clean", CLEAN);
    let c = &run.completeness;
    for (metric, value) in [
        ("wrapped_units", c.wrapped_units),
        ("missing_package_bodies", c.missing_package_bodies),
        ("dynamic_sql_sites", c.dynamic_sql_sites),
        ("opaque_dynamic_sql_sites", c.opaque_dynamic_sql_sites),
        ("db_link_edges", c.db_link_edges),
        ("unresolved_references", c.unresolved_references),
    ] {
        assert_eq!(value, Measured::Measured(0), "{metric}");
    }
    let json = serde_json::to_value(c).expect("serializes");
    assert_eq!(json["db_link_edges"], serde_json::json!(0));
}
