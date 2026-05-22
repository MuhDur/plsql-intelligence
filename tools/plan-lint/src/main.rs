#![forbid(unsafe_code)]

//! `plan-lint` — structural integrity checker for `plan.md`.
//!
//! Implements seven checks required by `PLSQL-PLAN-001`:
//!
//! 1. heading-number monotonicity (`##` and `###` numbering, with explicit
//!    inserts like `10A` allowed)
//! 2. ToC anchor validity (every `## Table of Contents` link resolves to a
//!    real heading slug)
//! 3. duplicate bead IDs (each `PLSQL-…-NNN` appears in at most one bead-seed
//!    table row)
//! 4. missing bead dependencies (every bead referenced in a `Depends` column
//!    is itself defined)
//! 5. stale section references (`§N` / `§N.M` references resolve to existing
//!    sections)
//! 6. component coverage matrix (every component named in §5 has a bead seed
//!    and at least one acceptance-criteria mention)
//! 7. banned release-wedge language scanner (Phase 1/2/3, MVP, alpha/beta
//!    release, "first wave", `Qn YYYY`, etc.), with a whitelist for the
//!    quoted historical changelog blocks under §28
//!
//! Defaults to scanning `plan.md` in the current directory. Pass
//! `--plan <path>` to override. `--robot-json` emits a stable-schema JSON
//! report to stdout. `--doctor` prints a summary of all rule outcomes.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::process::ExitCode;
use std::{env, fs};

use regex::Regex;
use serde::Serialize;

const SCHEMA_ID: &str = "plan-lint.report";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Severity {
    Error,
    Warn,
}

#[derive(Debug, Clone, Serialize)]
struct Finding {
    rule: &'static str,
    severity: Severity,
    line: usize,
    message: String,
}

#[derive(Debug, Serialize)]
struct Report {
    schema_id: &'static str,
    schema_version: u32,
    plan_path: String,
    findings: Vec<Finding>,
    rule_results: BTreeMap<&'static str, RuleResult>,
}

#[derive(Debug, Serialize)]
struct RuleResult {
    findings: usize,
    errors: usize,
}

#[derive(Debug)]
struct Heading {
    line: usize,
    level: u8,
    number: Option<String>, // e.g. "10A", "10A.1", "1.2"
    title: String,
    slug: String,
}

#[derive(Debug)]
struct BeadRow {
    line: usize,
    id: String,
    depends: Vec<String>,
}

#[derive(Debug)]
struct PlanDoc {
    text: String,
    lines: Vec<String>,
    headings: Vec<Heading>,
    toc_entries: Vec<(usize, String, String)>, // (line, title, anchor)
    beads: Vec<BeadRow>,
    section_refs: Vec<(usize, String)>,
    components: Vec<String>,
}

fn main() -> ExitCode {
    let args = parse_args(env::args().skip(1).collect());
    let args = match args {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("error: {msg}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    let text = match fs::read_to_string(&args.plan) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", args.plan.display());
            return ExitCode::from(2);
        }
    };

    let doc = match parse_plan(&text) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("error: failed to parse {}: {err}", args.plan.display());
            return ExitCode::from(2);
        }
    };

    let mut findings = Vec::new();
    findings.extend(check_heading_monotonicity(&doc));
    findings.extend(check_toc_anchors(&doc));
    findings.extend(check_duplicate_beads(&doc));
    findings.extend(check_missing_bead_deps(&doc));
    findings.extend(check_stale_section_refs(&doc));
    findings.extend(check_component_coverage(&doc));
    findings.extend(check_banned_language(&doc));

    let rule_results = summarize(&findings);
    let report = Report {
        schema_id: SCHEMA_ID,
        schema_version: SCHEMA_VERSION,
        plan_path: args.plan.display().to_string(),
        findings: findings.clone(),
        rule_results,
    };

    if args.robot_json {
        let json = serde_json::to_string_pretty(&report).expect("Report serializes cleanly");
        println!("{json}");
    } else {
        print_human(&report, args.doctor);
    }

    let has_error = report
        .findings
        .iter()
        .any(|f| matches!(f.severity, Severity::Error));
    if has_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[derive(Debug)]
struct Args {
    plan: PathBuf,
    robot_json: bool,
    doctor: bool,
}

fn parse_args(mut args: Vec<String>) -> Result<Args, String> {
    let mut plan = PathBuf::from("plan.md");
    let mut robot_json = false;
    let mut doctor = false;

    let mut iter = args.drain(..);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--plan" => {
                plan = iter
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| "--plan requires a path".to_string())?;
            }
            "--robot-json" => robot_json = true,
            "--doctor" => doctor = true,
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }

    Ok(Args {
        plan,
        robot_json,
        doctor,
    })
}

fn print_usage() {
    eprintln!("usage: plan-lint [--plan <path>] [--robot-json] [--doctor]");
    eprintln!();
    eprintln!("Validates structural integrity of plan.md (PLSQL-PLAN-001).");
    eprintln!();
    eprintln!("Flags:");
    eprintln!("  --plan <path>   Path to plan.md (default: ./plan.md)");
    eprintln!("  --robot-json    Emit a stable-schema JSON report to stdout");
    eprintln!("                  (schema_id={SCHEMA_ID}, schema_version={SCHEMA_VERSION})");
    eprintln!("  --doctor        Print a per-rule summary alongside individual findings");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  plan-lint                       # check ./plan.md, human output");
    eprintln!("  plan-lint --doctor              # add a rule-by-rule summary table");
    eprintln!("  plan-lint --plan docs/plan.md   # check a different file");
    eprintln!("  plan-lint --robot-json | jq .   # consume the JSON report");
    eprintln!();
    eprintln!("Exit codes:");
    eprintln!("  0  clean (no errors; warnings still printed)");
    eprintln!("  1  one or more error-severity findings");
    eprintln!("  2  invocation error (bad args, unreadable plan, etc.)");
}

// ----------------------------------------------------------------------------
// Parsing
// ----------------------------------------------------------------------------

fn parse_plan(text: &str) -> Result<PlanDoc, String> {
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let headings = collect_headings(&lines);
    let toc_entries = collect_toc_entries(&lines, &headings);
    let beads = collect_bead_rows(&lines);
    let section_refs = collect_section_refs(&lines);
    let components = collect_components(&lines, &headings);

    Ok(PlanDoc {
        text: text.to_owned(),
        lines,
        headings,
        toc_entries,
        beads,
        section_refs,
        components,
    })
}

fn collect_headings(lines: &[String]) -> Vec<Heading> {
    // `## 5. Dependency graph` or `### 7.5 Bench + quality targets` or `## 10A.1 Sub`.
    let re_numbered = Regex::new(r"^(#{1,6})\s+([0-9]+[A-Z]?(?:\.[0-9]+[A-Z]?)*)\.?\s+(.+?)\s*$")
        .expect("static regex compiles");
    let re_plain = Regex::new(r"^(#{1,6})\s+(.+?)\s*$").expect("static regex compiles");

    let mut out = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if let Some(caps) = re_numbered.captures(line) {
            let level = caps[1].len() as u8;
            let number = caps[2].to_owned();
            let title = caps[3].trim().to_owned();
            out.push(Heading {
                line: idx + 1,
                level,
                number: Some(number),
                slug: slugify(&title, Some(&caps[2])),
                title,
            });
        } else if let Some(caps) = re_plain.captures(line) {
            let level = caps[1].len() as u8;
            let title = caps[2].trim().to_owned();
            out.push(Heading {
                line: idx + 1,
                level,
                number: None,
                slug: slugify(&title, None),
                title,
            });
        }
    }

    // Filter out headings inside fenced code blocks; the regex doesn't know
    // about ``` fences.
    out.retain(|h| !recompute_fence_state(lines, h.line - 1));
    out
}

fn recompute_fence_state(lines: &[String], up_to_inclusive: usize) -> bool {
    let mut in_fence = false;
    for line in lines.iter().take(up_to_inclusive + 1) {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
    }
    // The heading at `up_to_inclusive` is "inside fence" iff a fence opened
    // strictly before it (the closing ``` would have toggled back already).
    // Our toggle above counts including the heading line; correct by toggling
    // once if the heading line itself starts a fence (which can't be a heading).
    if lines[up_to_inclusive].trim_start().starts_with("```") {
        in_fence = !in_fence;
    }
    in_fence
}

fn slugify(title: &str, number_prefix: Option<&str>) -> String {
    // GitHub-flavored slug: lowercase, strip everything that is not letter /
    // number / whitespace / hyphen, then replace each remaining whitespace
    // run-of-one with a single hyphen WITHOUT collapsing consecutive hyphens
    // (so `Layer 1 — Parser` → `layer-1--parser`, preserving the empty
    // segment where the em-dash used to live).
    let combined: String = match number_prefix {
        Some(num) => format!("{num}. {title}"),
        None => title.to_owned(),
    };
    let lower = combined.to_lowercase();
    let mut s = String::with_capacity(lower.len());
    for ch in lower.chars() {
        if ch.is_alphanumeric() || ch == '-' {
            s.push(ch);
        } else if ch == ' ' || ch == '\t' {
            s.push('-');
        }
        // Drop every other character (punctuation, em-dashes, etc.) without
        // emitting a hyphen — but the surrounding whitespace is preserved
        // as hyphens, which produces the doubled `--` seen in GitHub slugs.
    }
    s.trim_matches('-').to_owned()
}

fn collect_toc_entries(lines: &[String], _headings: &[Heading]) -> Vec<(usize, String, String)> {
    // Find the `## Table of Contents` heading and scan list items
    // beneath it until the next heading. Once we have entered (and
    // then exited) the ToC section, latch `past_toc` so a later H2
    // whose title happens to contain the words "Table of Contents"
    // does not re-open ToC collection.
    let mut out = Vec::new();
    let mut in_toc = false;
    let mut past_toc = false;
    let re_entry = Regex::new(r"\[([^\]]+)\]\(#([^\)]+)\)").expect("static regex compiles");
    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with("## ") {
            if past_toc {
                in_toc = false;
                continue;
            }
            if line.contains("Table of Contents") {
                in_toc = true;
            } else {
                // Leaving the ToC section. Latch so re-entry cannot
                // happen even if a later H2 mentions the same words.
                if in_toc {
                    past_toc = true;
                }
                in_toc = false;
            }
            continue;
        }
        if !in_toc {
            continue;
        }
        for caps in re_entry.captures_iter(line) {
            out.push((idx + 1, caps[1].to_owned(), caps[2].to_owned()));
        }
    }
    out
}

fn collect_bead_rows(lines: &[String]) -> Vec<BeadRow> {
    // Bead-seed table rows look like:
    //   | `PLSQL-…-NNN` | Title | Deps | Effort |
    //
    // We tolerate any number of columns, but require:
    //   * the first non-empty cell to be a backticked PLSQL- identifier
    //   * a "Depends" column somewhere — we find it by inspecting the table
    //     header row (cell containing "Depend") and remembering its index.
    let re_pipe_split = Regex::new(r"\s*\|\s*").expect("static regex compiles");
    // Bead IDs can have multi-segment families (`PLSQL-CORE-IDS-001`,
    // `PLSQL-STORE-DAEMON-002`, etc.). We allow `[A-Z][A-Z0-9-]*` for the
    // family portion; greedy backtracking still snaps the trailing `-NNN[A]`.
    let re_id = Regex::new(r"`(PLSQL-[A-Z][A-Z0-9-]*-\d+[A-Z]?)`").expect("static regex compiles");
    let re_bead_in_cell =
        Regex::new(r"PLSQL-[A-Z][A-Z0-9-]*-\d+[A-Z]?|(?:[A-Z]+(?:-[A-Z]+)*)-\d+[A-Z]?")
            .expect("static regex compiles");

    let mut out = Vec::new();
    let mut in_fence = false;
    // Bead-seed tables always use the same column layout
    // `| ID | Title | Depends | Effort |`, so column index 2 is a safe
    // default. We still memoize an explicit `Depends` column if we see a
    // header, in case the layout drifts.
    const DEFAULT_DEPENDS_COL: usize = 2;
    let mut depends_col: usize = DEFAULT_DEPENDS_COL;

    for (idx, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            depends_col = DEFAULT_DEPENDS_COL;
            continue;
        }
        if in_fence {
            continue;
        }
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            // Heading or blank line resets the layout memoization.
            if trimmed.starts_with('#') || trimmed.is_empty() {
                depends_col = DEFAULT_DEPENDS_COL;
            }
            continue;
        }

        // Skip the delimiter row (`|----|----|`).
        if trimmed.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            continue;
        }

        let cells: Vec<String> = re_pipe_split
            .split(trimmed.trim_matches('|'))
            .map(|c| c.trim().to_owned())
            .collect();

        // Header row? Memoize the Depends column if found.
        let header_has_bead = cells
            .iter()
            .any(|c| c.eq_ignore_ascii_case("Bead") || c.eq_ignore_ascii_case("ID"));
        if header_has_bead {
            depends_col = cells
                .iter()
                .position(|c| {
                    let lc = c.to_lowercase();
                    lc.starts_with("depend")
                })
                .unwrap_or(DEFAULT_DEPENDS_COL);
            continue;
        }

        // Bead data row: first cell must contain a backticked PLSQL ID.
        let id_cell = cells.first().map(|s| s.as_str()).unwrap_or("");
        let Some(id_caps) = re_id.captures(id_cell) else {
            continue;
        };
        let id = id_caps[1].to_owned();
        let depends: Vec<String> = cells
            .get(depends_col)
            .map(|cell| {
                re_bead_in_cell
                    .find_iter(cell)
                    .map(|m| normalize_bead_ref(m.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        out.push(BeadRow {
            line: idx + 1,
            id,
            depends,
        });
    }
    out
}

fn normalize_bead_ref(s: &str) -> String {
    if s.starts_with("PLSQL-") {
        s.to_owned()
    } else {
        format!("PLSQL-{s}")
    }
}

fn collect_section_refs(lines: &[String]) -> Vec<(usize, String)> {
    let re = Regex::new(r"§([0-9]+[A-Z]?(?:\.[0-9]+[A-Z]?)*)").expect("static regex compiles");
    let mut out = Vec::new();
    let mut in_fence = false;
    for (idx, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for caps in re.captures_iter(line) {
            out.push((idx + 1, caps[1].to_owned()));
        }
    }
    out
}

fn collect_components(lines: &[String], headings: &[Heading]) -> Vec<String> {
    // Find the first `## 5. Dependency graph` heading and read until the next
    // `##`. Extract any `plsql-<name>` or `plan-lint`/`corpus` identifiers
    // mentioned in the section.
    let Some(start) = headings.iter().find(|h| {
        h.level == 2 && h.number.as_deref() == Some("5") && h.title.contains("Dependency graph")
    }) else {
        return Vec::new();
    };
    let end = headings
        .iter()
        .find(|h| h.level == 2 && h.line > start.line)
        .map(|h| h.line)
        .unwrap_or_else(|| lines.len() + 1);

    let re = Regex::new(r"\b(plsql-[a-z][a-z0-9-]*|plan-lint)\b").expect("static regex compiles");
    let mut seen = BTreeSet::new();
    for line in &lines[start.line..end - 1] {
        for caps in re.captures_iter(line) {
            seen.insert(caps[1].to_owned());
        }
    }
    seen.into_iter().collect()
}

// ----------------------------------------------------------------------------
// Checks
// ----------------------------------------------------------------------------

fn check_heading_monotonicity(doc: &PlanDoc) -> Vec<Finding> {
    let mut findings = Vec::new();

    let h2: Vec<&Heading> = doc
        .headings
        .iter()
        .filter(|h| h.level == 2 && h.number.is_some())
        .collect();
    let mut prev: Option<(u64, &str)> = None;
    for h in &h2 {
        let num = h.number.as_deref().expect("filtered for Some(number)");
        let sortable = sort_key(num);
        if let Some((prev_key, prev_num)) = prev
            && sortable < prev_key
        {
            findings.push(Finding {
                rule: "heading-monotonicity",
                severity: Severity::Error,
                line: h.line,
                message: format!(
                    "## {num} appears after ## {prev_num}; H2 numbers must be monotonic"
                ),
            });
        }
        prev = Some((sortable, num));
    }

    // Within each H2 section, the H3 minor numbers must monotonically increase.
    let mut iter = doc.headings.iter().peekable();
    while let Some(h) = iter.next() {
        if h.level != 2 || h.number.is_none() {
            continue;
        }
        let parent = h.number.as_deref().expect("filtered above");
        let mut last_minor: Option<(u64, String)> = None;
        while let Some(next) = iter.peek() {
            if next.level <= 2 {
                break;
            }
            if next.level == 3
                && let Some(num) = next.number.as_deref()
                && let Some(minor_str) = num.strip_prefix(&format!("{parent}."))
            {
                let minor_key = sort_key(minor_str);
                if let Some((prev_key, prev_minor)) = &last_minor
                    && minor_key < *prev_key
                {
                    findings.push(Finding {
                        rule: "heading-monotonicity",
                        severity: Severity::Error,
                        line: next.line,
                        message: format!(
                            "### {num} appears after ### {parent}.{prev_minor} under ## {parent}; H3 minor numbers must be monotonic"
                        ),
                    });
                }
                last_minor = Some((minor_key, minor_str.to_owned()));
            }
            iter.next();
        }
    }

    findings
}

fn sort_key(num: &str) -> u64 {
    // Convert "10A.1" → 10*1000 + 'A' suffix bias, … good enough for ordering.
    // We bucket numeric segments into 4-digit slots.
    let mut key: u64 = 0;
    for part in num.split('.') {
        let (digits, suffix) = part
            .find(|c: char| !c.is_ascii_digit())
            .map(|i| part.split_at(i))
            .unwrap_or((part, ""));
        let n: u64 = digits.parse().unwrap_or(0);
        let suffix_bias = suffix.bytes().next().map(|b| b as u64).unwrap_or(0);
        key = key
            .saturating_mul(10_000)
            .saturating_add(n * 100 + suffix_bias);
    }
    key
}

fn check_toc_anchors(doc: &PlanDoc) -> Vec<Finding> {
    let valid_slugs: HashSet<&str> = doc.headings.iter().map(|h| h.slug.as_str()).collect();
    let mut findings = Vec::new();
    for (line, _title, anchor) in &doc.toc_entries {
        if !valid_slugs.contains(anchor.as_str()) {
            findings.push(Finding {
                rule: "toc-anchor",
                severity: Severity::Error,
                line: *line,
                message: format!("ToC anchor `#{anchor}` does not resolve to any heading slug"),
            });
        }
    }
    findings
}

fn check_duplicate_beads(doc: &PlanDoc) -> Vec<Finding> {
    let mut first_seen: HashMap<&str, usize> = HashMap::new();
    let mut findings = Vec::new();
    for row in &doc.beads {
        if let Some(prev) = first_seen.get(row.id.as_str()) {
            findings.push(Finding {
                rule: "duplicate-bead-id",
                severity: Severity::Error,
                line: row.line,
                message: format!(
                    "bead `{}` first defined on line {prev} is re-defined here",
                    row.id
                ),
            });
        } else {
            first_seen.insert(row.id.as_str(), row.line);
        }
    }
    findings
}

fn check_missing_bead_deps(doc: &PlanDoc) -> Vec<Finding> {
    let known: HashSet<&str> = doc.beads.iter().map(|b| b.id.as_str()).collect();
    let mut findings = Vec::new();
    for row in &doc.beads {
        for dep in &row.depends {
            if dep == "none" || dep.eq_ignore_ascii_case("n/a") {
                continue;
            }
            if !known.contains(dep.as_str()) {
                findings.push(Finding {
                    rule: "missing-bead-dependency",
                    severity: Severity::Error,
                    line: row.line,
                    message: format!("bead `{}` depends on unknown bead `{dep}`", row.id),
                });
            }
        }
    }
    findings
}

fn check_stale_section_refs(doc: &PlanDoc) -> Vec<Finding> {
    let valid: HashSet<&str> = doc
        .headings
        .iter()
        .filter_map(|h| h.number.as_deref())
        .collect();
    let mut findings = Vec::new();
    for (line, num) in &doc.section_refs {
        if !valid.contains(num.as_str()) {
            // It is common to reference top-level sections without
            // sub-section paths existing; tolerate top-level-only refs.
            let top = num.split('.').next().expect("split always yields one item");
            if !valid.iter().any(|v| v.starts_with(top)) {
                findings.push(Finding {
                    rule: "stale-section-ref",
                    severity: Severity::Error,
                    line: *line,
                    message: format!("reference §{num} does not resolve to any section"),
                });
            } else {
                findings.push(Finding {
                    rule: "stale-section-ref",
                    severity: Severity::Warn,
                    line: *line,
                    message: format!("reference §{num} has no exact heading match"),
                });
            }
        }
    }
    findings
}

fn check_component_coverage(doc: &PlanDoc) -> Vec<Finding> {
    // Subset of components routed to a future plan; not required to have beads.
    let future_components: HashSet<&str> = ["plsql-subset"].into_iter().collect();
    // Component aliases that mean "the same thing" as a bead-table family.
    let component_to_bead_family: HashMap<&str, &str> = [
        ("plsql-core", "PLSQL-CORE"),
        ("plsql-output", "PLSQL-WS"),
        ("plsql-render", "PLSQL-WS"),
        ("plsql-store", "PLSQL-WS"),
        ("plsql-parser", "PLSQL-PARSE"),
        ("plsql-project", "PLSQL-WS"),
        ("plsql-catalog", "PLSQL-CAT"),
        ("plsql-ir", "PLSQL-IR"),
        ("plsql-symbols", "PLSQL-SYM"),
        ("plsql-privileges", "PLSQL-PRIV"),
        ("plsql-sqlsem", "PLSQL-SQLSEM"),
        ("plsql-flow", "PLSQL-FLOW"),
        ("plsql-facts", "PLSQL-FACT"),
        ("plsql-depgraph", "PLSQL-DEP"),
        ("plsql-engine", "PLSQL-ENG"),
        ("plsql-scan", "PLSQL-SAST"),
        ("plsql-doc", "PLSQL-DOC"),
        ("plsql-bindgen", "PLSQL-BG"),
        ("plsql-lineage", "PLSQL-LIN"),
        ("plsql-cicd", "PLSQL-CICD"),
        ("plan-lint", "PLSQL-PLAN"),
    ]
    .into_iter()
    .collect();

    let mut bead_families: HashSet<String> = HashSet::new();
    for row in &doc.beads {
        if let Some(stem) = row.id.rsplit_once('-').map(|(left, _)| left) {
            bead_families.insert(stem.to_owned());
        }
    }

    let mut findings = Vec::new();
    for component in &doc.components {
        if future_components.contains(component.as_str()) {
            continue;
        }
        let Some(family) = component_to_bead_family.get(component.as_str()) else {
            findings.push(Finding {
                rule: "component-coverage",
                severity: Severity::Warn,
                line: 0,
                message: format!(
                    "component `{component}` from §5 has no known bead-seed family (update plan-lint's map if this is intentional)"
                ),
            });
            continue;
        };
        // Multi-segment families (`PLSQL-CORE-IDS-001`, `PLSQL-STORE-DAEMON-002`)
        // should count toward the base family, so we accept any bead family
        // that starts with the expected prefix.
        let covered = bead_families
            .iter()
            .any(|f| f.as_str() == *family || f.starts_with(&format!("{family}-")));
        if !covered {
            findings.push(Finding {
                rule: "component-coverage",
                severity: Severity::Error,
                line: 0,
                message: format!(
                    "component `{component}` has no bead seeds under family `{family}`"
                ),
            });
        }
        if !doc.text.contains(&format!("`{component}`")) {
            findings.push(Finding {
                rule: "component-coverage",
                severity: Severity::Warn,
                line: 0,
                message: format!("component `{component}` is named in §5 but never quoted as `{component}` elsewhere in plan.md"),
            });
        }
    }
    findings
}

fn check_banned_language(doc: &PlanDoc) -> Vec<Finding> {
    // Each pattern is a tuple (regex, label, severity).
    let patterns: Vec<(Regex, &'static str)> = vec![
        (
            Regex::new(r"\bPhase\s+[1-9]\b").expect("static regex"),
            "Phase N",
        ),
        (Regex::new(r"\b(?:M|m)VP\b").expect("static regex"), "MVP"),
        (
            Regex::new(r"\bfirst\s+wave\b").expect("static regex"),
            "first wave",
        ),
        (
            Regex::new(r"\b(?:alpha|beta)\s+release\b").expect("static regex"),
            "alpha/beta release",
        ),
        (
            Regex::new(r"\brelease\s+wedge\b").expect("static regex"),
            "release wedge",
        ),
        (
            Regex::new(r"\bQ[1-4]\s+\d{4}\b").expect("static regex"),
            "Qn YYYY",
        ),
    ];

    // Whitelist: changelog blocks live under the "Status / Version log"
    // section. The section number has drifted over time (it has been §28 in
    // the ToC, §26 in some revisions, etc.), so we match by title rather
    // than by number. Everything from that heading onward is treated as
    // historical.
    let status_section_start = doc
        .headings
        .iter()
        .find(|h| {
            h.level == 2
                && (h.title.contains("Status / Version log")
                    || h.title.contains("Version log")
                    || h.title.contains("Status log")
                    || h.title.eq_ignore_ascii_case("Changelog"))
        })
        .map(|h| h.line);

    let mut findings = Vec::new();
    let mut in_fence = false;
    for (idx, line) in doc.lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let lineno = idx + 1;
        if let Some(start) = status_section_start
            && lineno >= start
        {
            continue;
        }
        if line.trim_start().starts_with("> ") {
            continue;
        }
        // Allow self-references to the rule itself ("banned release-wedge language").
        if line.contains("banned release-wedge") || line.contains("`release wedge`") {
            continue;
        }
        for (re, label) in &patterns {
            if let Some(m) = re.find(line) {
                // Whitelist: if the match falls inside a quoted span on the
                // line, treat it as a meta-discussion (e.g. a sentence
                // saying `No "Phase 1," "Q3 2026," "first wave."` is the
                // plan describing terms it bans, not a violation).
                if is_inside_quotes(line, m.start()) {
                    continue;
                }
                findings.push(Finding {
                    rule: "banned-release-wedge-language",
                    severity: Severity::Error,
                    line: lineno,
                    message: format!(
                        "`{label}` language at column {}: {:?}",
                        m.start() + 1,
                        m.as_str()
                    ),
                });
            }
        }
    }
    findings
}

fn is_inside_quotes(line: &str, byte_offset: usize) -> bool {
    // Walk the line up to `byte_offset`, toggling an "open quote" flag each
    // time we cross a quote character. Handles ASCII ", U+201C/U+201D, and
    // U+2018/U+2019. Apostrophes in contractions (`don't`) are intentionally
    // also toggled — false positives there are rare in this corpus and
    // safer than missing real quotes.
    let mut in_quote = false;
    for (idx, ch) in line.char_indices() {
        if idx >= byte_offset {
            break;
        }
        if matches!(ch, '"' | '\u{201C}' | '\u{201D}' | '\u{2018}' | '\u{2019}') {
            in_quote = !in_quote;
        }
    }
    in_quote
}

// ----------------------------------------------------------------------------
// Reporting
// ----------------------------------------------------------------------------

fn summarize(findings: &[Finding]) -> BTreeMap<&'static str, RuleResult> {
    let mut out: BTreeMap<&'static str, RuleResult> = BTreeMap::new();
    for f in findings {
        let entry = out.entry(f.rule).or_insert(RuleResult {
            findings: 0,
            errors: 0,
        });
        entry.findings += 1;
        if matches!(f.severity, Severity::Error) {
            entry.errors += 1;
        }
    }
    // Ensure every rule appears even if it had no findings.
    for rule in [
        "heading-monotonicity",
        "toc-anchor",
        "duplicate-bead-id",
        "missing-bead-dependency",
        "stale-section-ref",
        "component-coverage",
        "banned-release-wedge-language",
    ] {
        out.entry(rule).or_insert(RuleResult {
            findings: 0,
            errors: 0,
        });
    }
    out
}

fn print_human(report: &Report, doctor: bool) {
    if doctor {
        println!("plan-lint doctor — {}", report.plan_path);
        println!("schema {} v{}", report.schema_id, report.schema_version);
        println!();
        for (rule, result) in &report.rule_results {
            let status = if result.errors == 0 { "ok" } else { "FAIL" };
            println!(
                "  {status:<4} {rule:<32} findings={} errors={}",
                result.findings, result.errors
            );
        }
        println!();
    }

    if report.findings.is_empty() {
        println!("plan-lint: ok (no findings)");
        return;
    }

    for f in &report.findings {
        let sev = match f.severity {
            Severity::Error => "error",
            Severity::Warn => "warn",
        };
        println!(
            "{}:{}: {sev}[{}]: {}",
            report.plan_path, f.line, f.rule, f.message
        );
    }
    let errors = report
        .findings
        .iter()
        .filter(|f| matches!(f.severity, Severity::Error))
        .count();
    let warns = report.findings.len() - errors;
    println!();
    println!("summary: {errors} error(s), {warns} warning(s)");
}

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_from(text: &str) -> PlanDoc {
        parse_plan(text).expect("synthetic input parses")
    }

    #[test]
    fn slug_matches_github_style() {
        assert_eq!(slugify("Identity", Some("1")), "1-identity");
        assert_eq!(
            slugify("Layer 1 — Parser Core", Some("7")),
            "7-layer-1--parser-core"
        );
        assert_eq!(slugify("Open decisions", Some("23")), "23-open-decisions");
        assert_eq!(
            slugify("Architectural rules (R-rules)", Some("4")),
            "4-architectural-rules-r-rules"
        );
    }

    #[test]
    fn heading_monotonicity_passes_on_clean_doc() {
        let text = "# Title\n\n## 1. A\n\n## 2. B\n\n### 2.1 a\n\n### 2.2 b\n\n## 3. C\n";
        let doc = doc_from(text);
        assert!(check_heading_monotonicity(&doc).is_empty());
    }

    #[test]
    fn heading_monotonicity_flags_regression() {
        let text = "# Title\n\n## 2. B\n\n## 1. A\n";
        let doc = doc_from(text);
        let findings = check_heading_monotonicity(&doc);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("must be monotonic"));
    }

    #[test]
    fn duplicate_bead_ids_caught() {
        let text = concat!(
            "## 1. Beads\n\n",
            "| Bead | Title | Depends | Effort |\n",
            "|------|-------|---------|--------|\n",
            "| `PLSQL-WS-001` | first | none | S |\n",
            "| `PLSQL-WS-001` | duplicate | none | S |\n",
        );
        let doc = doc_from(text);
        let findings = check_duplicate_beads(&doc);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("re-defined"));
    }

    #[test]
    fn missing_bead_dep_caught() {
        let text = concat!(
            "## 1. Beads\n\n",
            "| Bead | Title | Depends | Effort |\n",
            "|------|-------|---------|--------|\n",
            "| `PLSQL-WS-001` | first | none | S |\n",
            "| `PLSQL-WS-002` | second | WS-999 | S |\n",
        );
        let doc = doc_from(text);
        let findings = check_missing_bead_deps(&doc);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("unknown bead `PLSQL-WS-999`"));
    }

    #[test]
    fn stale_section_ref_caught() {
        let text = "## 1. A\n\nSee §99 for details.\n";
        let doc = doc_from(text);
        let findings = check_stale_section_refs(&doc);
        assert!(findings.iter().any(|f| f.message.contains("§99")));
    }

    #[test]
    fn banned_language_caught_outside_status_log() {
        let text = "## 1. A\n\nThis is Phase 2 work.\n";
        let doc = doc_from(text);
        let findings = check_banned_language(&doc);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("Phase N"));
    }

    #[test]
    fn banned_language_whitelisted_in_status_log() {
        let text = "## 28. Status / Version log\n\n- Phase 2 work was abandoned.\n";
        let doc = doc_from(text);
        let findings = check_banned_language(&doc);
        assert!(findings.is_empty());
    }

    #[test]
    fn banned_language_whitelisted_in_blockquote() {
        let text = "## 1. A\n\n> Phase 2 work was historical.\n";
        let doc = doc_from(text);
        let findings = check_banned_language(&doc);
        assert!(findings.is_empty());
    }

    #[test]
    fn toc_anchor_caught() {
        let text = "## Table of Contents\n\n- [Bogus](#no-such-section)\n\n## 1. Identity\n";
        let doc = doc_from(text);
        let findings = check_toc_anchors(&doc);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn past_toc_latch_blocks_late_heading_reentry() {
        // A later H2 whose title mentions "Table of Contents" must NOT
        // re-open ToC collection. The body lines below it look like
        // ToC entries but should be ignored.
        let text = concat!(
            "## Table of Contents\n\n",
            "- [Identity](#1-identity)\n\n",
            "## 1. Identity\n\n",
            "Body text here.\n\n",
            "## 99. Table of Contents normalization log\n\n",
            // The following list-items look like ToC entries; the latch
            // must prevent them from being collected.
            "- [Should not collect](#fake-anchor-1)\n",
            "- [Also not](#fake-anchor-2)\n",
        );
        let doc = doc_from(text);
        // Only the real ToC entry should be picked up.
        assert_eq!(doc.toc_entries.len(), 1);
        assert_eq!(doc.toc_entries[0].2, "1-identity");
    }
}
