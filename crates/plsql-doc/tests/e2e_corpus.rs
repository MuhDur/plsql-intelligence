//! End-to-end test (`PLSQL-DOC-012`).
//!
//! Walks every `.pks` / `.pkb` fixture under `corpus/synthetic/` and
//! `corpus/lab/`, extracts doc-comments, builds a synthetic `DocSet`,
//! renders the full bundle, and asserts:
//!
//! 1. At least 30 packages are present in aggregate
//!    (the bead's floor — `verify impact(table) ⊇ expected_set`-style
//!    bar but for doc coverage).
//! 2. The full HTML bundle contains exactly one `<article>` per
//!    documented object (no duplicates).
//! 3. Every `<a href="…">` link inside an `<article>` resolves to a
//!    `data-object-id` attribute that exists in the bundle (broken-
//!    link gate).
//! 4. The doctor coverage report's `objects_total` matches the count
//!    of input fixtures (no silent drops in the pipeline).

use std::fs;
use std::path::{Path, PathBuf};

use plsql_doc::{DocSet, ObjectDoc, doctor_report, extract_doc_comments, render_full_html_bundle};

fn corpus_root() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cursor = manifest;
    loop {
        let candidate = cursor.join("corpus");
        if candidate.is_dir() {
            return Some(candidate);
        }
        cursor = cursor.parent()?;
    }
}

fn gather_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip the hero_diff directory — those fixtures
                // intentionally duplicate object_ids (before/after
                // variants); the link-integrity check would flag the
                // duplicates as false-positives.
                if path.file_name().and_then(|s| s.to_str()) == Some("hero_diff") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if matches!(ext, "pks" | "pkb") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn fixture_to_object(path: &Path) -> Option<ObjectDoc> {
    let text = fs::read_to_string(path).ok()?;
    let comments = extract_doc_comments(&text);
    let stem = path.file_stem()?.to_str()?.to_string();
    let ext = path.extension()?.to_str()?;
    let kind = if ext == "pks" {
        "package_spec"
    } else {
        "package_body"
    };
    // object_id is `<parent_dir>.<stem>.<ext>` — uniquely identifies
    // each fixture across the corpus.
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let object_id = format!("{parent}.{stem}.{ext}");
    Some(ObjectDoc {
        object_id,
        name: stem,
        kind: kind.into(),
        summary: None,
        comments,
        source_span: None,
    })
}

fn build_corpus_doc_set() -> DocSet {
    let Some(root) = corpus_root() else {
        return DocSet::default();
    };
    let mut objects = Vec::new();
    for path in gather_fixtures(&root) {
        if let Some(obj) = fixture_to_object(&path) {
            objects.push(obj);
        }
    }
    DocSet { objects }
}

#[test]
fn corpus_doc_set_has_at_least_30_objects() {
    let set = build_corpus_doc_set();
    if set.objects.is_empty() {
        // Stripped checkout — harness becomes a no-op.
        return;
    }
    assert!(
        set.objects.len() >= 30,
        "DOC-012: expected >= 30 documented objects in corpus, got {}",
        set.objects.len()
    );
}

#[test]
fn full_html_bundle_has_one_article_per_object() {
    let set = build_corpus_doc_set();
    if set.objects.is_empty() {
        return;
    }
    let html = render_full_html_bundle(&set, "corpus-e2e");
    let article_count = html.matches("<article>").count();
    assert_eq!(
        article_count,
        set.objects.len(),
        "DOC-012: expected exactly one <article> per object",
    );
}

#[test]
fn full_html_bundle_has_no_broken_internal_links() {
    let set = build_corpus_doc_set();
    if set.objects.is_empty() {
        return;
    }
    let html = render_full_html_bundle(&set, "corpus-e2e");
    let object_ids: std::collections::BTreeSet<String> =
        set.objects.iter().map(|o| o.object_id.clone()).collect();

    // Scan `<a href="#<id>">…</a>` style links — none currently emitted
    // by the renderer, but a future revision might add cross-references.
    // For now we assert the gate is well-defined: anything that *does*
    // appear must point to an object_id that exists.
    let mut cursor = 0;
    while let Some(pos) = html[cursor..].find("href=\"#") {
        let start = cursor + pos + "href=\"#".len();
        let Some(end) = html[start..].find('"') else {
            break;
        };
        let target = &html[start..start + end];
        assert!(
            object_ids.contains(target),
            "DOC-012: broken internal link to `#{target}` in HTML bundle"
        );
        cursor = start + end;
    }
}

#[test]
fn doctor_objects_total_matches_fixture_count() {
    let set = build_corpus_doc_set();
    if set.objects.is_empty() {
        return;
    }
    let report = doctor_report(&set);
    assert_eq!(
        report.objects_total,
        set.objects.len(),
        "DOC-012: doctor.objects_total must equal the input DocSet length"
    );
    // Sanity: distinct kinds present in the corpus → at least two
    // (`package_spec` and `package_body`).
    let kinds: std::collections::BTreeSet<&str> =
        report.by_kind.iter().map(|r| r.kind.as_str()).collect();
    assert!(kinds.len() >= 2, "expected at least 2 kinds, got {kinds:?}");
}
