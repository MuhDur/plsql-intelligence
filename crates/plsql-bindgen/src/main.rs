#![forbid(unsafe_code)]

//! `plsql-bindgen` CLI — turns a `BindingPlan` (JSON on stdin or
//! from `--input <path>`) into a generated Rust source file.
//!
//! Flags:
//!
//! * `--input <path>` — path to a JSON `BindingPlan`. Defaults to
//!   stdin when omitted.
//! * `--package <name>` — override the plan's `package_name` (the
//!   wrapper module's display name in the emitted header
//!   comment). Optional.
//! * `--output <path>` — write the generated Rust to this file.
//!   Defaults to stdout.
//! * `--target rust` — target language. Currently the only
//!   supported value; the flag is accepted so the CLI surface is
//!   stable for the future `--target c-h` / `--target wasm-bind`
//!   work.
//! * `--robot-json` — emit a JSON envelope `{ schema_id,
//!   schema_version, package_id, generated_bytes }` instead of
//!   the bare Rust source. Used by the CI gate so it can audit
//!   the run without re-parsing the Rust.
//! * `--capabilities` — emit the machine-readable agent contract
//!   as JSON and exit.
//! * `--robot-docs` — print a paste-ready agent handbook and exit.
//! * `-h` / `--help` — print usage.
//!
//! Exit codes: `0` success; `1` runtime failure (read / parse /
//! emit); `2` invocation error (bad flags).
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference routing —
//!   the wrapper-emission contract follows the PL/SQL parameter-
//!   mode semantics (IN / OUT / IN OUT) anchored there.
//! * `LOW-LEVEL-CATALOGS.md` — the per-routine binding plan is
//!   derived from `ALL_PROCEDURES` / `ALL_ARGUMENTS` server-side
//!   (`live-db` feature path) or from the parser layer
//!   (source-only path); the CLI is agnostic to which.

use std::io::{Read, Write};
use std::process::ExitCode;

use plsql_bindgen::{BindingPlan, emit_wrappers};
use serde::Serialize;

const ENVELOPE_SCHEMA_ID: &str = "plsql.bindgen.emit_envelope";
const ENVELOPE_SCHEMA_VERSION: u32 = 1;

/// Stable contract version for the `--capabilities` payload. Bump only on a
/// breaking change to the JSON shape; a pinned regression test
/// (`capabilities_contract_is_pinned`) fails if the shape drifts without
/// this being bumped.
const CAPABILITIES_CONTRACT_VERSION: u32 = 1;

/// Sorted list of all valid flag names (used in error messages).
const VALID_FLAGS: &[&str] = &[
    "-V",
    "-h",
    "--capabilities",
    "--help",
    "--input",
    "--output",
    "--package",
    "--robot-docs",
    "--robot-json",
    "--target",
    "--version",
];

#[derive(Debug, Serialize)]
struct EmitEnvelope {
    schema_id: &'static str,
    schema_version: u32,
    package_id: String,
    package_name: String,
    routines: usize,
    generated_bytes: usize,
}

/// Build the `--capabilities` contract document. Factored out of the
/// flag handler so the schema can be pinned by a unit test without
/// spawning the binary (Axiom 17 — every contract surface has a
/// drift-guard test).
fn capabilities_json() -> serde_json::Value {
    serde_json::json!({
        "binary": "plsql-bindgen",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "mode": "stdin/--input BindingPlan JSON → Rust or --robot-json envelope",
        "flags": {
            "--input": "path to a JSON BindingPlan; defaults to stdin",
            "--output": "write generated Rust to this file; defaults to stdout",
            "--package": "override the plan's package_name (display only)",
            "--target": "target language; only \"rust\" is currently supported",
            "--robot-json": "emit plsql.bindgen.emit_envelope v1 JSON instead of bare Rust",
            "--capabilities": "print this machine-readable contract and exit",
            "--robot-docs": "print a paste-ready agent handbook and exit",
            "-h / --help": "print usage and exit"
        },
        "exit_codes": {
            "0": "success",
            "1": "runtime failure (read / parse / emit error)",
            "2": "invocation error (bad or unknown flags)"
        },
        "stdout_contract": "stdout is data only; all diagnostics go to stderr"
    })
}

/// Build the `--robot-docs` agent handbook text. Factored out so it is
/// unit-testable without spawning the binary.
fn robot_docs_text() -> String {
    format!(
        r#"plsql-bindgen agent handbook
==============================

WHAT IT DOES
  Reads a BindingPlan (JSON) and emits type-safe Rust FFI wrappers
  for PL/SQL packages/tables (PLSQL-BG-012).

CANONICAL INVOCATION
  Pipe a BindingPlan through stdin:
    echo '<BindingPlan JSON>' | plsql-bindgen [--output <path>]
  Or supply an explicit file:
    plsql-bindgen --input plan.json --output generated.rs

ROBOT-JSON MODE
  Add --robot-json to receive a plsql.bindgen.emit_envelope v1 envelope
  instead of the bare Rust source:
    {{ "schema_id": "plsql.bindgen.emit_envelope",
       "schema_version": 1,
       "package_id": "...",
       "package_name": "...",
       "routines": <n>,
       "generated_bytes": <n> }}

FLAGS SUMMARY
  --input <path>     BindingPlan JSON file (default: stdin)
  --output <path>    output file for generated Rust (default: stdout)
  --package <name>   override plan's package_name (display only)
  --target rust      target language; "rust" is the only current value
  --robot-json       emit emit_envelope v1 JSON instead of bare Rust
  --capabilities     machine-readable contract JSON (see below)
  --robot-docs       this handbook
  -h / --help        human usage summary

EXIT CODES
  0  success
  1  runtime failure (read / parse / emit)
  2  invocation error (bad / unknown flags)

STDOUT CONTRACT
  stdout is data only; all diagnostics go to stderr.
  An unknown flag prints the valid flag list to stderr and exits 2.

MACHINE-READABLE CONTRACT
  Run: plsql-bindgen --capabilities
  Pinned contract_version={contract_version}; bump signals a breaking change.
"#,
        contract_version = CAPABILITIES_CONTRACT_VERSION
    )
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse_args(&args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!("valid flags: {}", VALID_FLAGS.join(", "));
            eprintln!("run --capabilities for the machine-readable contract");
            return ExitCode::from(2);
        }
    };

    if parsed.version {
        println!("plsql-bindgen {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    if parsed.capabilities {
        let doc = capabilities_json();
        println!("{}", serde_json::to_string_pretty(&doc).unwrap());
        return ExitCode::SUCCESS;
    }

    if parsed.robot_docs {
        print!("{}", robot_docs_text());
        return ExitCode::SUCCESS;
    }

    if parsed.help {
        print_usage();
        return ExitCode::SUCCESS;
    }

    let json = match read_input(parsed.input.as_deref()) {
        Ok(s) => s,
        Err(err) => {
            eprintln!(
                "error: failed to read --input {}: {err}",
                parsed.input.as_deref().unwrap_or("<stdin>")
            );
            eprintln!(
                "  expected: a readable file path supplied via --input <path>, or valid data on stdin"
            );
            eprintln!("  run --capabilities for the machine-readable contract");
            return ExitCode::from(1);
        }
    };

    let mut plan: BindingPlan = match serde_json::from_str(&json) {
        Ok(p) => p,
        Err(err) => {
            eprintln!(
                "error: input from {} is not a valid BindingPlan JSON: {err}",
                parsed.input.as_deref().unwrap_or("--input <stdin>")
            );
            eprintln!("  expected: a JSON object matching the BindingPlan schema");
            eprintln!("  run --capabilities for the machine-readable contract");
            return ExitCode::from(1);
        }
    };
    if let Some(name) = parsed.package {
        plan.package_name = name;
    }

    let generated = emit_wrappers(&plan);

    let writer_result = if parsed.robot_json {
        let envelope = EmitEnvelope {
            schema_id: ENVELOPE_SCHEMA_ID,
            schema_version: ENVELOPE_SCHEMA_VERSION,
            package_id: plan.package_id.clone(),
            package_name: plan.package_name.clone(),
            routines: plan.routines.len(),
            generated_bytes: generated.len(),
        };
        let payload = serde_json::to_string_pretty(&envelope).unwrap_or_default();
        write_output(parsed.output.as_deref(), payload.as_bytes())
    } else {
        write_output(parsed.output.as_deref(), generated.as_bytes())
    };

    if let Err(err) = writer_result {
        eprintln!("error: write failed: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

#[derive(Debug, Default)]
struct ParsedArgs {
    input: Option<String>,
    output: Option<String>,
    package: Option<String>,
    target: String,
    robot_json: bool,
    capabilities: bool,
    robot_docs: bool,
    help: bool,
    version: bool,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut out = ParsedArgs {
        target: "rust".into(),
        ..ParsedArgs::default()
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => out.help = true,
            "-V" | "--version" => out.version = true,
            "--capabilities" => out.capabilities = true,
            "--robot-docs" => out.robot_docs = true,
            "--input" => {
                out.input = Some(iter.next().ok_or("--input requires a value")?.clone());
            }
            "--output" => {
                out.output = Some(iter.next().ok_or("--output requires a value")?.clone());
            }
            "--package" => {
                out.package = Some(iter.next().ok_or("--package requires a value")?.clone());
            }
            "--target" => {
                let v = iter.next().ok_or("--target requires a value")?;
                if v != "rust" {
                    return Err(format!(
                        "unsupported --target {v:?}; only \"rust\" is currently supported"
                    ));
                }
                out.target = v.clone();
            }
            "--robot-json" => out.robot_json = true,
            other => {
                return Err(format!(
                    "unknown flag {other:?}; valid flags: {}",
                    VALID_FLAGS.join(", ")
                ));
            }
        }
    }
    Ok(out)
}

fn print_usage() {
    eprintln!(
        "usage: plsql-bindgen [--input <path>] [--output <path>] [--package <name>] [--target rust] [--robot-json]"
    );
    eprintln!();
    eprintln!("Consume a `BindingPlan` (JSON) and emit Rust wrappers.");
    eprintln!();
    eprintln!("Defaults: --input stdin, --output stdout, --target rust.");
    eprintln!();
    eprintln!("Flags:");
    eprintln!("  --robot-json    Emit `plsql.bindgen.emit_envelope` v1 JSON instead of bare Rust.");
    eprintln!("  --package <n>   Override the plan's package_name (display only).");
    eprintln!("  --capabilities  Print the machine-readable agent contract as JSON and exit.");
    eprintln!("  --robot-docs    Print a paste-ready agent handbook and exit.");
    eprintln!();
    eprintln!("Exit codes: 0 ok, 1 runtime failure, 2 invocation error.");
}

fn read_input(path: Option<&str>) -> std::io::Result<String> {
    match path {
        Some(p) => std::fs::read_to_string(p),
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

fn write_output(path: Option<&str>, bytes: &[u8]) -> std::io::Result<()> {
    match path {
        Some(p) => std::fs::write(p, bytes),
        None => {
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(bytes)?;
            handle.write_all(b"\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drift-guard for the `--capabilities` agent contract (Axiom 17). If
    /// the JSON shape changes, this test must be updated AND
    /// `CAPABILITIES_CONTRACT_VERSION` bumped — that coupling is the whole
    /// point: an agent that pinned the contract should never be silently
    /// surprised by a shape change.
    #[test]
    fn capabilities_contract_is_pinned() {
        let c = capabilities_json();
        assert_eq!(c["binary"], "plsql-bindgen");
        assert_eq!(c["contract_version"], 1u32);
        assert_eq!(c["version"], env!("CARGO_PKG_VERSION"));
        for key in ["mode", "flags", "exit_codes", "stdout_contract"] {
            assert!(c.get(key).is_some(), "capabilities missing key `{key}`");
        }
        let flags = c["flags"].as_object().unwrap();
        for required in [
            "--input",
            "--output",
            "--package",
            "--target",
            "--robot-json",
            "--capabilities",
            "--robot-docs",
        ] {
            assert!(flags.contains_key(required), "missing flag `{required}`");
        }
        assert!(c["exit_codes"]["0"].is_string());
        assert!(c["exit_codes"]["1"].is_string());
        assert!(c["exit_codes"]["2"].is_string());
    }

    #[test]
    fn capabilities_is_valid_single_line_json_in_robot_mode() {
        let s = serde_json::to_string(&capabilities_json()).unwrap();
        assert!(!s.contains('\n'), "robot-json must be single-line");
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["binary"], "plsql-bindgen");
    }

    #[test]
    fn robot_docs_mentions_capabilities() {
        let docs = robot_docs_text();
        assert!(
            docs.contains("--capabilities"),
            "robot-docs must mention --capabilities"
        );
        assert!(
            docs.contains("plsql.bindgen.emit_envelope"),
            "robot-docs must document the emit_envelope schema"
        );
    }

    #[test]
    fn unknown_flag_returns_error_with_flag_list() {
        let args: Vec<String> = vec!["--bogus".to_string()];
        let result = parse_args(&args);
        assert!(result.is_err(), "unknown flag must be an error");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("--bogus"),
            "error message must name the unknown flag"
        );
        for flag in VALID_FLAGS {
            assert!(
                msg.contains(flag),
                "error message must include valid flag `{flag}`"
            );
        }
    }

    #[test]
    fn known_flags_parse_successfully() {
        let args: Vec<String> = vec![
            "--input".to_string(),
            "plan.json".to_string(),
            "--output".to_string(),
            "out.rs".to_string(),
            "--package".to_string(),
            "mypkg".to_string(),
            "--target".to_string(),
            "rust".to_string(),
            "--robot-json".to_string(),
        ];
        let result = parse_args(&args);
        assert!(result.is_ok(), "known flags must parse successfully");
        let parsed = result.unwrap();
        assert_eq!(parsed.input.as_deref(), Some("plan.json"));
        assert_eq!(parsed.output.as_deref(), Some("out.rs"));
        assert_eq!(parsed.package.as_deref(), Some("mypkg"));
        assert_eq!(parsed.target, "rust");
        assert!(parsed.robot_json);
    }

    #[test]
    fn capabilities_flag_parses() {
        let args: Vec<String> = vec!["--capabilities".to_string()];
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.capabilities);
    }

    #[test]
    fn robot_docs_flag_parses() {
        let args: Vec<String> = vec!["--robot-docs".to_string()];
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.robot_docs);
    }

    /// `--version` and `-V` are the universally expected flags for printing
    /// the binary version; both must parse successfully and set the version
    /// flag so the main loop can emit `plsql-bindgen <version>` and exit 0.
    /// Regression test for the unknown-flag rejection observed before the
    /// handler was added.
    #[test]
    fn version_flag_parses_long_and_short() {
        for arg in ["--version", "-V"] {
            let args: Vec<String> = vec![arg.to_string()];
            let parsed =
                parse_args(&args).unwrap_or_else(|e| panic!("{arg} should parse, got: {e}"));
            assert!(parsed.version, "{arg} should set version flag");
        }
    }

    /// `--version` / `-V` must appear in the published valid-flag list so the
    /// error message for genuinely-unknown flags is honest about what the CLI
    /// accepts.
    #[test]
    fn version_flags_are_in_valid_flags_list() {
        assert!(
            VALID_FLAGS.contains(&"--version"),
            "VALID_FLAGS must include --version"
        );
        assert!(VALID_FLAGS.contains(&"-V"), "VALID_FLAGS must include -V");
    }
}
