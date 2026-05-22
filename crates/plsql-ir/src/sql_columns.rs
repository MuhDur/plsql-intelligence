//! Projection + column read/write extraction (PLSQL-SQLSEM-003).
//!
//! Builds on the table/alias resolution from PLSQL-SQLSEM-002
//! (`sql_resolve`). Given a `SqlStatementModel` whose `tables` +
//! `alias_scope` are populated, this pass fills `projection`,
//! `reads`, and `writes` by walking the SELECT list, the
//! INSERT/UPDATE column targets, and the WHERE/SET/ON predicate
//! columns — attaching a [`ColumnResolution`] verdict to each.
//!
//! Resolution rules:
//!
//! * `alias.col` → look the alias up in `AliasScope`; if found,
//!   `Resolved`; if the alias isn't bound, `Unresolved`.
//! * bare `col` with exactly one table in scope → `Resolved`
//!   against that table.
//! * bare `col` with multiple tables in scope → `Unresolved`
//!   (ambiguous without catalog column lists; the catalog
//!   cross-check bead disambiguates later).
//! * `*` / `alias.*` → `StarExpansion`.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   SELECT-list / SET-clause / predicate grammar defers to the
//!   SQL Language Reference.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_TAB_COLUMNS` is the authority that turns an
//!   `Unresolved` bare column into a `Resolved` one once the
//!   catalog is available (deferred bead).

use crate::sql_sem::{
    ColumnResolution, ColumnUse, ProjectionItem, SqlSemanticModel, SqlSemanticVerb,
    SqlStatementModel,
};

/// Populate `projection`, `reads`, `writes` on `model` from its
/// raw text + already-resolved `tables` / `alias_scope`.
/// `raw` is the original SQL text (the model doesn't keep it).
pub fn extract_columns(model: &mut SqlStatementModel, raw: &str) {
    let single_table = model.tables.len() == 1;
    match model.verb {
        SqlSemanticVerb::Select => {
            let proj = parse_select_list(raw);
            for item in &proj {
                classify_projection_reads(item, single_table, model);
            }
            model.projection = proj;
            for c in predicate_columns(raw) {
                push_read(model, c, single_table);
            }
        }
        SqlSemanticVerb::Insert => {
            for c in insert_target_columns(raw) {
                push_write(model, c, single_table);
            }
            // Sub-SELECT projection columns are reads.
            for item in parse_select_list(raw) {
                if !item.is_star {
                    push_read_name(model, &item.expression_text, single_table);
                }
            }
        }
        SqlSemanticVerb::Update => {
            for c in update_set_columns(raw) {
                push_write(model, c, single_table);
            }
            for c in predicate_columns(raw) {
                push_read(model, c, single_table);
            }
        }
        SqlSemanticVerb::Delete => {
            for c in predicate_columns(raw) {
                push_read(model, c, single_table);
            }
        }
        SqlSemanticVerb::MergeUpdate
        | SqlSemanticVerb::MergeInsert
        | SqlSemanticVerb::MergeDelete => {
            for c in update_set_columns(raw) {
                push_write(model, c, single_table);
            }
            for c in predicate_columns(raw) {
                push_read(model, c, single_table);
            }
        }
    }
}

/// Convenience: run `extract_columns` over every statement in a
/// `SqlSemanticModel`. The caller supplies the raw text per
/// statement (the model is text-free by design).
pub fn extract_columns_for_model(model: &mut SqlSemanticModel, raws: &[String]) {
    for (i, stmt) in model.statements.iter_mut().enumerate() {
        if let Some(raw) = raws.get(i) {
            extract_columns(stmt, raw);
        }
    }
}

fn parse_select_list(raw: &str) -> Vec<ProjectionItem> {
    let upper = raw.to_ascii_uppercase();
    let Some(sel) = upper.find("SELECT") else {
        return Vec::new();
    };
    let after = sel + "SELECT".len();
    // Stop the projection list at INTO or FROM (whichever first).
    let into = upper[after..].find(" INTO ").map(|p| after + p);
    let from = upper[after..].find(" FROM ").map(|p| after + p);
    let end = [into, from]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(raw.len());
    let list = raw[after..end].trim();
    split_top_level_commas(list)
        .into_iter()
        .map(|piece| parse_projection_item(piece.trim()))
        .filter(|p| !p.expression_text.is_empty())
        .collect()
}

fn parse_projection_item(piece: &str) -> ProjectionItem {
    let is_star = piece == "*" || piece.ends_with(".*");
    // `expr AS alias` / `expr alias`.
    let upper = piece.to_ascii_uppercase();
    if let Some(as_pos) = upper.rfind(" AS ") {
        let expr = piece[..as_pos].trim().to_string();
        let alias = piece[as_pos + 4..].trim().to_string();
        return ProjectionItem {
            alias,
            expression_text: expr,
            is_star,
        };
    }
    // Trailing-token alias only if there's whitespace and the
    // last token is a bare identifier (avoid splitting
    // `a.b` or `fn(x)`).
    if let Some(ws) = piece.rfind(char::is_whitespace) {
        let head = piece[..ws].trim();
        let tail = piece[ws..].trim();
        if !head.is_empty()
            && tail.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !head.ends_with(['(', ','])
            && !is_star
        {
            return ProjectionItem {
                alias: tail.to_string(),
                expression_text: head.to_string(),
                is_star,
            };
        }
    }
    ProjectionItem {
        alias: String::new(),
        expression_text: piece.to_string(),
        is_star,
    }
}

fn classify_projection_reads(
    item: &ProjectionItem,
    single_table: bool,
    model: &mut SqlStatementModel,
) {
    if item.is_star {
        let (qual, _col) = split_qualifier(&item.expression_text);
        model.reads.push(ColumnUse {
            qualifier: qual,
            column: "*".to_string(),
            resolution: ColumnResolution::StarExpansion,
        });
        return;
    }
    // Pull bare column identifiers from the expression.
    for ident in column_idents(&item.expression_text) {
        push_read_name(model, &ident, single_table);
    }
}

fn push_read(model: &mut SqlStatementModel, col: String, single_table: bool) {
    push_read_name(model, &col, single_table);
}

fn push_read_name(model: &mut SqlStatementModel, name: &str, single_table: bool) {
    if let Some(cu) = make_column_use(name, single_table, model) {
        if !model.reads.contains(&cu) {
            model.reads.push(cu);
        }
    }
}

fn push_write(model: &mut SqlStatementModel, col: String, single_table: bool) {
    if let Some(cu) = make_column_use(&col, single_table, model) {
        if !model.writes.contains(&cu) {
            model.writes.push(cu);
        }
    }
}

fn make_column_use(name: &str, single_table: bool, model: &SqlStatementModel) -> Option<ColumnUse> {
    let name = name.trim();
    if name.is_empty() || is_sql_noise(name) {
        return None;
    }
    let (qualifier, column) = split_qualifier(name);
    if column.is_empty() || !column.chars().next()?.is_ascii_alphabetic() {
        return None;
    }
    let resolution = if !qualifier.is_empty() {
        if model.alias_scope.resolve(&qualifier).is_some() {
            ColumnResolution::Resolved
        } else {
            ColumnResolution::Unresolved
        }
    } else if single_table {
        ColumnResolution::Resolved
    } else {
        ColumnResolution::Unresolved
    };
    Some(ColumnUse {
        qualifier,
        column: column.to_ascii_uppercase(),
        resolution,
    })
}

fn split_qualifier(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((q, c)) => (q.trim().to_string(), c.trim().to_string()),
        None => (String::new(), name.trim().to_string()),
    }
}

fn column_idents(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in expr.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' || ch == '#' || ch == '.' {
            cur.push(ch);
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.into_iter().filter(|w| !is_sql_noise(w)).collect()
}

fn insert_target_columns(raw: &str) -> Vec<String> {
    // INSERT INTO t (c1, c2, …) VALUES …  — pull the paren list
    // immediately after the table name.
    let upper = raw.to_ascii_uppercase();
    let Some(into) = upper.find("INTO") else {
        return Vec::new();
    };
    let rest = &raw[into + 4..];
    let Some(open) = rest.find('(') else {
        return Vec::new();
    };
    let Some(close) = rest[open..].find(')') else {
        return Vec::new();
    };
    split_top_level_commas(&rest[open + 1..open + close])
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn update_set_columns(raw: &str) -> Vec<String> {
    let upper = raw.to_ascii_uppercase();
    let Some(set) = upper.find(" SET ") else {
        return Vec::new();
    };
    let after = set + 5;
    let end = upper[after..]
        .find(" WHERE ")
        .map(|p| after + p)
        .unwrap_or(raw.len());
    split_top_level_commas(&raw[after..end])
        .into_iter()
        .filter_map(|assign| assign.split('=').next().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

fn predicate_columns(raw: &str) -> Vec<String> {
    let upper = raw.to_ascii_uppercase();
    let Some(w) = upper.find(" WHERE ") else {
        return Vec::new();
    };
    let pred = &raw[w + 7..];
    // Stop at GROUP/ORDER/HAVING.
    let pu = pred.to_ascii_uppercase();
    let stop = ["GROUP ", "ORDER ", "HAVING ", "CONNECT "]
        .iter()
        .filter_map(|kw| pu.find(kw))
        .min()
        .unwrap_or(pred.len());
    column_idents(&pred[..stop])
}

fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut buf = String::new();
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                buf.push(ch);
            }
            ')' => {
                depth -= 1;
                buf.push(ch);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut buf)),
            _ => buf.push(ch),
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf);
    }
    out
}

fn is_sql_noise(w: &str) -> bool {
    let u = w.to_ascii_uppercase();
    matches!(
        u.as_str(),
        "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "IS"
            | "IN"
            | "LIKE"
            | "BETWEEN"
            | "EXISTS"
            | "TRUE"
            | "FALSE"
            | "FROM"
            | "WHERE"
            | "SELECT"
            | "INTO"
            | "VALUES"
            | "SET"
            | "DUAL"
            | "SYSDATE"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "DISTINCT"
            | "AS"
            | "ON"
            | "USING"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
    ) || u.chars().all(|c| c.is_ascii_digit() || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql_resolve::resolve_sql;

    #[test]
    fn select_list_columns_become_reads() {
        let raw = "SELECT e.id, e.name INTO a, b FROM employees e";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        assert_eq!(m.projection.len(), 2);
        let cols: Vec<&str> = m.reads.iter().map(|c| c.column.as_str()).collect();
        assert!(cols.contains(&"ID"));
        assert!(cols.contains(&"NAME"));
        // Alias `e` is bound → Resolved.
        assert!(
            m.reads
                .iter()
                .all(|c| c.resolution == ColumnResolution::Resolved)
        );
    }

    #[test]
    fn star_projection_is_star_expansion() {
        let raw = "SELECT * INTO r FROM employees";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        assert!(m.projection.iter().any(|p| p.is_star));
        assert!(
            m.reads
                .iter()
                .any(|c| c.resolution == ColumnResolution::StarExpansion)
        );
    }

    #[test]
    fn bare_column_single_table_resolved() {
        let raw = "SELECT salary INTO v FROM employees";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let c = m.reads.iter().find(|c| c.column == "SALARY").unwrap();
        assert_eq!(c.resolution, ColumnResolution::Resolved);
    }

    #[test]
    fn bare_column_multi_table_unresolved() {
        let raw = "SELECT amount INTO v FROM orders o, payments p WHERE o.id = p.oid";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let c = m.reads.iter().find(|c| c.column == "AMOUNT");
        assert_eq!(c.map(|c| c.resolution), Some(ColumnResolution::Unresolved));
    }

    #[test]
    fn qualified_unbound_alias_is_unresolved() {
        let raw = "SELECT zzz.col INTO v FROM employees e";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let c = m.reads.iter().find(|c| c.column == "COL").unwrap();
        assert_eq!(c.resolution, ColumnResolution::Unresolved);
    }

    #[test]
    fn insert_target_columns_become_writes() {
        let raw = "INSERT INTO audit (event_id, ts) VALUES (1, SYSDATE)";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let cols: Vec<&str> = m.writes.iter().map(|c| c.column.as_str()).collect();
        assert!(cols.contains(&"EVENT_ID"));
        assert!(cols.contains(&"TS"));
    }

    #[test]
    fn update_set_columns_become_writes() {
        let raw = "UPDATE employees e SET e.salary = e.salary * 1.1 WHERE e.id = 1";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        assert!(m.writes.iter().any(|c| c.column == "SALARY"));
        // WHERE column is a read.
        assert!(m.reads.iter().any(|c| c.column == "ID"));
    }

    #[test]
    fn delete_predicate_columns_are_reads() {
        let raw = "DELETE FROM stale WHERE created < SYSDATE - 30";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        assert!(m.reads.iter().any(|c| c.column == "CREATED"));
        assert!(m.writes.is_empty());
    }

    #[test]
    fn projection_alias_split() {
        let raw = "SELECT e.salary AS pay INTO v FROM employees e";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let p = &m.projection[0];
        assert_eq!(p.alias, "pay");
        assert_eq!(p.expression_text, "e.salary");
    }

    #[test]
    fn sql_noise_not_recorded_as_columns() {
        let raw = "SELECT id INTO v FROM employees WHERE id IS NOT NULL AND id > 0";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let cols: Vec<&str> = m.reads.iter().map(|c| c.column.as_str()).collect();
        assert!(
            !cols
                .iter()
                .any(|c| *c == "NULL" || *c == "AND" || *c == "NOT")
        );
        assert!(cols.contains(&"ID"));
    }

    #[test]
    fn extract_columns_for_model_walks_all_statements() {
        let mut model = SqlSemanticModel::default();
        model.push(resolve_sql("SELECT id INTO v FROM t1"));
        model.push(resolve_sql("SELECT name INTO v FROM t2"));
        extract_columns_for_model(
            &mut model,
            &[
                "SELECT id INTO v FROM t1".to_string(),
                "SELECT name INTO v FROM t2".to_string(),
            ],
        );
        assert!(model.statements[0].reads.iter().any(|c| c.column == "ID"));
        assert!(model.statements[1].reads.iter().any(|c| c.column == "NAME"));
    }

    #[test]
    fn serde_round_trip_preserves_column_resolution() {
        let raw = "SELECT salary INTO v FROM employees";
        let mut m = resolve_sql(raw);
        extract_columns(&mut m, raw);
        let json = serde_json::to_string(&m).unwrap();
        let back: SqlStatementModel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}
