//! Inline SVG call-graph rendering from a dep-graph view (PLSQL-DOC-006).
//!
//! The documentation site embeds small SVG diagrams of a package /
//! routine's call graph next to its object page. This module is the
//! pure renderer: given a [`CallGraphView`] (a minimal projection
//! of the depgraph's edges relevant to the object), it produces an
//! `<svg>...</svg>` string suitable for inlining into Markdown or
//! HTML pages.
//!
//! The layout is intentionally trivial: nodes are placed in
//! left-to-right "layers" by topological depth from the focal node.
//! Edges are straight `<line>` segments. The renderer ships no
//! external CSS — every visual attribute lands as an SVG attribute
//! so the output renders identically without a stylesheet.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference routing —
//!   the call-graph itself comes from PL/SQL static dependency
//!   analysis (see also `ALL_DEPENDENCIES` in
//!   `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families).
//!   This module is the source-only renderer; the data feeding
//!   it is produced by `plsql-depgraph`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;

use serde::{Deserialize, Serialize};

/// Minimal projection of the depgraph the renderer needs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallGraphView {
    /// Logical name of the focal node — drawn highlighted in the
    /// centre of the graph.
    pub focal: String,
    /// Distinct logical names of every node in the view, including
    /// the focal node. Render order follows insertion order so the
    /// caller controls layout grouping.
    pub nodes: Vec<String>,
    /// Directed call edges (caller → callee).
    pub edges: Vec<CallEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallEdge {
    pub from: String,
    pub to: String,
    /// Confidence tag — drives stroke style:
    /// "exact" / "heuristic" / "unknown".
    pub confidence: String,
}

const PX_NODE_WIDTH: u32 = 140;
const PX_NODE_HEIGHT: u32 = 36;
const PX_HORIZ_GAP: u32 = 60;
const PX_VERT_GAP: u32 = 18;
const PX_MARGIN: u32 = 24;

/// Render an SVG call-graph from `view`. The output is
/// deterministic — same input bytes → byte-identical output.
#[must_use]
pub fn render_call_graph_svg(view: &CallGraphView) -> String {
    if view.nodes.is_empty() {
        return String::from(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"0\" height=\"0\"></svg>",
        );
    }
    let layers = layer_nodes(view);
    let mut positions: HashMap<String, (u32, u32)> = HashMap::new();
    let total_layers = layers.len() as u32;
    let mut max_per_layer = 0_u32;
    for col in &layers {
        max_per_layer = max_per_layer.max(col.len() as u32);
    }

    let width = PX_MARGIN * 2
        + total_layers * PX_NODE_WIDTH
        + total_layers.saturating_sub(1) * PX_HORIZ_GAP;
    let height = PX_MARGIN * 2
        + max_per_layer * PX_NODE_HEIGHT
        + max_per_layer.saturating_sub(1) * PX_VERT_GAP;

    for (layer_idx, column) in layers.iter().enumerate() {
        let x = PX_MARGIN + (layer_idx as u32) * (PX_NODE_WIDTH + PX_HORIZ_GAP);
        for (row_idx, node) in column.iter().enumerate() {
            let y = PX_MARGIN + (row_idx as u32) * (PX_NODE_HEIGHT + PX_VERT_GAP);
            positions.insert(node.clone(), (x, y));
        }
    }

    let mut out = String::new();
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">",
    );
    out.push_str("<style>text{font-family:monospace;font-size:11px;dominant-baseline:central;text-anchor:middle;}</style>");

    // Edges first so nodes draw over them.
    for edge in &view.edges {
        if let (Some(from_pos), Some(to_pos)) = (positions.get(&edge.from), positions.get(&edge.to))
        {
            let (fx, fy) = (from_pos.0 + PX_NODE_WIDTH, from_pos.1 + PX_NODE_HEIGHT / 2);
            let (tx, ty) = (to_pos.0, to_pos.1 + PX_NODE_HEIGHT / 2);
            let (stroke, dash) = edge_style(&edge.confidence);
            let _ = write!(
                out,
                "<line x1=\"{fx}\" y1=\"{fy}\" x2=\"{tx}\" y2=\"{ty}\" stroke=\"{stroke}\" stroke-width=\"1\"{dash}/>",
            );
        }
    }

    for node in &view.nodes {
        if let Some(&(x, y)) = positions.get(node) {
            let fill = if node == &view.focal {
                "#fde68a"
            } else {
                "#e2e8f0"
            };
            let _ = write!(
                out,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{PX_NODE_WIDTH}\" height=\"{PX_NODE_HEIGHT}\" rx=\"4\" fill=\"{fill}\" stroke=\"#475569\" stroke-width=\"1\"/>",
            );
            let label_x = x + PX_NODE_WIDTH / 2;
            let label_y = y + PX_NODE_HEIGHT / 2;
            let _ = write!(
                out,
                "<text x=\"{label_x}\" y=\"{label_y}\">{escaped}</text>",
                escaped = escape_xml(node),
            );
        }
    }

    out.push_str("</svg>");
    out
}

fn edge_style(confidence: &str) -> (&'static str, &'static str) {
    match confidence {
        "exact" | "Exact" => ("#0f766e", ""),
        "heuristic" | "Heuristic" => ("#a16207", " stroke-dasharray=\"4 2\""),
        _ => ("#7c3aed", " stroke-dasharray=\"2 2\""),
    }
}

/// Layer nodes into left-to-right columns by BFS distance from the
/// focal node, walking outgoing edges (callees right, callers left
/// would require a separate reverse BFS — out of scope for this
/// minimal renderer).
fn layer_nodes(view: &CallGraphView) -> Vec<Vec<String>> {
    let mut layer_of: BTreeMap<String, u32> = BTreeMap::new();
    layer_of.insert(view.focal.clone(), 0);
    let mut queue: Vec<String> = vec![view.focal.clone()];
    while let Some(cursor) = queue.pop() {
        let depth = *layer_of.get(&cursor).unwrap_or(&0);
        for edge in &view.edges {
            if edge.from == cursor && !layer_of.contains_key(&edge.to) {
                layer_of.insert(edge.to.clone(), depth + 1);
                queue.push(edge.to.clone());
            }
        }
    }
    // Any node not reachable from the focal goes in layer 0.
    for node in &view.nodes {
        layer_of.entry(node.clone()).or_insert(0);
    }
    let max_layer = layer_of.values().copied().max().unwrap_or(0);
    let mut columns: Vec<Vec<String>> = vec![Vec::new(); (max_layer + 1) as usize];
    let mut seen: BTreeSet<String> = BTreeSet::new();
    // Preserve caller insertion order within a layer.
    for node in &view.nodes {
        if seen.insert(node.clone()) {
            let layer = *layer_of.get(node).unwrap_or(&0);
            columns[layer as usize].push(node.clone());
        }
    }
    columns
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(from: &str, to: &str, conf: &str) -> CallEdge {
        CallEdge {
            from: from.into(),
            to: to.into(),
            confidence: conf.into(),
        }
    }

    #[test]
    fn empty_view_produces_zero_size_svg() {
        let svg = render_call_graph_svg(&CallGraphView::default());
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("width=\"0\""));
    }

    #[test]
    fn single_node_renders_focal_rect_and_label() {
        let view = CallGraphView {
            focal: "foo".into(),
            nodes: vec!["foo".into()],
            edges: vec![],
        };
        let svg = render_call_graph_svg(&view);
        assert!(svg.contains("<rect"));
        assert!(svg.contains(">foo<"));
    }

    #[test]
    fn callee_lands_to_the_right_of_focal() {
        let view = CallGraphView {
            focal: "caller".into(),
            nodes: vec!["caller".into(), "callee".into()],
            edges: vec![edge("caller", "callee", "exact")],
        };
        let svg = render_call_graph_svg(&view);
        // Two rects + the edge.
        let rect_count = svg.matches("<rect").count();
        assert_eq!(rect_count, 2);
        assert!(svg.contains("<line"));
    }

    #[test]
    fn confidence_drives_edge_style() {
        let view = CallGraphView {
            focal: "a".into(),
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![edge("a", "b", "exact"), edge("a", "c", "heuristic")],
        };
        let svg = render_call_graph_svg(&view);
        // Exact edges use the teal stroke; heuristic edges have a
        // dasharray.
        assert!(svg.contains("#0f766e"));
        assert!(svg.contains("stroke-dasharray=\"4 2\""));
    }

    #[test]
    fn unknown_confidence_uses_violet_dotted() {
        let view = CallGraphView {
            focal: "a".into(),
            nodes: vec!["a".into(), "b".into()],
            edges: vec![edge("a", "b", "Unknown")],
        };
        let svg = render_call_graph_svg(&view);
        assert!(svg.contains("#7c3aed"));
        assert!(svg.contains("stroke-dasharray=\"2 2\""));
    }

    #[test]
    fn focal_node_has_distinct_fill() {
        let view = CallGraphView {
            focal: "a".into(),
            nodes: vec!["a".into(), "b".into()],
            edges: vec![edge("a", "b", "exact")],
        };
        let svg = render_call_graph_svg(&view);
        // Focal fill is #fde68a; non-focal #e2e8f0.
        assert!(svg.contains("#fde68a"));
        assert!(svg.contains("#e2e8f0"));
    }

    #[test]
    fn xml_special_chars_escaped_in_labels() {
        let view = CallGraphView {
            focal: "<>&".into(),
            nodes: vec!["<>&".into()],
            edges: vec![],
        };
        let svg = render_call_graph_svg(&view);
        assert!(svg.contains("&lt;&gt;&amp;"));
        assert!(!svg.contains(">< /text>"));
    }

    #[test]
    fn output_is_deterministic() {
        let view = CallGraphView {
            focal: "f".into(),
            nodes: vec!["f".into(), "g".into(), "h".into()],
            edges: vec![edge("f", "g", "exact"), edge("f", "h", "heuristic")],
        };
        let a = render_call_graph_svg(&view);
        let b = render_call_graph_svg(&view);
        assert_eq!(a, b);
    }

    #[test]
    fn unreachable_nodes_drop_into_layer_zero() {
        let view = CallGraphView {
            focal: "a".into(),
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![edge("a", "b", "exact")],
            // c is in nodes but not reachable from a — should still
            // render rather than vanish.
        };
        let svg = render_call_graph_svg(&view);
        assert!(svg.contains(">c<"));
    }
}
