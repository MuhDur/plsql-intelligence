#![forbid(unsafe_code)]

//! `corpus-grow` — synthetic PL/SQL generator that emits valid samples
//! from a small library of grammar-derived pattern descriptions.
//!
//! Each pattern is a self-contained template that produces one
//! deterministic .pks / .pkb / .sql file. Templates are intentionally
//! conservative — every output is parseable by the current text-
//! scanning pre-parser and exercises a single feature (CREATE PACKAGE
//! SPEC, package body with a procedure, simple view, simple trigger,
//! synonym, etc.). The output goes to `corpus/synthetic/grown/` so it
//! doesn't collide with the curated L1/L2/L3 hand-authored sets.
//!
//! This is a deterministic scaffold; randomness lives behind a fixed
//! `--seed` so test runs reproduce bit-for-bit. Future expansion
//! passes can introduce richer pattern families.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

const SCHEMA_ID: &str = "corpus-grow.manifest";
const SCHEMA_VERSION: u32 = 1;

/// Stable contract version for the `--capabilities` payload. Bump only
/// on a breaking change to the JSON shape.
const CAPABILITIES_CONTRACT_VERSION: u32 = 1;

const DEFAULT_OUTPUT_ROOT: &str = "corpus/synthetic/grown";

const VALID_FLAGS: &[&str] = &[
    "-V",
    "-h",
    "--capabilities",
    "--count",
    "--help",
    "--output",
    "--output-root",
    "--pattern",
    "--quiet",
    "--robot-docs",
    "--robot-json",
    "--seed",
    "--version",
];

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
    let patterns: Vec<&'static str> = templates().keys().copied().collect();
    serde_json::json!({
        "binary": "corpus-grow",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "mode": "generate deterministic synthetic PL/SQL samples from a fixed pattern library",
        "flags": {
            "--output / --output-root": format!("output directory (default: ./{DEFAULT_OUTPUT_ROOT})"),
            "--seed": "u64 seed (default: 1) — generation is fully deterministic",
            "--count": "files per pattern (default: 1; must be > 0)",
            "--pattern": "filter to a named pattern; repeatable; omit to emit every pattern",
            "--robot-json": format!("emit a stable-schema JSON manifest on stdout ({SCHEMA_ID} v{SCHEMA_VERSION})"),
            "--quiet": "suppress per-file lines in the human summary",
            "--capabilities": "print this machine-readable agent contract as JSON and exit",
            "--robot-docs": "print a paste-ready agent handbook and exit",
            "--version / -V": "print `corpus-grow <version>` and exit",
            "-h / --help": "print usage and exit"
        },
        "patterns": patterns,
        "exit_codes": {
            "0": "success",
            "2": "invocation error (bad args, output dir cannot be created)"
        },
        "stdout_contract": "stdout carries the manifest (human summary or JSON envelope under --robot-json); diagnostics go to stderr",
        "manifest_schema": { "id": SCHEMA_ID, "version": SCHEMA_VERSION }
    })
}

fn robot_docs_text() -> String {
    let patterns: Vec<&'static str> = templates().keys().copied().collect();
    format!(
        r#"corpus-grow agent handbook
============================

WHAT IT DOES
  Emits deterministic synthetic PL/SQL samples from a fixed library of
  grammar-derived patterns. Same seed → byte-identical output. Each
  template targets one structural feature (package spec/body, standalone
  procedure/function, synonym, view, trigger).

CANONICAL INVOCATION
  corpus-grow                                # write every pattern once (seed 1)
  corpus-grow --seed 42 --count 5            # 5 files per pattern, seed 42
  corpus-grow --pattern package_spec         # only one pattern
  corpus-grow --output /tmp/grown --quiet    # alternate destination
  corpus-grow --robot-json | jq '.generated' # stable JSON manifest

ROBOT-JSON ENVELOPE
  {{
    "schema_id":      "{schema_id}",
    "schema_version": {schema_version},
    "output_root":    "...",
    "seed":           N,
    "pattern_count":  N,
    "generated":      [ {{ "path": "...", "bytes": N, "pattern": "..." }} ]
  }}

PATTERNS
  {patterns}

FLAGS SUMMARY
  --output <dir>     output directory (default: ./{default_root})
  --seed <u64>       deterministic seed (default: 1)
  --count <N>        files per pattern (default: 1)
  --pattern <name>   filter to a pattern (repeatable)
  --robot-json       stable-schema JSON manifest on stdout
  --quiet            suppress per-file lines in human summary
  --capabilities     machine-readable agent contract (JSON)
  --robot-docs       this handbook
  --version / -V     print corpus-grow <version>
  -h / --help        usage summary

EXIT CODES
  0  success
  2  invocation error (bad args, output dir cannot be created)

DISCOVERY
  corpus-grow --capabilities    versioned agent contract
  corpus-grow --help            human usage summary

NOTE
  corpus-grow is a tiny generator; it has no `--robot-triage` mega-
  command (the surface is narrow enough that `--capabilities` is
  already a one-call bootstrap).
"#,
        schema_id = SCHEMA_ID,
        schema_version = SCHEMA_VERSION,
        default_root = DEFAULT_OUTPUT_ROOT,
        patterns = patterns.join(", "),
    )
}

type PatternFn = fn(u64) -> (String, String);
type TemplateMap = BTreeMap<&'static str, PatternFn>;

#[derive(Debug, Serialize)]
struct Manifest {
    schema_id: &'static str,
    schema_version: u32,
    output_root: String,
    seed: u64,
    pattern_count: usize,
    generated: Vec<GeneratedFile>,
}

#[derive(Debug, Serialize)]
struct GeneratedFile {
    path: String,
    bytes: usize,
    pattern: String,
}

/// Library of grammar-derived patterns. Each template uses `{n}` as a
/// deterministic ordinal so multiple invocations with the same seed
/// produce identical output.
fn templates() -> TemplateMap {
    let mut m: TemplateMap = BTreeMap::new();
    m.insert("package_spec", template_package_spec);
    m.insert("package_body", template_package_body);
    m.insert("standalone_procedure", template_standalone_procedure);
    m.insert("standalone_function", template_standalone_function);
    m.insert("synonym", template_synonym);
    m.insert("view", template_view);
    m.insert("trigger", template_trigger);
    m
}

fn template_package_spec(seed: u64) -> (String, String) {
    let body = format!(
        "CREATE OR REPLACE PACKAGE pkg_grown_{seed}\nAS\n    PROCEDURE do_thing_{seed}(p_id NUMBER);\n    FUNCTION compute_{seed}(p_x NUMBER) RETURN NUMBER;\nEND pkg_grown_{seed};\n/\n"
    );
    (format!("pkg_grown_{seed}.pks"), body)
}

fn template_package_body(seed: u64) -> (String, String) {
    let body = format!(
        "CREATE OR REPLACE PACKAGE BODY pkg_grown_{seed}\nAS\n    PROCEDURE do_thing_{seed}(p_id NUMBER)\n    IS\n    BEGIN\n        NULL;\n    END do_thing_{seed};\n\n    FUNCTION compute_{seed}(p_x NUMBER) RETURN NUMBER\n    IS\n    BEGIN\n        RETURN p_x + 1;\n    END compute_{seed};\nEND pkg_grown_{seed};\n/\n"
    );
    (format!("pkg_grown_{seed}.pkb"), body)
}

fn template_standalone_procedure(seed: u64) -> (String, String) {
    let body = format!(
        "CREATE OR REPLACE PROCEDURE proc_grown_{seed}(p_id NUMBER)\nIS\nBEGIN\n    NULL;\nEND proc_grown_{seed};\n/\n"
    );
    (format!("proc_grown_{seed}.sql"), body)
}

fn template_standalone_function(seed: u64) -> (String, String) {
    let body = format!(
        "CREATE OR REPLACE FUNCTION fn_grown_{seed}(p_x NUMBER) RETURN NUMBER\nIS\nBEGIN\n    RETURN p_x * 2;\nEND fn_grown_{seed};\n/\n"
    );
    (format!("fn_grown_{seed}.sql"), body)
}

fn template_synonym(seed: u64) -> (String, String) {
    let body = format!("CREATE OR REPLACE SYNONYM syn_grown_{seed} FOR base_table_{seed};\n");
    (format!("syn_grown_{seed}.sql"), body)
}

fn template_view(seed: u64) -> (String, String) {
    let body = format!(
        "CREATE OR REPLACE VIEW vw_grown_{seed}\nAS\nSELECT id, name FROM base_table_{seed};\n"
    );
    (format!("vw_grown_{seed}.sql"), body)
}

fn template_trigger(seed: u64) -> (String, String) {
    let body = format!(
        "CREATE OR REPLACE TRIGGER trg_grown_{seed}\nBEFORE INSERT ON base_table_{seed}\nFOR EACH ROW\nBEGIN\n    :new.id := NVL(:new.id, 0);\nEND;\n/\n"
    );
    (format!("trg_grown_{seed}.sql"), body)
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1).collect()) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("error: {msg}");
            // Error-path diagnostic: keep on stderr.
            let _ = print_usage_to(&mut std::io::stderr().lock());
            return ExitCode::from(2);
        }
    };

    if args.version {
        println!("corpus-grow {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    if args.capabilities {
        println!("{}", serde_json::to_string_pretty(&capabilities_json()).unwrap());
        return ExitCode::SUCCESS;
    }

    if args.robot_docs {
        print!("{}", robot_docs_text());
        return ExitCode::SUCCESS;
    }

    if let Err(err) = fs::create_dir_all(&args.output_root) {
        eprintln!("error: cannot create {}: {err}", args.output_root.display());
        return ExitCode::from(2);
    }

    let library = templates();
    let mut requested: Vec<&str> = if args.patterns.is_empty() {
        library.keys().copied().collect()
    } else {
        args.patterns.iter().map(String::as_str).collect()
    };
    requested.sort();
    requested.dedup();

    let mut generated: Vec<GeneratedFile> = Vec::new();
    for pattern in &requested {
        let Some(tmpl) = library.get(pattern) else {
            eprintln!("warn: unknown pattern {pattern:?}; skipping");
            continue;
        };
        // Walk `--count` deterministic ordinals starting from `seed`.
        for offset in 0..args.count {
            let n = args.seed.wrapping_add(offset);
            let (name, body) = tmpl(n);
            let dest = args.output_root.join(&name);
            if let Err(err) = fs::write(&dest, &body) {
                eprintln!("warn: write {} failed: {err}", dest.display());
                continue;
            }
            generated.push(GeneratedFile {
                path: dest.display().to_string(),
                bytes: body.len(),
                pattern: pattern.to_string(),
            });
        }
    }

    let manifest = Manifest {
        schema_id: SCHEMA_ID,
        schema_version: SCHEMA_VERSION,
        output_root: args.output_root.display().to_string(),
        seed: args.seed,
        pattern_count: requested.len(),
        generated,
    };
    if args.robot_json {
        let json = serde_json::to_string_pretty(&manifest).expect("Manifest serializes cleanly");
        println!("{json}");
    } else {
        println!(
            "corpus-grow: wrote {} files under {} (seed={}, patterns={})",
            manifest.generated.len(),
            manifest.output_root,
            manifest.seed,
            manifest.pattern_count,
        );
        if !args.quiet {
            for f in &manifest.generated {
                println!("  {} ({}B, {})", f.path, f.bytes, f.pattern);
            }
        }
    }
    ExitCode::SUCCESS
}

#[derive(Debug)]
struct Args {
    output_root: PathBuf,
    seed: u64,
    count: u64,
    patterns: Vec<String>,
    robot_json: bool,
    quiet: bool,
    capabilities: bool,
    robot_docs: bool,
    version: bool,
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut out = Args {
        output_root: PathBuf::from(DEFAULT_OUTPUT_ROOT),
        seed: 1,
        count: 1,
        patterns: Vec::new(),
        robot_json: false,
        quiet: false,
        capabilities: false,
        robot_docs: false,
        version: false,
    };
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--output" | "--output-root" => {
                out.output_root = PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--output requires a path".to_string())?,
                );
            }
            "--seed" => {
                out.seed = iter
                    .next()
                    .ok_or_else(|| "--seed requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--seed parse: {e}"))?;
            }
            "--count" => {
                out.count = iter
                    .next()
                    .ok_or_else(|| "--count requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--count parse: {e}"))?;
                if out.count == 0 {
                    return Err("--count must be > 0".to_string());
                }
            }
            "--pattern" => {
                out.patterns.push(
                    iter.next()
                        .ok_or_else(|| "--pattern requires a name".to_string())?,
                );
            }
            "--robot-json" => out.robot_json = true,
            "--quiet" => out.quiet = true,
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
        "usage: corpus-grow [--output <dir>] [--seed <u64>] [--count <N>] [--pattern <name>]... [--robot-json] [--quiet]"
    )?;
    writeln!(w)?;
    writeln!(w, "Generates synthetic PL/SQL samples from a fixed pattern library.")?;
    writeln!(w)?;
    writeln!(w, "Defaults: --output {DEFAULT_OUTPUT_ROOT}, --seed 1, --count 1.")?;
    writeln!(w)?;
    writeln!(w, "Patterns available:")?;
    for name in templates().keys() {
        writeln!(w, "  {name}")?;
    }
    writeln!(w)?;
    writeln!(w, "Pass `--pattern <name>` one or more times to filter; omit to emit")?;
    writeln!(w, "every pattern.")?;
    writeln!(w)?;
    writeln!(w, "Discovery flags:")?;
    writeln!(
        w,
        "  --capabilities    Print the machine-readable agent contract as JSON and exit"
    )?;
    writeln!(w, "  --robot-docs      Print a paste-ready agent handbook and exit")?;
    writeln!(w, "  -V, --version     Print corpus-grow <version> and exit")?;
    writeln!(w)?;
    writeln!(w, "Stable schema for the JSON manifest: {SCHEMA_ID} v{SCHEMA_VERSION}.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_produces_valid_create_statement() {
        for (name, tmpl) in templates() {
            let (path, body) = tmpl(42);
            assert!(
                path.starts_with("pkg_")
                    || path.starts_with("proc_")
                    || path.starts_with("fn_")
                    || path.starts_with("syn_")
                    || path.starts_with("vw_")
                    || path.starts_with("trg_"),
                "pattern {name} produced unexpected path {path}"
            );
            assert!(
                body.to_uppercase().contains("CREATE"),
                "pattern {name} body lacks CREATE keyword"
            );
        }
    }

    #[test]
    fn templates_are_deterministic() {
        for (_name, tmpl) in templates() {
            let a = tmpl(7);
            let b = tmpl(7);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn templates_change_with_seed() {
        for (_name, tmpl) in templates() {
            let a = tmpl(7);
            let b = tmpl(8);
            assert_ne!(a, b);
        }
    }

    #[test]
    fn write_a_pattern_then_observe_file() {
        use std::env;
        let dir = env::temp_dir().join(format!("corpus-grow-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let (name, body) = template_package_spec(11);
        let p = dir.join(name);
        fs::write(&p, &body).unwrap();
        let read_back = fs::read_to_string(&p).unwrap();
        assert!(read_back.contains("pkg_grown_11"));
    }

    // ----------------------------------------------------------------
    // Agent-ergonomics surface (capabilities / robot-docs / typo DYM).
    // ----------------------------------------------------------------

    #[test]
    fn capabilities_contract_is_pinned() {
        let c = capabilities_json();
        assert_eq!(c["binary"], "corpus-grow");
        assert_eq!(c["contract_version"], CAPABILITIES_CONTRACT_VERSION);
        assert_eq!(c["version"], env!("CARGO_PKG_VERSION"));
        for key in ["flags", "exit_codes", "stdout_contract", "manifest_schema", "patterns"] {
            assert!(c.get(key).is_some(), "capabilities missing key `{key}`");
        }
        let flags = c["flags"].as_object().unwrap();
        for required in [
            "--seed",
            "--count",
            "--robot-json",
            "--capabilities",
            "--robot-docs",
            "--version / -V",
        ] {
            assert!(flags.contains_key(required), "missing flag `{required}`");
        }
        assert!(c["patterns"].as_array().unwrap().len() >= 7);
    }

    #[test]
    fn capabilities_round_trips_as_single_line_json() {
        let s = serde_json::to_string(&capabilities_json()).unwrap();
        assert!(!s.contains('\n'));
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["binary"], "corpus-grow");
    }

    #[test]
    fn robot_docs_mentions_capabilities_and_patterns() {
        let docs = robot_docs_text();
        assert!(docs.contains("--capabilities"));
        assert!(docs.contains(SCHEMA_ID));
        assert!(docs.contains("package_spec"));
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
        assert!(err.contains("--robot-json"), "expected DYM hint; got: {err}");
        assert!(err.contains("did you mean"));
    }

    #[test]
    fn suggest_flag_finds_obvious_typos() {
        assert_eq!(
            suggest_flag("--robotjson", VALID_FLAGS),
            Some("--robot-json")
        );
        assert_eq!(suggest_flag("--patern", VALID_FLAGS), Some("--pattern"));
        assert_eq!(suggest_flag("--xyz-totally-unknown", VALID_FLAGS), None);
    }
}
