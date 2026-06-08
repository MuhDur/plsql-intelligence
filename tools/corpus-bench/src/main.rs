#![forbid(unsafe_code)]

//! `corpus-bench` — cold/warm parse-time benchmark harness across the
//! corpus.
//!
//! Walks `corpus/` for PL/SQL files (.sql / .pks / .pkb / .tps / .tpb /
//! .trg / .vw), parses each via `plsql_parser_antlr::lower::lower_source`,
//! and records cold + warm wall-clock timings. Prints a per-file table
//! plus a summary; emits a stable-schema JSON report under `--robot-json`.
//!
//! The "cold" timing is the first parse of each file (no warm-up). The
//! "warm" timing is the median of `--warm-iters N` subsequent parses
//! (default 5). Together they give an upper bound for a CI parse-time
//! budget gate.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use std::{env, fs};

use plsql_core::FileId;
use serde::Serialize;
use walkdir::WalkDir;

const SCHEMA_ID: &str = "corpus-bench.report";
const SCHEMA_VERSION: u32 = 1;

/// Stable contract version for the `--capabilities` payload. Bump only
/// on a breaking change to the JSON shape; the pinned regression test
/// fails if the shape drifts without this being bumped.
const CAPABILITIES_CONTRACT_VERSION: u32 = 1;

const DEFAULT_CORPUS_ROOT: &str = "corpus";

/// Sorted list of every valid flag name (used by the typo-suggestion
/// helper and surfaced in error messages on an unknown argument).
const VALID_FLAGS: &[&str] = &[
    "-V",
    "-h",
    "--capabilities",
    "--corpus-root",
    "--help",
    "--robot-docs",
    "--robot-json",
    "--version",
    "--warm-iters",
];

/// Levenshtein-1/2 "did you mean?" hint. Returns the closest known
/// flag if its edit-distance ≤ 2 from `unknown`, else `None`.
fn suggest_flag(unknown: &str, known: &[&'static str]) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for cand in known {
        let d = edit_distance(unknown, cand);
        if d <= 2 && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, *cand));
        }
    }
    best.map(|(_, s)| s)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

fn capabilities_json() -> serde_json::Value {
    serde_json::json!({
        "binary": "corpus-bench",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "mode": "cold + warm parse-time benchmark over a PL/SQL corpus tree",
        "flags": {
            "--corpus-root": format!("corpus root to walk (default: ./{DEFAULT_CORPUS_ROOT})"),
            "--warm-iters": "iterations to collect the warm-timing median (default: 5)",
            "--robot-json": format!("emit a stable-schema JSON report on stdout ({SCHEMA_ID} v{SCHEMA_VERSION})"),
            "--capabilities": "print this machine-readable agent contract as JSON and exit",
            "--robot-docs": "print a paste-ready agent handbook and exit",
            "--version / -V": "print `corpus-bench <version>` and exit",
            "-h / --help": "print usage and exit"
        },
        "exit_codes": {
            "0": "success (≥1 PL/SQL file benchmarked)",
            "2": "invocation error (bad args, or no PL/SQL files found under --corpus-root)"
        },
        "stdout_contract": "stdout carries the report (human table or JSON envelope under --robot-json); diagnostics — including the empty-corpus error — go to stderr",
        "report_schema": { "id": SCHEMA_ID, "version": SCHEMA_VERSION }
    })
}

fn robot_docs_text() -> String {
    format!(
        r#"corpus-bench agent handbook
=============================

WHAT IT DOES
  Walks a PL/SQL corpus tree, parses every .sql / .pks / .pkb / .tps /
  .tpb / .trg / .vw file via plsql_parser_antlr, and records cold +
  warm wall-clock timings. Emits a per-file table plus summary; under
  --robot-json the same data lands as a stable-schema JSON report.

CANONICAL INVOCATION
  corpus-bench                                # default: ./{default_root}, 5 warm iters
  corpus-bench --corpus-root corpus/synthetic # narrower target
  corpus-bench --warm-iters 9                 # tighter median
  corpus-bench --robot-json | jq .summary     # stable JSON report

ROBOT-JSON ENVELOPE
  {{
    "schema_id":      "{schema_id}",
    "schema_version": {schema_version},
    "corpus_root":    "...",
    "file_count":     N,
    "warm_iters":     N,
    "summary":        {{ ... }},
    "per_file":       [ ... ],
    "empty_corpus":   true  // only on the "no files found" path
  }}
  When `--corpus-root` points at a path with no PL/SQL files, the
  report carries `file_count: 0`, `summary: null`, `per_file: []`,
  `empty_corpus: true` — and a human-readable diagnostic still goes
  to stderr. The exit code is 2.

FLAGS SUMMARY
  --corpus-root <path>  corpus tree to walk (default: ./{default_root})
  --warm-iters <N>      warm-timing iterations (default: 5)
  --robot-json          stable-schema JSON report on stdout
  --capabilities        machine-readable agent contract (JSON)
  --robot-docs          this handbook
  --version / -V        print corpus-bench <version>
  -h / --help           usage summary

EXIT CODES
  0  success (≥1 PL/SQL file benchmarked)
  2  invocation error (bad args, or empty corpus under --corpus-root)

DISCOVERY
  corpus-bench --capabilities    versioned agent contract
  corpus-bench --help            human usage summary

NOTE
  corpus-bench is a small benchmark harness; it has no `--robot-triage`
  mega-command (the surface is narrow enough that `--capabilities`
  is already a one-call bootstrap).
"#,
        default_root = DEFAULT_CORPUS_ROOT,
        schema_id = SCHEMA_ID,
        schema_version = SCHEMA_VERSION,
    )
}

#[derive(Debug, Serialize)]
struct Report {
    schema_id: &'static str,
    schema_version: u32,
    corpus_root: String,
    file_count: usize,
    warm_iters: usize,
    summary: Option<Summary>,
    per_file: Vec<FileTiming>,
    /// Set on the empty-corpus path so an agent doing
    /// `corpus-bench --robot-json | jq '.summary'` gets a structured
    /// "no files found" signal alongside the exit-2 non-zero status,
    /// instead of crashing on the previously-unparseable stderr line.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    empty_corpus: bool,
}

#[derive(Debug, Default, Serialize)]
struct Summary {
    total_cold_us: u128,
    total_warm_median_us: u128,
    slowest_cold_path: String,
    slowest_cold_us: u128,
    fastest_warm_median_us: u128,
}

#[derive(Debug, Serialize)]
struct FileTiming {
    path: String,
    bytes: usize,
    cold_us: u128,
    warm_median_us: u128,
    warm_samples_us: Vec<u128>,
    decl_count: usize,
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1).collect()) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("error: {msg}");
            // Error-path diagnostic: keep on stderr.
            let _ = print_usage_to(&mut std::io::stderr().lock());
            return ExitCode::from(2);
        }
    };

    if args.version {
        println!("corpus-bench {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    if args.capabilities {
        println!(
            "{}",
            serde_json::to_string_pretty(&capabilities_json()).unwrap()
        );
        return ExitCode::SUCCESS;
    }

    if args.robot_docs {
        print!("{}", robot_docs_text());
        return ExitCode::SUCCESS;
    }

    let files = collect_corpus_files(&args.corpus_root);
    if files.is_empty() {
        // Empty-corpus path: keep the human diagnostic on STDERR (so
        // `corpus-bench --robot-json | jq .` does not see a non-JSON
        // line on stdout), and append the default-path hint so the
        // agent learns the canonical invocation in one round-trip.
        let hint = if args.corpus_root.as_path() != Path::new(DEFAULT_CORPUS_ROOT) {
            format!("\n  try: corpus-bench --corpus-root ./{DEFAULT_CORPUS_ROOT} (the default)")
        } else {
            String::new()
        };
        eprintln!(
            "corpus-bench: no PL/SQL files found under {} — nothing to benchmark{hint}",
            args.corpus_root.display()
        );
        // In --robot-json mode also emit a valid empty-report envelope
        // on stdout so jq pipelines do not break.
        if args.robot_json {
            let report = Report {
                schema_id: SCHEMA_ID,
                schema_version: SCHEMA_VERSION,
                corpus_root: args.corpus_root.display().to_string(),
                file_count: 0,
                warm_iters: args.warm_iters,
                summary: None,
                per_file: Vec::new(),
                empty_corpus: true,
            };
            if let Ok(s) = serde_json::to_string(&report) {
                println!("{s}");
            }
        }
        return ExitCode::from(2);
    }

    let mut per_file: Vec<FileTiming> = Vec::with_capacity(files.len());
    for path in &files {
        let bytes = match fs::read_to_string(path) {
            Ok(b) => b,
            Err(err) => {
                eprintln!("warn: skip {}: {err}", path.display());
                continue;
            }
        };
        let file_id = FileId::new(per_file.len() as u32);

        // Cold parse — first invocation.
        let cold = Instant::now();
        let ast = plsql_parser_antlr::lower::lower_source(&bytes, file_id);
        let cold_us = cold.elapsed().as_micros();

        // Warm parses — `warm_iters` invocations; collect samples + median.
        let mut samples: Vec<u128> = Vec::with_capacity(args.warm_iters);
        for _ in 0..args.warm_iters {
            let t = Instant::now();
            let _ = plsql_parser_antlr::lower::lower_source(&bytes, file_id);
            samples.push(t.elapsed().as_micros());
        }
        samples.sort_unstable();
        let warm_median_us = if samples.is_empty() {
            0
        } else {
            samples[samples.len() / 2]
        };

        per_file.push(FileTiming {
            path: path
                .strip_prefix(&args.corpus_root)
                .unwrap_or(path)
                .display()
                .to_string(),
            bytes: bytes.len(),
            cold_us,
            warm_median_us,
            warm_samples_us: samples,
            decl_count: ast.root.declarations.len(),
        });
    }

    let summary = summarise(&per_file);
    let report = Report {
        schema_id: SCHEMA_ID,
        schema_version: SCHEMA_VERSION,
        corpus_root: args.corpus_root.display().to_string(),
        file_count: per_file.len(),
        warm_iters: args.warm_iters,
        summary: Some(summary),
        per_file,
        empty_corpus: false,
    };

    if args.robot_json {
        let json = serde_json::to_string_pretty(&report).expect("Report serializes cleanly");
        println!("{json}");
    } else {
        print_human(&report);
    }

    ExitCode::SUCCESS
}

#[derive(Debug)]
struct Args {
    corpus_root: PathBuf,
    warm_iters: usize,
    robot_json: bool,
    capabilities: bool,
    robot_docs: bool,
    version: bool,
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut out = Args {
        corpus_root: PathBuf::from(DEFAULT_CORPUS_ROOT),
        warm_iters: 5,
        robot_json: false,
        capabilities: false,
        robot_docs: false,
        version: false,
    };
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--corpus-root" => {
                out.corpus_root = PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--corpus-root requires a path".to_string())?,
                );
            }
            "--warm-iters" => {
                out.warm_iters = iter
                    .next()
                    .ok_or_else(|| "--warm-iters requires a count".to_string())?
                    .parse()
                    .map_err(|e| format!("--warm-iters parse: {e}"))?;
            }
            "--robot-json" => out.robot_json = true,
            "--capabilities" => out.capabilities = true,
            "--robot-docs" => out.robot_docs = true,
            "-V" | "--version" => out.version = true,
            "-h" | "--help" => {
                // POSIX: user-requested help is data, not a
                // diagnostic — it goes to stdout so `--help | less`
                // and `--help > file` work the way agents and humans
                // both expect.
                let _ = print_usage_to(&mut std::io::stdout().lock());
                std::process::exit(0);
            }
            other => {
                let msg = match suggest_flag(other, VALID_FLAGS) {
                    Some(hint) => format!(
                        "unknown argument {other:?} (did you mean `{hint}`?)\nvalid flags: {}",
                        VALID_FLAGS.join(", ")
                    ),
                    None => format!(
                        "unknown argument {other:?}\nvalid flags: {}",
                        VALID_FLAGS.join(", ")
                    ),
                };
                return Err(msg);
            }
        }
    }
    Ok(out)
}

fn print_usage_to<W: std::io::Write>(w: &mut W) -> std::io::Result<()> {
    writeln!(
        w,
        "usage: corpus-bench [--corpus-root <path>] [--warm-iters <N>] [--robot-json]"
    )?;
    writeln!(w)?;
    writeln!(
        w,
        "Benchmarks cold + warm parse time for every PL/SQL file under"
    )?;
    writeln!(
        w,
        "the corpus root. Defaults: corpus-root=./{DEFAULT_CORPUS_ROOT}, warm-iters=5."
    )?;
    writeln!(w)?;
    writeln!(w, "Flags:")?;
    writeln!(
        w,
        "  --robot-json       Stable-schema JSON report (schema_id={SCHEMA_ID} v{SCHEMA_VERSION})"
    )?;
    writeln!(
        w,
        "  --warm-iters <N>   Iterations to collect warm-timing median"
    )?;
    writeln!(
        w,
        "  --capabilities     Print the machine-readable agent contract as JSON and exit"
    )?;
    writeln!(
        w,
        "  --robot-docs       Print a paste-ready agent handbook and exit"
    )?;
    writeln!(
        w,
        "  -V, --version      Print corpus-bench <version> and exit"
    )?;
    writeln!(w)?;
    writeln!(
        w,
        "Exit codes: 0 ok, 2 invocation error (bad args, empty corpus)."
    )?;
    Ok(())
}

fn collect_corpus_files(root: &Path) -> Vec<PathBuf> {
    let exts: BTreeSet<&str> = ["sql", "pks", "pkb", "tps", "tpb", "trg", "vw"]
        .into_iter()
        .collect();
    let mut out = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && exts.contains(ext.to_lowercase().as_str())
        {
            out.push(path.to_path_buf());
        }
    }
    out.sort();
    out
}

fn summarise(per_file: &[FileTiming]) -> Summary {
    let mut s = Summary::default();
    if per_file.is_empty() {
        return s;
    }
    s.total_cold_us = per_file.iter().map(|f| f.cold_us).sum();
    s.total_warm_median_us = per_file.iter().map(|f| f.warm_median_us).sum();
    if let Some(slow) = per_file.iter().max_by_key(|f| f.cold_us) {
        s.slowest_cold_path = slow.path.clone();
        s.slowest_cold_us = slow.cold_us;
    }
    if let Some(fast) = per_file
        .iter()
        .filter(|f| f.warm_median_us > 0)
        .min_by_key(|f| f.warm_median_us)
    {
        s.fastest_warm_median_us = fast.warm_median_us;
    }
    s
}

fn print_human(report: &Report) {
    println!(
        "corpus-bench: {} files under {} (warm_iters={})",
        report.file_count, report.corpus_root, report.warm_iters
    );
    println!(
        "{:<60} {:>10} {:>10} {:>10} {:>6}",
        "path", "bytes", "cold µs", "warm µs", "decls"
    );
    for f in &report.per_file {
        let p = if f.path.len() > 58 {
            format!("…{}", &f.path[f.path.len() - 57..])
        } else {
            f.path.clone()
        };
        println!(
            "{:<60} {:>10} {:>10} {:>10} {:>6}",
            p, f.bytes, f.cold_us, f.warm_median_us, f.decl_count
        );
    }
    println!();
    if let Some(summary) = &report.summary {
        println!(
            "total cold µs: {}, total warm-median µs: {}",
            summary.total_cold_us, summary.total_warm_median_us
        );
        println!(
            "slowest cold parse: {} ({} µs)",
            summary.slowest_cold_path, summary.slowest_cold_us
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_contract_is_pinned() {
        let c = capabilities_json();
        assert_eq!(c["binary"], "corpus-bench");
        assert_eq!(c["contract_version"], CAPABILITIES_CONTRACT_VERSION);
        assert_eq!(c["version"], env!("CARGO_PKG_VERSION"));
        for key in ["flags", "exit_codes", "stdout_contract", "report_schema"] {
            assert!(c.get(key).is_some(), "capabilities missing key `{key}`");
        }
        let flags = c["flags"].as_object().unwrap();
        for required in [
            "--corpus-root",
            "--warm-iters",
            "--robot-json",
            "--capabilities",
            "--robot-docs",
            "--version / -V",
        ] {
            assert!(flags.contains_key(required), "missing flag `{required}`");
        }
        assert_eq!(c["report_schema"]["id"], SCHEMA_ID);
        assert!(c["exit_codes"]["0"].is_string());
        assert!(c["exit_codes"]["2"].is_string());
    }

    #[test]
    fn capabilities_round_trips_as_single_line_json() {
        let s = serde_json::to_string(&capabilities_json()).unwrap();
        assert!(!s.contains('\n'));
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["binary"], "corpus-bench");
    }

    #[test]
    fn robot_docs_mentions_capabilities_and_empty_corpus() {
        let docs = robot_docs_text();
        assert!(docs.contains("--capabilities"));
        assert!(docs.contains("empty_corpus"));
        assert!(docs.contains(SCHEMA_ID));
    }

    #[test]
    fn version_flag_parses_long_and_short() {
        for arg in ["--version", "-V"] {
            let parsed = parse_args(vec![arg.into()])
                .unwrap_or_else(|e| panic!("{arg} should parse, got: {e}"));
            assert!(parsed.version, "{arg} should set version flag");
        }
    }

    #[test]
    fn unknown_flag_suggests_near_miss() {
        let err = parse_args(vec!["--robotjson".into()]).unwrap_err();
        assert!(
            err.contains("--robot-json"),
            "expected DYM hint; got: {err}"
        );
        assert!(err.contains("did you mean"));
    }

    #[test]
    fn empty_corpus_report_serializes() {
        // Pin the empty-corpus envelope shape so an agent parsing
        // `corpus-bench --robot-json` on an empty path gets a
        // well-formed JSON object on stdout.
        let r = Report {
            schema_id: SCHEMA_ID,
            schema_version: SCHEMA_VERSION,
            corpus_root: "/nonexistent".to_string(),
            file_count: 0,
            warm_iters: 5,
            summary: None,
            per_file: Vec::new(),
            empty_corpus: true,
        };
        let s = serde_json::to_string(&r).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["file_count"], 0);
        assert_eq!(v["empty_corpus"], true);
        assert!(v["summary"].is_null());
        assert_eq!(v["per_file"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn suggest_flag_finds_obvious_typos() {
        assert_eq!(
            suggest_flag("--robotjson", VALID_FLAGS),
            Some("--robot-json")
        );
        assert_eq!(suggest_flag("--versoin", VALID_FLAGS), Some("--version"));
        assert_eq!(suggest_flag("--xyz-totally-unknown", VALID_FLAGS), None);
    }
}
