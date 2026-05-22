#![forbid(unsafe_code)]

use tracing::instrument;

pub mod html {
    use super::escape_html;
    use tracing::instrument;

    #[must_use]
    #[instrument(level = "trace", skip(title, body))]
    pub fn shell(title: impl AsRef<str>, body: impl AsRef<str>) -> String {
        let title = escape_html(title.as_ref());
        let body = body.as_ref();
        format!(
            concat!(
                "<!DOCTYPE html>\n",
                "<html lang=\"en\">\n",
                "<head>\n",
                "  <meta charset=\"utf-8\" />\n",
                "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n",
                "  <title>{title}</title>\n",
                "  <style>\n",
                "    :root {{ color-scheme: light; font-family: ui-sans-serif, system-ui, sans-serif; }}\n",
                "    body {{ margin: 0; padding: 2rem; background: #f8fafc; color: #0f172a; }}\n",
                "    main {{ max-width: 72rem; margin: 0 auto; background: #ffffff; border: 1px solid #cbd5e1; border-radius: 16px; padding: 2rem; box-shadow: 0 18px 40px rgba(15, 23, 42, 0.08); }}\n",
                "  </style>\n",
                "</head>\n",
                "<body>\n",
                "  <main data-plsql-render=\"html-shell\">\n",
                "{body}\n",
                "  </main>\n",
                "</body>\n",
                "</html>\n"
            ),
            title = title,
            body = body,
        )
    }
}

pub mod markdown {
    use tracing::instrument;

    #[must_use]
    #[instrument(level = "trace", skip(headers, rows))]
    pub fn table(headers: &[impl AsRef<str>], rows: &[Vec<String>]) -> String {
        let header_line = headers
            .iter()
            .map(|header| escape_cell(header.as_ref()))
            .collect::<Vec<_>>()
            .join(" | ");
        let separator = headers
            .iter()
            .map(|_| "---")
            .collect::<Vec<_>>()
            .join(" | ");
        let row_lines = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| escape_cell(cell))
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .collect::<Vec<_>>();

        let mut rendered = String::new();
        rendered.push_str("| ");
        rendered.push_str(&header_line);
        rendered.push_str(" |\n| ");
        rendered.push_str(&separator);
        rendered.push_str(" |");

        for row in row_lines {
            rendered.push_str("\n| ");
            rendered.push_str(&row);
            rendered.push_str(" |");
        }

        rendered
    }

    fn escape_cell(value: &str) -> String {
        value.replace('|', "\\|").replace('\n', "<br>")
    }
}

pub mod graphviz {
    use tracing::instrument;

    #[must_use]
    #[instrument(level = "trace", skip(value))]
    pub fn escape_label(value: &str) -> String {
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }

    #[must_use]
    #[instrument(level = "trace", skip(value))]
    pub fn quote_id(value: impl AsRef<str>) -> String {
        format!("\"{}\"", escape_label(value.as_ref()))
    }
}

pub mod svg {
    use tracing::instrument;

    use super::escape_html;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct GraphNode {
        pub id: String,
        pub label: String,
        pub x: u32,
        pub y: u32,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct GraphEdge {
        pub from: String,
        pub to: String,
        pub label: Option<String>,
    }

    pub trait GraphView {
        fn width(&self) -> u32;
        fn height(&self) -> u32;
        fn nodes(&self) -> &[GraphNode];
        fn edges(&self) -> &[GraphEdge];
    }

    #[must_use]
    #[instrument(level = "trace", skip(graph))]
    pub fn node_graph(graph: &impl GraphView) -> String {
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"PLSQL graph\">\n",
            width = graph.width(),
            height = graph.height(),
        );
        svg.push_str(
            "  <defs>\n    <marker id=\"arrow\" markerWidth=\"10\" markerHeight=\"10\" refX=\"9\" refY=\"3\" orient=\"auto\">\n      <path d=\"M0,0 L0,6 L9,3 z\" fill=\"#475569\" />\n    </marker>\n  </defs>\n",
        );
        svg.push_str("  <rect width=\"100%\" height=\"100%\" fill=\"#f8fafc\" rx=\"18\" />\n");

        for edge in graph.edges() {
            let Some(from_node) = graph.nodes().iter().find(|node| node.id == edge.from) else {
                continue;
            };
            let Some(to_node) = graph.nodes().iter().find(|node| node.id == edge.to) else {
                continue;
            };

            svg.push_str(&format!(
                "  <line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" stroke=\"#475569\" stroke-width=\"2\" marker-end=\"url(#arrow)\" />\n",
                x1 = from_node.x,
                y1 = from_node.y,
                x2 = to_node.x,
                y2 = to_node.y,
            ));

            if let Some(label) = &edge.label {
                // PLSQL-RENDER-LINT-1 / multi-pass-bug-hunting: u32::midpoint
                // is overflow-safe; the previous `(a + b) / 2` form wraps if
                // both coords approach u32::MAX. In practice SVG coords are
                // tiny, but the lint exists for a reason and the fix is free.
                let label_x = u32::midpoint(from_node.x, to_node.x);
                let label_y = u32::midpoint(from_node.y, to_node.y);
                svg.push_str(&format!(
                    "  <text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" font-size=\"12\" fill=\"#334155\">{label}</text>\n",
                    x = label_x,
                    y = label_y.saturating_sub(8),
                    label = escape_html(label),
                ));
            }
        }

        for node in graph.nodes() {
            svg.push_str(&format!(
                "  <circle cx=\"{x}\" cy=\"{y}\" r=\"24\" fill=\"#ffffff\" stroke=\"#0f172a\" stroke-width=\"2\" />\n",
                x = node.x,
                y = node.y,
            ));
            svg.push_str(&format!(
                "  <text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" dominant-baseline=\"middle\" font-size=\"12\" fill=\"#0f172a\">{label}</text>\n",
                x = node.x,
                y = node.y,
                label = escape_html(&node.label),
            ));
        }

        svg.push_str("</svg>\n");
        svg
    }
}

#[must_use]
#[instrument(level = "trace", skip(value))]
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use crate::{
        graphviz, html, markdown,
        svg::{GraphEdge, GraphNode, GraphView, node_graph},
    };

    #[test]
    fn html_shell_wraps_body_content() {
        let rendered = html::shell("Catalog Report", "<section>ready</section>");

        assert!(rendered.contains("<title>Catalog Report</title>"));
        assert!(rendered.contains("<main data-plsql-render=\"html-shell\">"));
        assert!(rendered.contains("<section>ready</section>"));
    }

    #[test]
    fn markdown_table_escapes_pipes_and_newlines() {
        let rendered = markdown::table(
            &["Rule", "Result"],
            &[vec![String::from("A|B"), String::from("line1\nline2")]],
        );

        assert!(rendered.contains("| Rule | Result |"));
        assert!(rendered.contains("A\\|B"));
        assert!(rendered.contains("line1<br>line2"));
    }

    #[test]
    fn svg_node_graph_renders_nodes_and_edges() {
        struct SimpleGraph {
            nodes: Vec<GraphNode>,
            edges: Vec<GraphEdge>,
        }

        impl GraphView for SimpleGraph {
            fn width(&self) -> u32 {
                320
            }

            fn height(&self) -> u32 {
                200
            }

            fn nodes(&self) -> &[GraphNode] {
                &self.nodes
            }

            fn edges(&self) -> &[GraphEdge] {
                &self.edges
            }
        }

        let graph = SimpleGraph {
            nodes: vec![
                GraphNode {
                    id: String::from("pkg"),
                    label: String::from("PKG"),
                    x: 80,
                    y: 100,
                },
                GraphNode {
                    id: String::from("tab"),
                    label: String::from("TAB"),
                    x: 240,
                    y: 100,
                },
            ],
            edges: vec![GraphEdge {
                from: String::from("pkg"),
                to: String::from("tab"),
                label: Some(String::from("reads")),
            }],
        };

        let rendered = node_graph(&graph);

        assert!(rendered.contains("<svg"));
        assert!(rendered.contains("marker-end=\"url(#arrow)\""));
        assert!(rendered.contains(">PKG</text>"));
        assert!(rendered.contains(">reads</text>"));
    }

    #[test]
    fn graphviz_helpers_escape_labels_and_quote_ids() {
        assert_eq!(
            graphviz::escape_label("pkg\"name\nline"),
            "pkg\\\"name\\nline"
        );
        assert_eq!(graphviz::quote_id("node:1"), "\"node:1\"");
    }

    #[test]
    fn graphviz_escape_is_order_safe_for_backslash_and_quotes() {
        // Order-sensitive: `\` must be escaped BEFORE `"`/newline, else
        // the escape backslash introduced for `"` gets double-escaped
        // and every DOT label/id is corrupted (or injectable). The
        // existing test has no literal backslash, so a `\`/`"` reorder
        // regression would pass it. Lock the combined case.
        // Input bytes:  a \ b " c <NL> d
        let input = "a\\b\"c\nd";
        assert_eq!(
            graphviz::escape_label(input),
            "a\\\\b\\\"c\\nd",
            "backslash -> \\\\, then quote -> \\\", newline -> \\n; not double-escaped"
        );
        // A lone backslash stays a single escaped pair, not quadrupled.
        assert_eq!(graphviz::escape_label("\\"), "\\\\");
        // quote_id wraps in real quotes and escapes interior quotes so
        // the DOT identifier cannot be broken out of.
        assert_eq!(graphviz::quote_id("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}
