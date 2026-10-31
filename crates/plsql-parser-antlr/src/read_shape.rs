//! Rule-whitelisted syntactic shape of one Oracle read query (pvlqq.10.18).
//!
//! [`read_query_shape`] parses SQL text with the ANTLR grammar and reports
//! the syntax a read-only classifier needs: query blocks, CTEs, set
//! branches, FROM items with exact names and aliases, joins, value-position
//! names, function calls and subqueries. It applies no Oracle semantics: it
//! never resolves names, never decides which identifiers are built-ins and
//! never judges purity. Those decisions stay with the consumer.
//!
//! The walk fails closed. Every grammar rule is either interpreted at a known
//! position or listed as transparent; any other rule refuses. An identifier
//! reached anywhere except an interpreted position refuses
//! ([`ShapeRefusal::UnaccountedIdentifier`]), so a name the shape does not
//! report cannot exist in the statement. Lexer or parser diagnostics,
//! multiple statements, NUL bytes and size/depth limits all refuse.
#![cfg(feature = "antlr-codegen")]

use std::cell::Cell;
use std::rc::Rc;

use antlr4rust::common_token_stream::CommonTokenStream;
use antlr4rust::error_listener::ErrorListener;
use antlr4rust::errors::ANTLRError;
use antlr4rust::input_stream::InputStream;
use antlr4rust::parser::Parser;
use antlr4rust::recognizer::Recognizer;
use antlr4rust::token_factory::TokenFactory;

use crate::generated::plsqllexer::PlSqlLexer;
use crate::generated::plsqlparser::{_LITERAL_NAMES, PlSqlParser, PlSqlParserContext, ruleNames};

/// Largest accepted statement, in bytes.
pub const MAX_SHAPE_SQL_BYTES: usize = 256 * 1024;
const MAX_NODES: usize = 200_000;
const MAX_DEPTH: usize = 400;

/// Why a statement has no complete read shape. Every variant means "refuse".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeRefusal {
    /// The text contains a NUL byte or exceeds [`MAX_SHAPE_SQL_BYTES`].
    InvalidInput,
    /// The lexer or parser reported at least one syntax error.
    SyntaxError,
    /// The text is not exactly one SELECT statement.
    NotSingleSelect,
    /// A grammar rule outside the whitelist, named by its grammar rule.
    UnsupportedRule(&'static str),
    /// A construct the shape cannot represent exactly.
    UnsupportedConstruct(&'static str),
    /// An identifier appeared outside every interpreted position.
    UnaccountedIdentifier,
    /// The node or depth limit was exceeded.
    LimitExceeded,
}

/// One identifier exactly as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeIdent {
    /// The identifier text; for a quoted identifier, without the quotes.
    pub text: String,
    /// `true` for a `"quoted"` identifier.
    pub quoted: bool,
}

/// A complete read query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadQueryShape {
    pub query: ShapeQuery,
}

/// A query: optional CTEs, one or more set branches and trailing clauses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeQuery {
    pub ctes: Vec<ShapeCte>,
    /// The first branch, followed by each `(operator, branch)` in order.
    pub first: ShapeBranch,
    pub rest: Vec<(ShapeSetOp, ShapeBranch)>,
    /// ORDER BY, OFFSET and FETCH expressions of this query.
    pub order_by: Vec<ShapeExpr>,
    pub row_limit: Vec<ShapeExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeCte {
    pub name: ShapeIdent,
    pub columns: Vec<ShapeIdent>,
    pub query: ShapeQuery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeSetOp {
    Union,
    UnionAll,
    Intersect,
    Minus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeBranch {
    Select(Box<ShapeSelect>),
    /// A parenthesized query used as a set branch.
    Query(Box<ShapeQuery>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeSelect {
    pub distinct: bool,
    pub items: Vec<ShapeSelectItem>,
    pub from: Vec<ShapeFromItem>,
    pub where_clause: Option<ShapeExpr>,
    pub hierarchy: Option<ShapeHierarchy>,
    pub group_by: Vec<ShapeExpr>,
    pub having: Option<ShapeExpr>,
    pub order_by: Vec<ShapeExpr>,
    /// OFFSET and FETCH expressions of this query block.
    pub row_limit: Vec<ShapeExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeSelectItem {
    /// `*`
    Wildcard,
    /// `t.*` / `s.t.*`
    QualifiedWildcard(Vec<ShapeIdent>),
    Expr {
        expr: ShapeExpr,
        alias: Option<ShapeIdent>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeHierarchy {
    pub start_with: Option<ShapeExpr>,
    pub connect_by: ShapeExpr,
    pub nocycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeFromItem {
    pub source: ShapeTableRef,
    pub joins: Vec<ShapeJoin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeTableRef {
    pub source: ShapeSource,
    pub alias: Option<ShapeIdent>,
    /// `AS OF …` / `VERSIONS BETWEEN … AND …` expressions.
    pub flashback: Vec<ShapeExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeSource {
    Table {
        name: Vec<ShapeIdent>,
        db_link: Option<Vec<ShapeIdent>>,
        partition: Option<ShapePartition>,
    },
    Derived(Box<ShapeQuery>),
}

/// A partition extension: named partitions are not values; `PARTITION FOR`
/// keys are expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapePartition {
    Named(Vec<ShapeIdent>),
    For(Vec<ShapeExpr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeJoinKind {
    Inner,
    Left,
    Right,
    Full,
    Cross,
    CrossApply,
    OuterApply,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeJoin {
    pub kind: ShapeJoinKind,
    pub natural: bool,
    pub table: ShapeTableRef,
    pub on: Option<ShapeExpr>,
    pub using: Vec<ShapeIdent>,
    /// `PARTITION BY (…)` expressions of a partitioned outer join.
    pub partition_by: Vec<ShapeExpr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeSubqueryKind {
    Scalar,
    Exists,
    In,
    /// `ANY`/`SOME`/`ALL (subquery)` and `MULTISET (subquery)`.
    Quantified,
}

/// An expression reduced to what a read classifier needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeExpr {
    /// A literal, bind variable or keyword constant.
    Literal,
    /// `*` inside `COUNT(*)`.
    Star,
    /// A (possibly dotted) name in value position.
    Name(Vec<ShapeIdent>),
    /// A call. `builtin` is `true` for a grammar-level standard function
    /// (`COUNT`, `CAST`, `JSON_VALUE`, …) whose name is a keyword.
    Call {
        name: Vec<ShapeIdent>,
        builtin: bool,
        args: Vec<ShapeExpr>,
    },
    Subquery {
        kind: ShapeSubqueryKind,
        query: Box<ShapeQuery>,
    },
    /// Any operator, CASE or clause combining the listed sub-expressions.
    Group(Vec<ShapeExpr>),
}

type Node<'i> = Rc<dyn PlSqlParserContext<'i> + 'i>;

fn rule_of(node: &Node<'_>) -> Option<&'static str> {
    ruleNames.get(node.get_rule_index()).copied()
}

fn children<'i>(node: &Node<'i>) -> Vec<Node<'i>> {
    (0..node.get_child_count())
        .filter_map(|index| node.get_child(index))
        .collect()
}

fn keyword(node: &Node<'_>) -> Option<String> {
    rule_of(node)
        .is_none()
        .then(|| node.get_text().to_ascii_uppercase())
}

struct ErrorCount(Rc<Cell<usize>>);

impl<'a, T: Recognizer<'a>> ErrorListener<'a, T> for ErrorCount {
    fn syntax_error(
        &self,
        _recognizer: &T,
        _offending_symbol: Option<&<T::TF as TokenFactory<'a>>::Inner>,
        _line: isize,
        _column: isize,
        _msg: &str,
        _error: Option<&ANTLRError>,
    ) {
        self.0.set(self.0.get() + 1);
    }
}

/// Parse `sql` and return its complete read shape, or why there is none.
pub fn read_query_shape(sql: &str) -> Result<ReadQueryShape, ShapeRefusal> {
    if sql.contains('\0') || sql.len() > MAX_SHAPE_SQL_BYTES {
        return Err(ShapeRefusal::InvalidInput);
    }
    let errors = Rc::new(Cell::new(0));
    let mut lexer = PlSqlLexer::new(InputStream::new(sql));
    lexer.remove_error_listeners();
    lexer.add_error_listener(Box::new(ErrorCount(Rc::clone(&errors))));
    let mut parser = PlSqlParser::new(CommonTokenStream::new(lexer));
    parser.remove_error_listeners();
    parser.add_error_listener(Box::new(ErrorCount(Rc::clone(&errors))));
    let script = parser.sql_script().map_err(|_| ShapeRefusal::SyntaxError)?;
    if errors.get() != 0 {
        return Err(ShapeRefusal::SyntaxError);
    }
    let root: Node<'_> = script;
    let mut walker = Walker::default();
    walker.script(&root)
}

#[derive(Default)]
struct Walker {
    nodes: usize,
    depth: usize,
}

impl Walker {
    fn enter(&mut self) -> Result<(), ShapeRefusal> {
        self.nodes += 1;
        self.depth += 1;
        if self.nodes > MAX_NODES || self.depth > MAX_DEPTH {
            return Err(ShapeRefusal::LimitExceeded);
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn script(&mut self, root: &Node<'_>) -> Result<ReadQueryShape, ShapeRefusal> {
        let mut units = Vec::new();
        for child in children(root) {
            match rule_of(&child) {
                Some("unit_statement") => units.push(child),
                None if matches!(keyword(&child).as_deref(), Some(";" | "<EOF>")) => {}
                _ => return Err(ShapeRefusal::NotSingleSelect),
            }
        }
        let [unit] = units.as_slice() else {
            return Err(ShapeRefusal::NotSingleSelect);
        };
        let dml = sole_rule_child(unit, "data_manipulation_language_statements")?;
        let select = sole_rule_child(&dml, "select_statement")?;
        Ok(ReadQueryShape {
            query: self.select_statement(&select)?,
        })
    }

    /// `select_statement: select_only_statement (for_update | order_by |
    /// offset | fetch)*`
    fn select_statement(&mut self, node: &Node<'_>) -> Result<ShapeQuery, ShapeRefusal> {
        self.enter()?;
        let mut query = None;
        let mut order_by = Vec::new();
        let mut row_limit = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("select_only_statement") if query.is_none() => {
                    query = Some(self.select_only_statement(&child)?);
                }
                Some("order_by_clause") => order_by.extend(self.order_by_clause(&child)?),
                Some("offset_clause" | "fetch_clause") => {
                    row_limit.push(self.transparent_expr(&child)?);
                }
                Some("for_update_clause") => {
                    return Err(ShapeRefusal::UnsupportedConstruct("FOR UPDATE"));
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => return Err(ShapeRefusal::NotSingleSelect),
            }
        }
        let mut query = query.ok_or(ShapeRefusal::NotSingleSelect)?;
        query.order_by.extend(order_by);
        query.row_limit.extend(row_limit);
        self.leave();
        Ok(query)
    }

    /// `select_only_statement: with_clause? subquery`
    fn select_only_statement(&mut self, node: &Node<'_>) -> Result<ShapeQuery, ShapeRefusal> {
        self.enter()?;
        let mut ctes = Vec::new();
        let mut query = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("with_clause") if query.is_none() && ctes.is_empty() => {
                    ctes = self.with_clause(&child)?;
                }
                Some("subquery") if query.is_none() => query = Some(self.subquery(&child)?),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => return Err(ShapeRefusal::UnsupportedConstruct("select statement token")),
            }
        }
        let mut query = query.ok_or(ShapeRefusal::NotSingleSelect)?;
        query.ctes = ctes;
        self.leave();
        Ok(query)
    }

    fn with_clause(&mut self, node: &Node<'_>) -> Result<Vec<ShapeCte>, ShapeRefusal> {
        self.enter()?;
        let mut ctes = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("with_factoring_clause") => {
                    for factoring in children(&child) {
                        match rule_of(&factoring) {
                            Some("subquery_factoring_clause") => {
                                ctes.push(self.subquery_factoring_clause(&factoring)?);
                            }
                            // WITH FUNCTION/PROCEDURE and anything else.
                            Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                            None if keyword(&factoring).as_deref() == Some(",") => {}
                            None => return Err(ShapeRefusal::UnsupportedConstruct("WITH token")),
                        }
                    }
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if matches!(keyword(&child).as_deref(), Some("WITH" | ",")) => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("WITH token")),
            }
        }
        self.leave();
        if ctes.is_empty() {
            return Err(ShapeRefusal::UnsupportedConstruct("empty WITH"));
        }
        Ok(ctes)
    }

    /// `query_name paren_column_list? AS '(' subquery order_by? ')'`; search
    /// and cycle clauses are refused.
    fn subquery_factoring_clause(&mut self, node: &Node<'_>) -> Result<ShapeCte, ShapeRefusal> {
        self.enter()?;
        let mut name = None;
        let mut columns = Vec::new();
        let mut query = None;
        let mut order_by = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("query_name") if name.is_none() => {
                    name = Some(self.single_identifier(&child)?)
                }
                Some("paren_column_list") => columns = self.paren_column_list(&child)?,
                Some("subquery") if query.is_none() => query = Some(self.subquery(&child)?),
                Some("order_by_clause") => order_by = self.order_by_clause(&child)?,
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if matches!(keyword(&child).as_deref(), Some("AS" | "(" | ")")) => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("CTE token")),
            }
        }
        let mut query = query.ok_or(ShapeRefusal::UnsupportedConstruct("CTE without query"))?;
        query.order_by.extend(order_by);
        self.leave();
        Ok(ShapeCte {
            name: name.ok_or(ShapeRefusal::UnsupportedConstruct("CTE without name"))?,
            columns,
            query,
        })
    }

    /// `subquery: subquery_basic_elements subquery_operation_part*`
    fn subquery(&mut self, node: &Node<'_>) -> Result<ShapeQuery, ShapeRefusal> {
        self.enter()?;
        let mut first = None;
        let mut rest = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("subquery_basic_elements") if first.is_none() => {
                    first = Some(self.subquery_basic_elements(&child)?);
                }
                Some("subquery_operation_part") if first.is_some() => {
                    rest.push(self.subquery_operation_part(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => return Err(ShapeRefusal::UnsupportedConstruct("subquery token")),
            }
        }
        self.leave();
        Ok(ShapeQuery {
            ctes: Vec::new(),
            first: first.ok_or(ShapeRefusal::UnsupportedConstruct("empty subquery"))?,
            rest,
            order_by: Vec::new(),
            row_limit: Vec::new(),
        })
    }

    fn subquery_operation_part(
        &mut self,
        node: &Node<'_>,
    ) -> Result<(ShapeSetOp, ShapeBranch), ShapeRefusal> {
        self.enter()?;
        let mut words = Vec::new();
        let mut branch = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("subquery_basic_elements") if branch.is_none() => {
                    branch = Some(self.subquery_basic_elements(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => words.push(keyword(&child).unwrap_or_default()),
            }
        }
        let op = match words
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
            ["UNION"] => ShapeSetOp::Union,
            ["UNION", "ALL"] => ShapeSetOp::UnionAll,
            ["INTERSECT"] => ShapeSetOp::Intersect,
            ["MINUS"] | ["EXCEPT"] => ShapeSetOp::Minus,
            _ => return Err(ShapeRefusal::UnsupportedConstruct("set operator")),
        };
        self.leave();
        Ok((
            op,
            branch.ok_or(ShapeRefusal::UnsupportedConstruct("set branch"))?,
        ))
    }

    /// `query_block | '(' subquery ')'`
    fn subquery_basic_elements(&mut self, node: &Node<'_>) -> Result<ShapeBranch, ShapeRefusal> {
        self.enter()?;
        let mut branch = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("query_block") if branch.is_none() => {
                    branch = Some(ShapeBranch::Select(Box::new(self.query_block(&child)?)));
                }
                Some("subquery") if branch.is_none() => {
                    branch = Some(ShapeBranch::Query(Box::new(self.subquery(&child)?)));
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if matches!(keyword(&child).as_deref(), Some("(" | ")")) => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("query token")),
            }
        }
        self.leave();
        branch.ok_or(ShapeRefusal::UnsupportedConstruct("empty query"))
    }

    fn query_block(&mut self, node: &Node<'_>) -> Result<ShapeSelect, ShapeRefusal> {
        self.enter()?;
        let mut select = ShapeSelect {
            distinct: false,
            items: Vec::new(),
            from: Vec::new(),
            where_clause: None,
            hierarchy: None,
            group_by: Vec::new(),
            having: None,
            order_by: Vec::new(),
            row_limit: Vec::new(),
        };
        let mut saw_list = false;
        for child in children(node) {
            match rule_of(&child) {
                Some("selected_list") if !saw_list => {
                    select.items = self.selected_list(&child)?;
                    saw_list = true;
                }
                Some("from_clause") => select.from = self.parse_from_clause(&child)?,
                Some("where_clause") => select.where_clause = Some(self.where_clause(&child)?),
                Some("hierarchical_query_clause") => {
                    select.hierarchy = Some(self.hierarchical_query_clause(&child)?);
                }
                Some("group_by_clause") => {
                    let (group_by, having) = self.group_by_clause(&child)?;
                    select.group_by = group_by;
                    select.having = having;
                }
                Some("having_clause") => select.having = Some(self.having_clause(&child)?),
                Some("order_by_clause") => select.order_by = self.order_by_clause(&child)?,
                Some("offset_clause" | "fetch_clause") => {
                    select.row_limit.push(self.transparent_expr(&child)?);
                }
                Some("into_clause") => return Err(ShapeRefusal::UnsupportedConstruct("INTO")),
                Some("model_clause") => return Err(ShapeRefusal::UnsupportedConstruct("MODEL")),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some("SELECT" | "ALL") => {}
                    Some("DISTINCT" | "UNIQUE") => select.distinct = true,
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("query block token")),
                },
            }
        }
        if !saw_list || select.from.is_empty() {
            return Err(ShapeRefusal::UnsupportedConstruct("SELECT without FROM"));
        }
        self.leave();
        Ok(select)
    }

    fn selected_list(&mut self, node: &Node<'_>) -> Result<Vec<ShapeSelectItem>, ShapeRefusal> {
        self.enter()?;
        let mut items = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("select_list_elements") => items.push(self.select_list_element(&child)?),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some("*") => items.push(ShapeSelectItem::Wildcard),
                    Some(",") => {}
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("select list token")),
                },
            }
        }
        self.leave();
        Ok(items)
    }

    /// `tableview_name '.' '*' | expression column_alias?`
    fn select_list_element(&mut self, node: &Node<'_>) -> Result<ShapeSelectItem, ShapeRefusal> {
        self.enter()?;
        let kids = children(node);
        let item = match kids.as_slice() {
            [name, dot, star]
                if rule_of(name) == Some("tableview_name")
                    && keyword(dot).as_deref() == Some(".")
                    && keyword(star).as_deref() == Some("*") =>
            {
                let (parts, db_link, partition) = self.tableview_name(name)?;
                if db_link.is_some() || partition.is_some() {
                    return Err(ShapeRefusal::UnsupportedConstruct("qualified wildcard"));
                }
                ShapeSelectItem::QualifiedWildcard(parts)
            }
            [expr] if rule_of(expr) == Some("expression") => ShapeSelectItem::Expr {
                expr: self.expression(expr)?,
                alias: None,
            },
            [expr, alias]
                if rule_of(expr) == Some("expression")
                    && rule_of(alias) == Some("column_alias") =>
            {
                ShapeSelectItem::Expr {
                    expr: self.expression(expr)?,
                    alias: Some(self.column_alias(alias)?),
                }
            }
            _ => return Err(ShapeRefusal::UnsupportedConstruct("select list element")),
        };
        self.leave();
        Ok(item)
    }

    fn column_alias(&mut self, node: &Node<'_>) -> Result<ShapeIdent, ShapeRefusal> {
        self.enter()?;
        let mut alias = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("identifier" | "id_expression") if alias.is_none() => {
                    alias = Some(identifier(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if keyword(&child).as_deref() == Some("AS") => {}
                // A quoted-string alias is not an identifier position.
                None => return Err(ShapeRefusal::UnsupportedConstruct("column alias")),
            }
        }
        self.leave();
        alias.ok_or(ShapeRefusal::UnsupportedConstruct("column alias"))
    }

    fn parse_from_clause(&mut self, node: &Node<'_>) -> Result<Vec<ShapeFromItem>, ShapeRefusal> {
        self.enter()?;
        let mut items = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("table_ref_list") => {
                    for table_ref in children(&child) {
                        match rule_of(&table_ref) {
                            Some("table_ref") => items.push(self.table_ref(&table_ref)?),
                            Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                            None if keyword(&table_ref).as_deref() == Some(",") => {}
                            None => return Err(ShapeRefusal::UnsupportedConstruct("FROM token")),
                        }
                    }
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if keyword(&child).as_deref() == Some("FROM") => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("FROM token")),
            }
        }
        self.leave();
        Ok(items)
    }

    /// `table_ref: table_ref_aux join_clause*`; PIVOT/UNPIVOT refuse.
    fn table_ref(&mut self, node: &Node<'_>) -> Result<ShapeFromItem, ShapeRefusal> {
        self.enter()?;
        let mut source = None;
        let mut joins = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("table_ref_aux") if source.is_none() => {
                    source = Some(self.table_ref_aux(&child)?);
                }
                Some("join_clause") if source.is_some() => joins.push(self.join_clause(&child)?),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => return Err(ShapeRefusal::UnsupportedConstruct("table reference token")),
            }
        }
        self.leave();
        Ok(ShapeFromItem {
            source: source.ok_or(ShapeRefusal::UnsupportedConstruct("table reference"))?,
            joins,
        })
    }

    /// `table_ref_aux: table_ref_aux_internal flashback_query_clause* table_alias?`
    fn table_ref_aux(&mut self, node: &Node<'_>) -> Result<ShapeTableRef, ShapeRefusal> {
        self.enter()?;
        let mut source = None;
        let mut alias = None;
        let mut flashback = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("table_ref_aux_internal") if source.is_none() => {
                    source = Some(self.table_ref_aux_internal(&child)?);
                }
                Some("flashback_query_clause") if source.is_some() && alias.is_none() => {
                    flashback.extend(self.flashback_query_clause(&child)?);
                }
                Some("table_alias") if source.is_some() && alias.is_none() => {
                    alias = Some(self.table_alias(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => return Err(ShapeRefusal::UnsupportedConstruct("table token")),
            }
        }
        self.leave();
        Ok(ShapeTableRef {
            source: source.ok_or(ShapeRefusal::UnsupportedConstruct("table source"))?,
            alias,
            flashback,
        })
    }

    fn table_alias(&mut self, node: &Node<'_>) -> Result<ShapeIdent, ShapeRefusal> {
        self.enter()?;
        let kids = children(node);
        let alias = match kids.as_slice() {
            [ident] if matches!(rule_of(ident), Some("identifier" | "id_expression")) => {
                identifier(ident)?
            }
            _ => return Err(ShapeRefusal::UnsupportedConstruct("table alias")),
        };
        self.leave();
        Ok(alias)
    }

    /// Only `dml_table_expression_clause` with no PIVOT/UNPIVOT and no
    /// parenthesized join is supported.
    fn table_ref_aux_internal(&mut self, node: &Node<'_>) -> Result<ShapeSource, ShapeRefusal> {
        self.enter()?;
        let kids = children(node);
        let source = match kids.as_slice() {
            [clause] if rule_of(clause) == Some("dml_table_expression_clause") => {
                self.dml_table_expression_clause(clause)?
            }
            _ => return Err(ShapeRefusal::UnsupportedConstruct("table expression")),
        };
        self.leave();
        Ok(source)
    }

    /// `tableview_name | '(' select_statement ')'`; collection expressions,
    /// LATERAL and restriction clauses refuse.
    fn dml_table_expression_clause(
        &mut self,
        node: &Node<'_>,
    ) -> Result<ShapeSource, ShapeRefusal> {
        self.enter()?;
        let kids = children(node);
        let source = match kids.as_slice() {
            [name] if rule_of(name) == Some("tableview_name") => {
                let (name, db_link, partition) = self.tableview_name(name)?;
                ShapeSource::Table {
                    name,
                    db_link,
                    partition,
                }
            }
            [open, select, close]
                if keyword(open).as_deref() == Some("(")
                    && rule_of(select) == Some("select_statement")
                    && keyword(close).as_deref() == Some(")") =>
            {
                ShapeSource::Derived(Box::new(self.select_statement(select)?))
            }
            _ => return Err(ShapeRefusal::UnsupportedConstruct("table expression")),
        };
        self.leave();
        Ok(source)
    }

    /// `identifier ('.' id_expression)? ('@' link_name | partition_extension_clause)?`
    #[allow(clippy::type_complexity)]
    fn tableview_name(
        &mut self,
        node: &Node<'_>,
    ) -> Result<
        (
            Vec<ShapeIdent>,
            Option<Vec<ShapeIdent>>,
            Option<ShapePartition>,
        ),
        ShapeRefusal,
    > {
        self.enter()?;
        let mut parts = Vec::new();
        let mut db_link = None;
        let mut partition = None;
        let mut after_at = false;
        for child in children(node) {
            match rule_of(&child) {
                Some("identifier" | "id_expression") if !after_at && partition.is_none() => {
                    parts.push(identifier(&child)?);
                }
                Some("link_name") if after_at && db_link.is_none() => {
                    db_link = Some(dotted_identifiers(&child)?);
                }
                Some("partition_extension_clause") if partition.is_none() && !after_at => {
                    partition = Some(self.partition_extension_clause(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some(".") if !after_at => {}
                    Some("@") if !after_at => after_at = true,
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("table name token")),
                },
            }
        }
        if parts.is_empty() || parts.len() > 2 || (after_at && db_link.is_none()) {
            return Err(ShapeRefusal::UnsupportedConstruct("table name"));
        }
        self.leave();
        Ok((parts, db_link, partition))
    }

    /// `PARTITION (name, …)` names partitions; `PARTITION FOR (key, …)` has
    /// key expressions. Subpartitions are handled the same way.
    fn partition_extension_clause(
        &mut self,
        node: &Node<'_>,
    ) -> Result<ShapePartition, ShapeRefusal> {
        self.enter()?;
        let mut is_for = false;
        let mut exprs = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("expressions_") if exprs.is_none() => exprs = Some(child),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some("PARTITION" | "SUBPARTITION" | "(" | ")") => {}
                    Some("FOR") => is_for = true,
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("partition token")),
                },
            }
        }
        let exprs = exprs.ok_or(ShapeRefusal::UnsupportedConstruct("partition"))?;
        let result = if is_for {
            ShapePartition::For(self.expression_list(&exprs)?)
        } else {
            // Each named partition must be one bare identifier.
            let mut names = Vec::new();
            for expr in self.expression_list(&exprs)? {
                match expr {
                    ShapeExpr::Name(parts) if parts.len() == 1 => {
                        names.extend(parts);
                    }
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("partition name")),
                }
            }
            ShapePartition::Named(names)
        };
        self.leave();
        Ok(result)
    }

    fn flashback_query_clause(&mut self, node: &Node<'_>) -> Result<Vec<ShapeExpr>, ShapeRefusal> {
        self.enter()?;
        let mut exprs = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("expression") => exprs.push(self.expression(&child)?),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some(
                        "AS" | "OF" | "VERSIONS" | "BETWEEN" | "AND" | "SCN" | "TIMESTAMP"
                        | "SNAPSHOT",
                    ) => {}
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("flashback token")),
                },
            }
        }
        if exprs.is_empty() {
            return Err(ShapeRefusal::UnsupportedConstruct("flashback"));
        }
        self.leave();
        Ok(exprs)
    }

    /// `query_partition_clause? (CROSS|NATURAL)? (INNER|outer_join_type)?
    /// JOIN table_ref_aux query_partition_clause? (join_on_part|join_using_part)*`,
    /// plus `CROSS APPLY` / `OUTER APPLY`.
    fn join_clause(&mut self, node: &Node<'_>) -> Result<ShapeJoin, ShapeRefusal> {
        self.enter()?;
        let mut words: Vec<String> = Vec::new();
        let mut table = None;
        let mut on = None;
        let mut using = Vec::new();
        let mut partition_by = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("query_partition_clause") => {
                    partition_by.extend(self.query_partition_clause(&child)?);
                }
                Some("outer_join_type") => {
                    for word in children(&child) {
                        let Some(text) = keyword(&word) else {
                            return Err(ShapeRefusal::UnsupportedConstruct("join type"));
                        };
                        words.push(text);
                    }
                }
                Some("table_ref_aux") if table.is_none() => {
                    table = Some(self.table_ref_aux(&child)?);
                }
                Some("join_on_part") if on.is_none() && using.is_empty() => {
                    on = Some(self.join_on_part(&child)?);
                }
                Some("join_using_part") if on.is_none() && using.is_empty() => {
                    using = self.join_using_part(&child)?;
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => words.push(keyword(&child).unwrap_or_default()),
            }
        }
        let natural = words.iter().any(|word| word == "NATURAL");
        let words: Vec<&str> = words
            .iter()
            .map(String::as_str)
            .filter(|word| !matches!(*word, "NATURAL" | "OUTER" | "JOIN"))
            .collect();
        let apply = node.get_text().to_ascii_uppercase().contains("APPLY");
        let kind = match (words.as_slice(), apply) {
            ([] | ["INNER"], false) => ShapeJoinKind::Inner,
            (["LEFT"], false) => ShapeJoinKind::Left,
            (["RIGHT"], false) => ShapeJoinKind::Right,
            (["FULL"], false) => ShapeJoinKind::Full,
            (["CROSS"], false) => ShapeJoinKind::Cross,
            (["CROSS", "APPLY"], true) => ShapeJoinKind::CrossApply,
            (["APPLY"], true) => ShapeJoinKind::OuterApply,
            _ => return Err(ShapeRefusal::UnsupportedConstruct("join kind")),
        };
        self.leave();
        Ok(ShapeJoin {
            kind,
            natural,
            table: table.ok_or(ShapeRefusal::UnsupportedConstruct("join table"))?,
            on,
            using,
            partition_by,
        })
    }

    fn query_partition_clause(&mut self, node: &Node<'_>) -> Result<Vec<ShapeExpr>, ShapeRefusal> {
        self.enter()?;
        let mut exprs = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("expressions_") => exprs.extend(self.expression_list(&child)?),
                Some("expression") => exprs.push(self.expression(&child)?),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some("PARTITION" | "BY" | "(" | ")" | ",") => {}
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("partition by token")),
                },
            }
        }
        self.leave();
        Ok(exprs)
    }

    fn join_on_part(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        self.enter()?;
        let mut condition = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("condition") if condition.is_none() => {
                    condition = Some(self.transparent_expr(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if keyword(&child).as_deref() == Some("ON") => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("ON token")),
            }
        }
        self.leave();
        condition.ok_or(ShapeRefusal::UnsupportedConstruct("ON"))
    }

    fn join_using_part(&mut self, node: &Node<'_>) -> Result<Vec<ShapeIdent>, ShapeRefusal> {
        self.enter()?;
        let mut columns = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("paren_column_list") if columns.is_none() => {
                    columns = Some(self.paren_column_list(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if keyword(&child).as_deref() == Some("USING") => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("USING token")),
            }
        }
        self.leave();
        let columns = columns.ok_or(ShapeRefusal::UnsupportedConstruct("USING"))?;
        if columns.is_empty() {
            return Err(ShapeRefusal::UnsupportedConstruct("USING"));
        }
        Ok(columns)
    }

    /// `'(' column_list ')'` of bare column names.
    fn paren_column_list(&mut self, node: &Node<'_>) -> Result<Vec<ShapeIdent>, ShapeRefusal> {
        self.enter()?;
        let mut columns = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("column_list") => {
                    for column in children(&child) {
                        match rule_of(&column) {
                            Some("column_name") => {
                                let parts = dotted_identifiers(&column)?;
                                let [part] = parts.as_slice() else {
                                    return Err(ShapeRefusal::UnsupportedConstruct("column"));
                                };
                                columns.push(part.clone());
                            }
                            Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                            None if keyword(&column).as_deref() == Some(",") => {}
                            None => return Err(ShapeRefusal::UnsupportedConstruct("column")),
                        }
                    }
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if matches!(keyword(&child).as_deref(), Some("(" | ")")) => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("column list")),
            }
        }
        self.leave();
        Ok(columns)
    }

    fn where_clause(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        self.enter()?;
        let mut condition = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("condition") if condition.is_none() => {
                    condition = Some(self.transparent_expr(&child)?);
                }
                // WHERE CURRENT OF and anything else.
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if keyword(&child).as_deref() == Some("WHERE") => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("WHERE token")),
            }
        }
        self.leave();
        condition.ok_or(ShapeRefusal::UnsupportedConstruct("WHERE"))
    }

    fn hierarchical_query_clause(
        &mut self,
        node: &Node<'_>,
    ) -> Result<ShapeHierarchy, ShapeRefusal> {
        self.enter()?;
        let mut connect_by = None;
        let mut start_with = None;
        let mut nocycle = false;
        for child in children(node) {
            match rule_of(&child) {
                Some("condition") if connect_by.is_none() => {
                    connect_by = Some(self.transparent_expr(&child)?);
                }
                Some("start_part") if start_with.is_none() => {
                    start_with = Some(self.transparent_expr(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some("CONNECT" | "BY") => {}
                    Some("NOCYCLE") => nocycle = true,
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("CONNECT BY token")),
                },
            }
        }
        self.leave();
        Ok(ShapeHierarchy {
            start_with,
            connect_by: connect_by.ok_or(ShapeRefusal::UnsupportedConstruct("CONNECT BY"))?,
            nocycle,
        })
    }

    /// Plain expression grouping only; ROLLUP, CUBE and GROUPING SETS refuse.
    fn group_by_clause(
        &mut self,
        node: &Node<'_>,
    ) -> Result<(Vec<ShapeExpr>, Option<ShapeExpr>), ShapeRefusal> {
        self.enter()?;
        let mut group_by = Vec::new();
        let mut having = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("group_by_elements") => {
                    let kids = children(&child);
                    match kids.as_slice() {
                        [expr] if rule_of(expr) == Some("expression") => {
                            group_by.push(self.expression(expr)?);
                        }
                        _ => return Err(ShapeRefusal::UnsupportedConstruct("GROUP BY element")),
                    }
                }
                Some("having_clause") if having.is_none() => {
                    having = Some(self.having_clause(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some("GROUP" | "BY" | ",") => {}
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("GROUP BY token")),
                },
            }
        }
        self.leave();
        Ok((group_by, having))
    }

    fn having_clause(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        self.enter()?;
        let mut condition = None;
        for child in children(node) {
            match rule_of(&child) {
                Some("condition") if condition.is_none() => {
                    condition = Some(self.transparent_expr(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if keyword(&child).as_deref() == Some("HAVING") => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("HAVING token")),
            }
        }
        self.leave();
        condition.ok_or(ShapeRefusal::UnsupportedConstruct("HAVING"))
    }

    fn order_by_clause(&mut self, node: &Node<'_>) -> Result<Vec<ShapeExpr>, ShapeRefusal> {
        self.enter()?;
        let mut exprs = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("order_by_elements") => {
                    for part in children(&child) {
                        match rule_of(&part) {
                            Some("expression") => exprs.push(self.expression(&part)?),
                            Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                            None => match keyword(&part).as_deref() {
                                Some("ASC" | "DESC" | "NULLS" | "FIRST" | "LAST") => {}
                                _ => return Err(ShapeRefusal::UnsupportedConstruct("ORDER BY")),
                            },
                        }
                    }
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some("ORDER" | "BY" | "SIBLINGS" | ",") => {}
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("ORDER BY token")),
                },
            }
        }
        self.leave();
        Ok(exprs)
    }

    fn expression_list(&mut self, node: &Node<'_>) -> Result<Vec<ShapeExpr>, ShapeRefusal> {
        self.enter()?;
        let mut exprs = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("expression") => exprs.push(self.expression(&child)?),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if keyword(&child).as_deref() == Some(",") => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("expression list")),
            }
        }
        self.leave();
        Ok(exprs)
    }

    fn expression(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        self.transparent_expr(node)
    }

    /// Walk an expression-level node. Interpreted rules produce names, calls
    /// and subqueries; transparent rules contribute only their children;
    /// every other rule refuses.
    fn transparent_expr(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        self.enter()?;
        let result = match rule_of(node) {
            None => terminal_without_identifier(node).map(|()| ShapeExpr::Literal),
            Some("atom") => self.atom(node),
            Some("general_element") => self.general_element(node),
            Some("table_element") => table_element(node),
            Some("standard_function") => self.standard_function(node),
            Some("quantified_expression") => self.quantified_expression(node),
            Some("in_elements") => self.in_elements(node),
            Some("model_expression") => {
                // A `[…]` cell reference belongs to the MODEL clause.
                if children(node)
                    .iter()
                    .any(|child| keyword(child).as_deref() == Some("["))
                {
                    Err(ShapeRefusal::UnsupportedConstruct("MODEL cell"))
                } else {
                    self.group(node)
                }
            }
            Some("constant" | "literal" | "numeric" | "quoted_string" | "bind_variable") => {
                if contains_identifier_rule(node) {
                    Err(ShapeRefusal::UnaccountedIdentifier)
                } else {
                    Ok(ShapeExpr::Literal)
                }
            }
            Some(rule) if TRANSPARENT_EXPRESSION_RULES.contains(&rule) => self.group(node),
            Some(rule) if IDENTIFIER_RULES.contains(&rule) => {
                Err(ShapeRefusal::UnaccountedIdentifier)
            }
            Some("cursor_expression") => Err(ShapeRefusal::UnsupportedConstruct("CURSOR")),
            Some(rule) => Err(ShapeRefusal::UnsupportedRule(rule)),
        };
        self.leave();
        result
    }

    fn group(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        let mut parts = Vec::new();
        for child in children(node) {
            match self.transparent_expr(&child)? {
                ShapeExpr::Literal => {}
                ShapeExpr::Group(inner) => parts.extend(inner),
                other => parts.push(other),
            }
        }
        Ok(match parts.len() {
            0 => ShapeExpr::Literal,
            1 => parts.pop().unwrap_or(ShapeExpr::Literal),
            _ => ShapeExpr::Group(parts),
        })
    }

    /// `constant | bind_variable | general_element | '(' subquery ')' |
    /// '(' expressions ')'`
    fn atom(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        let kids = children(node);
        if let [open, subquery, close] = kids.as_slice()
            && keyword(open).as_deref() == Some("(")
            && rule_of(subquery) == Some("subquery")
            && keyword(close).as_deref() == Some(")")
        {
            return Ok(ShapeExpr::Subquery {
                kind: ShapeSubqueryKind::Scalar,
                query: Box::new(self.subquery(subquery)?),
            });
        }
        if kids
            .iter()
            .any(|child| matches!(rule_of(child), Some("subquery" | "select_only_statement")))
        {
            return Err(ShapeRefusal::UnsupportedConstruct("subquery position"));
        }
        self.group(node)
    }

    /// `EXISTS (select_only_statement)`, `ANY|SOME|ALL (subquery|expression)`.
    fn quantified_expression(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        let kids = children(node);
        let Some(word) = kids.first().and_then(keyword) else {
            return Err(ShapeRefusal::UnsupportedConstruct("quantified expression"));
        };
        let kind = match word.as_str() {
            "EXISTS" => ShapeSubqueryKind::Exists,
            "ANY" | "SOME" | "ALL" => ShapeSubqueryKind::Quantified,
            _ => return Err(ShapeRefusal::UnsupportedConstruct("quantified expression")),
        };
        let mut result = None;
        for child in &kids[1..] {
            match rule_of(child) {
                Some("select_only_statement") if result.is_none() => {
                    result = Some(ShapeExpr::Subquery {
                        kind,
                        query: Box::new(self.select_only_statement(child)?),
                    });
                }
                Some("subquery") if result.is_none() => {
                    result = Some(ShapeExpr::Subquery {
                        kind,
                        query: Box::new(self.subquery(child)?),
                    });
                }
                Some("expression") if result.is_none() && kind != ShapeSubqueryKind::Exists => {
                    result = Some(self.expression(child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if matches!(keyword(child).as_deref(), Some("(" | ")")) => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("quantified token")),
            }
        }
        result.ok_or(ShapeRefusal::UnsupportedConstruct("quantified expression"))
    }

    /// `'(' subquery ')' | '(' concatenation (',' concatenation)* ')' | …`
    fn in_elements(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        let kids = children(node);
        if let [open, subquery, close] = kids.as_slice()
            && keyword(open).as_deref() == Some("(")
            && rule_of(subquery) == Some("subquery")
            && keyword(close).as_deref() == Some(")")
        {
            return Ok(ShapeExpr::Subquery {
                kind: ShapeSubqueryKind::In,
                query: Box::new(self.subquery(subquery)?),
            });
        }
        if kids
            .iter()
            .any(|child| matches!(rule_of(child), Some("subquery" | "select_only_statement")))
        {
            return Err(ShapeRefusal::UnsupportedConstruct("IN subquery"));
        }
        self.group(node)
    }

    /// `general_element '.' general_element_part | general_element_part`.
    /// A dotted name, or a call whose final part carries arguments.
    fn general_element(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        let mut parts = Vec::new();
        let mut args: Option<Vec<ShapeExpr>> = None;
        self.collect_element(node, &mut parts, &mut args)?;
        if parts.is_empty() {
            return Err(ShapeRefusal::UnsupportedConstruct("empty name"));
        }
        Ok(match args {
            Some(args) => ShapeExpr::Call {
                name: parts,
                builtin: false,
                args,
            },
            None => ShapeExpr::Name(parts),
        })
    }

    fn collect_element(
        &mut self,
        node: &Node<'_>,
        parts: &mut Vec<ShapeIdent>,
        args: &mut Option<Vec<ShapeExpr>>,
    ) -> Result<(), ShapeRefusal> {
        self.enter()?;
        for child in children(node) {
            match rule_of(&child) {
                Some("general_element") => self.collect_element(&child, parts, args)?,
                Some("general_element_part") => {
                    // Nothing may follow a call: `f(x).attr` and `f(x)(y)`
                    // are object access, not a plain call.
                    if args.is_some() {
                        return Err(ShapeRefusal::UnsupportedConstruct("access after call"));
                    }
                    self.general_element_part(&child, parts, args)?;
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if keyword(&child).as_deref() == Some(".") => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("name token")),
            }
        }
        self.leave();
        Ok(())
    }

    /// `id_expression function_argument?`; INTRODUCER and `@link` refuse.
    fn general_element_part(
        &mut self,
        node: &Node<'_>,
        parts: &mut Vec<ShapeIdent>,
        args: &mut Option<Vec<ShapeExpr>>,
    ) -> Result<(), ShapeRefusal> {
        self.enter()?;
        let mut named = false;
        for child in children(node) {
            match rule_of(&child) {
                Some("id_expression") if !named && args.is_none() => {
                    parts.push(identifier(&child)?);
                    named = true;
                }
                Some("function_argument") if named && args.is_none() => {
                    *args = Some(self.function_argument(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => return Err(ShapeRefusal::UnsupportedConstruct("name part token")),
            }
        }
        if !named {
            return Err(ShapeRefusal::UnsupportedConstruct("name part"));
        }
        self.leave();
        Ok(())
    }

    /// `'(' (argument (',' argument)*)? ')' keep_clause?`
    fn function_argument(&mut self, node: &Node<'_>) -> Result<Vec<ShapeExpr>, ShapeRefusal> {
        self.enter()?;
        let mut args = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("argument") => {
                    // Named notation (`p => x`) puts a parameter name in an
                    // identifier position the shape cannot attribute.
                    let kids = children(&child);
                    match kids.as_slice() {
                        [expr] if rule_of(expr) == Some("expression") => {
                            args.push(self.expression(expr)?);
                        }
                        _ => return Err(ShapeRefusal::UnsupportedConstruct("named argument")),
                    }
                }
                Some("keep_clause") => args.push(self.transparent_keep(&child)?),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None if matches!(keyword(&child).as_deref(), Some("(" | ")" | ",")) => {}
                None => return Err(ShapeRefusal::UnsupportedConstruct("argument token")),
            }
        }
        self.leave();
        Ok(args)
    }

    fn transparent_keep(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        let mut parts = Vec::new();
        for child in children(node) {
            match rule_of(&child) {
                Some("order_by_clause") => parts.extend(self.order_by_clause(&child)?),
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
                None => match keyword(&child).as_deref() {
                    Some("KEEP" | "(" | ")" | "DENSE_RANK" | "FIRST" | "LAST") => {}
                    _ => return Err(ShapeRefusal::UnsupportedConstruct("KEEP token")),
                },
            }
        }
        Ok(ShapeExpr::Group(parts))
    }

    /// A grammar-level function (`COUNT`, `MAX`, `CAST`, `JSON_VALUE`, …).
    /// Its name is the first keyword below the wrapper rules; argument
    /// expressions, subqueries, KEEP and OVER clauses are collected; type
    /// names (`CAST … AS t`, RETURNING types) are not values.
    fn standard_function(&mut self, node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
        let mut name = None;
        let mut args = Vec::new();
        self.collect_function(node, &mut name, &mut args)?;
        let name = name.ok_or(ShapeRefusal::UnsupportedConstruct("function name"))?;
        Ok(ShapeExpr::Call {
            name: vec![ShapeIdent {
                text: name,
                quoted: false,
            }],
            builtin: true,
            args,
        })
    }

    fn collect_function(
        &mut self,
        node: &Node<'_>,
        name: &mut Option<String>,
        args: &mut Vec<ShapeExpr>,
    ) -> Result<(), ShapeRefusal> {
        self.enter()?;
        for child in children(node) {
            match rule_of(&child) {
                None => {
                    let word = keyword(&child).unwrap_or_default();
                    if name.is_none() && is_keyword_text(&word) {
                        *name = Some(word);
                    } else {
                        terminal_without_identifier(&child)?;
                    }
                }
                Some(
                    "numeric_function_wrapper"
                    | "numeric_function"
                    | "string_function"
                    | "other_function"
                    | "json_function"
                    | "over_clause"
                    | "windowing_clause"
                    | "windowing_type"
                    | "windowing_elements"
                    | "over_clause_keyword"
                    | "within_or_over_clause_keyword"
                    | "within_or_over_part"
                    | "function_argument_analytic"
                    | "function_argument_modeling"
                    | "json_value_return_clause"
                    | "json_value_on_mismatch_clause"
                    | "json_return_clause"
                    | "respect_or_ignore_nulls"
                    | "argument",
                ) => self.collect_function(&child, name, args)?,
                Some("keep_clause") => args.push(self.transparent_keep(&child)?),
                Some("order_by_clause") => args.extend(self.order_by_clause(&child)?),
                Some("query_partition_clause") => {
                    args.extend(self.query_partition_clause(&child)?);
                }
                Some(
                    "json_value_return_type"
                    | "type_spec"
                    | "datatype"
                    | "native_datatype_element"
                    | "precision_part",
                ) => {
                    // A type is not a value. A user-defined type name is still
                    // an identifier, which this shape does not attribute.
                    if contains_identifier_rule(&child) {
                        return Err(ShapeRefusal::UnsupportedConstruct("user-defined type"));
                    }
                }
                Some("expression" | "expressions_" | "concatenation" | "condition") => {
                    args.push(self.transparent_expr(&child)?);
                }
                Some("constant" | "literal" | "numeric" | "quoted_string" | "bind_variable") => {
                    if contains_identifier_rule(&child) {
                        return Err(ShapeRefusal::UnaccountedIdentifier);
                    }
                }
                Some("table_element") => args.push(table_element(&child)?),
                Some("subquery") => args.push(ShapeExpr::Subquery {
                    kind: ShapeSubqueryKind::Quantified,
                    query: Box::new(self.subquery(&child)?),
                }),
                Some("general_element_part") => {
                    return Err(ShapeRefusal::UnsupportedConstruct("access after call"));
                }
                Some("cursor_expression") => {
                    return Err(ShapeRefusal::UnsupportedConstruct("CURSOR"));
                }
                Some(rule) if IDENTIFIER_RULES.contains(&rule) => {
                    return Err(ShapeRefusal::UnaccountedIdentifier);
                }
                Some(rule) if TRANSPARENT_EXPRESSION_RULES.contains(&rule) => {
                    args.push(self.transparent_expr(&child)?);
                }
                Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
            }
        }
        self.leave();
        Ok(())
    }

    fn single_identifier(&mut self, node: &Node<'_>) -> Result<ShapeIdent, ShapeRefusal> {
        let parts = dotted_identifiers(node)?;
        let [part] = parts.as_slice() else {
            return Err(ShapeRefusal::UnsupportedConstruct("identifier"));
        };
        Ok(part.clone())
    }
}

/// Rules that only combine sub-expressions: operators, CASE, conditions and
/// the expression precedence chain. Their identifiers come only through the
/// interpreted rules below them.
const TRANSPARENT_EXPRESSION_RULES: &[&str] = &[
    "expression",
    "expressions_",
    "condition",
    "logical_expression",
    "unary_logical_expression",
    "unary_logical_operation",
    "logical_operation",
    "multiset_expression",
    "relational_expression",
    "relational_operator",
    "compound_expression",
    "concatenation",
    "unary_expression",
    "case_expression",
    "searched_case_expression",
    "simple_case_expression",
    "case_when_part_expression",
    "case_else_part_expression",
    "between_elements",
    "like_type",
    "like_escape_part",
    "interval_expression",
    "start_part",
    "offset_clause",
    "fetch_clause",
];

/// Rules that hold an identifier. Reaching one outside an interpreted
/// position refuses: the shape would otherwise miss a name.
const IDENTIFIER_RULES: &[&str] = &[
    "identifier",
    "id_expression",
    "regular_id",
    "non_reserved_keywords_pre12c",
    "non_reserved_keywords_in_12c",
    "column_name",
    "tableview_name",
    "type_name",
    "query_name",
    "char_set_name",
    "link_name",
];

/// `true` when `text` (any case) is a keyword token of the grammar.
fn is_keyword_text(text: &str) -> bool {
    static KEYWORDS: std::sync::OnceLock<std::collections::HashSet<String>> =
        std::sync::OnceLock::new();
    let upper = text.to_ascii_uppercase();
    KEYWORDS
        .get_or_init(|| {
            _LITERAL_NAMES
                .iter()
                .flatten()
                .filter_map(|literal| {
                    literal
                        .strip_prefix('\'')
                        .and_then(|l| l.strip_suffix('\''))
                })
                .filter(|literal| literal.chars().any(|c| c.is_ascii_alphabetic()))
                .map(str::to_ascii_uppercase)
                .collect()
        })
        .contains(&upper)
}

/// A terminal outside an interpreted position may be a keyword, a literal or
/// punctuation, never an identifier token: dropping it would hide a name.
fn terminal_without_identifier(node: &Node<'_>) -> Result<(), ShapeRefusal> {
    let text = node.get_text();
    let Some(first) = text.chars().next() else {
        return Ok(());
    };
    // `N'…'`, `q'[…]'` and `X'…'` are string literals, not names.
    let word_like = (first.is_alphabetic() && !text.contains('\'')) || first == '"';
    if word_like && !is_keyword_text(&text) && text != "<EOF>" {
        return Err(ShapeRefusal::UnaccountedIdentifier);
    }
    Ok(())
}

fn contains_identifier_rule(node: &Node<'_>) -> bool {
    let mut stack = vec![Rc::clone(node)];
    let mut seen = 0usize;
    while let Some(current) = stack.pop() {
        seen += 1;
        if seen > MAX_NODES {
            return true;
        }
        if let Some(rule) = rule_of(&current)
            && IDENTIFIER_RULES.contains(&rule)
        {
            return true;
        }
        stack.extend(children(&current));
    }
    false
}

/// The single identifier below an `identifier`/`id_expression`/`regular_id`
/// chain. Its text is one token: a quoted identifier keeps its exact case
/// without the quotes.
fn identifier(node: &Node<'_>) -> Result<ShapeIdent, ShapeRefusal> {
    let mut current = Rc::clone(node);
    loop {
        let kids = children(&current);
        match kids.as_slice() {
            [only] if rule_of(only).is_some() => current = Rc::clone(only),
            [token] if rule_of(token).is_none() => {
                let text = token.get_text();
                if let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                    if inner.is_empty() || inner.contains('"') {
                        return Err(ShapeRefusal::UnsupportedConstruct("quoted identifier"));
                    }
                    return Ok(ShapeIdent {
                        text: inner.to_string(),
                        quoted: true,
                    });
                }
                if text.is_empty()
                    || !text
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '#'))
                {
                    return Err(ShapeRefusal::UnsupportedConstruct("identifier"));
                }
                return Ok(ShapeIdent {
                    text,
                    quoted: false,
                });
            }
            _ => return Err(ShapeRefusal::UnsupportedConstruct("identifier")),
        }
    }
}

/// `identifier ('.' identifier)*` style rules (link names, column names).
fn dotted_identifiers(node: &Node<'_>) -> Result<Vec<ShapeIdent>, ShapeRefusal> {
    let mut parts = Vec::new();
    for child in children(node) {
        match rule_of(&child) {
            Some(rule) if IDENTIFIER_RULES.contains(&rule) => {
                if matches!(rule, "identifier" | "id_expression" | "regular_id") {
                    parts.push(identifier(&child)?);
                } else {
                    parts.extend(dotted_identifiers(&child)?);
                }
            }
            Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
            None if keyword(&child).as_deref() == Some(".") => {}
            None => {
                // A bare token here is the identifier itself.
                let text = child.get_text();
                if text.is_empty() {
                    return Err(ShapeRefusal::UnsupportedConstruct("identifier"));
                }
                parts.push(identifier(node)?);
                return Ok(parts);
            }
        }
    }
    if parts.is_empty() {
        return Err(ShapeRefusal::UnsupportedConstruct("identifier"));
    }
    Ok(parts)
}

/// `table_element: id_expression ('.' id_expression)*`, a column reference
/// some function grammars use instead of an expression. INTRODUCER refuses.
fn table_element(node: &Node<'_>) -> Result<ShapeExpr, ShapeRefusal> {
    let mut parts = Vec::new();
    for child in children(node) {
        match rule_of(&child) {
            Some("id_expression") => parts.push(identifier(&child)?),
            Some(other) => return Err(ShapeRefusal::UnsupportedRule(other)),
            None if keyword(&child).as_deref() == Some(".") => {}
            None => return Err(ShapeRefusal::UnsupportedConstruct("column reference")),
        }
    }
    if parts.is_empty() {
        return Err(ShapeRefusal::UnsupportedConstruct("column reference"));
    }
    Ok(ShapeExpr::Name(parts))
}

fn sole_rule_child<'i>(node: &Node<'i>, expected: &'static str) -> Result<Node<'i>, ShapeRefusal> {
    let kids = children(node);
    match kids.as_slice() {
        [only] if rule_of(only) == Some(expected) => Ok(Rc::clone(only)),
        _ => Err(ShapeRefusal::NotSingleSelect),
    }
}
