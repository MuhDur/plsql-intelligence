#![forbid(unsafe_code)]

//! `corpus-bench` — cold/warm parse-time benchmark harness across the
//! corpus (PLSQL-PARSE-016).
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

#[derive(Debug, Serialize)]
struct Report {
    schema_id: &'static str,
    schema_version: u32,
    corpus_root: String,
    file_count: usize,
    warm_iters: usize,
    summary: Summary,
    per_file: Vec<FileTiming>,
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
            print_usage();
            return ExitCode::from(2);
        }
    };

    let files = collect_corpus_files(&args.corpus_root);
    if files.is_empty() {
        eprintln!(
            "no PL/SQL files found under {} — nothing to benchmark",
            args.corpus_root.display()
        );
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
        summary,
        per_file,
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
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut out = Args {
        corpus_root: PathBuf::from("corpus"),
        warm_iters: 5,
        robot_json: false,
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
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(out)
}

fn print_usage() {
    eprintln!("usage: corpus-bench [--corpus-root <path>] [--warm-iters <N>] [--robot-json]");
    eprintln!();
    eprintln!("Benchmarks cold + warm parse time for every PL/SQL file under");
    eprintln!("the corpus root. Defaults: corpus-root=./corpus, warm-iters=5.");
    eprintln!();
    eprintln!("Flags:");
    eprintln!(
        "  --robot-json       Stable-schema JSON report (schema_id={SCHEMA_ID} v{SCHEMA_VERSION})"
    );
    eprintln!("  --warm-iters <N>   Iterations to collect warm-timing median");
    eprintln!();
    eprintln!("Exit codes: 0 ok, 2 invocation error (bad args, empty corpus).");
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
    println!(
        "total cold µs: {}, total warm-median µs: {}",
        report.summary.total_cold_us, report.summary.total_warm_median_us
    );
    println!(
        "slowest cold parse: {} ({} µs)",
        report.summary.slowest_cold_path, report.summary.slowest_cold_us
    );
}
