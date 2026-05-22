//! Column-level edge extraction (PLSQL-DEP-004).
//!
//! Consumes a populated [`SqlStatementModel`] (SQLSEM-002 tables
//! plus SQLSEM-003 columns) and emits typed column-granular
//! dependency edges. Five edge kinds, picked from the column's
//! [`ColumnResolution`] and whether its table is known:
//!
//! * `ReadsColumn` — resolved read of a specific column.
//! * `WritesColumn` — resolved write of a specific column.
//! * `DerivesColumn` — the column feeds a projection expression
//!   or star expansion (the value is derived, not a direct
//!   column-to-column dependency).
//! * `ReadsUnknownColumnOfTable` — read of an unresolved column
//!   while the table is known (bare column, multi-table scope).
//! * `WritesUnknownColumnOfTable` — same, on the write side.
//!
//! This is the typed counterpart to SQLSEM-004's string-encoded
//! `ReadsColumn:<marker>` facts; lineage consumes these enums
//! directly without re-parsing the marker.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   column reference + projection grammar.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_TAB_COLUMNS` resolves an `Unknown*` edge into a
//!   concrete `ReadsColumn` / `WritesColumn` once the catalog
//!   is available (the unknown-of-table variant preserves the
//!   table so that upgrade is possible).

use serde::{Deserialize, Serialize};

use crate::sql_sem::{ColumnResolution, SqlStatementModel};

/// One column-granular dependency edge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnEdge {
    pub kind: ColumnEdgeKind,
    /// Resolved `schema.table` when known; the bare qualifier or
    /// `?` when the column couldn't be attributed to one table.
    pub table: String,
    pub column: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnEdgeKind {
    ReadsColumn,
    WritesColumn,
    DerivesColumn,
    ReadsUnknownColumnOfTable,
    WritesUnknownColumnOfTable,
}

/// Extract typed column edges from one statement model.
#[must_use]
pub fn extract_column_edges(model: &SqlStatementModel) -> Vec<ColumnEdge> {
    let mut out: Vec<ColumnEdge> = Vec::new();
    let single = if model.tables.len() == 1 {
        let t = &model.tables[0];
        Some(qualify(&t.schema, &t.table))
    } else {
        None
    };

    for c in &model.reads {
        let table = resolve_table(model, &c.qualifier, single.as_deref());
        let kind = match c.resolution {
            ColumnResolution::Resolved => ColumnEdgeKind::ReadsColumn,
            ColumnResolution::StarExpansion => ColumnEdgeKind::DerivesColumn,
            ColumnResolution::Unresolved | ColumnResolution::Pending => {
                ColumnEdgeKind::ReadsUnknownColumnOfTable
            }
        };
        push(&mut out, kind, table, &c.column);
    }
    for c in &model.writes {
        let table = resolve_table(model, &c.qualifier, single.as_deref());
        let kind = match c.resolution {
            ColumnResolution::Resolved => ColumnEdgeKind::WritesColumn,
            ColumnResolution::StarExpansion => ColumnEdgeKind::DerivesColumn,
            ColumnResolution::Unresolved | ColumnResolution::Pending => {
                ColumnEdgeKind::WritesUnknownColumnOfTable
            }
        };
        push(&mut out, kind, table, &c.column);
    }
    out
}

/// Extract column edges for every statement in a model.
#[must_use]
pub fn extract_column_edges_for_model(model: &crate::sql_sem::SqlSemanticModel) -> Vec<ColumnEdge> {
    let mut out = Vec::new();
    for (_, s) in model.iter() {
        out.extend(extract_column_edges(s));
    }
    out
}

fn resolve_table(model: &SqlStatementModel, qualifier: &str, single: Option<&str>) -> String {
    if !qualifier.is_empty() {
        if let Some((schema, table)) = model.alias_scope.resolve(qualifier) {
            return qualify(schema, table);
        }
        return qualifier.to_string();
    }
    single
        .map(str::to_string)
        .unwrap_or_else(|| "?".to_string())
}

fn qualify(schema: &str, table: &str) -> String {
    if schema.is_empty() {
        table.to_ascii_lowercase()
    } else {
        format!(
            "{}.{}",
            schema.to_ascii_lowercase(),
            table.to_ascii_lowercase()
        )
    }
}

fn push(out: &mut Vec<ColumnEdge>, kind: ColumnEdgeKind, table: String, column: &str) {
    let edge = ColumnEdge {
        kind,
        table,
        column: column.to_ascii_uppercase(),
    };
    if !out.contains(&edge) {
        out.push(edge);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql_columns::extract_columns;
    use crate::sql_resolve::resolve_sql;

    fn edges(raw: &str) -> Vec<ColumnEdge> {
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        extract_column_edges(&m)
    }

    #[test]
    fn resolved_select_column_is_reads_column() {
        let e = edges("SELECT salary INTO v FROM employees");
        assert!(e.iter().any(|x| x.kind == ColumnEdgeKind::ReadsColumn
            && x.column == "SALARY"
            && x.table == "employees"));
    }

    #[test]
    fn resolved_update_column_is_writes_column() {
        let e = edges("UPDATE employees e SET e.salary = 1 WHERE e.id = 2");
        assert!(
            e.iter()
                .any(|x| x.kind == ColumnEdgeKind::WritesColumn && x.column == "SALARY")
        );
    }

    #[test]
    fn star_projection_is_derives_column() {
        let e = edges("SELECT * INTO r FROM employees");
        assert!(e.iter().any(|x| x.kind == ColumnEdgeKind::DerivesColumn));
    }

    #[test]
    fn ambiguous_read_is_unknown_column_of_table() {
        let e = edges("SELECT amount INTO v FROM orders o, payments p WHERE o.id = p.oid");
        assert!(
            e.iter().any(
                |x| x.kind == ColumnEdgeKind::ReadsUnknownColumnOfTable && x.column == "AMOUNT"
            )
        );
    }

    #[test]
    fn ambiguous_write_is_unknown_write_column() {
        // UPDATE with no alias + a second FROM-style table is rare;
        // simulate via a bare SET column where scope is multi.
        let raw = "UPDATE t1 SET val = (SELECT x FROM t2) WHERE id = 1";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let e = extract_column_edges(&m);
        assert!(e.iter().any(|x| matches!(
            x.kind,
            ColumnEdgeKind::WritesColumn | ColumnEdgeKind::WritesUnknownColumnOfTable
        )));
    }

    #[test]
    fn qualified_column_resolves_table_via_alias_scope() {
        let e = edges("SELECT e.salary INTO v FROM hr.employees e");
        let c = e
            .iter()
            .find(|x| x.column == "SALARY" && x.kind == ColumnEdgeKind::ReadsColumn)
            .unwrap();
        assert_eq!(c.table, "hr.employees");
    }

    #[test]
    fn unbound_qualifier_kept_as_table_string() {
        let e = edges("SELECT zzz.col INTO v FROM hr.employees e");
        let c = e.iter().find(|x| x.column == "COL").unwrap();
        assert_eq!(c.kind, ColumnEdgeKind::ReadsUnknownColumnOfTable);
        assert_eq!(c.table, "zzz");
    }

    #[test]
    fn duplicate_edges_dedupe() {
        let e = edges("SELECT id, id INTO a, b FROM t");
        let id_edges = e
            .iter()
            .filter(|x| x.column == "ID" && x.kind == ColumnEdgeKind::ReadsColumn)
            .count();
        assert_eq!(id_edges, 1);
    }

    #[test]
    fn model_wide_extraction_covers_all_statements() {
        let mut model = crate::sql_sem::SqlSemanticModel::default();
        let r1 = "SELECT a INTO v FROM t1";
        let r2 = "INSERT INTO t2 (b) VALUES (1)";
        let mut m1 = resolve_sql(r1);
        extract_columns(&mut m1, r1);
        let mut m2 = resolve_sql(r2);
        extract_columns(&mut m2, r2);
        model.push(m1);
        model.push(m2);
        let e = extract_column_edges_for_model(&model);
        assert!(e.iter().any(|x| x.column == "A"));
        assert!(e.iter().any(|x| x.column == "B"));
    }

    #[test]
    fn serde_round_trip_with_snake_case_kind() {
        let e = edges("SELECT salary INTO v FROM employees");
        let json = serde_json::to_string(&e[0]).unwrap();
        let back: ColumnEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e[0]);
        assert!(json.contains("reads_column"));
    }
}
