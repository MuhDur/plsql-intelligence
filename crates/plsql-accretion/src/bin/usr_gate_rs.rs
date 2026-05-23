#![forbid(unsafe_code)]
//! `usr-gate-rs` — the Rust-level check helper the §3 gate script
//! (`scripts/usr_gate.sh`) shells out to (P4).
//!
//! It is intentionally a thin CLI over the **public check primitives**
//! in [`plsql_accretion::gate`] so the adversarial self-test
//! (`tests/gate_selftest.rs`) exercises the exact same code path the
//! gate runs in production — the bar is identical, never weakened.
//!
//! ## Subcommands
//!
//! Each subcommand: exit `0` ⇒ stage PASS, non-zero ⇒ stage FAIL;
//! evidence to stdout, the gate script echoes it verbatim.
//!
//! ```text
//! roundtrip    <corpus-dir> <fixtures-dir>     G2
//! honesty      <candidate-diff>                G7
//! residue      <candidate-diff> <fixtures-dir> G8  (non-zero ⇒ I-PRIVACY abort)
//! baseline-cmp <baseline.json> <metrics-file>  G6
//! metrics      <estate-path>                   G6 (measurement helper)
//! pins         <candidate-diff>                G9
//! ```
//!
//! ## Flags
//!
//! ```text
//! -h, --help          print human usage and exit 0
//! -V, --version       print binary version and exit 0
//!     --capabilities  print the machine-readable agent contract (JSON) and exit 0
//!     --robot-docs    print a paste-ready agent handbook and exit 0
//! ```
//!
//! ## Exit codes
//!
//! ```text
//! 0  stage PASS / informational flag handled
//! 1  stage FAIL / I/O fault / unknown subcommand
//! 2  invocation error (handled separately by the gate script; this
//!    binary surfaces 1 for any non-pass to match the legacy contract)
//! ```

use std::path::Path;
use std::process::ExitCode;

use plsql_accretion::gate::{
    GateError, baseline_cmp, honesty_check, measure_estate_metrics, pins_check, residue_check,
    roundtrip_check,
};

/// Stable contract version for the `--capabilities` payload. Bump
/// only on a breaking change to the JSON shape (Axiom 17 — every
/// contract surface has a drift-guard test).
const CAPABILITIES_CONTRACT_VERSION: u32 = 1;

/// Sorted list of all valid subcommands and flags (used in the
/// `unknown subcommand` error and in `--capabilities`).
const SUBCOMMANDS: &[&str] = &[
    "baseline-cmp",
    "honesty",
    "metrics",
    "pins",
    "residue",
    "roundtrip",
];

const INFO_FLAGS: &[&str] = &[
    "--capabilities",
    "--help",
    "--robot-docs",
    "--version",
    "-V",
    "-h",
];

fn capabilities_json() -> serde_json::Value {
    serde_json::json!({
        "binary": "usr-gate-rs",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "mode": "subcommand-per-stage gate check helper (shelled by scripts/usr_gate.sh)",
        "subcommands": {
            "roundtrip":    "G2 — lossless reconstruct over <corpus-dir> + <fixtures-dir>",
            "honesty":      "G7 — D3 anti-gaming check over the candidate-diff manifest",
            "residue":      "G8 — I-PRIVACY estate-residue scan (non-zero ⇒ abort)",
            "baseline-cmp": "G6 — monotone-metric non-regression over <baseline.json> + <metrics>",
            "metrics":      "G6 helper — measure estate metrics under <estate-path>",
            "pins":         "G9 — revert-and-assert behavior pin (requires USR_GATE_TRUST_PINS=1)"
        },
        "info_flags": {
            "-h / --help":     "print human usage and exit 0",
            "-V / --version":  "print binary version and exit 0",
            "--capabilities":  "print this machine-readable contract and exit 0",
            "--robot-docs":    "print a paste-ready agent handbook and exit 0"
        },
        "exit_codes": {
            "0": "stage PASS or informational flag handled",
            "1": "stage FAIL, I/O fault, or unknown subcommand"
        },
        "env": {
            "USR_GATE_TRUST_PINS": "set to `1` to allow the `pins` subcommand to execute \
                                    candidate-supplied shell hooks (under the strict G9 \
                                    program-name allowlist); default off rejects all shell \
                                    pins fail-closed (oracle-k30w)"
        },
        "stdout_contract": "stdout is the stage <evidence>; the gate script captures it verbatim"
    })
}

fn robot_docs_text() -> String {
    format!(
        r#"usr-gate-rs agent handbook
============================

WHAT IT DOES
  Thin Rust CLI over the §3 gate's check primitives
  (plsql_accretion::gate). Each subcommand corresponds to one
  G-stage and prints the stage's <evidence> line to stdout. The
  shell gate script (scripts/usr_gate.sh) calls this binary
  per stage; the adversarial self-test exercises the same
  code path.

CANONICAL INVOCATION
  Drive it via the gate script, not directly:
    bash scripts/usr_gate.sh <candidate-diff>
  Or run one stage by hand:
    usr-gate-rs honesty <candidate-diff>
    usr-gate-rs pins   <candidate-diff>      # needs USR_GATE_TRUST_PINS=1

SUBCOMMANDS
  roundtrip    <corpus-dir> <fixtures-dir>     G2  lossless reconstruct
  honesty      <candidate-diff>                G7  anti-gaming manifest
  residue      <candidate-diff> <fixtures-dir> G8  privacy residue scan
  baseline-cmp <baseline.json> <metrics-file>  G6  monotone non-regression
  metrics      <estate-path>                   G6  measurement helper
  pins         <candidate-diff>                G9  revert-and-assert pin

INFO FLAGS
  -h / --help        this usage
  -V / --version     binary version
  --capabilities     machine-readable agent contract (JSON)
  --robot-docs       this handbook

EXIT CODES
  0  stage PASS / informational flag handled
  1  stage FAIL / I/O fault / unknown subcommand

ENV
  USR_GATE_TRUST_PINS=1   opt in to candidate-supplied shell hooks
                          for the `pins` subcommand. Default off
                          rejects all shell pins fail-closed (the
                          G9 shell-injection guard, oracle-k30w).

MACHINE-READABLE CONTRACT
  Run: usr-gate-rs --capabilities
  Pinned contract_version={contract_version}; a bump signals a
  breaking change to the JSON shape.
"#,
        contract_version = CAPABILITIES_CONTRACT_VERSION
    )
}

fn print_usage() {
    println!("usage: usr-gate-rs <subcommand> [args...]");
    println!();
    println!("Thin Rust gate-check helper for scripts/usr_gate.sh.");
    println!();
    println!("Subcommands:");
    println!("  roundtrip    <corpus-dir> <fixtures-dir>     G2 lossless reconstruct");
    println!("  honesty      <candidate-diff>                G7 anti-gaming manifest");
    println!("  residue      <candidate-diff> <fixtures-dir> G8 privacy residue scan");
    println!("  baseline-cmp <baseline.json> <metrics-file>  G6 monotone non-regression");
    println!("  metrics      <estate-path>                   G6 measurement helper");
    println!("  pins         <candidate-diff>                G9 revert-and-assert pin");
    println!();
    println!("Info flags:");
    println!("  -h, --help          this usage");
    println!("  -V, --version       binary version");
    println!("      --capabilities  machine-readable agent contract (JSON)");
    println!("      --robot-docs    paste-ready agent handbook");
    println!();
    println!(
        "Env: USR_GATE_TRUST_PINS=1 to opt-in to candidate-supplied shell hooks \
         for `pins` (default off; oracle-k30w shell-injection guard)."
    );
    println!();
    println!("Exit codes: 0 stage PASS / info-flag handled; 1 stage FAIL / fault.");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(String::as_str).unwrap_or("");

    // Info flags FIRST (Axiom 0 — `--help` is the first thing any
    // agent or human reaches for; it MUST never be an error). These
    // also handle the bare-invocation case (`sub == ""`) by printing
    // usage and returning success on `--help`, or a clear pointer on
    // the empty-arg case.
    match sub {
        "-h" | "--help" | "help" => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        "-V" | "--version" | "version" => {
            println!("usr-gate-rs {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        "--capabilities" | "capabilities" => {
            // serde_json::to_string_pretty cannot fail on a Value
            // built from the inline json! macro (no foreign Serialize).
            println!(
                "{}",
                serde_json::to_string_pretty(&capabilities_json()).unwrap_or_default()
            );
            return ExitCode::SUCCESS;
        }
        "--robot-docs" | "robot-docs" => {
            print!("{}", robot_docs_text());
            return ExitCode::SUCCESS;
        }
        "" => {
            eprintln!("usr-gate-rs: no subcommand supplied");
            eprintln!(
                "subcommands: {}; info flags: {}",
                SUBCOMMANDS.join(", "),
                INFO_FLAGS.join(", ")
            );
            eprintln!("run `usr-gate-rs --help` for usage");
            return ExitCode::from(1);
        }
        _ => {}
    }

    // A check primitive yields a typed `GateError`; a usage / file-read
    // fault is a plain string. Both fail the stage closed — unify them
    // into a single stringly evidence line the gate script captures.
    let result: Result<String, String> = match sub {
        "roundtrip" => match (args.get(2), args.get(3)) {
            (Some(c), Some(f)) => roundtrip_check(Path::new(c), Path::new(f)).map_err(err_str),
            _ => Err("usage: usr-gate-rs roundtrip <corpus-dir> <fixtures-dir>".into()),
        },
        "honesty" => match args.get(2) {
            Some(c) => match std::fs::read_to_string(c) {
                Ok(text) => honesty_check(&text).map_err(err_str),
                Err(e) => Err(format!("cannot read candidate-diff {c}: {e}")),
            },
            None => Err("usage: usr-gate-rs honesty <candidate-diff>".into()),
        },
        "residue" => match (args.get(2), args.get(3)) {
            (Some(c), Some(f)) => match std::fs::read_to_string(c) {
                Ok(text) => residue_check(&text, Path::new(f)).map_err(err_str),
                Err(e) => Err(format!("cannot read candidate-diff {c}: {e}")),
            },
            _ => Err("usage: usr-gate-rs residue <candidate-diff> <fixtures-dir>".into()),
        },
        "baseline-cmp" => match (args.get(2), args.get(3)) {
            (Some(b), Some(m)) => match (std::fs::read_to_string(b), std::fs::read_to_string(m)) {
                (Ok(bj), Ok(mt)) => baseline_cmp(&bj, &mt).map_err(err_str),
                _ => Err("cannot read baseline json or metrics file".into()),
            },
            _ => Err("usage: usr-gate-rs baseline-cmp <baseline.json> <metrics-file>".into()),
        },
        "metrics" => match args.get(2) {
            Some(estate) => measure_estate_metrics(Path::new(estate)).map_err(err_str),
            None => Err("usage: usr-gate-rs metrics <estate-path>".into()),
        },
        "pins" => match args.get(2) {
            Some(c) => match std::fs::read_to_string(c) {
                Ok(text) => {
                    let repo =
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                    pins_check(&repo, &text).map_err(err_str)
                }
                Err(e) => Err(format!("cannot read candidate-diff {c}: {e}")),
            },
            None => Err("usage: usr-gate-rs pins <candidate-diff>".into()),
        },
        other => Err(format!(
            "unknown subcommand {other:?}; subcommands: {}; info flags: {}; run \
             `usr-gate-rs --help` for usage",
            SUBCOMMANDS.join(", "),
            INFO_FLAGS.join(", ")
        )),
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

/// Render a typed [`GateError`] into the verbatim evidence line the
/// §3 gate script echoes for a FAIL stage. The `Display` impl already
/// carries the precise, stage-tagged failure text.
fn err_str(e: GateError) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drift-guard for the `--capabilities` agent contract (Axiom 17).
    /// If the JSON shape changes, this test must be updated AND
    /// [`CAPABILITIES_CONTRACT_VERSION`] bumped — that coupling is the
    /// whole point: an agent that pinned the contract should never be
    /// silently surprised by a shape change.
    #[test]
    fn capabilities_contract_is_pinned() {
        let c = capabilities_json();
        assert_eq!(c["binary"], "usr-gate-rs");
        assert_eq!(c["contract_version"], u64::from(CAPABILITIES_CONTRACT_VERSION));
        assert_eq!(c["version"], env!("CARGO_PKG_VERSION"));
        for key in [
            "mode",
            "subcommands",
            "info_flags",
            "exit_codes",
            "env",
            "stdout_contract",
        ] {
            assert!(c.get(key).is_some(), "capabilities missing key `{key}`");
        }
        let subs = c["subcommands"].as_object().expect("subcommands is a map");
        for required in SUBCOMMANDS {
            assert!(
                subs.contains_key(*required),
                "capabilities subcommands missing `{required}`"
            );
        }
        let info = c["info_flags"].as_object().expect("info_flags is a map");
        // The capabilities document collapses the short/long info-flag
        // pairs into "`-x / --long`" keys; assert each known pair is
        // present in some form so agents can grep for the long form.
        let combined = serde_json::to_string(&info).unwrap();
        for f in ["--help", "--version", "--capabilities", "--robot-docs"] {
            assert!(
                combined.contains(f),
                "capabilities info_flags must mention `{f}`, got {combined}"
            );
        }
        assert!(c["exit_codes"]["0"].is_string());
        assert!(c["exit_codes"]["1"].is_string());
        // Env entry must document the G9 trust opt-in (oracle-k30w).
        assert!(c["env"]["USR_GATE_TRUST_PINS"].is_string());
    }

    /// Robot-docs handbook must mention every subcommand and the G9
    /// trust opt-in — an agent reading `--robot-docs` should learn
    /// the full surface.
    #[test]
    fn robot_docs_mentions_every_subcommand_and_trust_env() {
        let docs = robot_docs_text();
        for sub in SUBCOMMANDS {
            assert!(
                docs.contains(*sub),
                "robot-docs must mention subcommand `{sub}`"
            );
        }
        for flag in ["--help", "--version", "--capabilities", "--robot-docs"] {
            assert!(
                docs.contains(flag),
                "robot-docs must mention info flag `{flag}`"
            );
        }
        assert!(
            docs.contains("USR_GATE_TRUST_PINS"),
            "robot-docs must document the G9 trust opt-in"
        );
    }

    /// The capabilities JSON must serialize to single-line JSON (no
    /// embedded literal newlines) so the gate script can capture it
    /// as one line.
    #[test]
    fn capabilities_serializes_to_single_line_json() {
        let s = serde_json::to_string(&capabilities_json()).unwrap();
        assert!(
            !s.contains('\n'),
            "single-line JSON must not contain newlines"
        );
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["binary"], "usr-gate-rs");
    }
}
