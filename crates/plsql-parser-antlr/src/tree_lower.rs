// tree_lower.rs — Real ANTLR parse-tree → Ast lowering.
//
// Feature-gated on `antlr-codegen`. Supersedes the text-scanning `lower_source`
// for the `ast` field of [`Antlr4RustBackend`].
//
// # Architecture
//
// 1. Build a full parser (`PlSqlParser`) from the ANTLR input stream.
// 2. Walk the `sql_script` tree: each `unit_statement` child dispatches to a
//    per-construct lowering function → one [`AstDecl`].
// 3. For each routine body walk `seq_of_statements` → `Vec<AstStatement>`.
//
// # Span extraction
//
// ANTLR token `get_start()` is the *inclusive* byte offset of the first byte;
// `get_stop()` is the *inclusive* byte offset of the last byte.
// We expose spans as `[start, stop+1)` (exclusive end) consistent with the rest.
//
// # Never-panic contract
//
// All fallible operations degrade to `AstDecl::Unknown` / `AstStatement::Unknown`
// plus a pushed `Diagnostic`. The caller wraps the whole call in `catch_unwind`.

#![cfg(feature = "antlr-codegen")]

use antlr_rust::common_token_stream::CommonTokenStream;
use antlr_rust::input_stream::InputStream;
use antlr_rust::parser_rule_context::ParserRuleContext;
use antlr_rust::token::Token;

use plsql_core::{Diagnostic, FileId, Position, Severity, Span};
use plsql_parser::ast::{Ast, AstDecl, AstStatement, SourceFile, SourceMap};

use crate::backend::ANTLR4RUST_DIAG_CODE;
use crate::generated::plsqllexer::PlSqlLexer;
use crate::generated::plsqlparser::{
    Assignment_statementContextAttrs, BodyContextAttrs, Call_statementContextAttrs,
    Create_function_bodyContextAttrs, Create_package_bodyContextAttrs, Create_packageContextAttrs,
    Create_procedure_bodyContextAttrs, Create_triggerContextAttrs, Create_typeContextAttrs,
    Create_viewContextAttrs, Data_manipulation_language_statementsContextAttrs,
    Execute_immediateContextAttrs, Function_bodyContextAttrs, Package_obj_bodyContextAttrs,
    PlSqlParser, Procedure_bodyContextAttrs, Return_statementContextAttrs,
    Seq_of_statementsContextAttrs, Sql_scriptContextAttrs, Sql_statementContextAttrs,
    StatementContextAttrs, Trigger_blockContextAttrs, Trigger_bodyContextAttrs,
    Type_bodyContextAttrs, Type_definitionContextAttrs, Unit_statementContextAttrs,
};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Lower an ANTLR parse tree for `source` into an [`Ast`].
///
/// On any internal failure the function degrades gracefully. The returned
/// `Ast` is always well-formed.
///
/// NUL-byte edge: if `source` contains `'\0'` the ANTLR runtime silently
/// truncates at the first NUL — a diagnostic is emitted and parsing continues
/// with the truncated input.
pub fn lower_parse_tree(source: &str, file_id: FileId, diagnostics: &mut Vec<Diagnostic>) -> Ast {
    // NUL-byte detection.
    if source.contains('\0') {
        diagnostics.push(Diagnostic::new(
            ANTLR4RUST_DIAG_CODE,
            Severity::Warn,
            "source contains NUL byte(s); ANTLR runtime will truncate input at first NUL — \
             parse tree lowering proceeds on truncated input"
                .to_string(),
        ));
    }

    // Saturating cast (oracle-kxb3 sibling): a >u32::MAX source
    // would wrap with `as u32` and produce a tiny span overlapping
    // every diagnostic. Saturate to `u32::MAX` so the worst we do
    // on a >4 GiB input is clip the trailing span; we never wrap.
    let total_len = u32::try_from(source.len()).unwrap_or(u32::MAX);
    let file_span = make_span(file_id, 0, total_len);

    // Build the full parser (lexer + token stream + parser).
    let input = InputStream::new(source);
    let lexer = PlSqlLexer::new(input);
    let token_stream = CommonTokenStream::new(lexer);
    let mut parser = PlSqlParser::new(token_stream);
    // Silence ANTLR's default stderr console listener.
    // Parser trait provides `remove_parse_listeners`.
    parser.remove_parse_listeners();

    // Parse the top-level sql_script rule.
    let script_ctx = match parser.sql_script() {
        Ok(ctx) => ctx,
        Err(e) => {
            diagnostics.push(Diagnostic::new(
                ANTLR4RUST_DIAG_CODE,
                Severity::Error,
                format!("parse-tree lowering: sql_script() failed: {e:?}"),
            ));
            return Ast {
                root: SourceFile {
                    span: file_span,
                    declarations: vec![],
                },
                source_map: SourceMap::new(),
                body_statements: vec![],
            };
        }
    };

    let mut decls: Vec<AstDecl> = Vec::new();
    let mut body_stmts: Vec<Vec<AstStatement>> = Vec::new();

    // Each unit_statement child of sql_script becomes one AstDecl —
    // unless it is parser-recovery debris (trailing SQL*Plus client
    // directives like `/`, `QUIT`, `EXIT`, `SET`, `PROMPT`, or a body
    // fragment splintered off an already-lowered object). Such phantom
    // `unit_statement`s carry NO Oracle top-level object and must not
    // be minted as `AstDecl::Unknown` — that would dishonestly inflate
    // the unrecognized-object count with non-objects. A genuine
    // unrecognized *object* form (slice begins with a top-level DDL
    // verb the typed handlers + text scanner could not classify) is
    // still surfaced honestly as `AstDecl::Unknown`.
    for unit in &script_ctx.unit_statement_all() {
        if let Some((d, stmts)) = lower_unit_statement(unit, source, file_id, diagnostics) {
            decls.push(d);
            body_stmts.push(stmts);
        }
    }

    Ast {
        root: SourceFile {
            span: file_span,
            declarations: decls,
        },
        source_map: SourceMap::new(),
        body_statements: body_stmts,
    }
}

// ---------------------------------------------------------------------------
// unit_statement dispatch
// ---------------------------------------------------------------------------

fn lower_unit_statement(
    unit: &crate::generated::plsqlparser::Unit_statementContextAll<'_>,
    source: &str,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(AstDecl, Vec<AstStatement>)> {
    let span = node_span(unit, file_id, source);

    if let Some(pkg) = unit.create_package() {
        return Some((lower_create_package(&pkg, source, file_id, span), vec![]));
    }
    if let Some(pkgb) = unit.create_package_body() {
        return Some(lower_create_package_body(
            &pkgb,
            source,
            file_id,
            span,
            diagnostics,
        ));
    }
    if let Some(proc) = unit.create_procedure_body() {
        return Some(lower_create_procedure_body(
            &proc,
            source,
            file_id,
            span,
            diagnostics,
        ));
    }
    if let Some(func) = unit.create_function_body() {
        return Some(lower_create_function_body(
            &func,
            source,
            file_id,
            span,
            diagnostics,
        ));
    }
    if let Some(trig) = unit.create_trigger() {
        return Some(lower_create_trigger(
            &trig,
            source,
            file_id,
            span,
            diagnostics,
        ));
    }
    if let Some(view) = unit.create_view() {
        return Some((lower_create_view(&view, source, file_id, span), vec![]));
    }
    if let Some(typ) = unit.create_type() {
        return Some((lower_create_type(&typ, source, file_id, span), vec![]));
    }

    // No typed handler matched. Decide whether this `unit_statement`
    // is a genuine top-level object or merely parser-recovery debris.
    //
    // ANTLR's `sql_script` rule wraps trailing SQL*Plus client
    // directives (`/`, `QUIT`, `EXIT`, `SET`, `PROMPT`, `SPOOL`,
    // `WHENEVER`, `DEFINE`, `@…`, `REM`, `CONNECT`, …) and
    // error-recovery body splinters (`BEGIN`/`IF`/`END`/`DECLARE`/
    // bare DML/local-var continuations, plus large APEX
    // `wwv_flow_imp*.create_*(...)` call sequences) into phantom
    // `unit_statement` nodes. These carry NO Oracle top-level object —
    // the real object in the file was already lowered above. Running
    // the text scanner over such debris would mint a flood of bogus
    // `AstDecl::Ddl` rows (every `create_…(` substring), and minting
    // them as `AstDecl::Unknown` would inflate the
    // unrecognized-object count with non-objects. So: only a slice
    // whose first significant token is a top-level object DDL verb
    // (`CREATE` / `ALTER` / `DROP`) is treated as an object.
    let slice_start = (span.start.offset as usize).min(source.len());
    let slice_end = (span.end.offset as usize).min(source.len());
    let slice = if slice_start < slice_end {
        &source[slice_start..slice_end]
    } else {
        source
    };
    if !slice_is_top_level_object_ddl(slice) {
        return None;
    }

    // Genuine top-level DDL the typed handlers did not cover
    // (CREATE/ALTER/DROP of a non-PLSQL object). The text scanner
    // returns a typed `AstDecl` (incl. `AstDecl::Ddl`); if even that
    // fails it is an honest unrecognized object → `AstDecl::Unknown`
    // (R13 — typed uncertainty, never masked).
    //
    // USR-loop §2.1: this `unit_statement` node *is* the ANTLR
    // grammar position the gap arose at. Stamp its rule path so the
    // downstream `IR_DDL_NOT_LOWERED` / `IR_UNCLASSIFIED_DECL`
    // diagnostic carries a fine-grained, dedup-stable signature
    // (grammar rule names only — I-PRIVACY).
    let rule_path = rule_path_of(unit);
    let text_ast = crate::lower::lower_source(slice, file_id);
    if let Some(d) = text_ast.root.declarations.into_iter().next() {
        let d = adjust_span(d, span.start.offset, file_id);
        return Some((with_rule_path(d, rule_path), vec![]));
    }
    Some((
        AstDecl::Unknown {
            span,
            antlr_rule_path: rule_path,
        },
        vec![],
    ))
}

/// True when `slice`'s first significant token (skipping leading
/// whitespace and `--` / `/* */` comments) is a top-level *object*
/// DDL verb — `CREATE`, `ALTER`, or `DROP`. Such a slice that the
/// typed handlers and text scanner could not classify is a genuine
/// unrecognized object worth surfacing as `AstDecl::Unknown`.
///
/// Anything else (SQL*Plus client directives, PL/SQL body fragments,
/// bare DML, `BEGIN`/`DECLARE`/`END`, local-variable continuations)
/// is parser-recovery debris, not an Oracle object.
fn slice_is_top_level_object_ddl(slice: &str) -> bool {
    let bytes = slice.as_bytes();
    let len = bytes.len();
    let mut pos = 0;
    // Skip leading whitespace + line/block comments.
    loop {
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos + 1 < len && bytes[pos] == b'-' && bytes[pos + 1] == b'-' {
            while pos < len && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        if pos + 1 < len && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            pos += 2;
            while pos + 1 < len && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                pos += 1;
            }
            pos = (pos + 2).min(len);
            continue;
        }
        break;
    }
    let kw_at = |needle: &[u8]| -> bool {
        if pos + needle.len() > len {
            return false;
        }
        for (i, &n) in needle.iter().enumerate() {
            if !bytes[pos + i].eq_ignore_ascii_case(&n) {
                return false;
            }
        }
        // Whole-word: next byte must not be an identifier char.
        match bytes.get(pos + needle.len()) {
            Some(&c) => !(c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'#'),
            None => true,
        }
    };
    kw_at(b"CREATE") || kw_at(b"ALTER") || kw_at(b"DROP")
}

// ---------------------------------------------------------------------------
// Per-construct lowerers
// ---------------------------------------------------------------------------

fn lower_create_package(
    ctx: &crate::generated::plsqlparser::Create_packageContextAll<'_>,
    source: &str,
    file_id: FileId,
    fallback_span: Span,
) -> AstDecl {
    AstDecl::PackageSpec {
        name: node_name_upper(source, ctx.package_name(0)),
        span: non_empty(node_span(ctx, file_id, source), fallback_span),
    }
}

fn lower_create_package_body(
    ctx: &crate::generated::plsqlparser::Create_package_bodyContextAll<'_>,
    source: &str,
    file_id: FileId,
    fallback_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> (AstDecl, Vec<AstStatement>) {
    let span = node_span(ctx, file_id, source);
    let name = node_name_upper(source, ctx.package_name(0));

    // Collect body statements from all routine members (procedure_body /
    // function_body) inside the package body, plus the optional
    // initialization seq_of_statements at the package level.
    let mut all_stmts: Vec<AstStatement> = Vec::new();
    for obj in &ctx.package_obj_body_all() {
        if let Some(pb) = obj.procedure_body() {
            if let Some(body_ctx) = pb.body() {
                let stmts = lower_body_stmts(&body_ctx, source, file_id, diagnostics);
                all_stmts.extend(stmts);
            }
        }
        if let Some(fb) = obj.function_body() {
            if let Some(body_ctx) = fb.body() {
                let stmts = lower_body_stmts(&body_ctx, source, file_id, diagnostics);
                all_stmts.extend(stmts);
            }
        }
    }
    // Package-level initialization block (after `BEGIN` at the package level).
    if let Some(seq) = ctx.seq_of_statements() {
        let stmts = lower_seq_of_statements(&seq, source, file_id, diagnostics);
        all_stmts.extend(stmts);
    }

    (
        AstDecl::PackageBody {
            name,
            span: non_empty(span, fallback_span),
        },
        all_stmts,
    )
}

fn lower_create_procedure_body(
    ctx: &crate::generated::plsqlparser::Create_procedure_bodyContextAll<'_>,
    source: &str,
    file_id: FileId,
    fallback_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> (AstDecl, Vec<AstStatement>) {
    // The procedure name may be schema-qualified; take the last component.
    let name = last_component(node_name_upper(source, ctx.procedure_name()));
    let stmts = ctx
        .body()
        .map(|b| lower_body_stmts(&b, source, file_id, diagnostics))
        .unwrap_or_default();
    (
        AstDecl::Procedure {
            name,
            span: non_empty(node_span(ctx, file_id, source), fallback_span),
        },
        stmts,
    )
}

fn lower_create_function_body(
    ctx: &crate::generated::plsqlparser::Create_function_bodyContextAll<'_>,
    source: &str,
    file_id: FileId,
    fallback_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> (AstDecl, Vec<AstStatement>) {
    let name = last_component(node_name_upper(source, ctx.function_name()));
    let stmts = ctx
        .body()
        .map(|b| lower_body_stmts(&b, source, file_id, diagnostics))
        .unwrap_or_default();
    (
        AstDecl::Function {
            name,
            span: non_empty(node_span(ctx, file_id, source), fallback_span),
        },
        stmts,
    )
}

fn lower_create_trigger(
    ctx: &crate::generated::plsqlparser::Create_triggerContextAll<'_>,
    source: &str,
    file_id: FileId,
    fallback_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> (AstDecl, Vec<AstStatement>) {
    let name = last_component(node_name_upper(source, ctx.trigger_name()));
    // Trigger body: trigger_body → trigger_block → body.
    let stmts = ctx
        .trigger_body()
        .and_then(|tb| tb.trigger_block())
        .and_then(|tbl| tbl.body())
        .map(|b| lower_body_stmts(&b, source, file_id, diagnostics))
        .unwrap_or_default();
    (
        AstDecl::Trigger {
            name,
            span: non_empty(node_span(ctx, file_id, source), fallback_span),
        },
        stmts,
    )
}

fn lower_create_view(
    ctx: &crate::generated::plsqlparser::Create_viewContextAll<'_>,
    source: &str,
    file_id: FileId,
    fallback_span: Span,
) -> AstDecl {
    // View name: stored in ctx.v (the first id_expression after VIEW keyword),
    // falling back to the first positional id_expression.
    let name = match ctx.v.clone() {
        Some(ie) => node_name_upper(source, Some(ie)),
        None => node_name_upper(source, ctx.id_expression(0)),
    };
    AstDecl::View {
        name,
        span: non_empty(node_span(ctx, file_id, source), fallback_span),
    }
}

fn lower_create_type(
    ctx: &crate::generated::plsqlparser::Create_typeContextAll<'_>,
    source: &str,
    file_id: FileId,
    fallback_span: Span,
) -> AstDecl {
    let span = || non_empty(node_span(ctx, file_id, source), fallback_span);

    if let Some(td) = ctx.type_definition() {
        return AstDecl::TypeSpec {
            name: node_name_upper(source, td.type_name()),
            span: span(),
        };
    }
    if let Some(tb) = ctx.type_body() {
        return AstDecl::TypeBody {
            name: node_name_upper(source, tb.type_name()),
            span: span(),
        };
    }
    // A `create_type` the typed handlers could not resolve to a
    // spec/body — an honest unrecognized object. Stamp the grammar
    // position (USR-loop §2.1; rule names only — I-PRIVACY).
    AstDecl::Unknown {
        span: fallback_span,
        antlr_rule_path: rule_path_of(ctx),
    }
}

// ---------------------------------------------------------------------------
// Statement body lowering (seq_of_statements)
// ---------------------------------------------------------------------------

/// Lower a `body` context (BEGIN … seq_of_statements … END) into a flat list
/// of [`AstStatement`]s. Called for procedures, functions, triggers.
pub fn lower_body_stmts(
    body_ctx: &crate::generated::plsqlparser::BodyContextAll<'_>,
    source: &str,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<AstStatement> {
    body_ctx
        .seq_of_statements()
        .map(|seq| lower_seq_of_statements(&seq, source, file_id, diagnostics))
        .unwrap_or_default()
}

/// Lower a `seq_of_statements` context into `AstStatement`s.
pub fn lower_seq_of_statements(
    seq: &crate::generated::plsqlparser::Seq_of_statementsContextAll<'_>,
    source: &str,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<AstStatement> {
    let mut out = Vec::new();
    for stmt in &seq.statement_all() {
        if let Some(ast_stmt) = lower_statement(stmt, source, file_id, diagnostics) {
            out.push(ast_stmt);
        }
    }
    out
}

/// Lower one `statement` node.
fn lower_statement(
    stmt: &crate::generated::plsqlparser::StatementContextAll<'_>,
    source: &str,
    file_id: FileId,
    _diagnostics: &mut Vec<Diagnostic>,
) -> Option<AstStatement> {
    let span = node_span(stmt, file_id, source);

    // NULL statement.
    if stmt.null_statement().is_some() {
        return Some(AstStatement::Null { span });
    }

    // Assignment.
    if let Some(asgn) = stmt.assignment_statement() {
        let target = asgn
            .general_element()
            .map(|ge| node_text(source, &*ge))
            .or_else(|| asgn.bind_variable().map(|bv| node_text(source, &*bv)))
            .unwrap_or_default()
            .trim()
            .to_string();
        let rhs_text = asgn
            .expression()
            .map(|e| node_text(source, &*e))
            .unwrap_or_default()
            .trim()
            .to_string();
        return Some(AstStatement::Assignment {
            target,
            rhs_text,
            span,
        });
    }

    // IF statement.
    if let Some(if_s) = stmt.if_statement() {
        let cond_text = node_text(source, &*if_s);
        return Some(AstStatement::If { cond_text, span });
    }

    // LOOP statement.
    if let Some(loop_s) = stmt.loop_statement() {
        let header_text = node_text(source, &*loop_s);
        return Some(AstStatement::Loop { header_text, span });
    }

    // RAISE statement.
    if let Some(raise_s) = stmt.raise_statement() {
        let full = node_text(source, &*raise_s);
        let exception = {
            let trimmed = full.trim();
            let upper = trimmed.to_ascii_uppercase();
            let rest = if upper.starts_with("RAISE") {
                trimmed[5..].trim().trim_end_matches(';').trim().to_string()
            } else {
                String::new()
            };
            if rest.is_empty() { None } else { Some(rest) }
        };
        return Some(AstStatement::Raise { exception, span });
    }

    // RETURN statement.
    if let Some(ret_s) = stmt.return_statement() {
        let value_text = ret_s
            .expression()
            .map(|e| node_text(source, &*e).trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(AstStatement::Return { value_text, span });
    }

    // SQL statement (DML / EXECUTE IMMEDIATE / cursor ops / transaction).
    if let Some(sql_s) = stmt.sql_statement() {
        // Execute immediate.
        if let Some(exec_imm) = sql_s.execute_immediate() {
            let sql_text = exec_imm
                .expression()
                .map(|e| node_text(source, &*e))
                .unwrap_or_default();
            let has_using = exec_imm.using_clause().is_some();
            return Some(AstStatement::ExecuteImmediate {
                sql_text,
                has_using,
                span,
            });
        }
        // DML statements.
        if let Some(dml) = sql_s.data_manipulation_language_statements() {
            let verb = if dml.select_statement().is_some() {
                "SELECT"
            } else if dml.insert_statement().is_some() {
                "INSERT"
            } else if dml.update_statement().is_some() {
                "UPDATE"
            } else if dml.delete_statement().is_some() {
                "DELETE"
            } else if dml.merge_statement().is_some() {
                "MERGE"
            } else {
                "SQL"
            };
            // Capture the verbatim DML source slice so the IR layer
            // (`extract_table_accesses`, PLSQL-DEP-003) can recover
            // table/column Read/Write dependencies.
            let raw_text = node_text(source, &*dml);
            return Some(AstStatement::Sql {
                verb: verb.to_string(),
                raw_text,
                span,
            });
        }
        // Cursor manipulation, transaction control, etc.
        let raw_text = node_text(source, &*sql_s);
        return Some(AstStatement::Sql {
            verb: "SQL".to_string(),
            raw_text,
            span,
        });
    }

    // Call statement.
    if let Some(call_s) = stmt.call_statement() {
        let callee = build_call_callee(&call_s, source);
        return Some(AstStatement::Call { callee, span });
    }

    // CASE statement, nested body, forall, pipe_row, etc. → Unknown.
    Some(AstStatement::Unknown { span })
}

// ---------------------------------------------------------------------------
// Callee name construction
// ---------------------------------------------------------------------------

fn build_call_callee(
    call_ctx: &crate::generated::plsqlparser::Call_statementContextAll<'_>,
    source: &str,
) -> String {
    let routine_names = call_ctx.routine_name_all();
    if routine_names.is_empty() {
        // Fallback: use raw text, strip CALL keyword.
        let raw = node_text(source, call_ctx);
        let trimmed = raw.trim();
        let upper = trimmed.to_ascii_uppercase();
        let stripped = if upper.starts_with("CALL ") {
            trimmed[5..].trim().to_string()
        } else {
            trimmed.to_string()
        };
        return stripped
            .split('(')
            .next()
            .unwrap_or(&stripped)
            .trim()
            .to_string();
    }
    // Build dotted name from all routine_name parts (pkg.proc or just proc).
    let parts: Vec<String> = routine_names
        .iter()
        .map(|rn| node_text(source, &**rn).trim().to_string())
        .collect();
    parts.join(".")
}

// ---------------------------------------------------------------------------
// ANTLR rule-path extraction (USR-loop §2.1 — fine-grained gap signatures)
// ---------------------------------------------------------------------------

/// Maximum number of *descendant* rule names appended to the start
/// node's own rule name in an `antlr_rule_path`. `1` yields the
/// gap-node rule plus its single matched object rule
/// (`unit_statement>create_table`) — exactly the spec's "the rule
/// the parser was in" (§2.1): the object-defining grammar
/// position. Deeper descent (`>tableview_name>identifier`) is
/// occurrence-coupled and *less* dedup-stable and less robust under
/// P2 minimisation, so it is deliberately not taken (anti-gaming:
/// the path is the stable grammar position of the gap *class*,
/// never a per-occurrence fingerprint).
const RULE_PATH_MAX_DEPTH: usize = 1;

/// Resolve an ANTLR rule index to its grammar rule *name* via the
/// generated `ruleNames` table. The table is a compile-time grammar
/// constant — the returned string is therefore *never* estate data
/// (I-PRIVACY): it can only ever be one of the 1205 fixed PL/SQL
/// grammar rule identifiers (lowercase ASCII / `_`). An
/// out-of-range index (cannot occur for a real context) degrades to
/// `None` rather than leaking the raw integer.
fn rule_name(rule_index: usize) -> Option<&'static str> {
    crate::generated::plsqlparser::ruleNames
        .get(rule_index)
        .copied()
}

/// The deepest single rule-context child of `start`, *descending*
/// through the parse tree, collecting up to [`RULE_PATH_MAX_DEPTH`]
/// grammar rule names root→leaf, joined with `>`.
///
/// Why descend, not ascend: a gap node like `unit_statement` whose
/// typed handlers did not match still has ANTLR's *real* matched
/// sub-rule as a child (e.g. `create_sequence`, `alter_table`,
/// `drop_index`) — that child rule name is the genuinely
/// fine-grained grammar position that distinguishes one
/// `IR_DDL_NOT_LOWERED` class from another. The ancestor path is
/// always the coarse `unit_statement>sql_script` and cannot
/// discriminate. We follow the chain only while a node has exactly
/// one rule-context child (an unambiguous spine); a branch point
/// stops the descent (the path stays the stable grammar position,
/// never a per-occurrence fingerprint).
///
/// **I-PRIVACY (absolute):** every component is a grammar rule
/// *name* from the generated `ruleNames` constant — a fixed table
/// of PL/SQL grammar identifiers. Terminal/token children resolve
/// to `None` via [`rule_name`] and are skipped, so no source byte,
/// identifier, or literal is ever read (we never touch `source` or
/// token text). **I-DETERMINISM:** pure function of the parse-tree
/// shape. **R20:** the crossing value is a plain `String`; no ANTLR
/// generated *type* escapes.
fn rule_path_of<'i, N>(node: &N) -> Option<String>
where
    N: ParserRuleContext<'i, Ctx = crate::generated::plsqlparser::PlSqlParserContextType> + ?Sized,
{
    let mut names: Vec<String> = Vec::with_capacity(RULE_PATH_MAX_DEPTH + 1);
    if let Some(n) = rule_name(node.get_rule_index()) {
        names.push(n.to_string());
    }

    // Descend the unambiguous rule-context spine. The first hop is
    // off the concrete generic `node`; subsequent hops are off the
    // `dyn PlSqlParserContext` children it yields (same trait, same
    // `get_child`/`get_rule_index` API), so the recursion type is
    // uniform and no ANTLR concrete type escapes (R20).
    let mut current = sole_rule_child(node);
    while names.len() <= RULE_PATH_MAX_DEPTH {
        let Some(c) = current.clone() else { break };
        if let Some(n) = rule_name(c.get_rule_index()) {
            names.push(n.to_string());
        }
        current = sole_rule_child_dyn(&*c);
    }

    if names.is_empty() {
        None
    } else {
        Some(names.join(">"))
    }
}

/// Type alias for a parse-tree node behind the crate-private
/// `PlSqlParserContext` trait object — the uniform shape every
/// `get_child` yields. Stays inside the crate (R20).
type DynCtx<'i> = std::rc::Rc<dyn crate::generated::plsqlparser::PlSqlParserContext<'i> + 'i>;

/// The unique rule-context child of a concrete generic `node`, or
/// `None` if it has zero or more than one. See [`sole_rule_child_dyn`].
fn sole_rule_child<'i, N>(node: &N) -> Option<DynCtx<'i>>
where
    N: ParserRuleContext<'i, Ctx = crate::generated::plsqlparser::PlSqlParserContextType> + ?Sized,
{
    pick_sole_rule_child(node.get_child_count(), |i| node.get_child(i))
}

/// The unique rule-context child of a `dyn` parse-tree node (the
/// recursive hop). Terminal/token children are skipped (they have
/// no grammar rule → [`rule_name`] is `None`); a branch point
/// (>1 rule child) returns `None` so the path stays the stable
/// grammar spine, never a per-occurrence fingerprint (anti-gaming).
fn sole_rule_child_dyn<'i>(
    node: &(dyn crate::generated::plsqlparser::PlSqlParserContext<'i> + 'i),
) -> Option<DynCtx<'i>> {
    pick_sole_rule_child(node.get_child_count(), |i| node.get_child(i))
}

/// Shared core: scan `count` children via `get`, return the single
/// one that is a grammar rule (not a terminal), or `None` if zero
/// or ambiguous.
fn pick_sole_rule_child<'i>(
    count: usize,
    get: impl Fn(usize) -> Option<DynCtx<'i>>,
) -> Option<DynCtx<'i>> {
    let mut found: Option<DynCtx<'i>> = None;
    for i in 0..count {
        let Some(child) = get(i) else { continue };
        if rule_name(child.get_rule_index()).is_some() {
            if found.is_some() {
                return None;
            }
            found = Some(child);
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Span / text utilities
// ---------------------------------------------------------------------------

/// ANTLR inclusive `(start, stop)` byte offsets of one parse-tree node.
fn node_offsets<'i, N>(node: &N) -> (isize, isize)
where
    N: ParserRuleContext<'i> + ?Sized,
{
    (node.start().get_start(), node.stop().get_stop())
}

/// Verbatim source text covering one parse-tree node.
fn node_text<'i, N>(source: &str, node: &N) -> String
where
    N: ParserRuleContext<'i> + ?Sized,
{
    let (s, e) = node_offsets(node);
    extract_text(source, s, e)
}

/// Source text covering one parse-tree node, trimmed and upper-cased —
/// the canonical way object/identifier names are extracted from a child
/// context. Returns `""` when `node` is `None`.
fn node_name_upper<'i, N, P>(source: &str, node: Option<P>) -> String
where
    N: ParserRuleContext<'i> + ?Sized,
    P: std::ops::Deref<Target = N>,
{
    node.map(|n| {
        let (s, e) = node_offsets(&*n);
        extract_text(source, s, e)
    })
    .unwrap_or_default()
    .trim()
    .to_ascii_uppercase()
}

/// [`ctx_span`] for a whole parse-tree node (start..=stop).
fn node_span<'i, N>(node: &N, file_id: FileId, source: &str) -> Span
where
    N: ParserRuleContext<'i> + ?Sized,
{
    let (s, e) = node_offsets(node);
    ctx_span(s, e, file_id, source)
}

/// Extract the source text slice from ANTLR inclusive byte offsets.
fn extract_text(source: &str, start_incl: isize, stop_incl: isize) -> String {
    let s = start_incl.max(0) as usize;
    let e = (stop_incl.max(0) as usize + 1).min(source.len());
    if s >= source.len() || s >= e {
        return String::new();
    }
    source[s..e].to_string()
}

/// Build a [`Span`] from ANTLR inclusive token byte offsets.
fn ctx_span(start_incl: isize, stop_incl: isize, file_id: FileId, source: &str) -> Span {
    // Saturating cast (oracle-kxb3 sibling): for a >u32::MAX source
    // the trailing `source.len() as u32` would wrap and clip every
    // span. Saturate to `u32::MAX` (worst case: trailing spans clip
    // at the u32 horizon — never wrap).
    let s = u32::try_from(start_incl.max(0)).unwrap_or(u32::MAX);
    let stop_u32 = u32::try_from(stop_incl.max(0)).unwrap_or(u32::MAX);
    let len_u32 = u32::try_from(source.len()).unwrap_or(u32::MAX);
    let e = stop_u32.saturating_add(1).min(len_u32);
    make_span(file_id, s, e)
}

/// Build a [`Span`] from byte offsets (start inclusive, end exclusive).
pub(crate) fn make_span(file_id: FileId, start: u32, end: u32) -> Span {
    Span::new(
        file_id,
        Position::new(1, start + 1, start),
        Position::new(1, end + 1, end),
    )
}

/// Return `preferred` if it is non-empty (start < end), else `fallback`.
fn non_empty(preferred: Span, fallback: Span) -> Span {
    if preferred.start.offset < preferred.end.offset {
        preferred
    } else {
        fallback
    }
}

/// Take the last dot-separated component of a schema-qualified name.
/// `"HR.EMPLOYEES"` → `"EMPLOYEES"`, `"P"` → `"P"`.
fn last_component(name: String) -> String {
    if let Some(last) = name.rsplit('.').next() {
        last.trim().to_string()
    } else {
        name
    }
}

/// `true` iff `p` is a *specific* rule path (carries a `>` —
/// i.e. a descended child rule or a keyword-classified text-scan
/// path), as opposed to a bare single rule name.
fn is_specific_path(p: &Option<String>) -> bool {
    p.as_deref().is_some_and(|s| s.contains('>'))
}

/// Choose the better of the ANTLR-derived `rule_path` and any path
/// the text scanner already stamped on `decl` (`Ddl`/`Unknown`
/// only). The *more specific* path wins (one with a `>` child /
/// keyword over a bare rule name); ANTLR's real grammar position
/// is preferred on a tie since it is the genuine parse position.
/// Both candidates are privacy-safe grammar strings (I-PRIVACY);
/// fully-classified variants carry no gap so keep `None`.
fn with_rule_path(decl: AstDecl, rule_path: Option<String>) -> AstDecl {
    let pick = |existing: Option<String>| -> Option<String> {
        match (is_specific_path(&rule_path), is_specific_path(&existing)) {
            (true, _) => rule_path.clone(),
            (false, true) => existing,
            (false, false) => rule_path.clone().or(existing),
        }
    };
    match decl {
        AstDecl::Ddl {
            kind,
            span,
            antlr_rule_path,
        } => AstDecl::Ddl {
            kind,
            span,
            antlr_rule_path: pick(antlr_rule_path),
        },
        AstDecl::Unknown {
            span,
            antlr_rule_path,
        } => AstDecl::Unknown {
            span,
            antlr_rule_path: pick(antlr_rule_path),
        },
        other => other,
    }
}

/// Adjust a declaration's span by adding `base_offset` to both ends.
/// Used when extracting a sub-slice for the text-scanner fallback.
fn adjust_span(decl: AstDecl, base_offset: u32, file_id: FileId) -> AstDecl {
    let shift = |s: Span| -> Span {
        make_span(
            file_id,
            s.start.offset + base_offset,
            s.end.offset + base_offset,
        )
    };
    match decl {
        AstDecl::PackageSpec { name, span } => AstDecl::PackageSpec {
            name,
            span: shift(span),
        },
        AstDecl::PackageBody { name, span } => AstDecl::PackageBody {
            name,
            span: shift(span),
        },
        AstDecl::Procedure { name, span } => AstDecl::Procedure {
            name,
            span: shift(span),
        },
        AstDecl::Function { name, span } => AstDecl::Function {
            name,
            span: shift(span),
        },
        AstDecl::Trigger { name, span } => AstDecl::Trigger {
            name,
            span: shift(span),
        },
        AstDecl::View { name, span } => AstDecl::View {
            name,
            span: shift(span),
        },
        AstDecl::TypeSpec { name, span } => AstDecl::TypeSpec {
            name,
            span: shift(span),
        },
        AstDecl::TypeBody { name, span } => AstDecl::TypeBody {
            name,
            span: shift(span),
        },
        AstDecl::Ddl {
            kind,
            span,
            antlr_rule_path,
        } => AstDecl::Ddl {
            kind,
            span: shift(span),
            antlr_rule_path,
        },
        AstDecl::Unknown {
            span,
            antlr_rule_path,
        } => AstDecl::Unknown {
            span: shift(span),
            antlr_rule_path,
        },
    }
}
