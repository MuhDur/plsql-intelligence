#![forbid(unsafe_code)]

//! CI gate for `corpus/manifest.toml`.
//!
//! Walks `corpus/public/` (and other corpora declared by policy) and
//! refuses to pass if any committed file lacks a `[[file]]` entry, or
//! if any entry points at a missing file.
//!
//! this binary is wired into `ci.yml`
//! to block PRs that vendor external corpora without recording the
//! provenance + license + redistribution status the project promises.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

const ENFORCED_ROOTS: &[&str] = &["public"];

/// Stable schema identifier for the `--robot-json` report.
const SCHEMA_ID: &str = "corpus-license-check.report";
const SCHEMA_VERSION: u32 = 1;

/// Stable contract version for the `--capabilities` payload.
const CAPABILITIES_CONTRACT_VERSION: u32 = 1;

const DEFAULT_CORPUS_ROOT: &str = "corpus";

const VALID_FLAGS: &[&str] = &[
    "-V",
    "-h",
    "--capabilities",
    "--corpus-root",
    "--doctor",
    "--help",
    "--robot-docs",
    "--robot-json",
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
    serde_json::json!({
        "binary": "corpus-license-check",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "mode": "CI gate: every committed file under enforced corpus roots has a manifest.toml entry",
        "enforced_roots": ENFORCED_ROOTS,
        "flags": {
            "--corpus-root": format!("path to the corpus directory (default: ./{DEFAULT_CORPUS_ROOT})"),
            "--robot-json": format!("emit a stable-schema JSON report on stdout ({SCHEMA_ID} v{SCHEMA_VERSION})"),
            "--doctor": "print a summary of policy + enforced-roots + counts",
            "--capabilities": "print this machine-readable agent contract as JSON and exit",
            "--robot-docs": "print a paste-ready agent handbook and exit",
            "--version / -V": "print `corpus-license-check <version>` and exit",
            "-h / --help": "print usage and exit"
        },
        "exit_codes": {
            "0": "clean (every committed file has a manifest entry; no orphaned entries)",
            "1": "one or more violations (missing entry, orphaned entry, invalid entry, incomplete support engagement)",
            "2": "invocation error (bad args, unreadable manifest.toml)"
        },
        "stdout_contract": "stdout carries the report (human or JSON envelope under --robot-json); diagnostics go to stderr",
        "report_schema": { "id": SCHEMA_ID, "version": SCHEMA_VERSION }
    })
}

fn robot_docs_text() -> String {
    format!(
        r#"corpus-license-check agent handbook
======================================

WHAT IT DOES
  Refuses to pass a CI run when corpus/public/ (and other enforced
  roots) contains a committed file with no `[[file]]` entry in
  manifest.toml — or vice versa, a manifest entry pointing at a missing
  file. Also enforces structural fields on each entry (license,
  source_url, redistribution_allowed) and the engagement-staging
  layout under corpus/support-staging/.

CANONICAL INVOCATION
  corpus-license-check                                  # default ./{default_root}
  corpus-license-check --corpus-root /alt               # alternate root
  corpus-license-check --doctor                         # policy/counts summary
  corpus-license-check --robot-json | jq .summary       # stable JSON report

ROBOT-JSON ENVELOPE
  {{
    "schema_id":         "{schema_id}",
    "schema_version":    {schema_version},
    "corpus_root":       "...",
    "enforced_roots":    [ ... ],
    "policy":            {{ ... }},
    "manifest_entry_count": N,
    "summary":           {{ ... }},
    "missing_entries":   [ ... ],
    "missing_files":     [ ... ],
    "invalid_entries":   [ ... ],
    "incomplete_support_engagements": [ ... ],
    "clean":             true|false
  }}

FLAGS SUMMARY
  --corpus-root <path>  corpus directory (default: ./{default_root})
  --robot-json          stable-schema JSON report on stdout
  --doctor              policy + enforced-roots + counts summary
  --capabilities        machine-readable agent contract (JSON)
  --robot-docs          this handbook
  --version / -V        print corpus-license-check <version>
  -h / --help           usage summary

EXIT CODES
  0  clean
  1  one or more violations
  2  invocation error (bad args, unreadable manifest)

DISCOVERY
  corpus-license-check --capabilities    versioned agent contract
  corpus-license-check --help            human usage summary

NOTE
  corpus-license-check is a narrow CI gate; it has no `--robot-triage`
  mega-command (the surface is small enough that `--capabilities` is
  already a one-call bootstrap).
"#,
        schema_id = SCHEMA_ID,
        schema_version = SCHEMA_VERSION,
        default_root = DEFAULT_CORPUS_ROOT,
    )
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    policy: Policy,
    #[serde(default, rename = "file")]
    files: Vec<FileEntry>,
}

#[derive(Debug, Default, Deserialize)]
struct Policy {
    #[serde(default)]
    require_public_entries: bool,
    #[serde(default)]
    allow_reference_only_sources: bool,
}

#[derive(Debug, Deserialize)]
struct FileEntry {
    path: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    license: String,
    #[serde(default)]
    redistribution_allowed: Option<bool>,
    #[serde(default)]
    fetched_on: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Default)]
struct Report {
    missing_entries: Vec<String>,
    missing_files: Vec<String>,
    invalid_entries: Vec<(String, String)>,
    /// subdirectories under `corpus/support-staging/`
    /// that lack a `redaction_delta.json` or `redacted/` directory.
    /// Reported as a per-engagement-path → list-of-missing-files map.
    incomplete_support_engagements: Vec<(String, Vec<String>)>,
}

impl Report {
    fn is_clean(&self) -> bool {
        self.missing_entries.is_empty()
            && self.missing_files.is_empty()
            && self.invalid_entries.is_empty()
            && self.incomplete_support_engagements.is_empty()
    }
}

#[derive(Debug, Serialize)]
struct RobotReport<'a> {
    schema_id: &'static str,
    schema_version: u32,
    corpus_root: String,
    enforced_roots: &'static [&'static str],
    policy: RobotPolicy,
    manifest_entry_count: usize,
    summary: RobotSummary,
    missing_entries: &'a [String],
    missing_files: &'a [String],
    invalid_entries: Vec<RobotInvalidEntry<'a>>,
    incomplete_support_engagements: Vec<RobotIncompleteEngagement<'a>>,
    clean: bool,
}

#[derive(Debug, Serialize)]
struct RobotPolicy {
    require_public_entries: bool,
    allow_reference_only_sources: bool,
}

#[derive(Debug, Serialize)]
struct RobotSummary {
    missing_entries: usize,
    missing_files: usize,
    invalid_entries: usize,
    /// counter.
    incomplete_support_engagements: usize,
}

#[derive(Debug, Serialize)]
struct RobotIncompleteEngagement<'a> {
    path: &'a str,
    missing: &'a [String],
}

#[derive(Debug, Serialize)]
struct RobotInvalidEntry<'a> {
    path: &'a str,
    reason: &'a str,
}

#[derive(Debug, Default)]
struct Args {
    corpus_root: Option<PathBuf>,
    robot_json: bool,
    doctor: bool,
    capabilities: bool,
    robot_docs: bool,
    version: bool,
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1)) {
        Ok(a) => a,
        Err(err) => {
            eprintln!("error: {err}");
            // Error-path diagnostic: keep on stderr.
            let _ = print_usage_to(&mut std::io::stderr().lock());
            return ExitCode::from(2);
        }
    };

    if args.version {
        println!("corpus-license-check {}", env!("CARGO_PKG_VERSION"));
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

    let corpus_root = args
        .corpus_root
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CORPUS_ROOT));

    let manifest_path = corpus_root.join("manifest.toml");
    let manifest = match load_manifest(&manifest_path) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("error: failed to read {}: {err}", manifest_path.display());
            return ExitCode::from(2);
        }
    };

    let report = check_corpus(&corpus_root, &manifest);

    if args.robot_json {
        let robot = RobotReport {
            schema_id: SCHEMA_ID,
            schema_version: SCHEMA_VERSION,
            corpus_root: corpus_root.display().to_string(),
            enforced_roots: ENFORCED_ROOTS,
            policy: RobotPolicy {
                require_public_entries: manifest.policy.require_public_entries,
                allow_reference_only_sources: manifest.policy.allow_reference_only_sources,
            },
            manifest_entry_count: manifest.files.len(),
            summary: RobotSummary {
                missing_entries: report.missing_entries.len(),
                missing_files: report.missing_files.len(),
                invalid_entries: report.invalid_entries.len(),
                incomplete_support_engagements: report.incomplete_support_engagements.len(),
            },
            missing_entries: &report.missing_entries,
            missing_files: &report.missing_files,
            invalid_entries: report
                .invalid_entries
                .iter()
                .map(|(path, reason)| RobotInvalidEntry { path, reason })
                .collect(),
            incomplete_support_engagements: report
                .incomplete_support_engagements
                .iter()
                .map(|(path, missing)| RobotIncompleteEngagement {
                    path,
                    missing: missing.as_slice(),
                })
                .collect(),
            clean: report.is_clean(),
        };
        let json = serde_json::to_string_pretty(&robot).expect("RobotReport serializes cleanly");
        println!("{json}");
    } else if args.doctor {
        print_doctor(&corpus_root, &manifest, &report);
    } else if report.is_clean() {
        println!(
            "corpus-license-check: ok ({} entries under {} enforced root(s))",
            manifest.files.len(),
            ENFORCED_ROOTS.len()
        );
    } else {
        print_report(&report);
    }

    if report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut iter = args.into_iter();
    let mut out = Args::default();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--corpus-root" => match iter.next() {
                Some(value) => out.corpus_root = Some(PathBuf::from(value)),
                None => return Err("--corpus-root requires a path argument".into()),
            },
            "--robot-json" => out.robot_json = true,
            "--doctor" => out.doctor = true,
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
        "usage: corpus-license-check [--corpus-root <path>] [--robot-json] [--doctor]"
    )?;
    writeln!(w)?;
    writeln!(w, "Enforces that every committed file under enforced roots")?;
    writeln!(w, "(currently: {ENFORCED_ROOTS:?}) has a `[[file]]` entry in")?;
    writeln!(w, "manifest.toml.")?;
    writeln!(w)?;
    writeln!(w, "Flags:")?;
    writeln!(
        w,
        "  --corpus-root <path>  Path to the corpus directory (default: ./{DEFAULT_CORPUS_ROOT})"
    )?;
    writeln!(w, "  --robot-json          Emit a stable-schema JSON report to stdout")?;
    writeln!(
        w,
        "                        (schema_id={SCHEMA_ID}, schema_version={SCHEMA_VERSION})"
    )?;
    writeln!(
        w,
        "  --doctor              Print a summary of policy + enforced-roots + counts"
    )?;
    writeln!(
        w,
        "  --capabilities        Print the machine-readable agent contract as JSON and exit"
    )?;
    writeln!(w, "  --robot-docs          Print a paste-ready agent handbook and exit")?;
    writeln!(w, "  -V, --version         Print corpus-license-check <version> and exit")?;
    writeln!(w)?;
    writeln!(w, "Exit codes:")?;
    writeln!(w, "  0  clean")?;
    writeln!(w, "  1  one or more violations")?;
    writeln!(w, "  2  invocation error (bad args, unreadable manifest, etc.)")?;
    Ok(())
}

fn print_doctor(corpus_root: &Path, manifest: &Manifest, report: &Report) {
    println!("corpus-license-check doctor");
    println!("  corpus_root           {}", corpus_root.display());
    println!("  enforced_roots        {ENFORCED_ROOTS:?}");
    println!(
        "  require_public_entries        {}",
        manifest.policy.require_public_entries
    );
    println!(
        "  allow_reference_only_sources  {}",
        manifest.policy.allow_reference_only_sources
    );
    println!("  manifest_entries       {}", manifest.files.len());
    println!("  missing_entries        {}", report.missing_entries.len());
    println!("  missing_files          {}", report.missing_files.len());
    println!("  invalid_entries        {}", report.invalid_entries.len());
    println!(
        "  status                {}",
        if report.is_clean() { "ok" } else { "FAIL" }
    );
    if !report.is_clean() {
        println!();
        print_report(report);
    }
}

fn load_manifest(path: &Path) -> Result<Manifest, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let manifest: Manifest = toml::from_str(&text)?;
    Ok(manifest)
}

fn check_corpus(corpus_root: &Path, manifest: &Manifest) -> Report {
    let mut report = Report::default();

    let manifest_by_path: BTreeMap<String, &FileEntry> = manifest
        .files
        .iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();

    let manifest_paths: BTreeSet<String> = manifest_by_path.keys().cloned().collect();

    for (path, entry) in &manifest_by_path {
        if let Err(reason) = validate_entry(entry, manifest.policy.allow_reference_only_sources) {
            report.invalid_entries.push((path.clone(), reason));
        }
    }

    let enforced = should_enforce_public_entries(manifest);
    let mut discovered: BTreeSet<String> = BTreeSet::new();

    for root in ENFORCED_ROOTS {
        let dir = corpus_root.join(root);
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(&dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if is_ignored(path) {
                continue;
            }
            let Ok(rel) = path.strip_prefix(corpus_root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            discovered.insert(rel.clone());
            if enforced && !manifest_paths.contains(&rel) {
                report.missing_entries.push(rel);
            }
        }
    }

    for entry in manifest.files.iter() {
        let abs = corpus_root.join(&entry.path);
        let in_enforced_root = ENFORCED_ROOTS.iter().any(|r| entry.path.starts_with(r));
        let reference_only = entry.source.as_deref() == Some("reference-only");
        if !in_enforced_root || reference_only {
            continue;
        }
        if !abs.exists() {
            report.missing_files.push(entry.path.clone());
        }
    }

    // PLSQL-SUPPORT-015: walk corpus/support-staging/ and require
    // every engagement subdirectory to carry redaction_delta.json +
    // a redacted/ directory.
    let staging = corpus_root.join("support-staging");
    if staging.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&staging) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                let mut missing = Vec::new();
                if !path.join("redaction_delta.json").is_file() {
                    missing.push("redaction_delta.json".into());
                }
                if !path.join("redacted").is_dir() {
                    missing.push("redacted/".into());
                }
                if !missing.is_empty() {
                    report
                        .incomplete_support_engagements
                        .push((format!("support-staging/{name}"), missing));
                }
            }
        }
    }

    report
}

fn is_ignored(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".gitkeep") | Some(".DS_Store")
    )
}

fn should_enforce_public_entries(manifest: &Manifest) -> bool {
    manifest.policy.require_public_entries
}

fn validate_entry(entry: &FileEntry, allow_reference_only_sources: bool) -> Result<(), String> {
    if entry.license.trim().is_empty() {
        return Err("license field is empty".into());
    }
    if entry.path.trim().is_empty() {
        return Err("path field is empty".into());
    }
    let reference_only = entry.source.as_deref() == Some("reference-only");
    if reference_only {
        if !allow_reference_only_sources {
            return Err("reference-only sources disallowed by policy".into());
        }
        if entry.source_url.is_none() {
            return Err("reference-only entry must declare source_url".into());
        }
    }
    let is_public_path = entry.path.starts_with("public/");
    if is_public_path
        && entry.source_url.is_none()
        && entry.source.as_deref() != Some("synthetic")
        && !reference_only
    {
        return Err("public entries must declare source_url or source = \"synthetic\"".into());
    }
    if is_public_path && entry.redistribution_allowed.is_none() && !reference_only {
        return Err("public entries must declare redistribution_allowed".into());
    }
    if entry.fetched_on.is_some() || entry.notes.is_some() {
        // documented optional fields; no validation, but ensure they parsed.
    }
    Ok(())
}

fn print_report(report: &Report) {
    eprintln!("corpus-license-check: violations detected");
    if !report.missing_entries.is_empty() {
        eprintln!();
        eprintln!(
            "  Files present under enforced roots but missing manifest entries ({}):",
            report.missing_entries.len()
        );
        for path in &report.missing_entries {
            eprintln!("    + {path}");
        }
    }
    if !report.missing_files.is_empty() {
        eprintln!();
        eprintln!(
            "  Manifest entries pointing at missing files ({}):",
            report.missing_files.len()
        );
        for path in &report.missing_files {
            eprintln!("    - {path}");
        }
    }
    if !report.invalid_entries.is_empty() {
        eprintln!();
        eprintln!(
            "  Manifest entries with invalid fields ({}):",
            report.invalid_entries.len()
        );
        for (path, reason) in &report.invalid_entries {
            eprintln!("    ! {path}: {reason}");
        }
    }
    if !report.incomplete_support_engagements.is_empty() {
        eprintln!();
        eprintln!(
            "  Support-engagement staging dirs missing required files ({}):",
            report.incomplete_support_engagements.len()
        );
        for (path, missing) in &report.incomplete_support_engagements {
            eprintln!("    ! {path}: missing {missing:?}");
        }
        eprintln!("    See corpus/support-staging/README.md (PLSQL-SUPPORT-015) for the layout.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(path).unwrap();
        file.write_all(body.as_bytes()).unwrap();
    }

    fn temp_corpus(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "corpus-license-check-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn manifest_for(toml_body: &str) -> Manifest {
        toml::from_str(toml_body).expect("manifest parses")
    }

    #[test]
    fn manifest_round_trip_parses() {
        let m = manifest_for(
            r#"
schema_version = 1

[policy]
require_public_entries = true
allow_reference_only_sources = true

[[file]]
path = "public/hr/create.sql"
source_url = "https://example.test/hr"
license = "UPL-1.0"
redistribution_allowed = true
fetched_on = "2026-05-11"
notes = "Oracle HR vendored"
"#,
        );
        assert!(m.policy.require_public_entries);
        assert_eq!(m.files.len(), 1);
        assert_eq!(m.files[0].path, "public/hr/create.sql");
        assert_eq!(m.files[0].license, "UPL-1.0");
    }

    #[test]
    fn validate_entry_accepts_well_formed_public_entry() {
        let entry = FileEntry {
            path: "public/x.sql".into(),
            source: None,
            source_url: Some("https://example.test/x".into()),
            license: "UPL-1.0".into(),
            redistribution_allowed: Some(true),
            fetched_on: None,
            notes: None,
        };
        assert!(validate_entry(&entry, true).is_ok());
    }

    #[test]
    fn validate_entry_rejects_public_entry_without_source() {
        let entry = FileEntry {
            path: "public/x.sql".into(),
            source: None,
            source_url: None,
            license: "UPL-1.0".into(),
            redistribution_allowed: Some(true),
            fetched_on: None,
            notes: None,
        };
        let err = validate_entry(&entry, true).unwrap_err();
        assert!(err.contains("source_url"));
    }

    #[test]
    fn validate_entry_rejects_public_entry_without_redistribution_field() {
        let entry = FileEntry {
            path: "public/x.sql".into(),
            source: None,
            source_url: Some("https://example.test/x".into()),
            license: "UPL-1.0".into(),
            redistribution_allowed: None,
            fetched_on: None,
            notes: None,
        };
        let err = validate_entry(&entry, true).unwrap_err();
        assert!(err.contains("redistribution_allowed"));
    }

    #[test]
    fn validate_entry_rejects_empty_license() {
        let entry = FileEntry {
            path: "public/x.sql".into(),
            source: None,
            source_url: Some("https://example.test/x".into()),
            license: "   ".into(),
            redistribution_allowed: Some(true),
            fetched_on: None,
            notes: None,
        };
        let err = validate_entry(&entry, true).unwrap_err();
        assert!(err.contains("license"));
    }

    #[test]
    fn validate_entry_accepts_synthetic_source() {
        let entry = FileEntry {
            path: "synthetic/x.sql".into(),
            source: Some("synthetic".into()),
            source_url: None,
            license: "Apache-2.0 OR MIT".into(),
            redistribution_allowed: Some(true),
            fetched_on: None,
            notes: None,
        };
        assert!(validate_entry(&entry, true).is_ok());
    }

    #[test]
    fn validate_entry_accepts_reference_only_when_allowed() {
        let entry = FileEntry {
            path: "public/restricted/code.sql".into(),
            source: Some("reference-only".into()),
            source_url: Some("https://example.test/restricted".into()),
            license: "Proprietary".into(),
            redistribution_allowed: Some(false),
            fetched_on: None,
            notes: None,
        };
        assert!(validate_entry(&entry, true).is_ok());
        assert!(validate_entry(&entry, false).is_err());
    }

    #[test]
    fn check_corpus_reports_missing_entry_for_unlisted_public_file() {
        let corpus = temp_corpus("missing-entry");
        write(
            &corpus.join("manifest.toml"),
            r#"
schema_version = 1
[policy]
require_public_entries = true
allow_reference_only_sources = true
"#,
        );
        write(&corpus.join("public/hr/create.sql"), "-- create");
        let manifest = load_manifest(&corpus.join("manifest.toml")).unwrap();
        let report = check_corpus(&corpus, &manifest);
        assert!(!report.is_clean());
        assert_eq!(report.missing_entries, vec!["public/hr/create.sql"]);
        assert!(report.missing_files.is_empty());
    }

    #[test]
    fn check_corpus_reports_missing_file_for_orphaned_entry() {
        let corpus = temp_corpus("orphan-entry");
        write(
            &corpus.join("manifest.toml"),
            r#"
schema_version = 1
[policy]
require_public_entries = true
allow_reference_only_sources = true

[[file]]
path = "public/hr/create.sql"
source_url = "https://example.test/hr"
license = "UPL-1.0"
redistribution_allowed = true
"#,
        );
        let manifest = load_manifest(&corpus.join("manifest.toml")).unwrap();
        let report = check_corpus(&corpus, &manifest);
        assert_eq!(report.missing_files, vec!["public/hr/create.sql"]);
    }

    #[test]
    fn check_corpus_passes_when_manifest_covers_public_tree() {
        let corpus = temp_corpus("clean");
        write(
            &corpus.join("manifest.toml"),
            r#"
schema_version = 1
[policy]
require_public_entries = true
allow_reference_only_sources = true

[[file]]
path = "public/hr/create.sql"
source_url = "https://example.test/hr"
license = "UPL-1.0"
redistribution_allowed = true
"#,
        );
        write(&corpus.join("public/hr/create.sql"), "-- create");
        let manifest = load_manifest(&corpus.join("manifest.toml")).unwrap();
        let report = check_corpus(&corpus, &manifest);
        assert!(report.is_clean(), "report: {report:?}");
    }

    #[test]
    fn check_corpus_ignores_gitkeep_files() {
        let corpus = temp_corpus("gitkeep");
        write(
            &corpus.join("manifest.toml"),
            r#"
schema_version = 1
[policy]
require_public_entries = true
allow_reference_only_sources = true
"#,
        );
        write(&corpus.join("public/.gitkeep"), "");
        let manifest = load_manifest(&corpus.join("manifest.toml")).unwrap();
        let report = check_corpus(&corpus, &manifest);
        assert!(report.is_clean(), "report: {report:?}");
    }

    #[test]
    fn check_corpus_skips_reference_only_entry_existence_check() {
        let corpus = temp_corpus("reference-only");
        write(
            &corpus.join("manifest.toml"),
            r#"
schema_version = 1
[policy]
require_public_entries = true
allow_reference_only_sources = true

[[file]]
path = "public/restricted/code.sql"
source = "reference-only"
source_url = "https://example.test/restricted"
license = "Proprietary"
redistribution_allowed = false
"#,
        );
        let manifest = load_manifest(&corpus.join("manifest.toml")).unwrap();
        let report = check_corpus(&corpus, &manifest);
        assert!(report.missing_files.is_empty());
        assert!(report.invalid_entries.is_empty());
    }

    #[test]
    fn project_corpus_manifest_passes_under_current_policy() {
        // Smoke test against the live repo so the binary never regresses
        // against the in-tree corpus.
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let corpus = here
            .parent()
            .and_then(Path::parent)
            .map(|p| p.join("corpus"))
            .expect("repo root reachable");
        if !corpus.join("manifest.toml").exists() {
            return;
        }
        let manifest = load_manifest(&corpus.join("manifest.toml")).unwrap();
        let report = check_corpus(&corpus, &manifest);
        assert!(
            report.is_clean(),
            "live corpus violates corpus-license-check: {report:?}"
        );
    }

    // ----------------------------------------------------------------
    // Agent-ergonomics surface (capabilities / robot-docs / typo DYM).
    // ----------------------------------------------------------------

    #[test]
    fn capabilities_contract_is_pinned() {
        let c = capabilities_json();
        assert_eq!(c["binary"], "corpus-license-check");
        assert_eq!(c["contract_version"], CAPABILITIES_CONTRACT_VERSION);
        assert_eq!(c["version"], env!("CARGO_PKG_VERSION"));
        for key in [
            "flags",
            "exit_codes",
            "stdout_contract",
            "report_schema",
            "enforced_roots",
        ] {
            assert!(c.get(key).is_some(), "capabilities missing key `{key}`");
        }
        let flags = c["flags"].as_object().unwrap();
        for required in [
            "--corpus-root",
            "--robot-json",
            "--doctor",
            "--capabilities",
            "--robot-docs",
            "--version / -V",
        ] {
            assert!(flags.contains_key(required), "missing flag `{required}`");
        }
        assert_eq!(c["report_schema"]["id"], SCHEMA_ID);
        assert!(c["exit_codes"]["1"].is_string());
    }

    #[test]
    fn capabilities_round_trips_as_single_line_json() {
        let s = serde_json::to_string(&capabilities_json()).unwrap();
        assert!(!s.contains('\n'));
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["binary"], "corpus-license-check");
    }

    #[test]
    fn robot_docs_mentions_capabilities_and_schema() {
        let docs = robot_docs_text();
        assert!(docs.contains("--capabilities"));
        assert!(docs.contains(SCHEMA_ID));
    }

    #[test]
    fn version_flag_parses_long_and_short() {
        for arg in ["--version", "-V"] {
            let parsed = parse_args(vec![arg.to_string()])
                .unwrap_or_else(|e| panic!("{arg} should parse, got: {e}"));
            assert!(parsed.version, "{arg} should set version flag");
        }
    }

    #[test]
    fn unknown_flag_suggests_near_miss() {
        let err = parse_args(vec!["--robotjson".to_string()]).unwrap_err();
        assert!(err.contains("--robot-json"), "expected DYM hint; got: {err}");
        assert!(err.contains("did you mean"));
    }

    #[test]
    fn suggest_flag_finds_obvious_typos() {
        assert_eq!(
            suggest_flag("--robotjson", VALID_FLAGS),
            Some("--robot-json")
        );
        assert_eq!(
            suggest_flag("--corpsroot", VALID_FLAGS),
            Some("--corpus-root")
        );
        assert_eq!(suggest_flag("--xyz-totally-unknown", VALID_FLAGS), None);
    }
}
