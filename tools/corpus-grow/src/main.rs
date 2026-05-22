#![forbid(unsafe_code)]

//! `corpus-grow` — synthetic PL/SQL generator that emits valid samples
//! from a small library of grammar-derived pattern descriptions
//! (PLSQL-PARSE-018).
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
//! beads can introduce richer pattern families.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

const SCHEMA_ID: &str = "corpus-grow.manifest";
const SCHEMA_VERSION: u32 = 1;

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
            print_usage();
            return ExitCode::from(2);
        }
    };

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
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut out = Args {
        output_root: PathBuf::from("corpus/synthetic/grown"),
        seed: 1,
        count: 1,
        patterns: Vec::new(),
        robot_json: false,
        quiet: false,
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
    eprintln!(
        "usage: corpus-grow [--output <dir>] [--seed <u64>] [--count <N>] [--pattern <name>]... [--robot-json] [--quiet]"
    );
    eprintln!();
    eprintln!("Generates synthetic PL/SQL samples from a fixed pattern library.");
    eprintln!();
    eprintln!("Defaults: --output corpus/synthetic/grown, --seed 1, --count 1.");
    eprintln!();
    eprintln!("Patterns available:");
    for name in templates().keys() {
        eprintln!("  {name}");
    }
    eprintln!();
    eprintln!("Pass `--pattern <name>` one or more times to filter; omit to emit");
    eprintln!("every pattern.");
    eprintln!();
    eprintln!("Stable schema for the JSON manifest: {SCHEMA_ID} v{SCHEMA_VERSION}.");
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
}
