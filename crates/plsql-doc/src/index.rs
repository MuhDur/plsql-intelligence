//! Schema-index page renderer.
//!
//! Emits a Markdown + HTML index that lists every documented object in
//! a `DocSet`, grouped by kind and sorted by `object_id`. Each row links
//! to the per-object page rendered by `render_object_markdown` /
//! `render_object_html` (-005).
//!
//! Search affordances:
//!
//! - **Markdown side**: an inline ASCII filterable table with the
//!   `object_id`, kind, and summary. Downstream Docusaurus export
//!   (DOC-009) layers MDX `<SearchBar/>` on top.
//! - **HTML side**: a single client-side `<input>` + a tiny JS filter
//!   that hides `<tr>` rows whose `data-object-id` doesn't match. No
//!   build pipeline; works as a static file.
//!
//! Determinism: the index is sorted by `(kind, object_id)` so successive
//! runs over the same `DocSet` produce byte-identical output.

use std::collections::BTreeMap;

use crate::render::{render_object_html, render_object_markdown};
use crate::{DocSet, ObjectDoc};

/// Render a [`DocSet`] as a Markdown schema-index page.
///
/// Shape:
///
/// ```text
/// # <project> object index
///
/// **Total objects:** N
///
/// ## <kind>
///
/// | ID | Name | Summary |
/// |----|------|---------|
/// | `…` | … | … |
/// ```
///
/// One section per distinct `kind`, sorted alphabetically. Objects in
/// each section are sorted by `object_id`.
#[must_use]
pub fn render_schema_index_markdown(set: &DocSet, project_label: &str) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(project_label);
    out.push_str(" object index\n\n");
    out.push_str("**Total objects:** ");
    out.push_str(&set.objects.len().to_string());
    out.push_str("\n\n");

    let by_kind = group_by_kind(set);
    if by_kind.is_empty() {
        out.push_str("_No documented objects yet._\n");
        return out;
    }

    for (kind, mut objects) in by_kind {
        objects.sort_by(|a, b| a.object_id.cmp(&b.object_id));
        out.push_str("## ");
        out.push_str(&kind);
        out.push_str("\n\n");
        out.push_str("| ID | Name | Summary |\n");
        out.push_str("|----|------|---------|\n");
        for obj in &objects {
            out.push_str("| `");
            out.push_str(&obj.object_id);
            out.push_str("` | ");
            out.push_str(&obj.name);
            out.push_str(" | ");
            out.push_str(obj.summary.as_deref().unwrap_or("—"));
            out.push_str(" |\n");
        }
        out.push('\n');
    }

    out
}

/// Render a [`DocSet`] as an HTML schema-index page with a client-side
/// search input. The input filters `<tr>` rows by matching against the
/// `data-object-id` attribute; no build pipeline needed.
#[must_use]
pub fn render_schema_index_html(set: &DocSet, project_label: &str) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n");
    out.push_str("<html><head><meta charset=\"utf-8\"><title>");
    out.push_str(&html_escape(project_label));
    out.push_str(" — object index</title>");
    out.push_str("<style>table{border-collapse:collapse;width:100%;}");
    out.push_str("th,td{border:1px solid #ddd;padding:6px 10px;text-align:left;}");
    out.push_str("input[type=search]{padding:8px;width:100%;max-width:480px;margin-bottom:12px;}");
    out.push_str(".kind{margin-top:24px;}</style></head><body>\n");
    out.push_str("<h1>");
    out.push_str(&html_escape(project_label));
    out.push_str(" object index</h1>\n");
    out.push_str("<p><strong>Total objects:</strong> ");
    out.push_str(&set.objects.len().to_string());
    out.push_str("</p>\n");
    out.push_str(
        "<input type=\"search\" id=\"q\" placeholder=\"Filter by id, name, or summary…\">\n",
    );

    let by_kind = group_by_kind(set);
    if by_kind.is_empty() {
        out.push_str("<p><em>No documented objects yet.</em></p>\n");
        out.push_str("</body></html>\n");
        return out;
    }

    for (kind, mut objects) in by_kind {
        objects.sort_by(|a, b| a.object_id.cmp(&b.object_id));
        out.push_str("<h2 class=\"kind\">");
        out.push_str(&html_escape(&kind));
        out.push_str("</h2>\n<table><thead><tr><th>ID</th><th>Name</th><th>Summary</th></tr></thead><tbody>\n");
        for obj in &objects {
            out.push_str("<tr data-object-id=\"");
            out.push_str(&html_escape(&obj.object_id));
            out.push_str("\"><td><code>");
            out.push_str(&html_escape(&obj.object_id));
            out.push_str("</code></td><td>");
            out.push_str(&html_escape(&obj.name));
            out.push_str("</td><td>");
            out.push_str(&html_escape(obj.summary.as_deref().unwrap_or("—")));
            out.push_str("</td></tr>\n");
        }
        out.push_str("</tbody></table>\n");
    }

    // Tiny client-side filter — no framework, no build step.
    out.push_str("<script>\n");
    out.push_str("(function(){var q=document.getElementById('q');\n");
    out.push_str("function filter(){var needle=q.value.toLowerCase();\n");
    out.push_str("document.querySelectorAll('tbody tr').forEach(function(row){\n");
    out.push_str("var hay=row.textContent.toLowerCase();\n");
    out.push_str("row.style.display=needle===''||hay.indexOf(needle)>-1?'':'none';});}\n");
    out.push_str("q.addEventListener('input',filter);})();\n");
    out.push_str("</script>\n</body></html>\n");
    out
}

/// Group objects by `kind` (case-folded). Returns a BTreeMap so kind
/// order is deterministic.
fn group_by_kind(set: &DocSet) -> BTreeMap<String, Vec<&ObjectDoc>> {
    let mut map: BTreeMap<String, Vec<&ObjectDoc>> = BTreeMap::new();
    for obj in &set.objects {
        map.entry(obj.kind.to_lowercase()).or_default().push(obj);
    }
    map
}

/// Render the index plus every per-object page in a single Markdown
/// bundle. Convenient one-shot for the `plsql-doc --serve` preview
/// server: one render call per request, no caching
/// shenanigans needed.
#[must_use]
pub fn render_full_markdown_bundle(set: &DocSet, project_label: &str) -> String {
    let mut out = render_schema_index_markdown(set, project_label);
    out.push_str("\n---\n\n");
    let mut by_id: Vec<&ObjectDoc> = set.objects.iter().collect();
    by_id.sort_by(|a, b| a.object_id.cmp(&b.object_id));
    for obj in by_id {
        out.push_str(&render_object_markdown(obj));
        out.push_str("\n---\n\n");
    }
    out
}

/// HTML analogue of [`render_full_markdown_bundle`] — a single page
/// with the index + every per-object section inlined. Same client-side
/// search input as [`render_schema_index_html`].
#[must_use]
pub fn render_full_html_bundle(set: &DocSet, project_label: &str) -> String {
    let mut out = render_schema_index_html(set, project_label);
    // Re-insert per-object sections before the closing `</body>`.
    let split_marker = "<script>\n";
    if let Some(idx) = out.find(split_marker) {
        let mut tail = out.split_off(idx);
        let mut by_id: Vec<&ObjectDoc> = set.objects.iter().collect();
        by_id.sort_by(|a, b| a.object_id.cmp(&b.object_id));
        for obj in by_id {
            let object_html = render_object_html(obj);
            // Strip the surrounding `<!doctype>`/`<html>`/`<head>` so
            // we splice only the `<article>` block.
            if let Some(start) = object_html.find("<article>") {
                if let Some(end) = object_html.rfind("</article>") {
                    out.push_str("<hr>\n");
                    out.push_str(&object_html[start..end + "</article>".len()]);
                    out.push('\n');
                }
            }
        }
        out.push_str(&tail);
        tail.clear();
    }
    out
}

fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocComment, DocSet, ObjectDoc};

    fn obj(id: &str, name: &str, kind: &str, summary: Option<&str>) -> ObjectDoc {
        ObjectDoc {
            object_id: id.into(),
            name: name.into(),
            kind: kind.into(),
            summary: summary.map(str::to_string),
            comments: vec![DocComment {
                tag: None,
                text: "body".into(),
                source_span: None,
            }],
            source_span: None,
        }
    }

    fn set_fixture() -> DocSet {
        DocSet {
            objects: vec![
                obj(
                    "billing.invoices",
                    "INVOICES",
                    "table",
                    Some("Invoice rows"),
                ),
                obj("billing.customers", "CUSTOMERS", "table", Some("Customers")),
                obj(
                    "billing.invoices_pkg",
                    "INVOICES_PKG",
                    "package",
                    Some("Invoice API"),
                ),
                obj(
                    "billing.idx_invoices_cust",
                    "IDX_INVOICES_CUST",
                    "index",
                    None,
                ),
            ],
        }
    }

    #[test]
    fn schema_index_markdown_groups_by_kind_and_sorts() {
        let md = render_schema_index_markdown(&set_fixture(), "billing");
        assert!(md.starts_with("# billing object index\n\n"));
        assert!(md.contains("**Total objects:** 4"));
        // Sections sorted alphabetically by kind.
        let idx_index = md.find("## index").unwrap();
        let pkg_index = md.find("## package").unwrap();
        let tbl_index = md.find("## table").unwrap();
        assert!(idx_index < pkg_index);
        assert!(pkg_index < tbl_index);
        // Within a kind, sorted by object_id.
        let cust = md.find("billing.customers").unwrap();
        let inv = md.find("billing.invoices`").unwrap();
        assert!(cust < inv);
    }

    #[test]
    fn schema_index_markdown_handles_empty_set() {
        let md = render_schema_index_markdown(&DocSet::default(), "empty");
        assert!(md.contains("**Total objects:** 0"));
        assert!(md.contains("_No documented objects yet._"));
    }

    #[test]
    fn schema_index_markdown_is_deterministic() {
        let s = set_fixture();
        let a = render_schema_index_markdown(&s, "x");
        let b = render_schema_index_markdown(&s, "x");
        assert_eq!(a, b);
    }

    #[test]
    fn schema_index_html_carries_search_affordance() {
        let html = render_schema_index_html(&set_fixture(), "billing");
        assert!(html.contains("<title>billing — object index</title>"));
        assert!(html.contains("<input type=\"search\""));
        assert!(html.contains("data-object-id=\"billing.invoices\""));
        // JS filter present.
        assert!(html.contains("addEventListener('input',filter)"));
        assert!(html.ends_with("</body></html>\n"));
    }

    #[test]
    fn schema_index_html_escapes_project_label() {
        let html = render_schema_index_html(&DocSet::default(), "<inj>");
        assert!(html.contains("&lt;inj&gt;"));
        assert!(!html.contains("<inj>"));
    }

    #[test]
    fn full_markdown_bundle_includes_every_object_page() {
        let md = render_full_markdown_bundle(&set_fixture(), "billing");
        // Top section: index.
        assert!(md.contains("# billing object index"));
        // Per-object sections in object_id order, separated by ---.
        assert!(md.contains("\n---\n"));
        assert!(md.contains("# CUSTOMERS"));
        assert!(md.contains("# IDX_INVOICES_CUST"));
        assert!(md.contains("# INVOICES"));
        assert!(md.contains("# INVOICES_PKG"));
    }

    #[test]
    fn full_html_bundle_inlines_each_article() {
        let html = render_full_html_bundle(&set_fixture(), "billing");
        // Index + 4 articles after it, hr-separated.
        let article_count = html.matches("<article>").count();
        assert_eq!(article_count, 4);
        let hr_count = html.matches("<hr>").count();
        assert_eq!(hr_count, 4);
    }
}
