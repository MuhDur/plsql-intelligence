//! Docusaurus-compatible MDX export (PLSQL-DOC-009).
//!
//! Wraps the Markdown form produced by `render_package_markdown` /
//! `render_object_markdown` with the front-matter and MDX-safe
//! escapes a Docusaurus site needs to embed the documentation
//! pages directly:
//!
//! 1. **Front-matter** — YAML block with `id`, `title`, `slug`,
//!    `sidebar_label`, and `description`. The `id` is derived from
//!    the object id; the `slug` is `/objects/<id>` so existing
//!    Docusaurus routing picks it up.
//! 2. **MDX-safe escapes** — JSX-significant character pairs
//!    (`{`, `}`, `<`, `>`) outside fenced code blocks are escaped
//!    so plain text doesn't accidentally trip the MDX parser.
//!    Inside fenced blocks (` ``` `) the text is left alone.
//!
//! The renderer is a thin layer over the existing Markdown
//! renderer — it does NOT change the prose. Tests pin the
//! escape behaviour byte-for-byte so the Docusaurus build stays
//! deterministic.

use crate::ObjectDoc;
use crate::render::{render_object_markdown, render_package_markdown};

/// Render an [`ObjectDoc`] as a Docusaurus MDX page. The output
/// contains a YAML front-matter block followed by the Markdown
/// body, with MDX-safe escapes applied to text outside fenced code
/// blocks.
#[must_use]
pub fn render_object_mdx(doc: &ObjectDoc) -> String {
    let body = match doc.kind.as_str() {
        "package" => render_package_markdown(doc),
        _ => render_object_markdown(doc),
    };
    wrap_with_frontmatter(doc, &mdx_safe_escape(&body))
}

/// Convenience: render every object in a slice as MDX, returning
/// `(slug, contents)` pairs so the caller can write each one out
/// without re-deriving the slug.
#[must_use]
pub fn render_objects_mdx(docs: &[ObjectDoc]) -> Vec<(String, String)> {
    docs.iter()
        .map(|d| (slug_for(d), render_object_mdx(d)))
        .collect()
}

fn slug_for(doc: &ObjectDoc) -> String {
    format!("/objects/{}", doc.object_id)
}

fn wrap_with_frontmatter(doc: &ObjectDoc, body: &str) -> String {
    let id = doc.object_id.replace('.', "_");
    let title = escape_yaml_scalar(&doc.name);
    let slug = slug_for(doc);
    let sidebar_label = escape_yaml_scalar(&doc.name);
    let description = doc
        .summary
        .as_deref()
        .map(escape_yaml_scalar)
        .unwrap_or_else(|| escape_yaml_scalar(&doc.name));

    format!(
        "---\nid: {id}\ntitle: {title}\nslug: {slug}\nsidebar_label: {sidebar_label}\ndescription: {description}\n---\n\n{body}",
    )
}

/// Quote a YAML scalar value safely. We use double-quoted form for
/// every value so colons, hashes, and leading symbols never trip
/// the YAML parser.
fn escape_yaml_scalar(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Apply MDX-safe escapes outside fenced code blocks. JSX would
/// otherwise interpret `<`, `>`, `{`, `}` as element / expression
/// delimiters and fail the build on plain prose that contains
/// PL/SQL syntax.
fn mdx_safe_escape(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 32);
    let mut in_fence = false;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
            continue;
        }
        if in_fence {
            out.push_str(line);
            continue;
        }
        for ch in line.chars() {
            match ch {
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '{' => out.push_str("&#123;"),
                '}' => out.push_str("&#125;"),
                _ => out.push(ch),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocComment, ObjectDoc};

    fn doc(kind: &str, summary: Option<&str>) -> ObjectDoc {
        ObjectDoc {
            object_id: "hr.demo_pkg".into(),
            name: "demo_pkg".into(),
            kind: kind.into(),
            summary: summary.map(|s| s.into()),
            comments: vec![DocComment {
                tag: None,
                text: "Hello {world} <stuff>".into(),
                source_span: None,
            }],
            source_span: None,
        }
    }

    #[test]
    fn frontmatter_block_emitted_before_body() {
        let mdx = render_object_mdx(&doc("package", Some("a summary")));
        assert!(mdx.starts_with("---\n"));
        assert!(mdx.contains("id: hr_demo_pkg"));
        assert!(mdx.contains("slug: /objects/hr.demo_pkg"));
        assert!(mdx.contains("sidebar_label: \"demo_pkg\""));
        assert!(mdx.contains("description: \"a summary\""));
    }

    #[test]
    fn jsx_significant_chars_escaped_in_prose() {
        let mdx = render_object_mdx(&doc("package", None));
        // The doc-comment body contains "Hello {world} <stuff>".
        // Different upstream renderers handle `<` differently — some
        // markdown variants pre-escape it to `&lt;`, others keep it
        // raw. We require curly-brace escaping (which only MDX
        // post-escape can produce) AND that the literal `<stuff>` /
        // `>` is not left as a JSX-significant character.
        assert!(
            mdx.contains("Hello &#123;world&#125;"),
            "expected braces escaped to &#123;{{/&#125; in {mdx:?}"
        );
        assert!(
            !mdx.contains("Hello {world}"),
            "unescaped braces remained in {mdx:?}"
        );
        assert!(
            !mdx.contains("<stuff>"),
            "unescaped <stuff> remained in {mdx:?}"
        );
    }

    #[test]
    fn fenced_code_blocks_left_alone() {
        let mut d = doc("package", None);
        d.comments.push(DocComment {
            tag: None,
            text: "Use this snippet:\n\n```sql\nSELECT * FROM t WHERE id = {param};\n```".into(),
            source_span: None,
        });
        let mdx = render_object_mdx(&d);
        // Inside the fence the `{param}` MUST remain unescaped.
        assert!(mdx.contains("WHERE id = {param};"));
    }

    #[test]
    fn yaml_scalars_quoted_for_safety() {
        let mut d = doc("package", Some("contains: a colon"));
        d.name = "weird\"name".into();
        let mdx = render_object_mdx(&d);
        // The colon in the description must not break YAML.
        assert!(mdx.contains("description: \"contains: a colon\""));
        // Embedded double-quote got backslash-escaped.
        assert!(mdx.contains("sidebar_label: \"weird\\\"name\""));
    }

    #[test]
    fn render_objects_mdx_returns_slug_per_input() {
        let docs = vec![doc("package", None), {
            let mut d = doc("view", None);
            d.object_id = "hr.demo_view".into();
            d.name = "demo_view".into();
            d
        }];
        let outputs = render_objects_mdx(&docs);
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].0, "/objects/hr.demo_pkg");
        assert_eq!(outputs[1].0, "/objects/hr.demo_view");
    }

    #[test]
    fn output_is_deterministic() {
        let d = doc("package", Some("x"));
        let a = render_object_mdx(&d);
        let b = render_object_mdx(&d);
        assert_eq!(a, b);
    }

    #[test]
    fn non_package_kinds_route_through_render_object_markdown() {
        let d = doc("table", Some("x"));
        let mdx = render_object_mdx(&d);
        // The Markdown emitted by render_object_markdown is shaped
        // differently from render_package_markdown, but both share
        // the # name header so we can sanity-check it landed.
        assert!(mdx.contains("# demo_pkg"));
    }
}
