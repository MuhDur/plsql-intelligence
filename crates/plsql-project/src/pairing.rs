//! Package spec/body pair detection + wrapped-source detection
//! (PLSQL-WS-009).
//!
//! Two related responsibilities live here:
//!
//! 1. **Spec/body pairing** — `.pks` and `.pkb` files come in
//!    matched pairs. The classifier groups discovered files by
//!    package name (case-insensitive, extension-stripped basename)
//!    and emits a [`PackagePair`] for every package. The result
//!    surfaces orphan specs and orphan bodies so the operator can
//!    flag missing halves before parsing.
//! 2. **Wrapped-source detection** — Oracle's `wrap` utility
//!    ships an obfuscated form of PL/SQL source preceded by a
//!    `CREATE OR REPLACE … wrapped` header. Wrapped bodies cannot
//!    be analysed source-only; they surface as
//!    [`UnknownReason::WrappedSource`] in downstream consumers, so
//!    detecting them up front lets the engine route opacity
//!    classification consistently.
//!
//! The detector is intentionally header-shaped: it scans the first
//! ~200 bytes of each file and looks for the literal `wrapped`
//! keyword in the standard position. False-negative rate is
//! negligible because Oracle's wrap output ships a fixed header
//! shape.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — Package
//!   chapter documents the `[BODY]` distinction and the legal
//!   filenames (`.pks` / `.pkb` are convention, not enforced).
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_SOURCE.WRAPPED` marks rows produced by the wrap utility;
//!   the source-only detector below catches the same content
//!   without a live connection.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::DiscoveredFile;

/// A package's `.pks` / `.pkb` half pair. Either side may be
/// absent — the caller decides whether that's acceptable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackagePair {
    /// Lower-cased package name (extension-stripped basename).
    pub name: String,
    /// Relative path of the `.pks` half if present.
    pub spec: Option<String>,
    /// Relative path of the `.pkb` half if present.
    pub body: Option<String>,
}

impl PackagePair {
    /// True when both halves are present.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.spec.is_some() && self.body.is_some()
    }
    /// True when only the spec is present (a header-only package).
    #[must_use]
    pub fn is_orphan_spec(&self) -> bool {
        self.spec.is_some() && self.body.is_none()
    }
    /// True when only the body is present (the spec is missing
    /// from the working tree — a common sign of an incomplete
    /// checkout).
    #[must_use]
    pub fn is_orphan_body(&self) -> bool {
        self.spec.is_none() && self.body.is_some()
    }
}

/// Group `files` by package name and emit one `PackagePair` per
/// distinct name. Files that are not `.pks` / `.pkb` are skipped.
#[must_use]
pub fn classify_pairs(files: &[DiscoveredFile]) -> Vec<PackagePair> {
    let mut buckets: BTreeMap<String, PackagePair> = BTreeMap::new();
    for f in files {
        let ext = f.extension.as_str();
        if ext != "pks" && ext != "pkb" {
            continue;
        }
        let name_lower = basename(&f.relative_path);
        let entry = buckets
            .entry(name_lower.clone())
            .or_insert_with(|| PackagePair {
                name: name_lower.clone(),
                spec: None,
                body: None,
            });
        if ext == "pks" {
            entry.spec = Some(f.relative_path.clone());
        } else {
            entry.body = Some(f.relative_path.clone());
        }
    }
    buckets.into_values().collect()
}

fn basename(path: &str) -> String {
    let after_slash = path.rsplit('/').next().unwrap_or(path);
    let dot = after_slash.rfind('.').unwrap_or(after_slash.len());
    after_slash[..dot].to_ascii_lowercase()
}

/// Returns true if `content` looks like the output of Oracle's
/// `wrap` utility. The check is conservative — we look for the
/// `wrapped` keyword in the standard header position and the
/// signature whitespace shape that the wrap utility emits.
#[must_use]
pub fn looks_wrapped(content: &str) -> bool {
    // A wrapped unit's header line ends with the `wrapped` keyword,
    // e.g. `CREATE OR REPLACE PACKAGE BODY x wrapped` (wrap utility)
    // OR `PACKAGE BODY x wrapped` (dictionary / ALL_SOURCE /
    // DBMS_METADATA form — NO `CREATE`). Detect either: the header
    // line's last token is WRAPPED and the line is an object
    // declaration (carries an object-type keyword). This avoids both
    // the old false-negative (dictionary form) and false-positives
    // on the word "wrapped" in comments/identifiers.
    //
    // `str::get` keeps the scan window on a char boundary (a naive
    // `&content[..4096]` panics on multibyte source).
    let scan_window = content.get(..4096).unwrap_or(content);
    const OBJECT_KEYWORDS: [&str; 6] = [
        "PACKAGE",
        "FUNCTION",
        "PROCEDURE",
        "TRIGGER",
        "TYPE",
        "LIBRARY",
    ];
    scan_window.lines().take(64).any(|line| {
        let upper = line.trim().to_ascii_uppercase();
        let mut tokens = upper.split_whitespace();
        let Some(last) = tokens.clone().last() else {
            return false;
        };
        last == "WRAPPED" && tokens.any(|t| OBJECT_KEYWORDS.contains(&t))
    })
}

/// Read each `DiscoveredFile`'s contents from `project_root` and
/// build a side-table of wrapped paths.
pub fn detect_wrapped(project_root: &Path, files: &[DiscoveredFile]) -> Vec<String> {
    let mut out = Vec::new();
    for f in files {
        let full = project_root.join(&f.relative_path);
        let Ok(text) = std::fs::read_to_string(&full) else {
            continue;
        };
        if looks_wrapped(&text) {
            out.push(f.relative_path.clone());
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn df(rel: &str, ext: &str) -> DiscoveredFile {
        DiscoveredFile {
            relative_path: rel.into(),
            extension: ext.into(),
            size_bytes: None,
        }
    }

    #[test]
    fn matched_pks_pkb_emit_one_complete_pair() {
        let files = vec![df("pkg_a.pks", "pks"), df("pkg_a.pkb", "pkb")];
        let pairs = classify_pairs(&files);
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].is_complete());
        assert_eq!(pairs[0].name, "pkg_a");
    }

    #[test]
    fn lone_pks_is_orphan_spec() {
        let files = vec![df("pkg_only.pks", "pks")];
        let pairs = classify_pairs(&files);
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].is_orphan_spec());
    }

    #[test]
    fn lone_pkb_is_orphan_body() {
        let files = vec![df("pkg_only.pkb", "pkb")];
        let pairs = classify_pairs(&files);
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].is_orphan_body());
    }

    #[test]
    fn case_insensitive_basename_collapses_pair() {
        let files = vec![df("Pkg_Mix.pks", "pks"), df("PKG_MIX.pkb", "pkb")];
        let pairs = classify_pairs(&files);
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].is_complete());
        assert_eq!(pairs[0].name, "pkg_mix");
    }

    #[test]
    fn non_package_files_skipped() {
        let files = vec![
            df("script.sql", "sql"),
            df("view.vw", "vw"),
            df("pkg_x.pks", "pks"),
        ];
        let pairs = classify_pairs(&files);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].name, "pkg_x");
    }

    #[test]
    fn wrap_header_detected() {
        let wrapped = "CREATE OR REPLACE PACKAGE BODY pkg wrapped\nblah\nblah\n";
        assert!(looks_wrapped(wrapped));
    }

    #[test]
    fn plain_source_not_marked_wrapped() {
        let plain = "CREATE OR REPLACE PACKAGE BODY pkg AS\nBEGIN NULL; END;\n";
        assert!(!looks_wrapped(plain));
    }

    #[test]
    fn dictionary_form_wrapped_without_create_is_detected() {
        // ALL_SOURCE / DBMS_METADATA wrapped bodies have NO `CREATE`
        // — they start at the object header. Must still be flagged
        // wrapped (R13: obfuscated source -> opacity, never analyzed
        // as real PL/SQL).
        for hdr in [
            "PACKAGE BODY billing.pay wrapped\na000000\nb2\n<encoded>\n",
            "FUNCTION calc_tax wrapped\n9\n<encoded>\n",
            "TYPE BODY t wrapped\n0\nabcd\n",
            "PROCEDURE p wrapped\n1\nxyz\n",
        ] {
            assert!(looks_wrapped(hdr), "dictionary wrapped header: {hdr:?}");
        }
    }

    #[test]
    fn word_wrapped_in_comment_or_identifier_is_not_a_false_positive() {
        // The literal word "wrapped" in a comment or identifier must
        // NOT trip detection (no object-header context).
        assert!(!looks_wrapped(
            "CREATE OR REPLACE PACKAGE BODY p AS\n-- this logic is wrapped in a loop\nBEGIN NULL; END;\n"
        ));
        assert!(!looks_wrapped(
            "CREATE PACKAGE BODY p AS\n  wrapped_count NUMBER;\nBEGIN NULL; END;\n"
        ));
    }

    #[test]
    fn detect_wrapped_scans_real_files() {
        let dir = env::temp_dir().join(format!("plsql-project-wrap-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("plain.pkb"),
            "CREATE OR REPLACE PACKAGE BODY p AS BEGIN NULL; END;",
        )
        .unwrap();
        fs::write(
            dir.join("wrapped.pkb"),
            "CREATE OR REPLACE PACKAGE BODY p wrapped\nblob",
        )
        .unwrap();
        let files = vec![df("plain.pkb", "pkb"), df("wrapped.pkb", "pkb")];
        let wrapped = detect_wrapped(&dir, &files);
        assert_eq!(wrapped, vec!["wrapped.pkb"]);
    }
}
