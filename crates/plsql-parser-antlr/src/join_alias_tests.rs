//! Parse-tree assertions for `isNotStartOfJoin` (pvlqq.10.18). Zero
//! diagnostics is not enough: with a join keyword missing from the predicate,
//! `t_a LEFT JOIN t_b` still parses cleanly, with `LEFT` taken as the alias
//! of `t_a`. These tests pin which tokens are aliases and which start joins.

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
use crate::generated::plsqlparser::{PlSqlParser, PlSqlParserContext, ruleNames};

type Node<'i> = Rc<dyn PlSqlParserContext<'i> + 'i>;

struct Errors(Rc<Cell<usize>>);

impl<'a, T: Recognizer<'a>> ErrorListener<'a, T> for Errors {
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

/// Every `(rule, text)` node in the tree, text with whitespace removed.
#[derive(Debug)]
struct Tree {
    nodes: Vec<(&'static str, String)>,
}

impl Tree {
    fn parse(sql: &str) -> Self {
        let errors = Rc::new(Cell::new(0));
        let mut lexer = PlSqlLexer::new(InputStream::new(sql));
        lexer.remove_error_listeners();
        lexer.add_error_listener(Box::new(Errors(Rc::clone(&errors))));
        let mut parser = PlSqlParser::new(CommonTokenStream::new(lexer));
        parser.remove_error_listeners();
        parser.add_error_listener(Box::new(Errors(Rc::clone(&errors))));
        let root: Node<'_> = parser.sql_script().expect("sql_script");
        assert_eq!(errors.get(), 0, "syntax errors in {sql}");
        let mut nodes = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if let Some(rule) = ruleNames.get(node.get_rule_index()) {
                nodes.push((*rule, node.get_text().to_ascii_uppercase()));
                for index in (0..node.get_child_count()).rev() {
                    if let Some(child) = node.get_child(index) {
                        stack.push(child);
                    }
                }
            }
        }
        Self { nodes }
    }

    fn texts(&self, rule: &str) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|(name, _)| *name == rule)
            .map(|(_, text)| text.as_str())
            .collect()
    }
}

/// `(sql, exact table aliases, join_clause count, outer_join_type texts)`.
const CASES: &[(&str, &[&str], usize, &[&str])] = &[
    ("SELECT t.id FROM t_emp t", &["T"], 0, &[]),
    (
        "SELECT x.id FROM t_emp alias1, t_dept x",
        &["ALIAS1", "X"],
        0,
        &[],
    ),
    // Each join keyword, without and with aliases.
    ("SELECT 1 FROM t_a JOIN t_b ON t_b.id = t_a.id", &[], 1, &[]),
    (
        "SELECT 1 FROM t_a a JOIN t_b b ON b.id = a.id",
        &["A", "B"],
        1,
        &[],
    ),
    (
        "SELECT 1 FROM t_a INNER JOIN t_b ON t_b.id = t_a.id",
        &[],
        1,
        &[],
    ),
    (
        "SELECT 1 FROM t_a a INNER JOIN t_b b ON b.id = a.id",
        &["A", "B"],
        1,
        &[],
    ),
    (
        "SELECT 1 FROM t_a LEFT JOIN t_b ON t_b.id = t_a.id",
        &[],
        1,
        &["LEFT"],
    ),
    (
        "SELECT 1 FROM t_a a LEFT JOIN t_b b ON b.id = a.id",
        &["A", "B"],
        1,
        &["LEFT"],
    ),
    (
        "SELECT 1 FROM t_a LEFT OUTER JOIN t_b ON t_b.id = t_a.id",
        &[],
        1,
        &["LEFTOUTER"],
    ),
    (
        "SELECT 1 FROM t_a RIGHT JOIN t_b ON t_b.id = t_a.id",
        &[],
        1,
        &["RIGHT"],
    ),
    (
        "SELECT 1 FROM t_a a RIGHT JOIN t_b b ON b.id = a.id",
        &["A", "B"],
        1,
        &["RIGHT"],
    ),
    (
        "SELECT 1 FROM t_a FULL JOIN t_b ON t_b.id = t_a.id",
        &[],
        1,
        &["FULL"],
    ),
    (
        "SELECT 1 FROM t_a a FULL OUTER JOIN t_b b ON b.id = a.id",
        &["A", "B"],
        1,
        &["FULLOUTER"],
    ),
    ("SELECT 1 FROM t_a CROSS JOIN t_b", &[], 1, &[]),
    ("SELECT 1 FROM t_a a CROSS JOIN t_b b", &["A", "B"], 1, &[]),
    ("SELECT 1 FROM t_a NATURAL JOIN t_b", &[], 1, &[]),
    (
        "SELECT 1 FROM t_a a NATURAL JOIN t_b b",
        &["A", "B"],
        1,
        &[],
    ),
    (
        "SELECT 1 FROM t_a CROSS APPLY (SELECT b.id FROM t_b b WHERE b.id = t_a.id) x",
        &["B", "X"],
        1,
        &[],
    ),
    (
        "SELECT 1 FROM t_a OUTER APPLY (SELECT b.id FROM t_b b WHERE b.id = t_a.id) x",
        &["B", "X"],
        1,
        &[],
    ),
    (
        "SELECT 1 FROM t_a a OUTER APPLY (SELECT b.id FROM t_b b WHERE b.id = a.id) x",
        &["A", "B", "X"],
        1,
        &[],
    ),
    // Partitioned outer join: PARTITION BY starts the join, not an alias.
    (
        "SELECT 1 FROM t1 PARTITION BY (k) RIGHT OUTER JOIN t2 ON t1.d = t2.d",
        &[],
        1,
        &["RIGHTOUTER"],
    ),
    (
        "SELECT 1 FROM t1 a PARTITION BY (a.k) RIGHT OUTER JOIN t2 b ON a.d = b.d",
        &["A", "B"],
        1,
        &["RIGHTOUTER"],
    ),
];

#[test]
fn join_keywords_start_joins_and_aliases_are_exact() {
    for (sql, aliases, joins, outer_types) in CASES {
        let tree = Tree::parse(sql);
        assert_eq!(
            tree.texts("table_alias"),
            *aliases,
            "table aliases of {sql}"
        );
        assert_eq!(
            tree.texts("join_clause").len(),
            *joins,
            "join clauses of {sql}"
        );
        assert_eq!(
            tree.texts("outer_join_type"),
            *outer_types,
            "outer join type of {sql}"
        );
        for keyword in [
            "JOIN",
            "INNER",
            "LEFT",
            "RIGHT",
            "FULL",
            "CROSS",
            "NATURAL",
            "OUTER",
            "APPLY",
            "PARTITION",
        ] {
            assert!(
                !tree
                    .texts("table_alias")
                    .iter()
                    .any(|alias| alias.starts_with(keyword)),
                "{keyword} became a table alias in {sql}"
            );
        }
    }
}

#[test]
fn partitioned_outer_join_keeps_its_partition_clause() {
    for sql in [
        "SELECT 1 FROM t1 PARTITION BY (k) RIGHT OUTER JOIN t2 ON t1.d = t2.d",
        "SELECT 1 FROM t1 a PARTITION BY (a.k) RIGHT OUTER JOIN t2 b ON a.d = b.d",
    ] {
        let tree = Tree::parse(sql);
        assert_eq!(
            tree.texts("query_partition_clause").len(),
            1,
            "partition clause of {sql}"
        );
        assert!(
            tree.texts("query_partition_clause")[0].starts_with("PARTITIONBY("),
            "{sql}"
        );
    }
}
