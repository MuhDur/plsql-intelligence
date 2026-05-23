//! Table-usage graph rendering.
//!
//! Sibling of the call-graph renderer (DOC-006). Where the call
//! graph shows routine-calls-routine edges, the table-usage graph
//! shows program-units-touching-tables. Each table is rendered as
//! a row of column chips so the operator can see which programs
//! read or write which columns at a glance.
//!
//! Input is a `TableUsageView` carrying the focal table, the set
//! of program units that touch it, and per-unit operation flags
//! (`reads`, `writes`). The renderer produces a deterministic
//! inline SVG suitable for embedding in Markdown or HTML pages.
//!
//! ## /oracle evidence
//!
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families — the
//!   reads/writes split comes from `ALL_TAB_PRIVS` + parser-level
//!   column-access analysis already present in
//!   `plsql_lineage::column_readers` / `column_writers`.
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference routing
//!   for the routine-level column access semantics.

use std::collections::BTreeMap;
use std::fmt::Write;

use serde::{Deserialize, Serialize};

const PX_MARGIN: u32 = 24;
const PX_NODE_WIDTH: u32 = 160;
const PX_NODE_HEIGHT: u32 = 40;
const PX_VERT_GAP: u32 = 20;
const PX_HORIZ_GAP: u32 = 80;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableUsageView {
    /// Logical name of the table the page is about. Drawn centred.
    pub focal_table: String,
    /// Columns of the focal table, in declaration order. Rendered
    /// as a vertical chip stack inside the table node.
    pub columns: Vec<String>,
    /// Program units touching this table.
    pub units: Vec<UnitUsage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitUsage {
    pub unit: String,
    /// Columns this unit reads. May be empty.
    pub reads: Vec<String>,
    /// Columns this unit writes. May be empty.
    pub writes: Vec<String>,
}

/// Render the table-usage graph as inline SVG. Output is
/// deterministic.
#[must_use]
pub fn render_table_usage_svg(view: &TableUsageView) -> String {
    if view.focal_table.is_empty() {
        return String::from(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"0\" height=\"0\"></svg>",
        );
    }
    // Saturating casts (oracle-kxb3 sibling): a view with
    // >u32::MAX units (or columns) would wrap the SVG coordinate
    // math with the legacy `as u32` cast. Saturate to `u32::MAX` so
    // the worst we render is a clipped canvas, never a corrupted
    // one.
    let unit_count = u32::try_from(view.units.len()).unwrap_or(u32::MAX);
    let column_count = u32::try_from(view.columns.len().max(1)).unwrap_or(u32::MAX);

    let table_height = PX_NODE_HEIGHT + column_count * 18 + 8;
    let units_block_height = unit_count.max(1) * (PX_NODE_HEIGHT + PX_VERT_GAP);
    let height = PX_MARGIN * 2 + table_height.max(units_block_height);
    let width = PX_MARGIN * 2 + PX_NODE_WIDTH * 2 + PX_HORIZ_GAP;

    let table_x = PX_MARGIN + PX_NODE_WIDTH + PX_HORIZ_GAP;
    let table_y = PX_MARGIN;
    let mut out = String::new();
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">",
    );
    out.push_str(
        "<style>text{font-family:monospace;font-size:11px;dominant-baseline:central;}</style>",
    );

    // Compute unit positions and edge anchor points.
    let mut unit_positions: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for (i, unit) in view.units.iter().enumerate() {
        let ux = PX_MARGIN;
        let uy = PX_MARGIN + (i as u32) * (PX_NODE_HEIGHT + PX_VERT_GAP);
        unit_positions.insert(unit.unit.clone(), (ux, uy));
    }

    // Edges first.
    for unit in &view.units {
        let Some(&(ux, uy)) = unit_positions.get(&unit.unit) else {
            continue;
        };
        let edge_fx = ux + PX_NODE_WIDTH;
        let edge_fy = uy + PX_NODE_HEIGHT / 2;
        let edge_tx = table_x;
        let edge_ty = table_y + PX_NODE_HEIGHT / 2;
        let (stroke, dash, dx) = if !unit.writes.is_empty() {
            ("#dc2626", "", 0_u32) // red writes
        } else if !unit.reads.is_empty() {
            ("#2563eb", " stroke-dasharray=\"4 2\"", 0)
        } else {
            ("#94a3b8", " stroke-dasharray=\"2 2\"", 0)
        };
        let _ = write!(
            out,
            "<line x1=\"{edge_fx}\" y1=\"{edge_fy}\" x2=\"{edge_tx}\" y2=\"{edge_ty}\" stroke=\"{stroke}\" stroke-width=\"1\"{dash}/>",
        );
        let _ = dx;
    }

    // Unit nodes.
    for unit in &view.units {
        let Some(&(ux, uy)) = unit_positions.get(&unit.unit) else {
            continue;
        };
        let _ = write!(
            out,
            "<rect x=\"{ux}\" y=\"{uy}\" width=\"{PX_NODE_WIDTH}\" height=\"{PX_NODE_HEIGHT}\" rx=\"4\" fill=\"#e2e8f0\" stroke=\"#475569\" stroke-width=\"1\"/>",
        );
        let label_x = ux + PX_NODE_WIDTH / 2;
        let label_y = uy + PX_NODE_HEIGHT / 2;
        let escaped = escape_xml(&unit.unit);
        let _ = write!(
            out,
            "<text x=\"{label_x}\" y=\"{label_y}\" text-anchor=\"middle\">{escaped}</text>",
        );
    }

    // Table node — header rect + column chips.
    let _ = write!(
        out,
        "<rect x=\"{table_x}\" y=\"{table_y}\" width=\"{PX_NODE_WIDTH}\" height=\"{table_height}\" rx=\"4\" fill=\"#fde68a\" stroke=\"#475569\" stroke-width=\"1\"/>",
    );
    let label_x = table_x + PX_NODE_WIDTH / 2;
    let label_y = table_y + PX_NODE_HEIGHT / 2;
    let _ = write!(
        out,
        "<text x=\"{label_x}\" y=\"{label_y}\" text-anchor=\"middle\" font-weight=\"bold\">{escaped}</text>",
        escaped = escape_xml(&view.focal_table),
    );

    // Column chips. Highlight columns mentioned in any unit's
    // reads/writes so the operator sees which columns matter.
    let mut highlighted: BTreeMap<String, &'static str> = BTreeMap::new();
    for unit in &view.units {
        for w in &unit.writes {
            highlighted.insert(w.clone(), "#dc2626");
        }
        for r in &unit.reads {
            highlighted.entry(r.clone()).or_insert("#2563eb");
        }
    }

    for (i, col) in view.columns.iter().enumerate() {
        let cy = table_y + PX_NODE_HEIGHT + 4 + (i as u32) * 18;
        let cx = table_x + 8;
        let stroke = highlighted.get(col).copied().unwrap_or("#cbd5f5");
        let _ = write!(
            out,
            "<rect x=\"{cx}\" y=\"{cy}\" width=\"{w}\" height=\"14\" rx=\"3\" fill=\"#fef3c7\" stroke=\"{stroke}\" stroke-width=\"1\"/>",
            w = PX_NODE_WIDTH - 16,
        );
        let _ = write!(
            out,
            "<text x=\"{tx}\" y=\"{ty}\" text-anchor=\"start\">{escaped}</text>",
            tx = cx + 6,
            ty = cy + 7,
            escaped = escape_xml(col),
        );
    }

    out.push_str("</svg>");
    out
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

    fn uu(name: &str, reads: &[&str], writes: &[&str]) -> UnitUsage {
        UnitUsage {
            unit: name.into(),
            reads: reads.iter().map(|s| (*s).to_string()).collect(),
            writes: writes.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn empty_focal_yields_zero_size_svg() {
        let svg = render_table_usage_svg(&TableUsageView::default());
        assert!(svg.contains("width=\"0\""));
    }

    #[test]
    fn focal_table_rendered_with_columns() {
        let view = TableUsageView {
            focal_table: "employees".into(),
            columns: vec!["id".into(), "name".into(), "salary".into()],
            units: vec![],
        };
        let svg = render_table_usage_svg(&view);
        assert!(svg.contains(">employees<"));
        assert!(svg.contains(">id<"));
        assert!(svg.contains(">name<"));
        assert!(svg.contains(">salary<"));
    }

    #[test]
    fn writes_get_red_stroke_reads_blue_dashed() {
        let view = TableUsageView {
            focal_table: "employees".into(),
            columns: vec!["id".into()],
            units: vec![uu("update_emp", &[], &["id"]), uu("read_emp", &["id"], &[])],
        };
        let svg = render_table_usage_svg(&view);
        assert!(svg.contains("#dc2626"));
        assert!(svg.contains("#2563eb"));
        assert!(svg.contains("stroke-dasharray=\"4 2\""));
    }

    #[test]
    fn unit_that_neither_reads_nor_writes_gets_dotted_grey() {
        let view = TableUsageView {
            focal_table: "t".into(),
            columns: vec!["a".into()],
            units: vec![uu("touch", &[], &[])],
        };
        let svg = render_table_usage_svg(&view);
        assert!(svg.contains("#94a3b8"));
        assert!(svg.contains("stroke-dasharray=\"2 2\""));
    }

    #[test]
    fn column_chip_inherits_highest_severity_colour() {
        let view = TableUsageView {
            focal_table: "t".into(),
            columns: vec!["c".into()],
            units: vec![uu("r", &["c"], &[]), uu("w", &[], &["c"])],
        };
        let svg = render_table_usage_svg(&view);
        // Writes take precedence → column chip outlined red.
        assert!(svg.contains("stroke=\"#dc2626\""));
    }

    #[test]
    fn xml_special_chars_escaped() {
        let view = TableUsageView {
            focal_table: "<t>".into(),
            columns: vec!["&c".into()],
            units: vec![],
        };
        let svg = render_table_usage_svg(&view);
        assert!(svg.contains("&lt;t&gt;"));
        assert!(svg.contains("&amp;c"));
    }

    #[test]
    fn output_is_deterministic() {
        let view = TableUsageView {
            focal_table: "t".into(),
            columns: vec!["a".into(), "b".into()],
            units: vec![uu("r", &["a"], &[]), uu("w", &[], &["b"])],
        };
        assert_eq!(render_table_usage_svg(&view), render_table_usage_svg(&view));
    }
}
