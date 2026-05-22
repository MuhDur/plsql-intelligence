#![forbid(unsafe_code)]

//! CI gate for `corpus/manifest.toml`.
//!
//! Walks `corpus/public/` (and other corpora declared by policy) and
//! refuses to pass if any committed file lacks a `[[file]]` entry, or
//! if any entry points at a missing file.
//!
//! `PLSQL-WS-014`: this binary is wired into `ci.yml` (`PLSQL-WS-015`)
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
    /// PLSQL-SUPPORT-015: subdirectories under `corpus/support-staging/`
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
    /// PLSQL-SUPPORT-015 counter.
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
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1)) {
        Ok(a) => a,
        Err(err) => {
            eprintln!("error: {err}");
            print_usage();
            return ExitCode::from(2);
        }
    };
    let corpus_root = args.corpus_root.unwrap_or_else(|| PathBuf::from("corpus"));

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
    eprintln!("usage: corpus-license-check [--corpus-root <path>] [--robot-json] [--doctor]");
    eprintln!();
    eprintln!("Enforces that every committed file under enforced roots");
    eprintln!("(currently: {ENFORCED_ROOTS:?}) has a `[[file]]` entry in");
    eprintln!("manifest.toml.");
    eprintln!();
    eprintln!("Flags:");
    eprintln!("  --corpus-root <path>  Path to the corpus directory (default: ./corpus)");
    eprintln!("  --robot-json          Emit a stable-schema JSON report to stdout");
    eprintln!("                        (schema_id={SCHEMA_ID}, schema_version={SCHEMA_VERSION})");
    eprintln!("  --doctor              Print a summary of policy + enforced-roots + counts");
    eprintln!();
    eprintln!("Exit codes:");
    eprintln!("  0  clean");
    eprintln!("  1  one or more violations");
    eprintln!("  2  invocation error (bad args, unreadable manifest, etc.)");
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
}
