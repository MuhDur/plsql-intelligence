//! Package members, initializers, init section and default-expression call
//! edges as separately addressable units (T7.2a / InvocationClosureV1).
//!
//! Every assertion runs on real parsed source through the full pipeline
//! (ANTLR parse -> AST units -> IR `PackageUnits` -> depgraph), never on
//! hand-built IR. Fixtures are synthetic. Each test prints one JSONL case
//! line `{case_id, expected, actual}`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use plsql_depgraph::{DepGraph, EdgeKind};
use plsql_engine::{AnalysisRequest, AnalysisRun, analyze_project};
use plsql_ir::{Declaration, FactPayload, OverloadIdentity, PackagePart, PackageUnits};

const HELPERS: &str = "\
CREATE OR REPLACE FUNCTION f_side_effect RETURN NUMBER IS BEGIN RETURN 1; END f_side_effect;
/
CREATE OR REPLACE FUNCTION f_body_init RETURN NUMBER IS BEGIN RETURN 2; END f_body_init;
/
CREATE OR REPLACE FUNCTION f_default RETURN NUMBER IS BEGIN RETURN 3; END f_default;
/
CREATE OR REPLACE PROCEDURE audit_log(p_msg VARCHAR2) IS BEGIN NULL; END audit_log;
/
";

/// Spec and body in one source, so overload ordinals come from the spec.
const PACKAGE: &str = r#"CREATE OR REPLACE PACKAGE omcp_fx_pkg AS
  g_spec NUMBER := f_side_effect();
  PROCEDURE p(p_id IN NUMBER);
  PROCEDURE p(p_name IN VARCHAR2, p_limit IN NUMBER DEFAULT f_default());
  FUNCTION total RETURN NUMBER;
  PROCEDURE "MixedCase";
END omcp_fx_pkg;
/
CREATE OR REPLACE PACKAGE BODY omcp_fx_pkg AS
  g_body NUMBER := f_body_init();
  PROCEDURE p(p_id IN NUMBER) IS
  BEGIN
    audit_log('one');
    UPDATE omcp_fx_orders SET status = 'X' WHERE id = p_id;
  END p;
  PROCEDURE p(p_name IN VARCHAR2, p_limit IN NUMBER DEFAULT f_default()) IS
  BEGIN
    DELETE FROM omcp_fx_orders WHERE name = p_name;
  END p;
  FUNCTION total RETURN NUMBER IS
    l_n NUMBER := f_side_effect();
  BEGIN
    SELECT COUNT(*) INTO l_n FROM omcp_fx_orders;
    RETURN l_n;
  END total;
  PROCEDURE "MixedCase" IS
  BEGIN
    NULL;
  END "MixedCase";
BEGIN
  INSERT INTO omcp_fx_log(msg) VALUES ('init');
  COMMIT;
  g_body := omcp_fx_seq.NEXTVAL;
  g_body := UTL_HTTP.REQUEST('http://example.invalid/');
END omcp_fx_pkg;
/
"#;

fn analyze(files: &[(&str, &str)]) -> AnalysisRun {
    let base: PathBuf = std::env::temp_dir().join(format!(
        "plsql-pkg-units-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&base).unwrap();
    for (name, text) in files {
        std::fs::write(base.join(name), text).unwrap();
    }
    let run = analyze_project(AnalysisRequest {
        project_root: base.clone(),
        ..AnalysisRequest::default()
    })
    .expect("analysis succeeds");
    let _ = std::fs::remove_dir_all(&base);
    run
}

fn fixture_run() -> AnalysisRun {
    analyze(&[("helpers.sql", HELPERS), ("omcp_fx_pkg.sql", PACKAGE)])
}

/// Lower one source to IR through the same parser the engine uses.
fn lower(src: &str) -> (Vec<Declaration>, Vec<plsql_core::Diagnostic>) {
    use plsql_parser::ParseBackend;
    let backend = plsql_parser_antlr::Antlr4RustBackend::new();
    let parsed = backend.parse(
        src,
        plsql_core::FileId::new(1),
        &plsql_parser::ParseOptions::default(),
    );
    let mut interner = plsql_core::SymbolInterner::new();
    let lowered = plsql_ir::lower_top_level(&parsed.ast, &mut interner);
    (lowered.declarations, lowered.diagnostics)
}

fn package_units(decls: &[Declaration], part: PackagePart) -> PackageUnits {
    decls
        .iter()
        .find_map(|d| match d {
            Declaration::Package(p) if p.units.part == part => Some(p.units.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {part:?} package in {decls:?}"))
}

/// `(edge kind, target logical id)` for every depgraph edge leaving the node
/// whose logical id is `from`.
fn edges_from(graph: &DepGraph, from: &str) -> BTreeSet<(String, String)> {
    let Some(node) = graph.nodes.values().find(|n| n.logical_id.as_str() == from) else {
        return BTreeSet::new();
    };
    graph
        .edges
        .iter()
        .filter(|e| e.from == node.id)
        .map(|e| {
            (
                format!("{:?}", e.kind),
                graph.nodes[&e.to].logical_id.as_str().to_string(),
            )
        })
        .collect()
}

fn has_node(graph: &DepGraph, id: &str) -> bool {
    graph.nodes.values().any(|n| n.logical_id.as_str() == id)
}

/// Every Calls fact (resolved or not) whose caller is `from`.
fn call_facts_from(run: &AnalysisRun, from: &str) -> BTreeSet<String> {
    run.fact_store
        .facts
        .iter()
        .filter_map(|f| match &f.payload {
            FactPayload::DependencyEdge {
                from_logical_id,
                to_logical_id,
                edge_kind,
            } if from_logical_id == from && edge_kind == "Calls" => Some(to_logical_id.clone()),
            _ => None,
        })
        .collect()
}

fn edge(kind: EdgeKind, to: &str) -> (String, String) {
    (format!("{kind:?}"), to.to_string())
}

fn case_log(case_id: &str, expected: &str, actual: impl std::fmt::Debug) {
    println!(
        "{}",
        serde_json::json!({
            "case_id": case_id,
            "expected": expected,
            "actual": format!("{actual:?}"),
        })
    );
}

#[test]
fn package_body_members_are_separate_units() {
    let run = fixture_run();
    let g = &run.dep_graph;
    let p1 = edges_from(g, "OMCP_FX_PKG.P#1");
    let total = edges_from(g, "OMCP_FX_PKG.TOTAL");
    let package_node = edges_from(g, "OMCP_FX_PKG");
    case_log(
        "package_body_members_are_separate_units",
        "each member node owns exactly its own edges; the package node owns none",
        (&p1, &total, &package_node),
    );
    assert!(p1.contains(&edge(EdgeKind::Calls, "AUDIT_LOG")), "{p1:?}");
    assert!(
        p1.contains(&edge(EdgeKind::Writes, "OMCP_FX_ORDERS")),
        "{p1:?}"
    );
    assert!(
        !p1.contains(&edge(EdgeKind::Calls, "F_SIDE_EFFECT")),
        "{p1:?}"
    );
    // TOTAL's local initializer is part of TOTAL's own execution.
    assert!(
        total.contains(&edge(EdgeKind::Calls, "F_SIDE_EFFECT")),
        "{total:?}"
    );
    assert!(
        total.contains(&edge(EdgeKind::Reads, "OMCP_FX_ORDERS")),
        "{total:?}"
    );
    assert!(
        !total.contains(&edge(EdgeKind::Calls, "AUDIT_LOG")),
        "{total:?}"
    );
    // A flattening lowerer would hang every member's edges off the package.
    assert!(
        package_node.is_empty(),
        "package node must carry no flattened body edges: {package_node:?}"
    );

    let (decls, _) = lower(PACKAGE);
    let body = package_units(&decls, PackagePart::Body);
    assert!(body.lowered);
    assert_eq!(body.members.len(), 4);
    for m in &body.members {
        assert!(
            m.name == "MixedCase" || !m.body.is_empty(),
            "member {} has its own statements",
            m.name
        );
    }
}

#[test]
fn package_overloads_keep_distinct_identity() {
    let (decls, diags) = lower(PACKAGE);
    let spec = package_units(&decls, PackagePart::Spec);
    let body = package_units(&decls, PackagePart::Body);
    let overloads = |u: &PackageUnits| -> Vec<(String, OverloadIdentity, usize)> {
        u.members
            .iter()
            .filter(|m| m.name == "P")
            .map(|m| (m.name.clone(), m.overload, m.params.len()))
            .collect()
    };
    case_log(
        "package_overloads_keep_distinct_identity",
        "spec and body P#1 (1 param) / P#2 (2 params); body joined to spec",
        (overloads(&spec), overloads(&body)),
    );
    assert_eq!(
        overloads(&spec),
        vec![
            ("P".into(), OverloadIdentity::Ordinal(1), 1),
            ("P".into(), OverloadIdentity::Ordinal(2), 2)
        ]
    );
    assert_eq!(overloads(&body), overloads(&spec));
    assert!(
        !diags
            .iter()
            .any(|d| d.code == "IR_PACKAGE_OVERLOAD_AMBIGUOUS"),
        "{diags:?}"
    );
    assert!(spec.is_complete() && body.is_complete());

    // Distinct nodes, distinct edges.
    let run = fixture_run();
    let p1 = edges_from(&run.dep_graph, "OMCP_FX_PKG.P#1");
    let p2 = edges_from(&run.dep_graph, "OMCP_FX_PKG.P#2");
    assert!(p1.contains(&edge(EdgeKind::Calls, "AUDIT_LOG")));
    assert!(!p2.contains(&edge(EdgeKind::Calls, "AUDIT_LOG")));
    assert!(p2.contains(&edge(EdgeKind::Writes, "OMCP_FX_ORDERS")));

    // Without the spec in the same source the ordinal is not guessed: the
    // identity is Ambiguous (Unknown downstream) and diagnosed.
    let body_only = &PACKAGE[PACKAGE.find("CREATE OR REPLACE PACKAGE BODY").unwrap()..];
    let (decls, diags) = lower(body_only);
    let body = package_units(&decls, PackagePart::Body);
    let ids: Vec<String> = body
        .members
        .iter()
        .filter(|m| m.name == "P")
        .map(|m| m.unit_id("OMCP_FX_PKG"))
        .collect();
    assert_eq!(ids, vec!["OMCP_FX_PKG.P#?1", "OMCP_FX_PKG.P#?2"]);
    assert!(!body.is_complete());
    assert!(
        diags
            .iter()
            .filter(|d| d.code == "IR_PACKAGE_OVERLOAD_AMBIGUOUS")
            .count()
            == 2,
        "{diags:?}"
    );
}

#[test]
fn package_body_initialization_section_is_its_own_unit() {
    let run = fixture_run();
    let init = edges_from(&run.dep_graph, "OMCP_FX_PKG#init");
    let init_calls = call_facts_from(&run, "OMCP_FX_PKG#init");
    let (decls, _) = lower(PACKAGE);
    let section = package_units(&decls, PackagePart::Body)
        .init_section
        .expect("init section unit");
    let rendered = format!("{:?}", section.body);
    case_log(
        "package_body_initialization_section_is_its_own_unit",
        "#init writes OMCP_FX_LOG, calls UTL_HTTP.REQUEST, holds COMMIT and NEXTVAL",
        (&init, &init_calls, &rendered),
    );
    assert!(
        init.contains(&edge(EdgeKind::Writes, "OMCP_FX_LOG")),
        "{init:?}"
    );
    assert!(init_calls.contains("UTL_HTTP.REQUEST"), "{init_calls:?}");
    assert!(rendered.contains("COMMIT"), "{rendered}");
    assert!(rendered.contains("NEXTVAL"), "{rendered}");
    assert_eq!(section.body.len(), 4, "{rendered}");
    // The init section's effects are not attributed to any member.
    for member in ["OMCP_FX_PKG.P#1", "OMCP_FX_PKG.P#2", "OMCP_FX_PKG.TOTAL"] {
        assert!(
            !edges_from(&run.dep_graph, member).contains(&edge(EdgeKind::Writes, "OMCP_FX_LOG")),
            "{member} must not own the init section's INSERT"
        );
    }
}

#[test]
fn package_declaration_initializer_call_edge_preserved() {
    let run = fixture_run();
    let spec_init = edges_from(&run.dep_graph, "OMCP_FX_PKG#spec_initializers");
    let body_init = edges_from(&run.dep_graph, "OMCP_FX_PKG#body_initializers");
    case_log(
        "package_declaration_initializer_call_edge_preserved",
        "spec initializers call F_SIDE_EFFECT; body initializers call F_BODY_INIT",
        (&spec_init, &body_init),
    );
    assert!(
        spec_init.contains(&edge(EdgeKind::Calls, "F_SIDE_EFFECT")),
        "{spec_init:?}"
    );
    assert!(
        body_init.contains(&edge(EdgeKind::Calls, "F_BODY_INIT")),
        "{body_init:?}"
    );
    assert!(!body_init.contains(&edge(EdgeKind::Calls, "F_SIDE_EFFECT")));
}

#[test]
fn default_parameter_expression_call_edge_preserved() {
    let run = fixture_run();
    let body_default = edges_from(&run.dep_graph, "OMCP_FX_PKG.P#2(P_LIMIT)@body#default");
    let spec_default = edges_from(&run.dep_graph, "OMCP_FX_PKG.P#2(P_LIMIT)@spec#default");
    let member = edges_from(&run.dep_graph, "OMCP_FX_PKG.P#2");
    let (decls, _) = lower(PACKAGE);
    let p2 = package_units(&decls, PackagePart::Body)
        .members
        .into_iter()
        .find(|m| m.overload == OverloadIdentity::Ordinal(2))
        .expect("P#2");
    case_log(
        "default_parameter_expression_call_edge_preserved",
        "P_LIMIT's default is its own unit (spec and body) calling F_DEFAULT; the member body does not",
        (&body_default, &spec_default, &member, &p2.params),
    );
    assert!(
        body_default.contains(&edge(EdgeKind::Calls, "F_DEFAULT")),
        "{body_default:?}"
    );
    assert!(
        spec_default.contains(&edge(EdgeKind::Calls, "F_DEFAULT")),
        "{spec_default:?}"
    );
    // Supplying the argument must not evaluate the default: the member's own
    // unit does not call it.
    assert!(
        !member.contains(&edge(EdgeKind::Calls, "F_DEFAULT")),
        "{member:?}"
    );
    let limit = &p2.params[1];
    assert_eq!(limit.name, "P_LIMIT");
    assert_eq!(limit.default_text.as_deref(), Some("f_default()"));
    assert!(limit.default.is_some());
    assert!(p2.params[0].default.is_none());
}

#[test]
fn quoted_member_identity_case_preserved() {
    let (decls, _) = lower(PACKAGE);
    let body = package_units(&decls, PackagePart::Body);
    let spec = package_units(&decls, PackagePart::Spec);
    let names: Vec<&str> = body.members.iter().map(|m| m.name.as_str()).collect();
    let run = fixture_run();
    case_log(
        "quoted_member_identity_case_preserved",
        "\"MixedCase\" keeps its case; unquoted names fold to upper",
        (&names, has_node(&run.dep_graph, "OMCP_FX_PKG.MixedCase")),
    );
    assert!(names.contains(&"MixedCase"), "{names:?}");
    assert!(!names.contains(&"MIXEDCASE"), "{names:?}");
    assert!(names.contains(&"TOTAL"), "{names:?}");
    assert!(spec.members.iter().any(|m| m.name == "MixedCase"));
    assert!(has_node(&run.dep_graph, "OMCP_FX_PKG.MixedCase"));
}

#[test]
fn unattributed_package_construct_lowers_completeness() {
    let src = "\
CREATE OR REPLACE PACKAGE BODY omcp_fx_cc AS
  PROCEDURE ok IS BEGIN NULL; END ok;
  $IF $$omcp_debug $THEN
  PROCEDURE dbg IS BEGIN NULL; END dbg;
  $END
END omcp_fx_cc;
/
";
    let (decls, diags) = lower(src);
    let body = package_units(&decls, PackagePart::Body);
    let unattributed: Vec<&str> = body
        .unattributed
        .iter()
        .map(|u| u.reason.as_str())
        .collect();
    case_log(
        "unattributed_package_construct_lowers_completeness",
        "the $IF block is unattributed; the package is incomplete and diagnosed",
        (&unattributed, body.is_complete()),
    );
    assert_eq!(unattributed, vec!["conditional_compilation"]);
    assert!(!body.is_complete());
    assert!(
        diags.iter().any(|d| d.code == "IR_PACKAGE_UNATTRIBUTED"
            && d.unknown_reasons
                .contains(&plsql_core::UnknownReason::ConditionalCompilationBranch)),
        "{diags:?}"
    );
}

/// Positive control for the completeness signal: the fully attributable
/// fixture is complete in both parts and raises no package diagnostics.
#[test]
fn fully_attributable_package_is_complete() {
    let (decls, diags) = lower(PACKAGE);
    assert!(package_units(&decls, PackagePart::Spec).is_complete());
    assert!(package_units(&decls, PackagePart::Body).is_complete());
    assert!(
        !diags.iter().any(|d| d.code.starts_with("IR_PACKAGE_")),
        "{diags:?}"
    );
}
