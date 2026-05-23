//! ** §2[C]/§2.1 signature-conformance gate
//! (mandatory, anti-gaming).**
//!
//! Spec §2[C]: `signature = (diag_code, antlr_rule_path,
//! token-shape hash)` — *no span width*. Spec §2.1: `signature`
//! is the "content hash of code+rule+token-shape"; `span_shape` is
//! the "token-kind sequence, never text".
//!
//! P1 folded a span-WIDTH bucket (`W512`, …) into the signature (a
//! documented stopgap because P1 had no lexer in reach), which made
//! the `text_scan>*` fallback classes unminimisable *by
//! construction* (ddmin narrows the block → width bucket flips →
//! signature changes → the unchanged `SignatureOracle` rejects the
//! minimised form). This suite proves the corrected signature is:
//!
//! 1. **Fine-grained** — genuinely different gap classes get
//!    DIFFERENT signatures (≥4 distinct: CREATE TABLE vs DROP vs
//!    ALTER TRIGGER vs two different rule paths vs two different
//!    token-kind shapes). A degenerate "everything one signature"
//!    implementation FAILS this.
//! 2. **Minimisation-stable** — the signature is invariant under a
//!    ddmin-style change to the surrounding span geometry (the whole
//!    point of the fix): same `(code, rule)` ⇒ same signature
//!    regardless of span width / line count / offset.
//! 3. **Deterministic** — same diagnostic + commit ⇒ byte-identical
//!    signature (I-DETERMINISM).
//! 4. **I-PRIVACY** — the `span_shape` (token-kind sequence) is
//!    grammar-constant kind names only; it never carries estate
//!    identifiers/literals or a span width/offset.

use plsql_accretion::{GapRecord, capture_gaps_with_commit};
use plsql_core::{Diagnostic, Evidence, Position, Severity, Span, UnknownReason};
use plsql_engine::AnalysisRun;
use serde_json::Value;

/// FileId(0) span. Offsets are NOT source — `Span` carries no text.
fn span(start_off: u32, end_off: u32, start_line: u32, end_line: u32) -> Span {
    Span::new(
        plsql_core::FileId::new(0),
        Position::new(start_line, 1, start_off),
        Position::new(end_line, 1, end_off),
    )
}

/// A repairable diagnostic stamped with an `antlr_rule_path`
/// evidence attribute, exactly the contract `plsql-ir`'s
/// `stamp_antlr_rule_path` writes and `gap::antlr_rule_path_of`
/// reads.
fn diag_with_rule(code: &str, rule_path: &str, sp: Span) -> Diagnostic {
    Diagnostic::new(code, Severity::Info, "gap")
        .with_primary_span(sp)
        .with_evidence(
            Evidence::new("ANTLR_RULE_PATH", "rule pos")
                .with_attribute("antlr_rule_path", Value::String(rule_path.to_string())),
        )
}

fn rec(code: &str, rule_path: &str, sp: Span) -> GapRecord {
    let run = AnalysisRun {
        parser_backend: "antlr4rust".to_string(),
        diagnostics: vec![diag_with_rule(code, rule_path, sp)],
        ..AnalysisRun::default()
    };
    capture_gaps_with_commit(&run, "deadbeef")
        .into_iter()
        .next()
        .expect("repairable diagnostic must produce a gap record")
}

/// Acceptance #1: genuinely different gap classes get DIFFERENT
/// signatures. A degenerate single-signature implementation FAILS.
#[test]
fn distinct_gap_classes_get_distinct_signatures() {
    // Differ by rule-path leaf construct.
    let create_table = rec(
        "IR_DDL_NOT_LOWERED",
        "unit_statement>create_table",
        span(0, 50, 1, 3),
    );
    let drop_ = rec("IR_DDL_NOT_LOWERED", "text_scan>drop", span(0, 12, 1, 1));
    let alter_trigger = rec(
        "IR_DDL_NOT_LOWERED",
        "text_scan>alter_trigger",
        span(0, 90, 1, 5),
    );
    // Differ by rule path only (create vs comment), same code.
    let comment = rec("IR_DDL_NOT_LOWERED", "text_scan>comment", span(0, 30, 1, 2));
    let create = rec("IR_DDL_NOT_LOWERED", "text_scan>create", span(0, 30, 1, 2));
    // Differ by diag_code only, same rule path.
    let create_table_unclass = rec(
        "IR_UNCLASSIFIED_DECL",
        "unit_statement>create_table",
        span(0, 50, 1, 3),
    );

    let sigs = [
        &create_table.signature,
        &drop_.signature,
        &alter_trigger.signature,
        &comment.signature,
        &create.signature,
        &create_table_unclass.signature,
    ];
    let distinct: std::collections::BTreeSet<&String> = sigs.iter().copied().collect();
    assert!(
        distinct.len() >= 4,
        "spec §2[C] requires fine-grained signatures: CREATE TABLE / DROP / \
         ALTER TRIGGER / COMMENT / CREATE / (code-differing) must yield \
         ≥4 distinct signatures, got {} distinct of {}: {sigs:?}",
        distinct.len(),
        sigs.len()
    );
    assert_eq!(
        distinct.len(),
        sigs.len(),
        "every genuinely-different class here is distinct (no collapse): {sigs:?}"
    );

    // The token-KIND shape axis is genuinely load-bearing where the
    // construct skeletons have different lexical shapes: `CREATE
    // TABLE` (KW KW) vs `DROP` (KW) differ in the shape itself, not
    // only in the rule-path component. (Constructs whose skeletons
    // lex to the *same* KIND sequence — e.g. `CREATE` vs `COMMENT`,
    // both a single `KW` — still get distinct signatures because the
    // spec §2[C] signature also folds `antlr_rule_path`; that the
    // full signatures above are all distinct already proves this.)
    assert_ne!(
        create_table.span_shape, drop_.span_shape,
        "CREATE TABLE (KW KW) and DROP (KW) must have different token-kind shapes"
    );
}

/// Acceptance #3 (the fix's purpose): the signature is INVARIANT
/// under a ddmin-style change to the surrounding span geometry.
/// Pre-fix this FAILED (the width bucket flipped W512→W128 →
/// signature changed → the gap was unminimisable). Post-fix the
/// signature depends only on `(code, rule, token-kind-shape)`.
#[test]
fn signature_is_invariant_under_span_geometry_change() {
    // Same construct/code; only the span width + line count differ —
    // exactly what ddmin does when it narrows a block.
    let wide = rec("IR_DDL_NOT_LOWERED", "text_scan>drop", span(0, 4000, 1, 80));
    let narrow = rec("IR_DDL_NOT_LOWERED", "text_scan>drop", span(0, 12, 1, 1));
    let tiny = rec("IR_DDL_NOT_LOWERED", "text_scan>drop", span(5, 6, 1, 1));
    assert_eq!(
        wide.signature, narrow.signature,
        "ddmin narrowing a block MUST NOT change the signature \
         (span-width stopgap removed): wide={} narrow={}",
        wide.signature, narrow.signature
    );
    assert_eq!(
        narrow.signature, tiny.signature,
        "signature must be a gap-class id, not a block-size fingerprint"
    );
    // And it is NOT degenerate: a different construct still differs.
    let other = rec("IR_DDL_NOT_LOWERED", "text_scan>create", span(0, 12, 1, 1));
    assert_ne!(
        wide.signature, other.signature,
        "invariance under width must not collapse distinct constructs"
    );
}

/// Acceptance #5: I-DETERMINISM — same diagnostic + commit ⇒
/// byte-identical signature, every time.
#[test]
fn signature_is_deterministic() {
    let a = rec(
        "IR_DDL_NOT_LOWERED",
        "unit_statement>create_table",
        span(0, 50, 1, 3),
    );
    let b = rec(
        "IR_DDL_NOT_LOWERED",
        "unit_statement>create_table",
        span(7, 999, 4, 9),
    );
    assert_eq!(
        a.signature, b.signature,
        "deterministic + geometry-invariant: identical signature"
    );
    assert_eq!(a.signature.len(), 64, "sha256 hex");
    // Repeat the exact same capture — byte-identical.
    let c = rec(
        "IR_DDL_NOT_LOWERED",
        "unit_statement>create_table",
        span(0, 50, 1, 3),
    );
    assert_eq!(a.signature, c.signature);
    assert_eq!(a.span_shape, c.span_shape);
}

/// Acceptance #5: I-PRIVACY — the §2.1 `span_shape` (token-kind
/// sequence) is grammar-constant KIND names only. It never contains
/// an estate identifier/literal, and — the corrected behaviour —
/// never a span width/line/offset marker (`W*`, `L*`, `SPAN_*`).
#[test]
fn span_shape_is_token_kinds_only_no_width_no_estate() {
    // A rule path whose leaf carries an allowlisted object keyword;
    // there is no estate text anywhere in the input that could reach
    // the shape (the shape is derived from the rule path skeleton).
    let g = rec(
        "IR_DDL_NOT_LOWERED",
        "text_scan>create_table",
        span(0, 4000, 1, 80),
    );
    assert!(
        !g.span_shape.is_empty(),
        "shape present for a real rule path"
    );
    for marker in &g.span_shape {
        // Grammar-constant KIND token only: uppercase / digits / _.
        assert!(
            marker
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
            "span_shape marker {marker:?} is not a token-KIND constant"
        );
        // The P1 width/line/geometry stopgap markers MUST be gone.
        assert!(
            !marker.starts_with('W') || marker == "WHEN", /* never produced here */
            "span-WIDTH bucket {marker:?} must not appear in the signature shape"
        );
        assert_ne!(marker, "W0");
        assert_ne!(marker, "W512");
        assert_ne!(marker, "WBIG");
        assert!(
            !(marker.starts_with('L')
                && marker.len() <= 4
                && marker[1..].chars().all(|c| c.is_ascii_digit() || c == 'B')),
            "line-count bucket {marker:?} must not appear"
        );
        assert!(
            !marker.starts_with("SPAN_"),
            "span-geometry marker {marker:?} must not appear"
        );
    }
    // No span width/offset is even an input: two wildly different
    // geometries yield the identical shape.
    let g2 = rec(
        "IR_DDL_NOT_LOWERED",
        "text_scan>create_table",
        span(0, 4, 1, 1),
    );
    assert_eq!(
        g.span_shape, g2.span_shape,
        "shape must not vary with span geometry"
    );

    // A no-rule-path gap (the honest no-parse-tree case) gets a
    // fixed deterministic marker, never a width-derived one.
    let run = AnalysisRun {
        parser_backend: "antlr4rust".to_string(),
        diagnostics: vec![
            Diagnostic::new("PARSE-ANTLR4RUST-001", Severity::Error, "syntax")
                .with_primary_span(span(0, 9999, 1, 200)),
        ],
        ..AnalysisRun::default()
    };
    let nr = capture_gaps_with_commit(&run, "x")
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(
        nr.span_shape,
        vec!["RULE_ABSENT".to_string()],
        "no-rule-path shape is the fixed sentinel, not a width bucket"
    );
}

/// Acceptance #1 (token-kind-shape axis is a real lexer product):
/// the `span_shape` is the real ANTLR lexer's `TokenKind` sequence
/// over the rule-path skeleton — a genuine multi-token sequence, and
/// it varies with the construct's *lexical shape*. Constructs with
/// different token counts (`DROP` = 1 KW vs `CREATE TABLE` = 2 KW)
/// have different shapes; constructs that happen to lex to the same
/// KIND sequence still get distinct signatures via the §2[C]
/// `antlr_rule_path` component. Both facts are asserted here so a
/// degenerate (constant-shape) implementation FAILS.
#[test]
fn token_kind_shape_axis_is_load_bearing() {
    let create_table = rec(
        "IR_DDL_NOT_LOWERED",
        "text_scan>create_table",
        span(0, 1, 1, 1),
    );
    let create_index = rec(
        "IR_DDL_NOT_LOWERED",
        "text_scan>create_index",
        span(0, 1, 1, 1),
    );
    let create_user = rec(
        "IR_DDL_NOT_LOWERED",
        "text_scan>create_user",
        span(0, 1, 1, 1),
    );
    let drop_ = rec("IR_DDL_NOT_LOWERED", "text_scan>drop", span(0, 1, 1, 1));

    // Signatures are distinct across all four (spec §2[C] folds the
    // rule path) — the loop can tell these gap classes apart.
    let mut sigs = std::collections::BTreeSet::new();
    for g in [&create_table, &create_index, &create_user, &drop_] {
        sigs.insert(g.signature.clone());
    }
    assert_eq!(sigs.len(), 4, "all four constructs get distinct signatures");

    // The shape is a REAL lexer product, not a constant: a 2-keyword
    // skeleton lexes to a 2-element KIND sequence, a 1-keyword
    // skeleton to a 1-element one.
    assert_eq!(
        create_table.span_shape.len(),
        2,
        "{:?}",
        create_table.span_shape
    );
    assert_eq!(drop_.span_shape.len(), 1, "{:?}", drop_.span_shape);
    assert_ne!(
        create_table.span_shape, drop_.span_shape,
        "different lexical shapes ⇒ different token-kind sequences"
    );
    // Every element is a grammar-constant KIND token.
    for k in &create_table.span_shape {
        assert!(k.chars().all(|c| c.is_ascii_uppercase()), "{k:?}");
    }

    // The diag_code axis is independently live: same rule, different
    // code ⇒ different signature.
    let typed_unknown = rec(
        "IR_UNCLASSIFIED_DECL",
        "text_scan>create_table",
        span(0, 1, 1, 1),
    );
    let _ = UnknownReason::ParserRecoveryRegion; // (kept: spec d-class lane)
    assert_ne!(
        create_table.signature, typed_unknown.signature,
        "differing diag_code with identical rule must still differ (code axis live)"
    );
}
