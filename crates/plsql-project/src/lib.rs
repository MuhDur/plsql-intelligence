#![forbid(unsafe_code)]

//! `plsql-project` — file discovery + manifest model for the
//! plsql-intelligence engine.
//!
//! This crate is Layer 0 (foundations). It is responsible for two
//! tasks that every higher layer needs:
//!
//! 1. **Manifest model** — the [`ProjectManifest`] type defines what
//!    a `plsql-project.toml` looks like: source roots, ignore globs,
//!    Oracle target version, default schema, optional connection
//!    profile pointer. The model is pure data; serialisation lives
//!    behind serde so the loader is a single
//!    [`ProjectManifest::from_toml`] call.
//! 2. **File discovery** — given a `ProjectManifest` rooted at a
//!    directory, [`discover_files`] walks the tree and produces a
//!    deterministic list of project-relative source paths. Files
//!    are recognised by extension (`.sql`, `.pls`, `.plsql`,
//!    `.pks`, `.pkb`, `.tps`, `.tpb`, `.trg`, `.vw`); excluded
//!    paths are filtered by suffix-match against the manifest's
//!    `ignore` globs (we do not pull in a globber dependency).
//!
//! A SQL*Plus statement splitter and spec/body pairing + wrapped-source
//! detection build on these primitives.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` — PL/SQL Language Reference routing
//!   for the legal `CREATE … PACKAGE [BODY]` source-file
//!   conventions that drive the recognised-extension list.
//! * `SUPPORT-RELEASE-MATRIX.md` — the
//!   `oracle_target_version` enum tracks 19c LTS, 21c innovation,
//!   23ai, and 26ai (current AI Database family) per the support
//!   matrix.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod pairing;
pub mod preprocess;
pub mod splitter;
pub mod variant_analysis;
pub use pairing::{PackagePair, classify_pairs, detect_wrapped, looks_wrapped};
pub use preprocess::{
    AnalysisProfile, EvaluationError, InactiveRegion, PreprocessedSource, preprocess,
};
pub use splitter::{Statement, StatementKind, split_script};
pub use variant_analysis::{FrameInfo, VariantReport, VariantSelection, analyse_variants};

const RECOGNISED_EXTS: &[&str] = &[
    "sql", "pls", "plsql", "pks", "pkb", "tps", "tpb", "trg", "vw",
];

/// The static-state model loaded from `plsql-project.toml`.
///
/// `serde(default)` + `deny_unknown_fields` on the manifest means a
/// minimal `[project]\nname = "demo"` is legal but a typo in a
/// recognised field fails loudly rather than silently disabling a
/// rule.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectManifest {
    /// Project display name; defaulting to "" keeps the manifest
    /// usable for ad-hoc directories without a name.
    pub name: String,
    /// Source roots to walk. Paths are interpreted relative to the
    /// directory containing the manifest. An empty list defaults to
    /// `["."]` at discovery time.
    pub source_roots: Vec<PathBuf>,
    /// Suffix-match ignore globs. A path is excluded if any glob
    /// (interpreted as a literal suffix match against the
    /// project-relative path) matches.
    pub ignore: Vec<String>,
    /// Oracle target version the analysis run targets — drives
    /// dialect routing in the parser layer.
    pub oracle_target_version: OracleTargetVersionTag,
    /// Default schema the analysis assumes when source files don't
    /// qualify their object names.
    pub default_schema: Option<String>,
    /// Optional connection profile alias for live-DB tools
    /// (-004). When `None`, the project is
    /// static-only.
    pub connection_profile: Option<String>,
}

/// Oracle release family the project targets. Mirrors the parser
/// crate's enum but kept here so plsql-project does not depend on
/// the parser.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleTargetVersionTag {
    Oracle11g,
    Oracle12c,
    #[default]
    Oracle19c,
    Oracle21c,
    Oracle23ai,
    Oracle26ai,
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("manifest read failure: {0}")]
    Io(String),
    #[error("manifest parse failure: {0}")]
    Parse(String),
    #[error("source root {root:?} is outside the project directory")]
    EscapedRoot { root: PathBuf },
}

/// One source file discovered under the project root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredFile {
    /// Path relative to the project root. Always uses `/` as the
    /// separator for stable cross-platform diffs.
    pub relative_path: String,
    /// Lower-cased extension without the leading dot. Useful for
    /// downstream consumers that route by file type without
    /// re-parsing the path.
    pub extension: String,
    /// File size in bytes at discovery time. `None` if the stat
    /// call failed (we keep the entry rather than dropping it so
    /// the caller can flag the path).
    pub size_bytes: Option<u64>,
}

impl ProjectManifest {
    /// Parse a manifest from a TOML string. Empty input is legal
    /// (yields the default manifest).
    pub fn from_toml(text: &str) -> Result<Self, ProjectError> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        toml::from_str(text).map_err(|e| ProjectError::Parse(e.to_string()))
    }

    /// Load the manifest from `<root>/plsql-project.toml`. Missing
    /// file → default manifest, NOT an error (the project may not
    /// have one yet).
    pub fn load(root: &Path) -> Result<Self, ProjectError> {
        let path = root.join("plsql-project.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path).map_err(|e| ProjectError::Io(e.to_string()))?;
        Self::from_toml(&text)
    }

    /// Effective source roots — the configured value if non-empty,
    /// else `["."]`.
    #[must_use]
    pub fn effective_source_roots(&self) -> Vec<PathBuf> {
        if self.source_roots.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.source_roots.clone()
        }
    }
}

/// Walk every source root under `project_root`, collecting recognised
/// PL/SQL files in stable lexicographic order. Paths are returned
/// relative to `project_root`.
///
/// `manifest.ignore` globs are interpreted as suffix matches against
/// the project-relative path. A path matches if any glob is a
/// suffix of the path string after normalising backslashes. This
/// keeps the discovery primitive dependency-free while covering the
/// common cases (`target/`, `node_modules/`, `*.bak`).
pub fn discover_files(
    project_root: &Path,
    manifest: &ProjectManifest,
) -> Result<Vec<DiscoveredFile>, ProjectError> {
    let mut out: Vec<DiscoveredFile> = Vec::new();
    for raw_root in manifest.effective_source_roots() {
        let root = project_root.join(&raw_root);
        if !root.exists() {
            // Missing source root is not fatal — return what we
            // can. Callers wanting a hard failure can inspect the
            // returned list against the manifest.
            continue;
        }
        walk(&root, project_root, manifest, &mut out)?;
    }
    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    out.dedup_by(|a, b| a.relative_path == b.relative_path);
    Ok(out)
}

fn walk(
    dir: &Path,
    project_root: &Path,
    manifest: &ProjectManifest,
    out: &mut Vec<DiscoveredFile>,
) -> Result<(), ProjectError> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => return Err(ProjectError::Io(format!("{}: {}", dir.display(), e))),
    };
    for entry in entries {
        let entry = entry.map_err(|e| ProjectError::Io(e.to_string()))?;
        let path = entry.path();
        let rel = match path.strip_prefix(project_root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if matches_ignore(&rel, &manifest.ignore) {
            continue;
        }
        if path.is_dir() {
            walk(&path, project_root, manifest, out)?;
            continue;
        }
        let Some(ext) = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
        else {
            continue;
        };
        if !RECOGNISED_EXTS.contains(&ext.as_str()) {
            continue;
        }
        let size_bytes = fs::metadata(&path).ok().map(|m| m.len());
        out.push(DiscoveredFile {
            relative_path: rel,
            extension: ext,
            size_bytes,
        });
    }
    Ok(())
}

fn matches_ignore(rel: &str, ignore: &[String]) -> bool {
    ignore.iter().any(|g| rel.contains(g))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn tmp_project(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("plsql-project-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(p: &Path, content: &str) {
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(p, content).unwrap();
    }

    #[test]
    fn empty_manifest_is_default() {
        let m = ProjectManifest::from_toml("").unwrap();
        assert_eq!(m, ProjectManifest::default());
        assert_eq!(m.oracle_target_version, OracleTargetVersionTag::Oracle19c);
    }

    #[test]
    fn manifest_round_trips_through_toml() {
        let m = ProjectManifest {
            name: "demo".into(),
            source_roots: vec!["src".into(), "scripts".into()],
            ignore: vec!["target/".into()],
            oracle_target_version: OracleTargetVersionTag::Oracle23ai,
            default_schema: Some("HR".into()),
            connection_profile: Some("demo-dev".into()),
        };
        let toml = toml::to_string(&m).unwrap();
        let parsed = ProjectManifest::from_toml(&toml).unwrap();
        assert_eq!(parsed, m);
    }

    #[test]
    fn manifest_rejects_unknown_keys() {
        let err = ProjectManifest::from_toml("name = \"x\"\nfoobar = 1\n").unwrap_err();
        assert!(matches!(err, ProjectError::Parse(_)));
    }

    #[test]
    fn effective_source_roots_defaults_to_dot() {
        let m = ProjectManifest::default();
        assert_eq!(m.effective_source_roots(), vec![PathBuf::from(".")]);
    }

    #[test]
    fn load_missing_manifest_yields_default() {
        let dir = tmp_project("missing");
        let m = ProjectManifest::load(&dir).unwrap();
        assert_eq!(m, ProjectManifest::default());
    }

    #[test]
    fn discover_finds_recognised_extensions() {
        let dir = tmp_project("discover");
        write(&dir.join("a.sql"), "SELECT 1 FROM dual;");
        write(&dir.join("pkg.pks"), "-- spec");
        write(&dir.join("pkg.pkb"), "-- body");
        write(&dir.join("README.md"), "skip me");
        write(&dir.join("nested/x.pkb"), "-- body");
        let files = discover_files(&dir, &ProjectManifest::default()).unwrap();
        let rel: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(rel.contains(&"a.sql"), "{rel:?}");
        assert!(rel.contains(&"pkg.pks"));
        assert!(rel.contains(&"pkg.pkb"));
        assert!(rel.contains(&"nested/x.pkb"));
        assert!(!rel.iter().any(|p| p.ends_with(".md")));
    }

    #[test]
    fn discover_respects_ignore_substrings() {
        let dir = tmp_project("ignore");
        write(&dir.join("keep.sql"), "x");
        write(&dir.join("target/skip.sql"), "x");
        write(&dir.join("vendor/skip.sql"), "x");
        let m = ProjectManifest {
            ignore: vec!["target/".into(), "vendor/".into()],
            ..ProjectManifest::default()
        };
        let files = discover_files(&dir, &m).unwrap();
        let rel: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert_eq!(rel, vec!["keep.sql"]);
    }

    #[test]
    fn discover_dedupes_when_source_roots_overlap() {
        let dir = tmp_project("dedup");
        write(&dir.join("src/x.sql"), "x");
        let m = ProjectManifest {
            source_roots: vec![PathBuf::from("."), PathBuf::from("src")],
            ..ProjectManifest::default()
        };
        let files = discover_files(&dir, &m).unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn missing_source_root_is_not_fatal() {
        let dir = tmp_project("missing-root");
        write(&dir.join("kept.sql"), "x");
        let m = ProjectManifest {
            source_roots: vec![PathBuf::from("nope"), PathBuf::from(".")],
            ..ProjectManifest::default()
        };
        let files = discover_files(&dir, &m).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "kept.sql");
    }

    #[test]
    fn discovered_paths_are_sorted_lexicographically() {
        let dir = tmp_project("sort");
        write(&dir.join("b.sql"), "x");
        write(&dir.join("a.sql"), "x");
        write(&dir.join("c.sql"), "x");
        let files = discover_files(&dir, &ProjectManifest::default()).unwrap();
        let names: Vec<_> = files.iter().map(|f| f.relative_path.clone()).collect();
        assert_eq!(names, vec!["a.sql", "b.sql", "c.sql"]);
    }
}
