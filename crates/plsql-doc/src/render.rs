//! Object-page renderer for packages.
//!
//! Emits Markdown and HTML object pages from a [`ObjectDoc`]. The
//! Markdown variant is the primary surface — downstream Docusaurus
//! export consumes the same string. The HTML variant is a
//! deliberately minimal renderer suitable for `plsql-doc --serve`;
//! rich templating lands later.
//!
//! Conventions:
//!
//! - Output is **deterministic** — same `ObjectDoc` produces
//!   byte-identical output. Tests that diff successive runs work.
//! - HTML escaping is done at the renderer boundary; callers must NOT
//!   pre-escape.
//! - Comment blocks without a tag render as free-form Markdown
//!   paragraphs. Tagged blocks (`@param`, `@return`, etc.) render with
//!   a label + body.
//! - Empty fields are elided so the renderer doesn't emit empty
//!   `## Parameters` headings when no `@param` is present.

use crate::{DocComment, ObjectDoc};

/// Dispatch to the right renderer for the object's `kind`. Tables,
/// views, triggers, sequences, and similar SQL objects route through
/// [`render_sql_object_markdown`]; packages fall through to
/// [`render_package_markdown`].
#[must_use]
pub fn render_object_markdown(doc: &ObjectDoc) -> String {
    match doc.kind.as_str() {
        "table" | "view" | "materialized_view" | "trigger" | "sequence" | "synonym" | "index" => {
            render_sql_object_markdown(doc)
        }
        _ => render_package_markdown(doc),
    }
}

/// HTML dispatch analogue of [`render_object_markdown`].
#[must_use]
pub fn render_object_html(doc: &ObjectDoc) -> String {
    match doc.kind.as_str() {
        "table" | "view" | "materialized_view" | "trigger" | "sequence" | "synonym" | "index" => {
            render_sql_object_html(doc)
        }
        _ => render_package_html(doc),
    }
}

/// Render a SQL object (table, view, materialized view, trigger,
/// sequence, synonym, index) as a Markdown object page. Same shape
/// vocabulary as [`render_package_markdown`] but uses SQL-flavoured
/// section headings (`## Columns`, `## Indexes`, `## Constraints`,
/// `## Triggers` — surfaced from tagged comments when present).
///
#[must_use]
pub fn render_sql_object_markdown(doc: &ObjectDoc) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(&doc.name);
    out.push_str("\n\n");
    out.push_str("**Kind:** ");
    out.push_str(&doc.kind);
    out.push_str("  \n**ID:** `");
    out.push_str(&doc.object_id);
    out.push_str("`\n\n");

    if let Some(summary) = &doc.summary {
        if !summary.is_empty() {
            out.push_str("> ");
            out.push_str(summary);
            out.push_str("\n\n");
        }
    }

    let untagged: Vec<&DocComment> = doc.comments.iter().filter(|c| c.tag.is_none()).collect();
    if !untagged.is_empty() {
        out.push_str("## Description\n\n");
        for comment in &untagged {
            out.push_str(comment.text.trim());
            out.push_str("\n\n");
        }
    }

    render_tagged_list(&mut out, doc, "column", "Columns");
    render_tagged_list(&mut out, doc, "index", "Indexes");
    render_tagged_list(&mut out, doc, "constraint", "Constraints");
    render_tagged_list(&mut out, doc, "trigger", "Triggers");
    render_tagged_paragraphs(&mut out, doc, "see", "See also");

    // Any other tags fall through to a deterministic Other section.
    let known = [
        "column",
        "index",
        "constraint",
        "trigger",
        "see",
        "description",
    ];
    let mut other_tags: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| {
            c.tag
                .as_deref()
                .map(|t| !known.contains(&t))
                .unwrap_or(false)
        })
        .collect();
    other_tags.sort_by(|a, b| a.tag.cmp(&b.tag));
    if !other_tags.is_empty() {
        out.push_str("## Other tags\n\n");
        for comment in &other_tags {
            let tag = comment.tag.as_deref().unwrap_or("?");
            out.push_str("- **`@");
            out.push_str(tag);
            out.push_str("`** — ");
            out.push_str(comment.text.trim());
            out.push('\n');
        }
        out.push('\n');
    }

    out
}

/// HTML analogue of [`render_sql_object_markdown`].
#[must_use]
pub fn render_sql_object_html(doc: &ObjectDoc) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n");
    out.push_str("<html><head><meta charset=\"utf-8\"><title>");
    out.push_str(&html_escape(&doc.name));
    out.push_str("</title></head><body><article>\n");
    out.push_str("<h1>");
    out.push_str(&html_escape(&doc.name));
    out.push_str("</h1>\n<p><strong>Kind:</strong> ");
    out.push_str(&html_escape(&doc.kind));
    out.push_str("<br><strong>ID:</strong> <code>");
    out.push_str(&html_escape(&doc.object_id));
    out.push_str("</code></p>\n");
    if let Some(summary) = &doc.summary {
        if !summary.is_empty() {
            out.push_str("<blockquote>");
            out.push_str(&html_escape(summary));
            out.push_str("</blockquote>\n");
        }
    }
    let untagged: Vec<&DocComment> = doc.comments.iter().filter(|c| c.tag.is_none()).collect();
    if !untagged.is_empty() {
        out.push_str("<h2>Description</h2>\n");
        for comment in &untagged {
            out.push_str("<p>");
            out.push_str(&html_escape(comment.text.trim()));
            out.push_str("</p>\n");
        }
    }
    render_tagged_list_html(&mut out, doc, "column", "Columns");
    render_tagged_list_html(&mut out, doc, "index", "Indexes");
    render_tagged_list_html(&mut out, doc, "constraint", "Constraints");
    render_tagged_list_html(&mut out, doc, "trigger", "Triggers");
    out.push_str("</article></body></html>\n");
    out
}

fn render_tagged_list(out: &mut String, doc: &ObjectDoc, tag: &str, heading: &str) {
    let rows: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| c.tag.as_deref() == Some(tag))
        .collect();
    if rows.is_empty() {
        return;
    }
    out.push_str("## ");
    out.push_str(heading);
    out.push_str("\n\n");
    for comment in &rows {
        // For tagged list entries the first whitespace-separated token
        // is treated as the identifier (column name / index name /
        // etc.) and the remainder as descriptive prose.
        let (id, body) = split_first_token(&comment.text);
        if id.is_empty() {
            out.push_str("- ");
            out.push_str(body);
            out.push('\n');
        } else {
            out.push_str("- **`");
            out.push_str(id);
            out.push_str("`** — ");
            out.push_str(body);
            out.push('\n');
        }
    }
    out.push('\n');
}

fn render_tagged_list_html(out: &mut String, doc: &ObjectDoc, tag: &str, heading: &str) {
    let rows: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| c.tag.as_deref() == Some(tag))
        .collect();
    if rows.is_empty() {
        return;
    }
    out.push_str("<h2>");
    out.push_str(&html_escape(heading));
    out.push_str("</h2>\n<ul>\n");
    for comment in &rows {
        let (id, body) = split_first_token(&comment.text);
        out.push_str("<li>");
        if !id.is_empty() {
            out.push_str("<strong><code>");
            out.push_str(&html_escape(id));
            out.push_str("</code></strong> — ");
        }
        out.push_str(&html_escape(body));
        out.push_str("</li>\n");
    }
    out.push_str("</ul>\n");
}

fn render_tagged_paragraphs(out: &mut String, doc: &ObjectDoc, tag: &str, heading: &str) {
    let rows: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| c.tag.as_deref() == Some(tag))
        .collect();
    if rows.is_empty() {
        return;
    }
    out.push_str("## ");
    out.push_str(heading);
    out.push_str("\n\n");
    for comment in &rows {
        out.push_str("- ");
        out.push_str(comment.text.trim());
        out.push('\n');
    }
    out.push('\n');
}

fn split_first_token(text: &str) -> (&str, &str) {
    let trimmed = text.trim();
    match trimmed.split_once(|c: char| c.is_whitespace()) {
        Some((head, rest)) => (head, rest.trim()),
        None => (trimmed, ""),
    }
}

/// Render an [`ObjectDoc`] as a Markdown object page.
///
/// Shape (deterministic, stable across calls):
///
/// ```text
/// # <name>
///
/// **Kind:** package
/// **ID:** `<schema>.<name>`
///
/// > <summary>
///
/// ## Description
///
/// <free-form body>
///
/// ## Parameters
///
/// - **`<name>`** — <text>
///
/// ## Returns
///
/// <text>
///
/// ## Raises
///
/// - <text>
///
/// ## See also
///
/// - <text>
/// ```
///
/// Sections with no matching comments are omitted entirely.
#[must_use]
pub fn render_package_markdown(doc: &ObjectDoc) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(&doc.name);
    out.push_str("\n\n");
    out.push_str("**Kind:** ");
    out.push_str(&doc.kind);
    out.push_str("  \n**ID:** `");
    out.push_str(&doc.object_id);
    out.push_str("`\n\n");

    if let Some(summary) = &doc.summary {
        if !summary.is_empty() {
            out.push_str("> ");
            out.push_str(summary);
            out.push_str("\n\n");
        }
    }

    let untagged: Vec<&DocComment> = doc.comments.iter().filter(|c| c.tag.is_none()).collect();
    let params: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| c.tag.as_deref() == Some("param"))
        .collect();
    let returns: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| c.tag.as_deref() == Some("return"))
        .collect();
    let raises: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| c.tag.as_deref() == Some("raises") || c.tag.as_deref() == Some("throws"))
        .collect();
    let see_also: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| c.tag.as_deref() == Some("see"))
        .collect();

    let mut other_tags: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| {
            !matches!(
                c.tag.as_deref(),
                None | Some("param")
                    | Some("return")
                    | Some("raises")
                    | Some("throws")
                    | Some("see")
                    | Some("description")
            )
        })
        .collect();
    // Sort for determinism.
    other_tags.sort_by(|a, b| a.tag.cmp(&b.tag));

    if !untagged.is_empty() {
        out.push_str("## Description\n\n");
        for comment in &untagged {
            out.push_str(comment.text.trim());
            out.push_str("\n\n");
        }
    }

    if !params.is_empty() {
        out.push_str("## Parameters\n\n");
        for comment in &params {
            let (name, body) = split_param_name(&comment.text);
            out.push_str("- **`");
            out.push_str(name);
            out.push_str("`** — ");
            out.push_str(body);
            out.push('\n');
        }
        out.push('\n');
    }

    if !returns.is_empty() {
        out.push_str("## Returns\n\n");
        for comment in &returns {
            out.push_str(comment.text.trim());
            out.push_str("\n\n");
        }
    }

    if !raises.is_empty() {
        out.push_str("## Raises\n\n");
        for comment in &raises {
            out.push_str("- ");
            out.push_str(comment.text.trim());
            out.push('\n');
        }
        out.push('\n');
    }

    if !see_also.is_empty() {
        out.push_str("## See also\n\n");
        for comment in &see_also {
            out.push_str("- ");
            out.push_str(comment.text.trim());
            out.push('\n');
        }
        out.push('\n');
    }

    if !other_tags.is_empty() {
        out.push_str("## Other tags\n\n");
        for comment in &other_tags {
            let tag = comment.tag.as_deref().unwrap_or("?");
            out.push_str("- **`@");
            out.push_str(tag);
            out.push_str("`** — ");
            out.push_str(comment.text.trim());
            out.push('\n');
        }
        out.push('\n');
    }

    out
}

/// Render an [`ObjectDoc`] as a minimal HTML object page. Embeds the
/// Markdown rendering inline as an `<article>` with manual HTML escape;
/// rich templating is reserved for a later pass.
#[must_use]
pub fn render_package_html(doc: &ObjectDoc) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n");
    out.push_str("<html><head><meta charset=\"utf-8\"><title>");
    out.push_str(&html_escape(&doc.name));
    out.push_str("</title></head><body><article>\n");
    out.push_str("<h1>");
    out.push_str(&html_escape(&doc.name));
    out.push_str("</h1>\n<p><strong>Kind:</strong> ");
    out.push_str(&html_escape(&doc.kind));
    out.push_str("<br><strong>ID:</strong> <code>");
    out.push_str(&html_escape(&doc.object_id));
    out.push_str("</code></p>\n");

    if let Some(summary) = &doc.summary {
        if !summary.is_empty() {
            out.push_str("<blockquote>");
            out.push_str(&html_escape(summary));
            out.push_str("</blockquote>\n");
        }
    }

    let untagged: Vec<&DocComment> = doc.comments.iter().filter(|c| c.tag.is_none()).collect();
    let params: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| c.tag.as_deref() == Some("param"))
        .collect();
    let returns: Vec<&DocComment> = doc
        .comments
        .iter()
        .filter(|c| c.tag.as_deref() == Some("return"))
        .collect();

    if !untagged.is_empty() {
        out.push_str("<h2>Description</h2>\n");
        for comment in &untagged {
            out.push_str("<p>");
            out.push_str(&html_escape(comment.text.trim()));
            out.push_str("</p>\n");
        }
    }
    if !params.is_empty() {
        out.push_str("<h2>Parameters</h2>\n<ul>\n");
        for comment in &params {
            let (name, body) = split_param_name(&comment.text);
            out.push_str("<li><strong><code>");
            out.push_str(&html_escape(name));
            out.push_str("</code></strong> — ");
            out.push_str(&html_escape(body));
            out.push_str("</li>\n");
        }
        out.push_str("</ul>\n");
    }
    if !returns.is_empty() {
        out.push_str("<h2>Returns</h2>\n");
        for comment in &returns {
            out.push_str("<p>");
            out.push_str(&html_escape(comment.text.trim()));
            out.push_str("</p>\n");
        }
    }
    out.push_str("</article></body></html>\n");
    out
}

/// Split a `@param` body into `(name, body)`. The first whitespace-
/// separated token is the parameter name; everything after is the
/// description. If the body is empty, both halves come back empty.
fn split_param_name(text: &str) -> (&str, &str) {
    let trimmed = text.trim();
    match trimmed.split_once(|c: char| c.is_whitespace()) {
        Some((name, rest)) => (name, rest.trim()),
        None => (trimmed, ""),
    }
}

/// Minimal HTML-entity escape covering the five XML-mandatory chars +
/// the common ones a doc-comment might carry. Sufficient for trusted
/// PL/SQL source; not a full XSS hardening pass.
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
    use crate::{DocComment, ObjectDoc};

    fn fixture() -> ObjectDoc {
        ObjectDoc {
            object_id: "billing.invoices_pkg".into(),
            name: "INVOICES_PKG".into(),
            kind: "package".into(),
            summary: Some("Manage invoice lifecycle.".into()),
            comments: vec![
                DocComment {
                    tag: None,
                    text: "Public API for creating and finalising invoices.".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("param".into()),
                    text: "p_customer_id The customer id; FK to customers.id".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("param".into()),
                    text: "p_amount The amount in the currency's smallest unit".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("return".into()),
                    text: "The new invoice's id.".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("raises".into()),
                    text: "INVALID_AMOUNT when p_amount <= 0".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("see".into()),
                    text: "billing.customers_pkg.create_customer".into(),
                    source_span: None,
                },
            ],
            source_span: None,
        }
    }

    #[test]
    fn markdown_has_canonical_shape() {
        let md = render_package_markdown(&fixture());
        assert!(md.starts_with("# INVOICES_PKG\n\n"));
        assert!(md.contains("**Kind:** package"));
        assert!(md.contains("**ID:** `billing.invoices_pkg`"));
        assert!(md.contains("> Manage invoice lifecycle."));
        assert!(md.contains("## Description\n\nPublic API for creating"));
        assert!(md.contains("## Parameters\n\n- **`p_customer_id`** — The customer id"));
        assert!(md.contains("- **`p_amount`** — The amount in the currency"));
        assert!(md.contains("## Returns\n\nThe new invoice's id."));
        assert!(md.contains("## Raises\n\n- INVALID_AMOUNT when"));
        assert!(md.contains("## See also\n\n- billing.customers_pkg"));
    }

    #[test]
    fn markdown_is_deterministic() {
        let doc = fixture();
        let a = render_package_markdown(&doc);
        let b = render_package_markdown(&doc);
        assert_eq!(a, b);
    }

    #[test]
    fn markdown_elides_empty_sections() {
        let doc = ObjectDoc {
            object_id: "x.y".into(),
            name: "Y".into(),
            kind: "package".into(),
            summary: None,
            comments: vec![],
            source_span: None,
        };
        let md = render_package_markdown(&doc);
        assert!(!md.contains("## Description"));
        assert!(!md.contains("## Parameters"));
        assert!(!md.contains("## Returns"));
        assert!(!md.contains("## Raises"));
        assert!(!md.contains("## See also"));
        assert!(!md.contains("## Other tags"));
    }

    #[test]
    fn markdown_handles_unknown_tags_in_other_section() {
        let doc = ObjectDoc {
            object_id: "x.y".into(),
            name: "Y".into(),
            kind: "package".into(),
            summary: None,
            comments: vec![
                DocComment {
                    tag: Some("since".into()),
                    text: "2024.1".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("deprecated".into()),
                    text: "Use NEW_API instead".into(),
                    source_span: None,
                },
            ],
            source_span: None,
        };
        let md = render_package_markdown(&doc);
        assert!(md.contains("## Other tags"));
        // Sorted by tag for determinism — `deprecated` before `since`.
        let dep_idx = md.find("@deprecated").unwrap();
        let sin_idx = md.find("@since").unwrap();
        assert!(dep_idx < sin_idx);
    }

    #[test]
    fn html_escapes_dangerous_chars() {
        let doc = ObjectDoc {
            object_id: "x.y".into(),
            name: "<script>alert(1)</script>".into(),
            kind: "package".into(),
            summary: Some("\"quoted\" & 'apos'".into()),
            comments: vec![],
            source_span: None,
        };
        let html = render_package_html(&doc);
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&quot;quoted&quot; &amp; &#x27;apos&#x27;"));
    }

    #[test]
    fn html_renders_full_shape() {
        let html = render_package_html(&fixture());
        assert!(html.starts_with("<!doctype html>\n"));
        assert!(html.contains("<h1>INVOICES_PKG</h1>"));
        assert!(html.contains("<strong>Kind:</strong> package"));
        assert!(html.contains("<code>billing.invoices_pkg</code>"));
        assert!(html.contains("<blockquote>Manage invoice lifecycle.</blockquote>"));
        assert!(html.contains("<h2>Parameters</h2>"));
        assert!(html.contains("<code>p_customer_id</code>"));
        assert!(html.contains("<h2>Returns</h2>"));
        assert!(html.ends_with("</article></body></html>\n"));
    }

    #[test]
    fn split_param_name_handles_edge_cases() {
        assert_eq!(split_param_name("  p_x value  "), ("p_x", "value"));
        assert_eq!(split_param_name("p_alone"), ("p_alone", ""));
        assert_eq!(split_param_name(""), ("", ""));
        assert_eq!(split_param_name("   "), ("", ""));
    }

    // -----------------------------------------------------------------
    // PLSQL-DOC-005 / oracle-dvj — SQL object renderer tests.
    // -----------------------------------------------------------------

    fn table_fixture() -> ObjectDoc {
        ObjectDoc {
            object_id: "billing.invoices".into(),
            name: "INVOICES".into(),
            kind: "table".into(),
            summary: Some("Invoice header rows.".into()),
            comments: vec![
                DocComment {
                    tag: None,
                    text: "One row per invoice issued.".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("column".into()),
                    text: "INVOICE_ID Primary key — surrogate from INVOICES_SEQ".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("column".into()),
                    text: "AMOUNT Net amount in the currency's smallest unit".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("index".into()),
                    text: "IDX_INVOICES_CUSTOMER BTREE(customer_id)".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("constraint".into()),
                    text: "FK_INVOICES_CUSTOMER references customers(id)".into(),
                    source_span: None,
                },
                DocComment {
                    tag: Some("trigger".into()),
                    text: "INVOICES_BIU before insert/update — audit stamping".into(),
                    source_span: None,
                },
            ],
            source_span: None,
        }
    }

    #[test]
    fn sql_object_markdown_routes_table_to_sql_renderer() {
        let md = render_object_markdown(&table_fixture());
        assert!(md.contains("# INVOICES"));
        assert!(md.contains("**Kind:** table"));
        assert!(md.contains("## Description\n\nOne row per invoice"));
        assert!(md.contains("## Columns"));
        assert!(md.contains("- **`INVOICE_ID`** — Primary key"));
        assert!(md.contains("- **`AMOUNT`** — Net amount"));
        assert!(md.contains("## Indexes"));
        assert!(md.contains("- **`IDX_INVOICES_CUSTOMER`** — BTREE"));
        assert!(md.contains("## Constraints"));
        assert!(md.contains("- **`FK_INVOICES_CUSTOMER`** — references"));
        assert!(md.contains("## Triggers"));
        assert!(md.contains("- **`INVOICES_BIU`** — before insert"));
    }

    #[test]
    fn sql_object_markdown_routes_view_through_same_renderer() {
        let mut doc = table_fixture();
        doc.kind = "view".into();
        let md = render_object_markdown(&doc);
        assert!(md.contains("**Kind:** view"));
        assert!(md.contains("## Columns"));
    }

    #[test]
    fn sql_object_dispatch_falls_back_to_package_for_unknown_kind() {
        let mut doc = table_fixture();
        doc.kind = "package".into();
        let md = render_object_markdown(&doc);
        // Package renderer doesn't emit `## Columns` — it routes
        // through `## Other tags` for `column`.
        assert!(md.contains("## Other tags"));
        assert!(!md.contains("## Columns"));
    }

    #[test]
    fn sql_object_markdown_is_deterministic() {
        let doc = table_fixture();
        let a = render_object_markdown(&doc);
        let b = render_object_markdown(&doc);
        assert_eq!(a, b);
    }

    #[test]
    fn sql_object_markdown_elides_empty_sections() {
        let bare = ObjectDoc {
            object_id: "x.y".into(),
            name: "Y".into(),
            kind: "sequence".into(),
            summary: None,
            comments: vec![],
            source_span: None,
        };
        let md = render_object_markdown(&bare);
        assert!(!md.contains("## Description"));
        assert!(!md.contains("## Columns"));
        assert!(!md.contains("## Indexes"));
        assert!(!md.contains("## Constraints"));
        assert!(!md.contains("## Triggers"));
    }

    #[test]
    fn sql_object_html_escapes_dangerous_chars_and_renders_lists() {
        let mut doc = table_fixture();
        doc.name = "<INJ>".into();
        let html = render_object_html(&doc);
        assert!(html.contains("&lt;INJ&gt;"));
        assert!(html.contains("<h2>Columns</h2>"));
        assert!(html.contains("<code>INVOICE_ID</code>"));
        assert!(html.contains("<h2>Indexes</h2>"));
        assert!(html.ends_with("</article></body></html>\n"));
    }

    #[test]
    fn split_first_token_matches_split_param_name_contract() {
        assert_eq!(split_first_token("  C1 desc  "), ("C1", "desc"));
        assert_eq!(split_first_token("C2"), ("C2", ""));
        assert_eq!(split_first_token(""), ("", ""));
    }
}
