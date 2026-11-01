//! `read_query_shape` (pvlqq.10.18): the Oracle read constructs that
//! sqlparser 0.63 cannot parse produce complete, exact shapes; every
//! statement that is not a plain read, or uses a construct the shape cannot
//! represent, refuses with a typed reason.
#![cfg(feature = "antlr-codegen")]

use plsql_parser_antlr::read_shape::{
    ReadQueryShape, ShapeBranch, ShapeExpr, ShapeIdent, ShapeJoinKind, ShapePartition,
    ShapeRefusal, ShapeSelect, ShapeSelectItem, ShapeSource, ShapeSubqueryKind,
    StatementValueShape, read_query_shape, statement_value_shape,
};

#[test]
fn statement_value_shape_reuses_query_projection_without_admitting_mutations_as_reads() {
    let plain = shape("SELECT APP.F, D.DUMMY FROM SYS.DUAL D");
    let StatementValueShape::Query(into) =
        statement_value_shape("SELECT APP.F, D.DUMMY INTO V, W FROM SYS.DUAL D").unwrap()
    else {
        panic!("expected query");
    };
    assert_eq!(
        plain.query, into,
        "INTO lvalues must not become expression names"
    );
    for sql in [
        "SELECT F INTO V FROM DUAL",
        "SELECT F",
        "SELECT ID FROM APP.T FOR UPDATE",
    ] {
        assert!(statement_value_shape(sql).is_ok(), "{sql}");
        assert!(
            read_query_shape(sql).is_err(),
            "read admission remains unchanged: {sql}"
        );
    }
}

#[test]
fn statement_value_shape_keeps_dml_sources_and_value_names_separate_from_targets() {
    for (sql, expected, source_count) in [
        (
            "UPDATE APP.T T SET ID = APP.F WHERE T.ID = 1",
            vec![vec!["APP", "F"], vec!["T", "ID"]],
            1,
        ),
        (
            "INSERT INTO APP.T(ID) VALUES(APP.F)",
            vec![vec!["APP", "F"]],
            0,
        ),
        (
            "DELETE FROM APP.T T WHERE T.ID = APP.F",
            vec![vec!["T", "ID"], vec!["APP", "F"]],
            1,
        ),
        (
            "MERGE INTO APP.T T USING APP.U U ON (T.ID = U.ID) WHEN MATCHED THEN UPDATE SET T.ID = APP.F",
            vec![vec!["T", "ID"], vec!["U", "ID"], vec!["APP", "F"]],
            2,
        ),
    ] {
        let StatementValueShape::Dml { sources, values } =
            statement_value_shape(sql).unwrap_or_else(|reason| panic!("{sql}: {reason:?}"))
        else {
            panic!("expected DML");
        };
        assert_eq!(sources.len(), source_count, "{sql}");
        let mut names = Vec::new();
        for value in &values {
            names_in(value, &mut names);
        }
        assert_eq!(names, expected, "{sql}");
        assert!(read_query_shape(sql).is_err());
    }
    for sql in [
        "SELECT 1 FROM DUAL; SELECT APP.F FROM DUAL",
        "BEGIN APP.F; END;",
        "CALL APP.F()",
    ] {
        assert!(statement_value_shape(sql).is_err(), "{sql}");
    }
}

fn ident(text: &str) -> ShapeIdent {
    ShapeIdent {
        text: text.to_string(),
        quoted: false,
    }
}

fn name(parts: &[&str]) -> ShapeExpr {
    ShapeExpr::Name(parts.iter().map(|part| ident(part)).collect())
}

fn shape(sql: &str) -> ReadQueryShape {
    read_query_shape(sql).unwrap_or_else(|refusal| panic!("{sql}: {refusal:?}"))
}

fn root_select(shape: &ReadQueryShape) -> &ShapeSelect {
    match &shape.query.first {
        ShapeBranch::Select(select) => select,
        ShapeBranch::Query(_) => panic!("root is not a SELECT"),
    }
}

/// Every value-position name anywhere in an expression, in order.
fn names_in(expr: &ShapeExpr, out: &mut Vec<Vec<String>>) {
    match expr {
        ShapeExpr::Name(parts) => out.push(parts.iter().map(|p| p.text.clone()).collect()),
        ShapeExpr::Call { args, .. } => args.iter().for_each(|arg| names_in(arg, out)),
        ShapeExpr::Group(parts) => parts.iter().for_each(|part| names_in(part, out)),
        ShapeExpr::Subquery { .. } | ShapeExpr::Literal | ShapeExpr::Star => {}
    }
}

fn item_names(select: &ShapeSelect) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for item in &select.items {
        if let ShapeSelectItem::Expr { expr, .. } = item {
            names_in(expr, &mut out);
        }
    }
    out
}

fn table_of(select: &ShapeSelect, index: usize) -> (&[ShapeIdent], &Option<ShapePartition>) {
    match &select.from[index].source.source {
        ShapeSource::Table {
            name, partition, ..
        } => (name.as_slice(), partition),
        ShapeSource::Derived(_) => panic!("derived table"),
    }
}

#[test]
fn read_shape_flashback_queries_keep_their_scn_and_time_expressions() {
    let scn = shape("SELECT * FROM emp AS OF SCN 12345");
    let select = root_select(&scn);
    assert_eq!(table_of(select, 0).0, [ident("emp")]);
    assert_eq!(select.from[0].source.flashback, [ShapeExpr::Literal]);

    // Oracle places the flashback clause before the table alias.
    let time = shape("SELECT * FROM emp AS OF TIMESTAMP SYSTIMESTAMP - INTERVAL '1' HOUR e");
    let select = root_select(&time);
    assert_eq!(select.from[0].source.alias, Some(ident("e")));
    assert_eq!(select.from[0].source.flashback, [name(&["SYSTIMESTAMP"])]);

    let versions =
        shape("SELECT versions_starttime, id FROM emp VERSIONS BETWEEN SCN MINVALUE AND MAXVALUE");
    let select = root_select(&versions);
    assert_eq!(
        item_names(select),
        [
            vec!["versions_starttime".to_string()],
            vec!["id".to_string()]
        ]
    );
    assert_eq!(select.from[0].source.flashback.len(), 2);
}

#[test]
fn read_shape_keep_and_conversion_defaults_keep_every_value() {
    let keep = shape("SELECT MAX(sal) KEEP (DENSE_RANK FIRST ORDER BY hired) FROM emp");
    let select = root_select(&keep);
    let ShapeSelectItem::Expr {
        expr:
            ShapeExpr::Call {
                name: fname,
                builtin,
                ..
            },
        ..
    } = &select.items[0]
    else {
        panic!("{select:?}");
    };
    assert_eq!(fname, &[ident("MAX")]);
    assert!(builtin);
    // The KEEP ordering column is a value of this block.
    assert_eq!(
        item_names(select),
        [vec!["sal".to_string()], vec!["hired".to_string()]]
    );

    for (sql, expected) in [
        (
            "SELECT JSON_VALUE(doc, '$.a' RETURNING NUMBER DEFAULT 0 ON ERROR) FROM t",
            "doc",
        ),
        (
            "SELECT TO_NUMBER(s DEFAULT 0 ON CONVERSION ERROR) FROM t",
            "s",
        ),
        ("SELECT VALIDATE_CONVERSION(s AS NUMBER) FROM t", "s"),
        ("SELECT TO_CHAR(hired, 'YYYY') FROM emp", "hired"),
    ] {
        let parsed = shape(sql);
        assert_eq!(
            item_names(root_select(&parsed)),
            [vec![expected.to_string()]],
            "{sql}"
        );
    }
}

#[test]
fn read_shape_partition_extensions_name_partitions_not_values() {
    let named = shape("SELECT * FROM sales PARTITION (p1)");
    let select = root_select(&named);
    assert_eq!(
        table_of(select, 0),
        (
            [ident("sales")].as_slice(),
            &Some(ShapePartition::Named(vec![ident("p1")]))
        )
    );

    let keyed = shape("SELECT * FROM sales PARTITION FOR (DATE '2024-01-01')");
    let select = root_select(&keyed);
    assert_eq!(
        table_of(select, 0).1,
        &Some(ShapePartition::For(vec![ShapeExpr::Literal]))
    );
}

#[test]
fn read_shape_partitioned_outer_join_keeps_join_kind_and_partition_values() {
    let parsed = shape("SELECT * FROM t1 a PARTITION BY (a.k) RIGHT OUTER JOIN t2 b ON a.d = b.d");
    let select = root_select(&parsed);
    assert_eq!(select.from[0].source.alias, Some(ident("a")));
    let [join] = select.from[0].joins.as_slice() else {
        panic!("{select:?}");
    };
    assert_eq!(join.kind, ShapeJoinKind::Right);
    assert_eq!(join.table.alias, Some(ident("b")));
    assert_eq!(join.partition_by, [name(&["a", "k"])]);
    let mut on = Vec::new();
    names_in(join.on.as_ref().expect("ON"), &mut on);
    assert_eq!(on, [vec!["a", "d"], vec!["b", "d"]]);
}

#[test]
fn read_shape_records_blocks_aliases_joins_and_subqueries() {
    let parsed = shape(
        "WITH c AS (SELECT id FROM hr.emp) \
         SELECT c.id, (SELECT MAX(x) FROM t2 WHERE t2.k = c.id) AS mx \
         FROM c JOIN dept d USING (id) \
         WHERE EXISTS (SELECT 1 FROM t3 WHERE t3.id = c.id) AND c.id IN (SELECT id FROM t4) \
         UNION ALL SELECT id, 0 FROM t5",
    );
    assert_eq!(parsed.query.ctes.len(), 1);
    assert_eq!(parsed.query.ctes[0].name, ident("c"));
    let cte = match &parsed.query.ctes[0].query.first {
        ShapeBranch::Select(select) => select,
        ShapeBranch::Query(_) => panic!("CTE body"),
    };
    assert_eq!(table_of(cte, 0).0, [ident("hr"), ident("emp")]);
    assert_eq!(parsed.query.rest.len(), 1);

    let select = root_select(&parsed);
    let ShapeSelectItem::Expr { alias, expr } = &select.items[1] else {
        panic!("{select:?}");
    };
    assert_eq!(alias, &Some(ident("mx")));
    assert!(matches!(
        expr,
        ShapeExpr::Subquery {
            kind: ShapeSubqueryKind::Scalar,
            ..
        }
    ));
    assert_eq!(select.from[0].joins[0].using, [ident("id")]);
    assert_eq!(select.from[0].joins[0].kind, ShapeJoinKind::Inner);
    let ShapeExpr::Group(parts) = select.where_clause.as_ref().expect("WHERE") else {
        panic!("{select:?}");
    };
    let kinds: Vec<_> = parts
        .iter()
        .filter_map(|part| match part {
            ShapeExpr::Subquery { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, [ShapeSubqueryKind::Exists, ShapeSubqueryKind::In]);
}

#[test]
fn read_shape_keeps_calls_quoted_names_and_row_limits() {
    let parsed = shape(
        r#"SELECT pkg.fn(t.id), UPPER(t."Name"), x.* FROM "Mixed" t, emp x ORDER BY 1 FETCH FIRST 5 ROWS ONLY"#,
    );
    let select = root_select(&parsed);
    let ShapeSelectItem::Expr {
        expr:
            ShapeExpr::Call {
                name: fname,
                builtin,
                args,
            },
        ..
    } = &select.items[0]
    else {
        panic!("{select:?}");
    };
    assert_eq!(fname, &[ident("pkg"), ident("fn")]);
    assert!(!builtin);
    assert_eq!(args, &[name(&["t", "id"])]);
    let ShapeSelectItem::Expr {
        expr: ShapeExpr::Call { args, .. },
        ..
    } = &select.items[1]
    else {
        panic!("{select:?}");
    };
    assert_eq!(
        args,
        &[ShapeExpr::Name(vec![
            ident("t"),
            ShapeIdent {
                text: "Name".into(),
                quoted: true
            }
        ])]
    );
    assert_eq!(
        select.items[2],
        ShapeSelectItem::QualifiedWildcard(vec![ident("x")])
    );
    assert_eq!(
        table_of(select, 0).0,
        [ShapeIdent {
            text: "Mixed".into(),
            quoted: true
        }]
    );
    assert_eq!(select.row_limit.len(), 1);
}

#[test]
fn read_shape_refuses_everything_that_is_not_a_representable_read() {
    use ShapeRefusal::*;
    for (sql, expected) in [
        // Effects and non-reads.
        (
            "SELECT * FROM emp FOR UPDATE",
            UnsupportedConstruct("FOR UPDATE"),
        ),
        (
            "SELECT * FROM emp FOR UPDATE SKIP LOCKED",
            UnsupportedConstruct("FOR UPDATE"),
        ),
        ("SELECT id INTO v FROM emp", UnsupportedConstruct("INTO")),
        (
            "WITH FUNCTION f RETURN NUMBER IS BEGIN RETURN 1; END; SELECT f FROM dual",
            UnsupportedRule("function_body"),
        ),
        (
            "SELECT CURSOR(SELECT * FROM dept) FROM dual",
            UnsupportedConstruct("CURSOR"),
        ),
        ("INSERT INTO t VALUES (1)", NotSingleSelect),
        ("UPDATE t SET a = 1", NotSingleSelect),
        ("DELETE FROM t", NotSingleSelect),
        (
            "MERGE INTO t USING s ON (t.id = s.id) WHEN MATCHED THEN UPDATE SET a = 1",
            NotSingleSelect,
        ),
        ("WITH c AS (SELECT id FROM t) DELETE FROM t", SyntaxError),
        ("LOCK TABLE t IN EXCLUSIVE MODE", NotSingleSelect),
        ("BEGIN NULL; END;", NotSingleSelect),
        ("CALL p()", NotSingleSelect),
        ("SELECT 1 FROM dual; SELECT 2 FROM dual", NotSingleSelect),
        // Shapes the classifier cannot represent exactly.
        (
            "SELECT * FROM t MODEL DIMENSION BY (id) MEASURES (v) RULES (v[1] = 0)",
            UnsupportedConstruct("MODEL"),
        ),
        (
            "SELECT CAST(MULTISET(SELECT id FROM emp) AS num_tab) FROM dual",
            UnsupportedConstruct("user-defined type"),
        ),
        (
            "SELECT TREAT(VALUE(p) AS emp_t).name FROM people p",
            UnsupportedConstruct("user-defined type"),
        ),
        (
            "SELECT f(p => 1) FROM dual",
            UnsupportedConstruct("named argument"),
        ),
        (
            "SELECT XMLQUERY('/a' PASSING doc RETURNING CONTENT) FROM t",
            UnsupportedRule("xml_passing_clause"),
        ),
        (
            "SELECT EXTRACT(YEAR FROM hired) FROM emp",
            UnaccountedIdentifier,
        ),
        ("SELECT * FROM emp@remote", UnsupportedRule("database")),
        (
            "WITH r (id, p) AS (SELECT id, mgr FROM emp UNION ALL SELECT e.id, e.mgr FROM emp e \
             JOIN r ON e.mgr = r.id) SEARCH DEPTH FIRST BY id SET ord SELECT * FROM r",
            UnsupportedRule("search_clause"),
        ),
        // Not parseable by the grammar, partial or invalid.
        (
            "SELECT NTH_VALUE(sal, 2) FROM LAST OVER (ORDER BY id) FROM emp",
            SyntaxError,
        ),
        ("SELECT * FROM", SyntaxError),
        ("garbage ((", SyntaxError),
        ("SELECT id FROM emp WHERE", SyntaxError),
        ("SELECT 1 FROM dual\0", InvalidInput),
    ] {
        assert_eq!(read_query_shape(sql), Err(expected), "{sql}");
    }
    let oversized = format!(
        "SELECT 1 FROM dual WHERE 'x' = '{}'",
        "x".repeat(plsql_parser_antlr::read_shape::MAX_SHAPE_SQL_BYTES)
    );
    assert_eq!(
        read_query_shape(&oversized),
        Err(ShapeRefusal::InvalidInput)
    );
}
