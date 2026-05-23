//! Parse-corpus test harness + corpus dashboard.
//!
//! Walks the vendored corpora under `corpus/public/` and exercises the
//! `plsql_parser_antlr::lower::lower_source` pre-parser against every
//! `.sql` / `.pls` / `.plb` / `.pks` / `.pkb` fixture. The harness:
//!
//! 1. Counts files processed.
//! 2. Counts declarations recognized by kind (`Package*`, `Procedure`,
//!    `Function`, `Trigger`, `View`, `Type*`, `Ddl`, `Unknown`).
//! 3. Computes a *parse-success rate*: the share of files that produced
//!    **at least one** typed declaration (i.e. the pre-parser found at
//!    least one CREATE/ALTER/DROP/GRANT/REVOKE/COMMENT). A file that
//!    contained only standalone DML or a PL/SQL block with no DDL header
//!    is counted as *recognized-empty* — neither success nor parse
//!    failure — and surfaced separately so the metric stays honest.
//! 4. Renders a markdown dashboard (`render_dashboard_markdown`) with
//!    parse-quality metrics: clean-rate, recovered-rate, skipped-token
//!    ratio, top-level recognition rate, per-kind histogram. Visible
//!    via `cargo test corpus_dashboard -- --nocapture` and consumable
//!    by the PARSE-019 CI gate.
//! 5. Asserts soft floors so a future drop in coverage trips CI rather
//!    than rotting silently.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use plsql_core::FileId;
use plsql_parser::ast::AstDecl;
use plsql_parser_antlr::lower::lower_source;

/// Aggregated metrics emitted by one corpus pass.
#[derive(Debug, Default)]
struct CorpusReport {
    files_total: usize,
    files_with_decls: usize,
    files_empty_recognized: usize,
    decls_total: usize,
    decls_by_kind: BTreeMap<&'static str, usize>,
    files_with_unknown: usize,
}

impl CorpusReport {
    fn record(&mut self, decls: &[AstDecl]) {
        self.files_total += 1;
        if decls.is_empty() {
            self.files_empty_recognized += 1;
            return;
        }
        self.files_with_decls += 1;
        let mut saw_unknown = false;
        for decl in decls {
            self.decls_total += 1;
            let kind = decl_kind(decl);
            *self.decls_by_kind.entry(kind).or_insert(0) += 1;
            if matches!(decl, AstDecl::Unknown { .. }) {
                saw_unknown = true;
            }
        }
        if saw_unknown {
            self.files_with_unknown += 1;
        }
    }

    fn success_rate(&self) -> f32 {
        if self.files_total == 0 {
            return 0.0;
        }
        (self.files_with_decls as f32) / (self.files_total as f32)
    }

    /// Files that produced AT LEAST one typed declaration AND no
    /// `Unknown` entries — the strict "clean" rate.
    fn clean_rate(&self) -> f32 {
        if self.files_total == 0 {
            return 0.0;
        }
        let clean = self
            .files_with_decls
            .saturating_sub(self.files_with_unknown);
        clean as f32 / self.files_total as f32
    }

    /// Share of files where the pre-parser emitted at least one
    /// `Unknown` — proxy for "recovered" until the real backend lands.
    fn recovered_rate(&self) -> f32 {
        if self.files_total == 0 {
            return 0.0;
        }
        self.files_with_unknown as f32 / self.files_total as f32
    }

    /// Render a markdown dashboard for the PARSE-019 CI gate. The shape
    /// is intentionally stable so downstream tooling (parse-quality
    /// trends, CI annotations) can diff successive runs.
    #[allow(dead_code)] // exercised via `corpus_dashboard_renders_markdown`
    fn render_dashboard_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Parse-corpus quality dashboard\n\n");
        out.push_str("Source: `corpus/public/`. Backend: text-scanning pre-parser ");
        out.push_str("(`plsql_parser_antlr::lower::lower_source`).\n\n");

        out.push_str("## Headline metrics\n\n");
        out.push_str("| Metric | Value |\n");
        out.push_str("|--------|------:|\n");
        out.push_str(&format!("| Files scanned | {} |\n", self.files_total));
        out.push_str(&format!(
            "| Files with >=1 typed decl (top-level recognition) | {} ({:.1}%) |\n",
            self.files_with_decls,
            self.success_rate() * 100.0
        ));
        out.push_str(&format!(
            "| Files recognized-empty (no DDL header) | {} |\n",
            self.files_empty_recognized
        ));
        out.push_str(&format!(
            "| Files with at least one `AstDecl::Unknown` (recovered proxy) | {} ({:.1}%) |\n",
            self.files_with_unknown,
            self.recovered_rate() * 100.0
        ));
        out.push_str(&format!(
            "| Clean rate (>=1 decl AND no Unknown) | {:.1}% |\n",
            self.clean_rate() * 100.0
        ));
        out.push_str(&format!(
            "| Total declarations | {} |\n\n",
            self.decls_total
        ));

        out.push_str("## Declarations by kind\n\n");
        out.push_str("| Kind | Count |\n");
        out.push_str("|------|------:|\n");
        for (kind, count) in &self.decls_by_kind {
            out.push_str(&format!("| {kind} | {count} |\n"));
        }
        out.push('\n');

        out.push_str("## Notes on backend gaps\n\n");
        out.push_str(
            "- `skipped-token ratio` is reported as **not yet measurable** at this layer ",
        );
        out.push_str("— the text-scanning pre-parser does not produce a token tape. Real values ");
        out.push_str("appear once the ANTLR backend wires through `ParseBackend`.\n");
        out.push_str(
            "- `recovered rate` here is a proxy: it measures `AstDecl::Unknown` emission, ",
        );
        out.push_str("not parse-error-recovery sites. Both are 0 in the current snapshot.\n");
        out.push_str("- The success-rate metric counts a file as recognized iff at least one ");
        out.push_str(
            "`AstDecl` is produced — PL/SQL blocks without `CREATE`/`ALTER`/`DROP`/`GRANT`/",
        );
        out.push_str("`REVOKE`/`COMMENT` headers are honestly classified as `recognized-empty`.\n");
        out
    }
}

fn decl_kind(decl: &AstDecl) -> &'static str {
    match decl {
        AstDecl::PackageSpec { .. } => "package-spec",
        AstDecl::PackageBody { .. } => "package-body",
        AstDecl::Procedure { .. } => "procedure",
        AstDecl::Function { .. } => "function",
        AstDecl::Trigger { .. } => "trigger",
        AstDecl::View { .. } => "view",
        AstDecl::TypeSpec { .. } => "type-spec",
        AstDecl::TypeBody { .. } => "type-body",
        AstDecl::Ddl { .. } => "ddl",
        AstDecl::Unknown { .. } => "unknown",
    }
}

/// Locate the workspace-relative `corpus/public/` directory.
///
/// Walks up from the crate's `CARGO_MANIFEST_DIR` until a directory
/// named `corpus/public/` is found. Returns `None` if the checkout
/// doesn't ship the corpora (e.g. a stripped tarball release).
fn corpus_root() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cursor = manifest;
    loop {
        let candidate = cursor.join("corpus").join("public");
        if candidate.is_dir() {
            return Some(candidate);
        }
        cursor = cursor.parent()?;
    }
}

/// Recursively gather every parseable fixture under `root`.
fn gather_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if matches!(
                ext.to_ascii_lowercase().as_str(),
                "sql" | "pls" | "plb" | "pks" | "pkb"
            ) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn run_pass() -> Option<CorpusReport> {
    let root = corpus_root()?;
    let fixtures = gather_fixtures(&root);
    if fixtures.is_empty() {
        return Some(CorpusReport::default());
    }
    let mut report = CorpusReport::default();
    for (idx, path) in fixtures.iter().enumerate() {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let file_id = FileId::new(idx as u32);
        let ast = lower_source(&text, file_id);
        report.record(&ast.root.declarations);
    }
    Some(report)
}

#[test]
fn corpus_pass_emits_report() {
    let Some(report) = run_pass() else {
        // No corpus shipped → harness is a no-op, not a failure.
        return;
    };
    // The repo ships 18+ fixtures across antlr-grammars-v4 and the
    // oracle-samples HR/OE/SH schemas. Anything less means a regression
    // in `corpus/public/` ingestion.
    assert!(
        report.files_total >= 18,
        "expected >=18 fixtures, saw {}",
        report.files_total
    );
}

#[test]
fn corpus_success_rate_meets_floor() {
    let Some(report) = run_pass() else {
        return;
    };
    // The pre-parser is text-scanning so its recognition is best-effort
    // on Oracle sample dumps. A 60% floor catches accidental wholesale
    // regression without being so tight that adding a tricky fixture
    // immediately breaks CI. Once PARSE-005/006/007 land, the floor
    // tightens in PARSE-019 (corpus dashboard).
    let rate = report.success_rate();
    assert!(
        rate >= 0.60,
        "parse success rate {rate:.2} below 0.60 floor — {}/{} files had >=1 decl",
        report.files_with_decls,
        report.files_total
    );
}

#[test]
fn corpus_recognizes_each_top_level_kind_at_least_once() {
    let Some(report) = run_pass() else {
        return;
    };
    // The HR/OE/SH oracle-samples plus antlr-grammars-v4 fixtures
    // between them contain at least one TABLE/INDEX (=> Ddl), VIEW,
    // TRIGGER, and one of PROCEDURE/FUNCTION. Packages/types are NOT
    // gated here because the shipped oracle-samples bundle doesn't
    // currently include `CREATE PACKAGE` or `CREATE TYPE` headers —
    // they remain surfaced in the report (`decls_by_kind`) so the
    // PARSE-019 dashboard can track them, but not asserted.
    for required in &["ddl", "view", "trigger"] {
        let count = report.decls_by_kind.get(*required).copied().unwrap_or(0);
        assert!(
            count > 0,
            "no {required} declarations recognized in the corpus — \
             pre-parser may have regressed (full kind map: {:?})",
            report.decls_by_kind
        );
    }
}

#[test]
fn corpus_dashboard_renders_markdown() {
    // PLSQL-PARSE-019: emits a stable markdown dashboard. Visible via
    // `cargo test corpus_dashboard -- --nocapture` and consumable by
    // the CI parse-quality gate downstream of PARSE-012.
    let Some(report) = run_pass() else {
        return;
    };
    let dashboard = report.render_dashboard_markdown();
    assert!(
        dashboard.starts_with("# Parse-corpus quality dashboard"),
        "dashboard header missing"
    );
    assert!(
        dashboard.contains("Files scanned"),
        "headline metrics section missing"
    );
    assert!(
        dashboard.contains("Declarations by kind"),
        "kind histogram missing"
    );
    assert!(
        dashboard.contains("Notes on backend gaps"),
        "honesty footer missing"
    );
    // Echo the dashboard so `cargo test -- --nocapture` lands the
    // current snapshot in the test output.
    println!("\n{dashboard}");
}

#[test]
fn corpus_dashboard_clean_and_recovered_rates_are_well_defined() {
    let Some(report) = run_pass() else {
        return;
    };
    // PLSQL-PARSE-019: both rates are in [0,1] and the clean rate
    // never exceeds the success rate (clean = success - with_unknown).
    let clean = report.clean_rate();
    let recovered = report.recovered_rate();
    let success = report.success_rate();
    assert!(
        (0.0..=1.0).contains(&clean),
        "clean rate out of range: {clean}"
    );
    assert!(
        (0.0..=1.0).contains(&recovered),
        "recovered rate out of range: {recovered}"
    );
    assert!(
        clean <= success + f32::EPSILON,
        "clean rate {clean} must not exceed success rate {success}"
    );
}

#[test]
fn corpus_no_unknown_decls() {
    let Some(report) = run_pass() else {
        return;
    };
    // The current pre-parser never emits `AstDecl::Unknown` directly —
    // anything it can't classify becomes `Ddl { kind: "<verb>" }`. If
    // a future change starts emitting Unknown for sample-corpus inputs
    // we want to know immediately so it lands a typed UnknownReason
    // upstream rather than silently dropping the row.
    assert_eq!(
        report.files_with_unknown, 0,
        "AstDecl::Unknown emitted for {} corpus files; expected 0",
        report.files_with_unknown
    );
}
