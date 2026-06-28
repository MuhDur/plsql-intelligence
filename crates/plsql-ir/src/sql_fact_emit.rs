//! Emit SQL table/column-use facts with precision markers.
//!
//! Walks a populated [`SqlStatementModel`] (tables + columns
//! filled by SQLSEM-002 / SQLSEM-003) and emits normalized
//! [`Fact`]s into a [`FactStore`]. Every fact carries a precision
//! marker so the lineage layer can weight the edge:
//!
//! * `exact` — table/column resolved against a single bound
//!   alias or single-table scope.
//! * `expression` — column came from a projection expression
//!   (function call / arithmetic) rather than a bare reference.
//! * `unknown` — bare column with ambiguous (multi-table) scope
//!   or a qualifier that didn't bind.
//!
//! The marker is encoded into the `DependencyEdge.edge_kind`
//! string (`ReadsColumn:exact`, `WritesColumn:unknown`, …) so it
//! survives the FACT-001 wire shape without a schema change.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   table/column reference grammar.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_TAB_COLUMNS` / `ALL_DEPENDENCIES` are the server-side
//!   mirrors; the precision marker records how confident the
//!   source-only pass is before that cross-check.

use crate::fact::{FactPayload, FactProvenance, FactStore};
use crate::sql_sem::{ColumnResolution, ColumnUse, SqlStatementModel, TableUsageKind};

/// Emit table-level + column-level use facts for one statement.
/// `owner_logical_id` is the routine the statement lives in (the
/// `from` side of every edge). Returns the post-dedup count of
/// facts added.
pub fn emit_sql_use_facts(
    store: &mut FactStore,
    prov: &FactProvenance,
    owner_logical_id: &str,
    model: &SqlStatementModel,
) -> usize {
    let before = store.len();

    // Table-level edges.
    for t in &model.tables {
        let target = qualify(&t.schema, &t.table);
        let kind = match t.usage {
            TableUsageKind::Read => "Reads",
            TableUsageKind::Write => "Writes",
            TableUsageKind::ReadWrite => "ReadsWrites",
        };
        push_edge(store, prov, owner_logical_id, &target, kind);
    }

    // Column-level edges with precision markers.
    for c in &model.reads {
        emit_column(store, prov, owner_logical_id, model, c, "ReadsColumn");
    }
    for c in &model.writes {
        emit_column(store, prov, owner_logical_id, model, c, "WritesColumn");
    }

    store.len() - before
}

/// Emit use facts for every statement in a `SqlSemanticModel`.
pub fn emit_sql_use_facts_for_model(
    store: &mut FactStore,
    prov: &FactProvenance,
    owner_logical_id: &str,
    model: &crate::sql_sem::SqlSemanticModel,
) -> usize {
    let before = store.len();
    for (_, s) in model.iter() {
        emit_sql_use_facts(store, prov, owner_logical_id, s);
    }
    store.len() - before
}

fn emit_column(
    store: &mut FactStore,
    prov: &FactProvenance,
    owner: &str,
    model: &SqlStatementModel,
    c: &ColumnUse,
    base_kind: &str,
) {
    let marker = precision_marker(c);
    // Resolve the column's table via the alias scope when the
    // qualifier is bound; otherwise leave the qualifier as-is so
    // the catalog cross-check can finish the job.
    let target = if c.qualifier.is_empty() {
        // single-table scope: attribute to the lone table.
        if model.tables.len() == 1 {
            let t = &model.tables[0];
            format!("{}.{}", qualify(&t.schema, &t.table), c.column)
        } else {
            format!("?.{}", c.column)
        }
    } else if let Some((schema, table)) = model.alias_scope.resolve(&c.qualifier) {
        format!("{}.{}", qualify(schema, table), c.column)
    } else {
        format!("{}.{}", c.qualifier, c.column)
    };
    push_edge(
        store,
        prov,
        owner,
        &target,
        &format!("{base_kind}:{marker}"),
    );
}

fn precision_marker(c: &ColumnUse) -> &'static str {
    match c.resolution {
        ColumnResolution::Resolved => "exact",
        ColumnResolution::StarExpansion => "expression",
        ColumnResolution::Unresolved => "unknown",
        ColumnResolution::Pending => "unknown",
    }
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

fn push_edge(store: &mut FactStore, prov: &FactProvenance, from: &str, to: &str, edge_kind: &str) {
    let f = crate::fact::mint_fact(
        prov.clone(),
        FactPayload::DependencyEdge {
            from_logical_id: from.to_string(),
            to_logical_id: to.to_string(),
            edge_kind: edge_kind.to_string(),
        },
    );
    store.push(f);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fact::FactKind;
    use crate::sql_columns::extract_columns;
    use crate::sql_resolve::resolve_sql;

    fn prov() -> FactProvenance {
        FactProvenance {
            component: "plsql-ir".into(),
            component_version: "0.1.0".into(),
            run_id: String::new(),
            source_logical_id: None,
            source_file: None,
        }
    }

    fn edge_kinds(store: &FactStore) -> Vec<String> {
        store
            .by_kind(FactKind::DependencyEdge)
            .filter_map(|f| match &f.payload {
                FactPayload::DependencyEdge { edge_kind, .. } => Some(edge_kind.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn select_emits_reads_table_and_exact_columns() {
        let raw = "SELECT salary INTO v FROM employees";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let mut store = FactStore::default();
        let n = emit_sql_use_facts(&mut store, &prov(), "hr.run", &m);
        assert!(n >= 2);
        let kinds = edge_kinds(&store);
        assert!(kinds.iter().any(|k| k == "Reads"));
        assert!(kinds.iter().any(|k| k == "ReadsColumn:exact"));
    }

    #[test]
    fn ambiguous_column_marked_unknown() {
        let raw = "SELECT amount INTO v FROM orders o, payments p WHERE o.id = p.oid";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let mut store = FactStore::default();
        emit_sql_use_facts(&mut store, &prov(), "hr.run", &m);
        let kinds = edge_kinds(&store);
        assert!(kinds.iter().any(|k| k == "ReadsColumn:unknown"));
    }

    #[test]
    fn star_projection_marked_expression() {
        let raw = "SELECT * INTO r FROM employees";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let mut store = FactStore::default();
        emit_sql_use_facts(&mut store, &prov(), "hr.run", &m);
        let kinds = edge_kinds(&store);
        assert!(kinds.iter().any(|k| k == "ReadsColumn:expression"));
    }

    #[test]
    fn insert_emits_writes_table_and_columns() {
        let raw = "INSERT INTO audit (event_id, ts) VALUES (1, SYSDATE)";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let mut store = FactStore::default();
        emit_sql_use_facts(&mut store, &prov(), "hr.run", &m);
        let kinds = edge_kinds(&store);
        assert!(kinds.iter().any(|k| k == "Writes"));
        assert!(kinds.iter().any(|k| k.starts_with("WritesColumn:")));
    }

    #[test]
    fn merge_emits_readswrites_table_edge() {
        let raw = "MERGE INTO target t USING source s ON (t.id = s.id) WHEN MATCHED THEN UPDATE SET t.v = s.v";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let mut store = FactStore::default();
        emit_sql_use_facts(&mut store, &prov(), "hr.run", &m);
        let kinds = edge_kinds(&store);
        assert!(kinds.iter().any(|k| k == "ReadsWrites"));
        assert!(kinds.iter().any(|k| k == "Reads"));
    }

    #[test]
    fn column_target_resolves_through_alias_scope() {
        let raw = "SELECT e.salary INTO v FROM hr.employees e";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let mut store = FactStore::default();
        emit_sql_use_facts(&mut store, &prov(), "hr.run", &m);
        let targets: Vec<String> = store
            .by_kind(FactKind::DependencyEdge)
            .filter_map(|f| match &f.payload {
                FactPayload::DependencyEdge {
                    to_logical_id,
                    edge_kind,
                    ..
                } if edge_kind.starts_with("ReadsColumn") => Some(to_logical_id.clone()),
                _ => None,
            })
            .collect();
        assert!(targets.iter().any(|t| t == "hr.employees.SALARY"));
    }

    #[test]
    fn facts_dedupe_on_repeat_emit() {
        let raw = "SELECT salary INTO v FROM employees";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let mut store = FactStore::default();
        emit_sql_use_facts(&mut store, &prov(), "hr.run", &m);
        let after_first = store.len();
        let n2 = emit_sql_use_facts(&mut store, &prov(), "hr.run", &m);
        assert_eq!(n2, 0);
        assert_eq!(store.len(), after_first);
    }

    #[test]
    fn model_wide_emit_covers_every_statement() {
        let mut model = crate::sql_sem::SqlSemanticModel::default();
        let r1 = "SELECT id INTO v FROM t1";
        let r2 = "INSERT INTO t2 (c) VALUES (1)";
        let mut m1 = resolve_sql(r1);
        extract_columns(&mut m1, r1);
        let mut m2 = resolve_sql(r2);
        extract_columns(&mut m2, r2);
        model.push(m1);
        model.push(m2);
        let mut store = FactStore::default();
        let n = emit_sql_use_facts_for_model(&mut store, &prov(), "hr.run", &model);
        assert!(n >= 4);
        let kinds = edge_kinds(&store);
        assert!(kinds.iter().any(|k| k == "Reads"));
        assert!(kinds.iter().any(|k| k == "Writes"));
    }

    #[test]
    fn precision_marker_maps_all_resolutions() {
        let mk = |r| ColumnUse {
            qualifier: String::new(),
            column: "C".into(),
            resolution: r,
        };
        assert_eq!(precision_marker(&mk(ColumnResolution::Resolved)), "exact");
        assert_eq!(
            precision_marker(&mk(ColumnResolution::StarExpansion)),
            "expression"
        );
        assert_eq!(
            precision_marker(&mk(ColumnResolution::Unresolved)),
            "unknown"
        );
        assert_eq!(precision_marker(&mk(ColumnResolution::Pending)), "unknown");
    }

    #[test]
    fn empty_model_emits_nothing() {
        let m = SqlStatementModel::default();
        let mut store = FactStore::default();
        let n = emit_sql_use_facts(&mut store, &prov(), "hr.run", &m);
        assert_eq!(n, 0);
    }
}
