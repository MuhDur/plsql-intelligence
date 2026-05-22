#![forbid(unsafe_code)]
//! `usr-gate-rs` — the Rust-level check helper the §3 gate script
//! (`scripts/usr_gate.sh`) shells out to (PLSQL-USR-001, P4).
//!
//! It is intentionally a thin CLI over the **public check primitives**
//! in [`plsql_accretion::gate`] so the adversarial self-test
//! (`tests/gate_selftest.rs`) exercises the exact same code path the
//! gate runs in production — the bar is identical, never weakened.
//!
//! Subcommands (each: exit 0 ⇒ stage PASS, non-zero ⇒ stage FAIL;
//! evidence to stdout, the gate script echoes it verbatim):
//!
//!   roundtrip   <corpus-dir> <fixtures-dir>     G2
//!   honesty     <candidate-diff>                G7
//!   residue     <candidate-diff> <fixtures-dir> G8  (non-zero ⇒ I-PRIVACY abort)
//!   baseline-cmp <baseline.json> <metrics-file> G6
//!   pins        <candidate-diff>                G9

use std::path::Path;
use std::process::ExitCode;

use plsql_accretion::gate::{
    baseline_cmp, honesty_check, measure_estate_metrics, pins_check, residue_check, roundtrip_check,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(String::as_str).unwrap_or("");
    let result: Result<String, String> = match sub {
        "roundtrip" => match (args.get(2), args.get(3)) {
            (Some(c), Some(f)) => roundtrip_check(Path::new(c), Path::new(f)),
            _ => Err("usage: usr-gate-rs roundtrip <corpus-dir> <fixtures-dir>".into()),
        },
        "honesty" => match args.get(2) {
            Some(c) => match std::fs::read_to_string(c) {
                Ok(text) => honesty_check(&text),
                Err(e) => Err(format!("cannot read candidate-diff {c}: {e}")),
            },
            None => Err("usage: usr-gate-rs honesty <candidate-diff>".into()),
        },
        "residue" => match (args.get(2), args.get(3)) {
            (Some(c), Some(f)) => match std::fs::read_to_string(c) {
                Ok(text) => residue_check(&text, Path::new(f)),
                Err(e) => Err(format!("cannot read candidate-diff {c}: {e}")),
            },
            _ => Err("usage: usr-gate-rs residue <candidate-diff> <fixtures-dir>".into()),
        },
        "baseline-cmp" => match (args.get(2), args.get(3)) {
            (Some(b), Some(m)) => match (std::fs::read_to_string(b), std::fs::read_to_string(m)) {
                (Ok(bj), Ok(mt)) => baseline_cmp(&bj, &mt),
                _ => Err("cannot read baseline json or metrics file".into()),
            },
            _ => Err("usage: usr-gate-rs baseline-cmp <baseline.json> <metrics-file>".into()),
        },
        "metrics" => match args.get(2) {
            Some(estate) => measure_estate_metrics(Path::new(estate)),
            None => Err("usage: usr-gate-rs metrics <estate-path>".into()),
        },
        "pins" => match args.get(2) {
            Some(c) => match std::fs::read_to_string(c) {
                Ok(text) => {
                    let repo =
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                    pins_check(&repo, &text)
                }
                Err(e) => Err(format!("cannot read candidate-diff {c}: {e}")),
            },
            None => Err("usage: usr-gate-rs pins <candidate-diff>".into()),
        },
        other => Err(format!("unknown subcommand {other:?}")),
    };

    match result {
        Ok(evidence) => {
            println!("{evidence}");
            ExitCode::SUCCESS
        }
        Err(reason) => {
            // stdout (not stderr) so the gate script captures it as
            // the stage's <evidence> in one read.
            println!("{reason}");
            ExitCode::from(1)
        }
    }
}
