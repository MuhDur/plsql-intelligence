#![forbid(unsafe_code)]
//! `plsql-doc` — type definitions for the documentation generator (PLSQL-DOC-001).
//!
//! These are the canonical types consumed by the documentation generator and any
//! downstream report renderer. They are intentionally minimal at this layer; the
//! rendering pipeline (HTML / Markdown) and tag-parser live in dedicated modules
//! that build on top of these structures.

use serde::{Deserialize, Serialize};

pub mod doctor;
pub mod index;
pub mod mdx;
pub mod render;
pub mod serve;
pub mod svg_graph;
pub mod table_usage;

// Re-export the local HTTP preview server (PLSQL-DOC-010 / oracle-x8z).
pub use serve::serve_preview_blocking;

// Re-export the documentation-coverage doctor (PLSQL-DOC-011 / oracle-23b).
pub use doctor::{
    DocCoverageReport, DocPosture, KindCoverageRow, UNDOCUMENTED_LIST_LIMIT, doctor_report,
};

// Re-export the object-page renderers.
// `render_package_*` lands package pages (PLSQL-DOC-004 / oracle-xtp);
// `render_object_*` lands tables/views/triggers/sequences/etc.
// (PLSQL-DOC-005 / oracle-dvj) by routing on `ObjectDoc::kind`.
pub use render::{
    render_object_html, render_object_markdown, render_package_html, render_package_markdown,
};
// Re-export the schema-index renderer (PLSQL-DOC-008 / oracle-vqu).
pub use index::{
    render_full_html_bundle, render_full_markdown_bundle, render_schema_index_html,
    render_schema_index_markdown,
};
pub use mdx::{render_object_mdx, render_objects_mdx};
pub use svg_graph::{CallEdge, CallGraphView, render_call_graph_svg};
pub use table_usage::{TableUsageView, UnitUsage, render_table_usage_svg};

/// A byte-offset half-open range `[start, end)` into a source file, suitable for
/// JSON-serializable doc fixtures. Concrete file/line resolution lives upstream
/// in `plsql-core::Span`; this is a stable wire format for doc artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSpan {
    pub start: u32,
    pub end: u32,
}

/// A collection of documented Oracle objects produced by a documentation run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocSet {
    /// Every documented object in the run.
    pub objects: Vec<ObjectDoc>,
}

/// Documentation extracted for a single Oracle object (package, procedure, view, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectDoc {
    /// Stable identifier for the object (schema-qualified, lower-cased).
    pub object_id: String,
    /// Original-case object name.
    pub name: String,
    /// Object kind (e.g. `package`, `procedure`, `function`, `view`).
    pub kind: String,
    /// One-line summary extracted from the doc-comment header.
    pub summary: Option<String>,
    /// Parsed doc-comment blocks attached to this object.
    pub comments: Vec<DocComment>,
    /// Source span where the object was declared.
    pub source_span: Option<DocSpan>,
}

/// A single doc-comment tag block (e.g. `@param`, `@description`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocComment {
    /// The tag name, without the leading `@`. `None` for free-form text.
    pub tag: Option<String>,
    /// Body of the comment block.
    pub text: String,
    /// Source span of the comment block in the original PL/SQL file.
    pub source_span: Option<DocSpan>,
}

// ---------------------------------------------------------------------------
// Doc-comment lexer (DOC-002)
// ---------------------------------------------------------------------------

/// Extract doc comments from PL/SQL source text.
///
/// Recognizes two conventions:
///
/// 1. **Javadoc-style** (`/** ... */`): Block comments starting with `/**`.
///    Lines inside the block have leading `*` and whitespace stripped.
///    Emitted as a single `DocComment` with `tag: None`.
///
/// 2. **Legacy preceding-line** (`-- ...`): One or more consecutive `--` comment
///    lines immediately above a declaration keyword. Emitted as a single
///    `DocComment` with `tag: None`.
///
/// Tag parsing (`@param`, etc.) is NOT performed here — this lexer emits raw
/// text with byte-offset spans.
pub fn extract_doc_comments(source: &str) -> Vec<DocComment> {
    let mut result = Vec::new();
    let lines: Vec<(usize, usize, &str)> = source_lines(source);

    // Phase 1: Extract /** */ blocks (may span multiple lines)
    extract_javadoc_blocks(source, &lines, &mut result);

    // Phase 2: Extract -- preceding-line runs above declarations
    extract_dash_runs(&lines, &mut result);

    result
}

/// Split source into (line_start_byte, line_end_byte, line_text) tuples.
fn source_lines(source: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, ch) in source.char_indices() {
        if ch == '\n' {
            out.push((start, i, &source[start..i]));
            start = i + 1;
        }
    }
    // Last line (may not end with newline)
    if start < source.len() {
        out.push((start, source.len(), &source[start..]));
    }
    out
}

fn extract_javadoc_blocks(
    _source: &str,
    lines: &[(usize, usize, &str)],
    result: &mut Vec<DocComment>,
) {
    let mut i = 0;
    while i < lines.len() {
        let (_, _, text) = lines[i];
        let trimmed = text.trim_start();

        if trimmed.starts_with("/**") {
            let block_start = lines[i].0;
            let mut text_parts = Vec::new();
            let mut block_end = lines[i].1;
            let mut closed = false;

            // First line: text after /**
            let after_open = trimmed.strip_prefix("/**").unwrap_or(trimmed);
            let first_content = after_open
                .trim_end()
                .strip_suffix("*/")
                .unwrap_or(after_open.trim_end());
            let first_stripped = strip_doc_line(first_content);
            if !first_stripped.is_empty() {
                text_parts.push(first_stripped.to_string());
            }
            if after_open.contains("*/") {
                closed = true;
            }

            // Subsequent lines
            if !closed {
                i += 1;
                while i < lines.len() {
                    let (_, end, text) = lines[i];
                    block_end = end;
                    let line_trimmed = text.trim();

                    if let Some(before_close) = line_trimmed.strip_suffix("*/") {
                        let stripped = strip_doc_line(before_close);
                        if !stripped.is_empty() {
                            text_parts.push(stripped.to_string());
                        }

                        break;
                    }

                    let stripped = strip_doc_line(line_trimmed);
                    if !stripped.is_empty() {
                        text_parts.push(stripped.to_string());
                    }
                    i += 1;
                }
            }

            if !text_parts.is_empty() {
                result.push(DocComment {
                    tag: None,
                    text: text_parts.join("\n"),
                    source_span: Some(DocSpan {
                        start: block_start as u32,
                        end: block_end as u32,
                    }),
                });
            }
        }
        i += 1;
    }
}

fn extract_dash_runs(lines: &[(usize, usize, &str)], result: &mut Vec<DocComment>) {
    let mut i = 0;
    while i < lines.len() {
        let (_, _, text) = lines[i];
        if text.trim_start().starts_with("--") {
            // Collect the dash run
            let run_start = lines[i].0;
            let mut dash_texts = Vec::new();
            let mut run_end = lines[i].1;

            while i < lines.len() {
                let (_, end, text) = lines[i];
                let trimmed = text.trim_start();
                if !trimmed.starts_with("--") {
                    break;
                }
                let content = trimmed.strip_prefix("--").unwrap_or("").trim();
                dash_texts.push(content.to_string());
                run_end = end;
                i += 1;
            }

            // Check if next non-blank line starts a declaration
            let mut j = i;
            while j < lines.len() && lines[j].2.trim().is_empty() {
                j += 1;
            }
            if j < lines.len() && starts_declaration(lines[j].2.trim_start()) {
                let text = dash_texts.join("\n");
                if !text.is_empty() {
                    result.push(DocComment {
                        tag: None,
                        text,
                        source_span: Some(DocSpan {
                            start: run_start as u32,
                            end: run_end as u32,
                        }),
                    });
                }
            }
        } else {
            i += 1;
        }
    }
}

/// Strip leading whitespace and optional `*` from a doc-block line.
fn strip_doc_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let after_star = if let Some(rest) = trimmed.strip_prefix('*') {
        rest.trim_start()
    } else {
        trimmed
    };
    // Trim trailing whitespace
    after_star.trim_end().to_string()
}

/// Check if a line starts a PL/SQL declaration keyword.
fn starts_declaration(line: &str) -> bool {
    let upper = line.to_uppercase();
    let trimmed = upper.trim_start();
    trimmed.starts_with("CREATE")
        || trimmed.starts_with("PROCEDURE")
        || trimmed.starts_with("FUNCTION")
        || trimmed.starts_with("PACKAGE")
        || trimmed.starts_with("TYPE")
        || trimmed.starts_with("VIEW")
        || trimmed.starts_with("TRIGGER")
        || trimmed.starts_with("BEGIN")
}

// ---------------------------------------------------------------------------
// Tag parser (DOC-003)
// ---------------------------------------------------------------------------

/// Parse doc-comment text into tagged blocks.
///
/// Takes the raw text from a `DocComment` (as emitted by `extract_doc_comments`)
/// and splits it into one `DocComment` per tag. Rules:
///
/// - A line starting with `@<word>` opens a new tagged block. The word becomes
///   the `tag` field; the rest of the line is the first line of `text`.
/// - Continuation lines (next line NOT starting with `@`) append to the current
///   block, separated by newlines.
/// - Lines before the first `@` tag are emitted as a free-form block
///   (`tag: None`).
///
/// Supported tags (not validated — any `@word` is accepted):
/// `@description`, `@param`, `@returns`, `@throws`, `@example`,
/// `@deprecated`, `@see`, `@since`, `@author`.
pub fn parse_doc_tags(text: &str) -> Vec<DocComment> {
    let mut result = Vec::new();
    let mut current_tag: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim_start();

        if let Some(tag_line) = parse_tag_line(trimmed) {
            // Flush previous block
            flush_block(&mut current_tag, &mut current_lines, &mut result);

            // Start new tagged block
            current_tag = Some(tag_line.0);
            if !tag_line.1.is_empty() {
                current_lines.push(tag_line.1);
            }
        } else {
            // Continuation line (or free-form text before any tag)
            current_lines.push(line.to_string());
        }
    }

    // Flush remaining
    flush_block(&mut current_tag, &mut current_lines, &mut result);

    result
}

/// If the line starts with `@<word>`, return `Some((tag_name, rest_of_line))`.
fn parse_tag_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('@') {
        return None;
    }

    // Find end of tag word (alphanumeric + underscore + hyphen)
    let after_at = &trimmed[1..];
    let tag_end = after_at
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .unwrap_or(after_at.len());

    if tag_end == 0 {
        return None; // Just a bare @ with no word
    }

    let tag = after_at[..tag_end].to_lowercase();
    let rest = after_at[tag_end..].trim().to_string();

    Some((tag, rest))
}

fn flush_block(
    current_tag: &mut Option<String>,
    current_lines: &mut Vec<String>,
    result: &mut Vec<DocComment>,
) {
    if current_lines.is_empty() && current_tag.is_none() {
        return;
    }

    let text = current_lines.join("\n");
    if !text.is_empty() || current_tag.is_some() {
        result.push(DocComment {
            tag: current_tag.take(),
            text,
            source_span: None, // Spans are on the outer block; inner tags don't carry spans
        });
    }
    current_lines.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docset_default_is_empty() {
        let ds = DocSet::default();
        assert!(ds.objects.is_empty());
    }

    #[test]
    fn objectdoc_roundtrip_json() {
        let obj = ObjectDoc {
            object_id: "hr.emp_pkg".into(),
            name: "EMP_PKG".into(),
            kind: "package".into(),
            summary: Some("Employee package".into()),
            comments: vec![DocComment {
                tag: Some("description".into()),
                text: "Manages employees.".into(),
                source_span: Some(DocSpan { start: 0, end: 18 }),
            }],
            source_span: Some(DocSpan { start: 0, end: 200 }),
        };
        let json = serde_json::to_string(&obj).unwrap();
        let back: ObjectDoc = serde_json::from_str(&json).unwrap();
        assert_eq!(back.object_id, obj.object_id);
        assert_eq!(back.comments.len(), 1);
        assert_eq!(back.source_span, obj.source_span);
    }

    #[test]
    fn javadoc_style_block() {
        let source = "\n/**\n * Compute least-cost route.\n * @description Compute route\n * @param p_dest Destination number\n * @returns Route object\n */\nFUNCTION find_route(p_dest VARCHAR2) RETURN route_t;\n";
        let comments = super::extract_doc_comments(source);
        assert_eq!(comments.len(), 1, "expected one javadoc block");
        let c = &comments[0];
        assert!(c.tag.is_none());
        assert!(c.text.contains("Compute least-cost route"));
        assert!(c.text.contains("@description"));
        assert!(c.text.contains("@param p_dest"));
        assert!(c.text.contains("@returns"));
        assert!(c.source_span.is_some());
    }

    #[test]
    fn legacy_preceding_line_comments() {
        let source = "-- Calculate the total price\n-- for all items in an order.\nFUNCTION calc_total(p_order_id NUMBER) RETURN NUMBER;";
        let comments = super::extract_doc_comments(source);
        assert_eq!(comments.len(), 1, "expected one preceding-line block");
        let c = &comments[0];
        assert!(c.tag.is_none());
        assert!(c.text.contains("Calculate the total price"));
        assert!(c.text.contains("for all items in an order"));
    }

    #[test]
    fn javadoc_strips_leading_asterisks() {
        let source = "/**\n * First line.\n * Second line.\n */\nPROCEDURE p;";
        let comments = super::extract_doc_comments(source);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text, "First line.\nSecond line.");
    }

    #[test]
    fn dash_comments_not_above_declaration_ignored() {
        let source = "-- just a random comment\nSELECT 1 FROM dual;";
        let comments = super::extract_doc_comments(source);
        assert!(comments.is_empty());
    }

    #[test]
    fn multiple_blocks_in_one_file() {
        let source = "-- Package for billing operations.\n-- Handles invoicing.\nCREATE OR REPLACE PACKAGE billing_api AS\n\n/**\n * Process a payment.\n */\nPROCEDURE process_payment(p_id NUMBER);\n\nEND;";
        let comments = super::extract_doc_comments(source);
        assert_eq!(comments.len(), 2, "expected 2 doc blocks");
        assert!(
            comments
                .iter()
                .any(|c| c.text.contains("billing operations"))
        );
        assert!(
            comments
                .iter()
                .any(|c| c.text.contains("Process a payment"))
        );
    }

    #[test]
    fn empty_source_returns_empty() {
        let comments = super::extract_doc_comments("");
        assert!(comments.is_empty());
    }

    #[test]
    fn unterminated_javadoc_no_panic() {
        let source = "/**\n * This block never closes.\n";
        let comments = super::extract_doc_comments(source);
        let _ = comments; // No panic = pass
    }

    // --- parse_doc_tags tests ---

    #[test]
    fn single_tag() {
        let text = "@description Compute the total for an order.";
        let tags = super::parse_doc_tags(text);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag.as_deref(), Some("description"));
        assert_eq!(tags[0].text, "Compute the total for an order.");
    }

    #[test]
    fn multiple_tags() {
        let text = "@description Process payment\n@param p_id The invoice ID\n@param p_amount Amount to charge\n@returns Payment confirmation ID\n@throws -20001 If invoice not found";
        let tags = super::parse_doc_tags(text);
        assert_eq!(tags.len(), 5);
        assert_eq!(tags[0].tag.as_deref(), Some("description"));
        assert_eq!(tags[1].tag.as_deref(), Some("param"));
        assert_eq!(tags[1].text, "p_id The invoice ID");
        assert_eq!(tags[2].tag.as_deref(), Some("param"));
        assert_eq!(tags[2].text, "p_amount Amount to charge");
        assert_eq!(tags[3].tag.as_deref(), Some("returns"));
        assert_eq!(tags[4].tag.as_deref(), Some("throws"));
    }

    #[test]
    fn free_form_text_before_tags() {
        let text = "This procedure handles billing.\nIt is called nightly.\n@description Billing handler\n@param p_batch Batch ID";
        let tags = super::parse_doc_tags(text);
        assert_eq!(tags.len(), 3);
        assert!(tags[0].tag.is_none(), "free-form text should have no tag");
        assert!(tags[0].text.contains("handles billing"));
        assert!(tags[0].text.contains("called nightly"));
        assert_eq!(tags[1].tag.as_deref(), Some("description"));
        assert_eq!(tags[2].tag.as_deref(), Some("param"));
    }

    #[test]
    fn continuation_lines_join() {
        let text = "@description This is a long description\nthat spans multiple lines\nand continues here.\n@param p_id The ID";
        let tags = super::parse_doc_tags(text);
        assert_eq!(tags.len(), 2);
        assert!(tags[0].text.contains("long description"));
        assert!(tags[0].text.contains("spans multiple lines"));
        assert!(tags[0].text.contains("continues here"));
    }

    #[test]
    fn empty_text_returns_empty() {
        let tags = super::parse_doc_tags("");
        assert!(tags.is_empty());
    }

    #[test]
    fn only_free_form_no_tags() {
        let text = "Just a plain comment.\nNothing special here.";
        let tags = super::parse_doc_tags(text);
        assert_eq!(tags.len(), 1);
        assert!(tags[0].tag.is_none());
        assert!(tags[0].text.contains("plain comment"));
    }

    #[test]
    fn tags_case_insensitive() {
        let text = "@Description Desc text\n@PARAM p_id ID";
        let tags = super::parse_doc_tags(text);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].tag.as_deref(), Some("description"));
        assert_eq!(tags[1].tag.as_deref(), Some("param"));
    }

    #[test]
    fn tag_with_no_body() {
        let text = "@deprecated\n@description Replacement info";
        let tags = super::parse_doc_tags(text);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].tag.as_deref(), Some("deprecated"));
        assert!(tags[0].text.is_empty());
        assert_eq!(tags[1].tag.as_deref(), Some("description"));
    }

    #[test]
    fn example_tag_multiline() {
        let text = "@example\nSELECT calc_total(42) FROM dual;\n-- returns 100";
        let tags = super::parse_doc_tags(text);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag.as_deref(), Some("example"));
        assert!(tags[0].text.contains("SELECT calc_total"));
        assert!(tags[0].text.contains("returns 100"));
    }
}
