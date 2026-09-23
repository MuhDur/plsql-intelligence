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

use antlr4rust::common_token_stream::CommonTokenStream;
use antlr4rust::error_listener::ErrorListener;
use antlr4rust::errors::ANTLRError;
use antlr4rust::input_stream::InputStream;
use antlr4rust::parser::Parser;
use antlr4rust::parser_rule_context::ParserRuleContext;
use antlr4rust::recognizer::Recognizer;
use antlr4rust::token::Token;
use antlr4rust::token_factory::TokenFactory;
use std::cell::RefCell;
use std::rc::Rc;

use plsql_core::{Diagnostic, FileId, Position, Severity, Span};
use plsql_parser::ast::{
    Ast, AstDecl, AstInitSection, AstInitializer, AstOverload, AstPackageCursor, AstPackageMember,
    AstPackageUnits, AstParam, AstParamMode, AstRoutineKind, AstStatement, AstUnattributed,
    SourceFile, SourceMap,
};

use crate::backend::ANTLR4RUST_DIAG_CODE;
use crate::generated::plsqllexer::PlSqlLexer;
use crate::generated::plsqlparser::{
    Assignment_statementContextAttrs, BodyContextAttrs, Call_statementContextAttrs,
    Create_function_bodyContextAttrs, Create_package_bodyContextAttrs, Create_packageContextAttrs,
    Create_procedure_bodyContextAttrs, Create_triggerContextAttrs, Create_typeContextAttrs,
    Create_viewContextAttrs, Cursor_declarationContextAttrs,
    Data_manipulation_language_statementsContextAttrs, Declare_specContextAttrs,
    Default_value_partContextAttrs, Exception_handlerContextAttrs, Execute_immediateContextAttrs,
    Function_bodyContextAttrs, Function_specContextAttrs, Package_obj_bodyContextAttrs,
    Package_obj_specContextAttrs, ParameterContextAttrs, PlSqlParser, Procedure_bodyContextAttrs,
    Procedure_specContextAttrs, Return_statementContextAttrs, Seq_of_declare_specsContextAttrs,
    Seq_of_statementsContextAttrs, Sql_scriptContextAttrs, Sql_statementContextAttrs,
    StatementContextAttrs, Trigger_blockContextAttrs, Trigger_bodyContextAttrs,
    Type_bodyContextAttrs, Type_definitionContextAttrs, Unit_statementContextAttrs,
    Variable_declarationContextAttrs,
};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

struct ParserErrors(Rc<RefCell<Vec<Diagnostic>>>);

impl<'a, T: Recognizer<'a>> ErrorListener<'a, T> for ParserErrors {
    fn syntax_error(
        &self,
        _recognizer: &T,
        _offending_symbol: Option<&<T::TF as TokenFactory<'a>>::Inner>,
        line: isize,
        column: isize,
        _msg: &str,
        _error: Option<&ANTLRError>,
    ) {
        // The generated parser's default console listener emits enormous
        // expected-token lists and does not feed BackendParseResult. Capture
        // a bounded diagnostic instead: recovery must never look Clean.
        self.0.borrow_mut().push(Diagnostic::new(
            ANTLR4RUST_DIAG_CODE,
            Severity::Error,
            format!("ANTLR syntax error at {line}:{column}"),
        ));
    }
}

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
    let parser_errors = Rc::new(RefCell::new(Vec::new()));
    parser.remove_error_listeners();
    parser.add_error_listener(Box::new(ParserErrors(Rc::clone(&parser_errors))));

    // Parse the top-level sql_script rule.
    let parsed_script = parser.sql_script();
    diagnostics.extend(parser_errors.borrow_mut().drain(..));
    let script_ctx = match parsed_script {
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

    resolve_body_overloads(&mut decls);

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
    let mut units = AstPackageUnits {
        lowered: true,
        ..AstPackageUnits::default()
    };
    for obj in &ctx.package_obj_spec_all() {
        if let Some(ps) = obj.procedure_spec() {
            units.members.push(AstPackageMember {
                name: ident_of(source, ps.identifier()),
                kind: AstRoutineKind::Procedure,
                overload: AstOverload::NotOverloaded,
                params: lower_params(&ps.parameter_all(), source, file_id),
                statements: Vec::new(),
                span: node_span(&*ps, file_id, source),
            });
            if ps.call_spec().is_some() {
                units
                    .unattributed
                    .push(unattributed(&*ps, "call_spec", file_id, source));
            }
        } else if let Some(fs) = obj.function_spec() {
            units.members.push(AstPackageMember {
                name: ident_of(source, fs.identifier()),
                kind: AstRoutineKind::Function,
                overload: AstOverload::NotOverloaded,
                params: lower_params(&fs.parameter_all(), source, file_id),
                statements: Vec::new(),
                span: node_span(&*fs, file_id, source),
            });
            if fs.call_spec().is_some() {
                units
                    .unattributed
                    .push(unattributed(&*fs, "call_spec", file_id, source));
            }
        } else if let Some(var) = obj.variable_declaration() {
            push_initializer(&mut units, &var, source, file_id);
        } else if let Some(cur) = obj.cursor_declaration() {
            push_cursor(&mut units, &cur, source, file_id);
        } else if obj.pragma_declaration().is_some() {
            // A package-level pragma can change invocation semantics. Keep
            // it visible to closure analysis instead of declaring it inert.
            units
                .unattributed
                .push(unattributed(&**obj, "package_pragma", file_id, source));
        } else if obj.type_declaration().is_none()
            && obj.subtype_declaration().is_none()
            && obj.exception_declaration().is_none()
        {
            units
                .unattributed
                .push(unattributed(&**obj, "unrecognized", file_id, source));
        }
    }
    assign_positional_overloads(&mut units.members, true);
    AstDecl::PackageSpec {
        name: node_name_exact(source, ctx.package_name(0)),
        span: non_empty(node_span(ctx, file_id, source), fallback_span),
        units,
    }
}

/// A package body lowers to separately addressable units — one per member
/// (with its overload identity, parameters, defaults and everything it
/// executes), the declaration initializers, the cursors and the
/// initialization section — instead of one flattened statement list, so
/// an invocation closure can union exactly the units a call runs. What
/// cannot be attributed is recorded in `units.unattributed` (fail closed).
fn lower_create_package_body(
    ctx: &crate::generated::plsqlparser::Create_package_bodyContextAll<'_>,
    source: &str,
    file_id: FileId,
    fallback_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> (AstDecl, Vec<AstStatement>) {
    let span = node_span(ctx, file_id, source);
    let name = node_name_exact(source, ctx.package_name(0));
    let mut units = AstPackageUnits {
        lowered: true,
        ..AstPackageUnits::default()
    };
    let mut package_pragmas = Vec::new();

    for obj in &ctx.package_obj_body_all() {
        if let Some(pb) = obj.procedure_body() {
            let mut statements = lower_declare_specs(
                pb.seq_of_declare_specs(),
                source,
                file_id,
                diagnostics,
                &mut units.unattributed,
            );
            if let Some(body) = pb.body() {
                statements.extend(lower_body_with_handlers(
                    &body,
                    source,
                    file_id,
                    diagnostics,
                ));
            }
            if pb.call_spec().is_some() {
                units
                    .unattributed
                    .push(unattributed(&*pb, "call_spec", file_id, source));
            }
            units.members.push(AstPackageMember {
                name: ident_of(source, pb.identifier()),
                kind: AstRoutineKind::Procedure,
                overload: AstOverload::NotOverloaded,
                params: lower_params(&pb.parameter_all(), source, file_id),
                statements,
                span: node_span(&*pb, file_id, source),
            });
        } else if let Some(fb) = obj.function_body() {
            let mut statements = lower_declare_specs(
                fb.seq_of_declare_specs(),
                source,
                file_id,
                diagnostics,
                &mut units.unattributed,
            );
            if let Some(body) = fb.body() {
                statements.extend(lower_body_with_handlers(
                    &body,
                    source,
                    file_id,
                    diagnostics,
                ));
            }
            if fb.call_spec().is_some() {
                units
                    .unattributed
                    .push(unattributed(&*fb, "call_spec", file_id, source));
            }
            units.members.push(AstPackageMember {
                name: ident_of(source, fb.identifier()),
                kind: AstRoutineKind::Function,
                overload: AstOverload::NotOverloaded,
                params: lower_params(&fb.parameter_all(), source, file_id),
                statements,
                span: node_span(&*fb, file_id, source),
            });
        } else if let Some(var) = obj.variable_declaration() {
            push_initializer(&mut units, &var, source, file_id);
        } else if let Some(cur) = obj.cursor_declaration() {
            push_cursor(&mut units, &cur, source, file_id);
        } else if obj.selection_directive().is_some() {
            units.unattributed.push(unattributed(
                &**obj,
                "conditional_compilation",
                file_id,
                source,
            ));
        } else if let Some(pragma) = obj.pragma_declaration() {
            package_pragmas.push(AstStatement::Sql {
                verb: "PRAGMA".to_string(),
                raw_text: node_text(source, &*pragma),
                span: node_span(&*pragma, file_id, source),
            });
        } else if obj.procedure_spec().is_none()
            && obj.function_spec().is_none()
            && obj.type_declaration().is_none()
            && obj.subtype_declaration().is_none()
            && obj.exception_declaration().is_none()
        {
            // Forward declarations, types, subtypes, exceptions and pragmas
            // have no executable effect; anything else is not attributable.
            units
                .unattributed
                .push(unattributed(&**obj, "unrecognized", file_id, source));
        }
    }

    // The initialization section (`BEGIN … [EXCEPTION …] END` after the
    // members) runs once per session at instantiation, with its handlers.
    let seq = ctx.seq_of_statements();
    let handlers = ctx.exception_handler_all();
    if seq.is_some() || !handlers.is_empty() {
        let mut statements = package_pragmas.clone();
        statements.extend(
            seq.as_ref()
                .map(|seq| lower_seq_of_statements(seq, source, file_id, diagnostics))
                .unwrap_or_default(),
        );
        for handler in &handlers {
            if let Some(hseq) = handler.seq_of_statements() {
                statements.extend(lower_seq_of_statements(&hseq, source, file_id, diagnostics));
            }
        }
        let section_span = seq
            .as_ref()
            .map(|seq| node_span(&**seq, file_id, source))
            .unwrap_or(span);
        units.init_section = Some(AstInitSection {
            statements,
            span: section_span,
        });
    } else if !package_pragmas.is_empty() {
        units.init_section = Some(AstInitSection {
            statements: package_pragmas,
            span,
        });
    }
    assign_positional_overloads(&mut units.members, false);

    (
        AstDecl::PackageBody {
            name,
            span: non_empty(span, fallback_span),
            units,
        },
        Vec::new(),
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
    let name = last_component(node_name_exact(source, ctx.procedure_name()));
    let mut unattributed = Vec::new();
    let mut stmts = lower_declare_specs(
        ctx.seq_of_declare_specs(),
        source,
        file_id,
        diagnostics,
        &mut unattributed,
    );
    if !unattributed.is_empty() {
        stmts.push(AstStatement::Unknown {
            span: node_span(ctx, file_id, source),
        });
    }
    if let Some(body) = ctx.body() {
        stmts.extend(lower_body_stmts(&body, source, file_id, diagnostics));
    }
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
    let name = last_component(node_name_exact(source, ctx.function_name()));
    let mut unattributed = Vec::new();
    let mut stmts = lower_declare_specs(
        ctx.seq_of_declare_specs(),
        source,
        file_id,
        diagnostics,
        &mut unattributed,
    );
    if !unattributed.is_empty() {
        stmts.push(AstStatement::Unknown {
            span: node_span(ctx, file_id, source),
        });
    }
    if let Some(body) = ctx.body() {
        stmts.extend(lower_body_stmts(&body, source, file_id, diagnostics));
    }
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
    let name = last_component(node_name_exact(source, ctx.trigger_name()));
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
        Some(ie) => node_name_exact(source, Some(ie)),
        None => node_name_exact(source, ctx.id_expression(0)),
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
            name: node_name_exact(source, td.type_name()),
            span: span(),
        };
    }
    if let Some(tb) = ctx.type_body() {
        return AstDecl::TypeBody {
            name: node_name_exact(source, tb.type_name()),
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
// Package units (members, initializers, cursors, init section)
// ---------------------------------------------------------------------------

type ParameterCtx<'i> = std::rc::Rc<crate::generated::plsqlparser::ParameterContextAll<'i>>;

/// Exact identity of a PL/SQL identifier: a quoted name keeps its case
/// verbatim (without the quotes); an unquoted name folds to upper case.
fn exact_ident(text: &str) -> String {
    let t = text.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_ascii_uppercase()
    }
}

fn lower_params(params: &[ParameterCtx<'_>], source: &str, file_id: FileId) -> Vec<AstParam> {
    params
        .iter()
        .map(|p| {
            let name = p
                .parameter_name()
                .map(|n| exact_ident(&node_text(source, &*n)))
                .unwrap_or_default();
            let mode = if !p.INOUT_all().is_empty()
                || (!p.IN_all().is_empty() && !p.OUT_all().is_empty())
            {
                AstParamMode::InOut
            } else if !p.OUT_all().is_empty() {
                AstParamMode::Out
            } else {
                AstParamMode::In
            };
            let type_text = p
                .type_spec()
                .map(|t| node_text(source, &*t).trim().to_string())
                .unwrap_or_default();
            let default_text = p
                .default_value_part()
                .and_then(|d| d.expression())
                .map(|e| node_text(source, &*e).trim().to_string())
                .filter(|s| !s.is_empty());
            AstParam {
                name,
                mode,
                type_text,
                default_text,
                span: node_span(&**p, file_id, source),
            }
        })
        .collect()
}

fn unattributed<'i, N>(node: &N, reason: &str, file_id: FileId, source: &str) -> AstUnattributed
where
    N: ParserRuleContext<'i, Ctx = crate::generated::plsqlparser::PlSqlParserContextType> + ?Sized,
{
    AstUnattributed {
        span: node_span(node, file_id, source),
        reason: reason.to_string(),
        antlr_rule_path: rule_path_of(node),
    }
}

/// A `body` (BEGIN … [EXCEPTION …] END) lowered with its exception
/// handlers: a handler's statements run as part of the same unit.
fn lower_body_with_handlers(
    body: &crate::generated::plsqlparser::BodyContextAll<'_>,
    source: &str,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<AstStatement> {
    let mut out = lower_body_stmts(body, source, file_id, diagnostics);
    for handler in &body.exception_handler_all() {
        if let Some(seq) = handler.seq_of_statements() {
            out.extend(lower_seq_of_statements(&seq, source, file_id, diagnostics));
        }
    }
    out
}

/// Statements a routine's local declaration section contributes to the
/// routine's own execution: variable initializers (as assignments), cursor
/// queries (as SELECTs, conservatively: they run when opened) and nested
/// subprograms (conservatively attributed to the enclosing member).
fn lower_declare_specs(
    specs: Option<std::rc::Rc<crate::generated::plsqlparser::Seq_of_declare_specsContextAll<'_>>>,
    source: &str,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
    unattributed_out: &mut Vec<AstUnattributed>,
) -> Vec<AstStatement> {
    let mut out = Vec::new();
    let Some(specs) = specs else { return out };
    for spec in &specs.declare_spec_all() {
        if let Some(var) = spec.variable_declaration() {
            if let Some(init) = var.default_value_part().and_then(|d| d.expression()) {
                out.push(AstStatement::Assignment {
                    target: var
                        .identifier()
                        .map(|i| exact_ident(&node_text(source, &*i)))
                        .unwrap_or_default(),
                    rhs_text: node_text(source, &*init).trim().to_string(),
                    span: node_span(&*var, file_id, source),
                });
            }
        } else if let Some(cur) = spec.cursor_declaration() {
            if let Some(q) = cur.select_statement() {
                out.push(AstStatement::Sql {
                    verb: "SELECT".to_string(),
                    raw_text: node_text(source, &*q),
                    span: node_span(&*cur, file_id, source),
                });
            }
        } else if let Some(pb) = spec.procedure_body() {
            out.extend(lower_declare_specs(
                pb.seq_of_declare_specs(),
                source,
                file_id,
                diagnostics,
                unattributed_out,
            ));
            if let Some(b) = pb.body() {
                out.extend(lower_body_with_handlers(&b, source, file_id, diagnostics));
            }
            if pb.call_spec().is_some() {
                unattributed_out.push(unattributed(&*pb, "call_spec", file_id, source));
            }
        } else if let Some(fb) = spec.function_body() {
            out.extend(lower_declare_specs(
                fb.seq_of_declare_specs(),
                source,
                file_id,
                diagnostics,
                unattributed_out,
            ));
            if let Some(b) = fb.body() {
                out.extend(lower_body_with_handlers(&b, source, file_id, diagnostics));
            }
            if fb.call_spec().is_some() {
                unattributed_out.push(unattributed(&*fb, "call_spec", file_id, source));
            }
        } else if spec.selection_directive().is_some() {
            unattributed_out.push(unattributed(
                &**spec,
                "conditional_compilation",
                file_id,
                source,
            ));
        } else if let Some(pragma) = spec.pragma_declaration() {
            out.push(AstStatement::Sql {
                verb: "PRAGMA".to_string(),
                raw_text: node_text(source, &*pragma),
                span: node_span(&*pragma, file_id, source),
            });
        } else if spec.type_declaration().is_some()
            || spec.subtype_declaration().is_some()
            || spec.exception_declaration().is_some()
            || spec.procedure_spec().is_some()
            || spec.function_spec().is_some()
        {
            // No executable effect.
        } else {
            unattributed_out.push(unattributed(&**spec, "unrecognized", file_id, source));
        }
    }
    out
}

/// Exact identity of an optional identifier node (see [`exact_ident`]).
fn ident_of<'i, N, P>(source: &str, node: Option<P>) -> String
where
    N: ParserRuleContext<'i> + ?Sized,
    P: std::ops::Deref<Target = N>,
{
    node.map(|n| exact_ident(&node_text(source, &*n)))
        .unwrap_or_default()
}

fn push_initializer(
    units: &mut AstPackageUnits,
    var: &crate::generated::plsqlparser::Variable_declarationContextAll<'_>,
    source: &str,
    file_id: FileId,
) {
    let name = ident_of(source, var.identifier());
    units.state_variables.push(name.clone());
    if let Some(init) = var.default_value_part().and_then(|d| d.expression()) {
        units.initializers.push(AstInitializer {
            name,
            initializer_text: node_text(source, &*init).trim().to_string(),
            span: node_span(var, file_id, source),
        });
    }
}

fn push_cursor(
    units: &mut AstPackageUnits,
    cur: &crate::generated::plsqlparser::Cursor_declarationContextAll<'_>,
    source: &str,
    file_id: FileId,
) {
    units.cursors.push(AstPackageCursor {
        name: ident_of(source, cur.identifier()),
        query_text: cur
            .select_statement()
            .map(|q| node_text(source, &*q))
            .unwrap_or_default(),
        span: node_span(cur, file_id, source),
    });
}

/// Overload identity within one package part, by order of appearance.
///
/// In a specification this *is* Oracle's `ALL_ARGUMENTS.OVERLOAD`: the nth
/// overloading by appearance, `NULL` for a name that occurs once. In a body
/// the order is not Oracle's (it may interleave private subprograms), so an
/// overloaded body name is provisionally `Ambiguous` until
/// [`resolve_body_overloads`] joins it to the specification.
fn assign_positional_overloads(members: &mut [AstPackageMember], is_spec: bool) {
    let mut totals: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for m in members.iter() {
        *totals.entry(m.name.clone()).or_default() += 1;
    }
    let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for m in members.iter_mut() {
        let position = {
            let e = seen.entry(m.name.clone()).or_default();
            *e += 1;
            *e
        };
        m.overload = if totals[&m.name] == 1 {
            AstOverload::NotOverloaded
        } else if is_spec {
            AstOverload::Ordinal(position)
        } else {
            AstOverload::Ambiguous { position }
        };
    }
}

/// The conformance key Oracle requires between a spec header and its body:
/// kind plus each parameter's name, mode and (case/space-normalized) type.
fn signature_key(m: &AstPackageMember) -> (AstRoutineKind, Vec<(String, AstParamMode, String)>) {
    (
        m.kind,
        m.params
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    p.mode,
                    p.type_text
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_ascii_uppercase(),
                )
            })
            .collect(),
    )
}

/// Join each body member to its specification header when the spec is in
/// the same source: a body member whose name the spec declares takes the
/// spec's overload identity if exactly one spec header conforms to it, and
/// is `Ambiguous` otherwise (never guessed). Body-private members keep
/// their positional identity. Without a spec in this source, positional
/// identities stand.
fn resolve_body_overloads(decls: &mut [AstDecl]) {
    let specs: Vec<(String, Vec<AstPackageMember>)> = decls
        .iter()
        .filter_map(|d| match d {
            AstDecl::PackageSpec { name, units, .. } if units.lowered => {
                Some((name.clone(), units.members.clone()))
            }
            _ => None,
        })
        .collect();
    for decl in decls.iter_mut() {
        let AstDecl::PackageBody { name, units, .. } = decl else {
            continue;
        };
        let Some((_, spec_members)) = specs.iter().find(|(n, _)| n == name) else {
            continue;
        };
        let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for m in units.members.iter_mut() {
            let position = {
                let e = seen.entry(m.name.clone()).or_default();
                *e += 1;
                *e
            };
            let same_name: Vec<&AstPackageMember> =
                spec_members.iter().filter(|s| s.name == m.name).collect();
            if same_name.is_empty() {
                continue;
            }
            let key = signature_key(m);
            let conforming: Vec<&&AstPackageMember> = same_name
                .iter()
                .filter(|s| signature_key(s) == key)
                .collect();
            m.overload = match conforming.as_slice() {
                [only] => only.overload,
                _ => AstOverload::Ambiguous { position },
            };
        }
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
            // USING expressions and INTO targets can themselves carry
            // effects. Until those AST children are modeled, the IR must
            // retain an uncertainty marker for either clause.
            let has_using = exec_imm.using_clause().is_some()
                || exec_imm.into_clause().is_some()
                || exec_imm.dynamic_returning_clause().is_some();
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
        return Some(AstStatement::Call {
            callee,
            raw_text: node_text(source, &*call_s),
            span,
        });
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

/// Oracle-canonical name from a parse-tree node: unquoted identifiers fold
/// to upper case; a quoted identifier retains its exact case.
fn node_name_exact<'i, N, P>(source: &str, node: Option<P>) -> String
where
    N: ParserRuleContext<'i> + ?Sized,
    P: std::ops::Deref<Target = N>,
{
    let raw = node.map(|n| {
        let (s, e) = node_offsets(&*n);
        extract_text(source, s, e)
    });
    exact_ident(raw.unwrap_or_default().trim())
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
        AstDecl::PackageSpec { name, span, units } => AstDecl::PackageSpec {
            name,
            span: shift(span),
            units,
        },
        AstDecl::PackageBody { name, span, units } => AstDecl::PackageBody {
            name,
            span: shift(span),
            units,
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
