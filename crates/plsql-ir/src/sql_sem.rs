//! Semantic model for embedded SQL statements (PLSQL-SQLSEM-001).
//!
//! `plsql_ir::Statement::Sql` carries the raw SQL text. This
//! module adds the typed structure downstream lineage needs:
//! tables referenced, columns read / written, projection items,
//! and alias scope. Together these form [`SqlStatementModel`] —
//! one per embedded SQL statement — and [`SqlSemanticModel`] —
//! the per-package aggregate the lineage layer consumes.
//!
//! Population happens in two passes:
//!
//! 1. A heuristic recogniser (out of scope for this bead — lands
//!    in PLSQL-SQLSEM-002) walks the raw SQL and emits the
//!    structural pieces.
//! 2. The IR canonicaliser (PLSQL-IR-006) is responsible for
//!    fully-qualifying every `TableUse.table` and
//!    `ColumnUse.column` reference once the alias scope has
//!    been resolved.
//!
//! This bead ships only the types + the constructor helpers so
//! the downstream consumers (lineage, doc, bindings) can program
//! against a stable surface today.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   embedded-SQL grammar plus the column / table / alias
//!   semantics come from the SQL Language Reference chapter
//!   the PL/SQL Language Reference defers to.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_TAB_COLUMNS` is the server-side authority a future
//!   bead will use to cross-check `ColumnUse.column` against
//!   the table's declared columns.

use serde::{Deserialize, Serialize};

/// One embedded SQL statement seen from inside a PL/SQL routine
/// body. Carries the SQL verb (already in `Statement::Sql`), the
/// list of tables touched, the columns read / written, the
/// projection (for SELECT) and the alias-to-table map.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlStatementModel {
    pub verb: SqlSemanticVerb,
    pub tables: Vec<TableUse>,
    pub reads: Vec<ColumnUse>,
    pub writes: Vec<ColumnUse>,
    pub projection: Vec<ProjectionItem>,
    pub alias_scope: AliasScope,
}

/// Aggregate over every embedded SQL statement found in a
/// routine body / package. The lineage layer consumes the
/// aggregate; the doc + bindings layers consume the per-statement
/// model.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlSemanticModel {
    pub statements: Vec<SqlStatementModel>,
}

impl SqlSemanticModel {
    /// Append a statement model. Returns the position so callers
    /// can correlate the model with their source-statement
    /// pointers.
    pub fn push(&mut self, m: SqlStatementModel) -> usize {
        let pos = self.statements.len();
        self.statements.push(m);
        pos
    }

    /// Iterator over every (statement_index, statement) pair.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &SqlStatementModel)> {
        self.statements.iter().enumerate()
    }

    /// Sum of unique `(schema, table)` references across every
    /// statement in the model.
    #[must_use]
    pub fn distinct_tables(&self) -> Vec<(String, String)> {
        let mut out = std::collections::BTreeSet::new();
        for s in &self.statements {
            for t in &s.tables {
                out.insert((t.schema.clone(), t.table.clone()));
            }
        }
        out.into_iter().collect()
    }
}

/// SQL verb classification — distinct from `plsql_ir::SqlVerb`
/// because the semantic model needs to express MERGE's
/// dual-update + insert nature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlSemanticVerb {
    #[default]
    Select,
    Insert,
    Update,
    Delete,
    MergeUpdate,
    MergeInsert,
    MergeDelete,
}

/// One referenced table / view / synonym. `alias` is set when
/// the FROM clause supplied one; otherwise it's the empty string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableUse {
    pub schema: String,
    pub table: String,
    pub alias: String,
    pub usage: TableUsageKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableUsageKind {
    /// Read-side reference (FROM clause, subquery, USING clause).
    Read,
    /// Write-side reference (INSERT INTO / UPDATE / DELETE FROM /
    /// MERGE INTO).
    Write,
    /// Both: a MERGE INTO target that the same statement also
    /// reads from in the USING clause.
    ReadWrite,
}

/// One referenced column. `qualifier` is the alias / table that
/// scopes the reference; empty when the source SQL referenced
/// the column bare (an alias-scope resolver will rewrite later).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnUse {
    pub qualifier: String,
    pub column: String,
    /// Column resolution state — drives lineage's
    /// `ColumnAccessResult::resolution_error`.
    pub resolution: ColumnResolution,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnResolution {
    /// Alias-scope resolver mapped the column to a known table.
    Resolved,
    /// Star expansion (`*` / `t.*`) — resolved structurally but
    /// the column list is the table's full projection.
    StarExpansion,
    /// Resolver could not find the column on any in-scope table.
    Unresolved,
    /// Resolver hasn't run yet; default state right after the
    /// recogniser populates the model.
    #[default]
    Pending,
}

/// One item in a SELECT's projection list. `alias` is the SQL
/// alias (after `AS`) if present; `expression_text` carries the
/// raw expression so downstream readers can re-parse it via
/// `plsql_ir::lower_expression`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionItem {
    pub alias: String,
    pub expression_text: String,
    /// True when this item is a literal star (`*`) or a
    /// qualified star (`t.*`).
    pub is_star: bool,
}

/// Map of alias → fully-qualified table. The lineage resolver
/// consults this to rewrite bare `col` into `<schema>.<table>.<col>`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasScope {
    pub bindings: Vec<AliasBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasBinding {
    pub alias: String,
    pub schema: String,
    pub table: String,
}

impl AliasScope {
    /// Add a binding. Later bindings shadow earlier ones with the
    /// same alias (Oracle behaviour on duplicate alias).
    pub fn bind(&mut self, alias: &str, schema: &str, table: &str) {
        self.bindings.retain(|b| b.alias != alias);
        self.bindings.push(AliasBinding {
            alias: alias.into(),
            schema: schema.into(),
            table: table.into(),
        });
    }

    /// Look up the fully-qualified target for `alias`, returning
    /// `(schema, table)` if bound. Lookup is case-insensitive on
    /// the alias key.
    #[must_use]
    pub fn resolve(&self, alias: &str) -> Option<(&str, &str)> {
        let needle = alias.to_ascii_uppercase();
        self.bindings
            .iter()
            .rev()
            .find(|b| b.alias.eq_ignore_ascii_case(&needle))
            .map(|b| (b.schema.as_str(), b.table.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(schema: &str, name: &str, alias: &str, usage: TableUsageKind) -> TableUse {
        TableUse {
            schema: schema.into(),
            table: name.into(),
            alias: alias.into(),
            usage,
        }
    }

    fn col(qual: &str, name: &str) -> ColumnUse {
        ColumnUse {
            qualifier: qual.into(),
            column: name.into(),
            resolution: ColumnResolution::Pending,
        }
    }

    #[test]
    fn default_model_is_empty_select() {
        let m = SqlStatementModel::default();
        assert_eq!(m.verb, SqlSemanticVerb::Select);
        assert!(m.tables.is_empty());
        assert!(m.projection.is_empty());
        assert!(m.alias_scope.bindings.is_empty());
    }

    #[test]
    fn push_returns_position_and_appends() {
        let mut m = SqlSemanticModel::default();
        let p0 = m.push(SqlStatementModel::default());
        let p1 = m.push(SqlStatementModel::default());
        assert_eq!(p0, 0);
        assert_eq!(p1, 1);
        assert_eq!(m.statements.len(), 2);
    }

    #[test]
    fn distinct_tables_dedupes_across_statements() {
        let mut model = SqlSemanticModel::default();
        let mut s = SqlStatementModel::default();
        s.tables
            .push(table("HR", "EMPLOYEES", "e", TableUsageKind::Read));
        model.push(s.clone());
        model.push(s); // duplicate
        assert_eq!(model.distinct_tables().len(), 1);
    }

    #[test]
    fn distinct_tables_keeps_distinct_schema_table_pairs() {
        let mut model = SqlSemanticModel::default();
        let mut s1 = SqlStatementModel::default();
        s1.tables
            .push(table("HR", "EMPLOYEES", "", TableUsageKind::Read));
        let mut s2 = SqlStatementModel::default();
        s2.tables
            .push(table("HR", "DEPARTMENTS", "", TableUsageKind::Read));
        model.push(s1);
        model.push(s2);
        let distinct = model.distinct_tables();
        assert_eq!(distinct.len(), 2);
    }

    #[test]
    fn alias_scope_bind_and_resolve() {
        let mut scope = AliasScope::default();
        scope.bind("e", "HR", "EMPLOYEES");
        scope.bind("d", "HR", "DEPARTMENTS");
        assert_eq!(scope.resolve("e"), Some(("HR", "EMPLOYEES")));
        // Case-insensitive lookup.
        assert_eq!(scope.resolve("E"), Some(("HR", "EMPLOYEES")));
        assert_eq!(scope.resolve("d"), Some(("HR", "DEPARTMENTS")));
        assert_eq!(scope.resolve("x"), None);
    }

    #[test]
    fn alias_scope_shadows_duplicate_alias() {
        let mut scope = AliasScope::default();
        scope.bind("t", "HR", "EMPLOYEES");
        scope.bind("t", "HR", "DEPARTMENTS");
        // Latest binding wins.
        assert_eq!(scope.resolve("t"), Some(("HR", "DEPARTMENTS")));
        // And only one binding remains.
        assert_eq!(scope.bindings.len(), 1);
    }

    #[test]
    fn column_resolution_default_is_pending() {
        let c = col("e", "salary");
        assert_eq!(c.resolution, ColumnResolution::Pending);
    }

    #[test]
    fn projection_item_carries_alias_and_star_flag() {
        let p = ProjectionItem {
            alias: "name_lower".into(),
            expression_text: "LOWER(e.name)".into(),
            is_star: false,
        };
        assert!(!p.is_star);
        let star = ProjectionItem {
            alias: String::new(),
            expression_text: "*".into(),
            is_star: true,
        };
        assert!(star.is_star);
    }

    #[test]
    fn merge_verbs_are_distinct_from_select() {
        assert_ne!(SqlSemanticVerb::MergeUpdate, SqlSemanticVerb::Select);
        assert_ne!(SqlSemanticVerb::MergeInsert, SqlSemanticVerb::MergeUpdate);
    }

    #[test]
    fn round_trip_through_serde() {
        let mut model = SqlSemanticModel::default();
        let mut s = SqlStatementModel {
            verb: SqlSemanticVerb::Update,
            ..SqlStatementModel::default()
        };
        s.tables
            .push(table("HR", "EMPLOYEES", "e", TableUsageKind::Write));
        s.writes.push(col("e", "salary"));
        s.alias_scope.bind("e", "HR", "EMPLOYEES");
        model.push(s);
        let json = serde_json::to_string(&model).unwrap();
        let back: SqlSemanticModel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, model);
        // Snake-case wire tags.
        assert!(json.contains("\"verb\":\"update\""));
    }

    #[test]
    fn iter_yields_each_statement_with_index() {
        let mut model = SqlSemanticModel::default();
        model.push(SqlStatementModel::default());
        model.push(SqlStatementModel::default());
        model.push(SqlStatementModel::default());
        let collected: Vec<usize> = model.iter().map(|(i, _)| i).collect();
        assert_eq!(collected, vec![0, 1, 2]);
    }
}
