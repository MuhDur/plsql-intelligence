//! Call-site edge extraction (PLSQL-DEP-002).
//!
//! Walks a lowered statement body and pulls out every
//! procedure / function invocation as a [`CallSite`]. The
//! dependency-graph layer resolves each `callee` to a concrete
//! node (via `plsql_symbols::resolve_reference`) and mints a
//! `Calls` edge; this module's job is purely *extraction* — find
//! the call sites and their shape.
//!
//! Calls appear in three places:
//!
//! 1. Statement-level procedure calls — a bare
//!    `Statement::Unrecognized` line whose text is
//!    `pkg.proc(args);` (the stmt recogniser leaves these
//!    unclassified because they're neither assignment nor
//!    control flow).
//! 2. Expression-embedded function calls — inside an
//!    `Assignment.rhs_text`, an `If` arm condition, a loop
//!    range, a `Return` value, etc.
//! 3. Nested calls — `nvl(compute(x), 0)` yields both `nvl`
//!    and `compute`.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   call grammar (positional / named notation, package-
//!   qualified vs bare) drives what counts as a callee.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_DEPENDENCIES` with `DEPENDENCY_TYPE` is the
//!   server-side mirror the depgraph cross-checks `Calls`
//!   edges against.

use serde::{Deserialize, Serialize};

use crate::expr::{Expr, lower_expression};
use crate::stmt::Statement;

/// One extracted call site.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    /// Dotted callee path, case-folded for the lookup key.
    pub callee_parts: Vec<String>,
    /// Source-form callee path preserved for diagnostics.
    pub callee_display: String,
    /// Number of positional arguments at the call. Named-notation
    /// args still count toward arity here; the depgraph's overload
    /// resolver (SYM-009) handles named-vs-positional matching.
    pub arg_count: usize,
    /// Context the call appeared in — drives the edge's
    /// confidence + the report wording.
    pub context: CallContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallContext {
    /// Statement-level procedure call (`pkg.proc(args);`).
    Statement,
    /// Function call inside an assignment RHS.
    Assignment,
    /// Function call inside a control-flow condition / range.
    ControlFlow,
    /// Function call inside a RETURN expression.
    ReturnValue,
}

/// Extract every call site from a lowered statement body.
///
/// Backwards-compatible wrapper around
/// [`extract_call_sites_bounded`]: the recursion is depth-guarded
/// (`oracle-v4wa`) so a malformed unit whose re-lowered body fails
/// to shrink can never stack-overflow. Callers that need to surface
/// the typed [`plsql_core::UnknownReason::AnalysisRecursionLimit`]
/// degradation should call [`extract_call_sites_bounded`] directly.
#[must_use]
pub fn extract_call_sites(stmts: &[Statement]) -> Vec<CallSite> {
    extract_call_sites_bounded(stmts).0
}

/// Depth-bounded variant of [`extract_call_sites`]. Returns the
/// extracted call sites plus a [`RecursionOutcome`] recording
/// whether (and how often) a nested body was abandoned at the
/// recursion-depth cap rather than walked unbounded. The caller is
/// responsible for emitting an honest typed diagnostic when
/// `outcome.limit_hit` (R13 — never silently truncate).
#[must_use]
pub fn extract_call_sites_bounded(stmts: &[Statement]) -> (Vec<CallSite>, crate::RecursionOutcome) {
    let mut out: Vec<CallSite> = Vec::new();
    let mut outcome = crate::RecursionOutcome::default();
    walk_call_sites(stmts, 0, &mut out, &mut outcome);
    (out, outcome)
}

fn walk_call_sites(
    stmts: &[Statement],
    depth: usize,
    out: &mut Vec<CallSite>,
    outcome: &mut crate::RecursionOutcome,
) {
    // Recurse into a re-lowered body only while we have depth
    // budget left. At the cap we stop descending and record the
    // truncation so the caller can surface it honestly — we do
    // NOT silently drop it and we do NOT keep recursing (which
    // would stack-overflow on a non-shrinking malformed slice).
    macro_rules! recurse_body {
        ($text:expr) => {{
            if depth + 1 >= crate::MAX_RELOWER_DEPTH {
                outcome.note_truncated();
            } else {
                let lowered = crate::lower_statement_body($text);
                walk_call_sites(&lowered, depth + 1, out, outcome);
            }
        }};
    }
    for stmt in stmts {
        match stmt {
            Statement::Assignment { rhs_text, .. } => {
                collect_calls(&lower_expression(rhs_text), CallContext::Assignment, out);
            }
            Statement::Return {
                value_text: Some(v),
            } => {
                collect_calls(&lower_expression(v), CallContext::ReturnValue, out);
            }
            Statement::If {
                arms,
                else_body_text,
            } => {
                for arm in arms {
                    collect_calls(
                        &lower_expression(&arm.cond_text),
                        CallContext::ControlFlow,
                        out,
                    );
                    recurse_body!(&arm.body_text);
                }
                if let Some(eb) = else_body_text {
                    recurse_body!(eb);
                }
            }
            Statement::WhileLoop {
                cond_text,
                body_text,
            } => {
                collect_calls(&lower_expression(cond_text), CallContext::ControlFlow, out);
                recurse_body!(body_text);
            }
            Statement::ForLoop {
                range_text,
                body_text,
                ..
            } => {
                collect_calls(&lower_expression(range_text), CallContext::ControlFlow, out);
                recurse_body!(body_text);
            }
            Statement::BareLoop { body_text } => {
                recurse_body!(body_text);
            }
            Statement::NestedBlock { body_text } => {
                // Strip the BEGIN…END / DECLARE…END wrapper before
                // re-lowering, otherwise the stmt recogniser keeps
                // classifying the same text as a NestedBlock and
                // recursion never terminates.
                let inner = strip_block_wrapper(body_text);
                if inner != body_text.as_str() {
                    recurse_body!(inner);
                } else {
                    // No wrapper to strip — treat the text as a
                    // single expression candidate instead of
                    // recursing.
                    collect_calls(&lower_expression(body_text), CallContext::Statement, out);
                }
            }
            Statement::Unrecognized { raw_text, .. } => {
                // Statement-level procedure call: `pkg.proc(args);`.
                let e = lower_expression(raw_text);
                collect_calls(&e, CallContext::Statement, out);
            }
            _ => {}
        }
    }
}

/// Strip a leading `DECLARE`/`BEGIN` and a trailing `END[;]`
/// from a block body so the inner statements can be re-lowered
/// without re-triggering the NestedBlock classification.
fn strip_block_wrapper(text: &str) -> &str {
    let trimmed = text.trim();
    let upper = trimmed.to_ascii_uppercase();
    let after_open = if let Some(rest) = upper.strip_prefix("DECLARE") {
        &trimmed[trimmed.len() - rest.len()..]
    } else if let Some(rest) = upper.strip_prefix("BEGIN") {
        &trimmed[trimmed.len() - rest.len()..]
    } else {
        return text;
    };
    let after_open = after_open.trim_start();
    // Drop a trailing `END;` / `END`.
    let upper_inner = after_open.to_ascii_uppercase();
    if let Some(pos) = upper_inner.rfind("END") {
        after_open[..pos].trim_end()
    } else {
        after_open
    }
}

fn collect_calls(expr: &Expr, ctx: CallContext, out: &mut Vec<CallSite>) {
    match expr {
        Expr::Call { callee, args } => {
            out.push(CallSite {
                callee_parts: callee.parts.clone(),
                callee_display: callee.display.clone(),
                arg_count: args.len(),
                context: ctx,
            });
            for a in args {
                collect_calls(a, ctx, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_calls(lhs, ctx, out);
            collect_calls(rhs, ctx, out);
        }
        Expr::Unary { operand, .. } => collect_calls(operand, ctx, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower_statement_body;

    #[test]
    fn assignment_rhs_call_extracted() {
        let stmts = lower_statement_body("v_total := compute_sum(a, b);");
        let calls = extract_call_sites(&stmts);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].callee_parts, vec!["COMPUTE_SUM"]);
        assert_eq!(calls[0].arg_count, 2);
        assert_eq!(calls[0].context, CallContext::Assignment);
    }

    #[test]
    fn nested_call_yields_both_callees() {
        let stmts = lower_statement_body("v := nvl(compute(x), 0);");
        let calls = extract_call_sites(&stmts);
        let names: Vec<&str> = calls.iter().map(|c| c.callee_display.as_str()).collect();
        assert!(names.contains(&"nvl"));
        assert!(names.contains(&"compute"));
    }

    #[test]
    fn return_value_call_context() {
        let stmts = lower_statement_body("RETURN compute_total(p_id);");
        let calls = extract_call_sites(&stmts);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].context, CallContext::ReturnValue);
    }

    #[test]
    fn statement_level_proc_call_extracted() {
        let stmts = lower_statement_body("billing_pkg.post_invoice(p_id, p_amount);");
        let calls = extract_call_sites(&stmts);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].callee_parts, vec!["BILLING_PKG", "POST_INVOICE"]);
        assert_eq!(calls[0].context, CallContext::Statement);
        assert_eq!(calls[0].arg_count, 2);
    }

    #[test]
    fn if_condition_and_body_calls_extracted() {
        let src = "IF is_valid(p_id) THEN log_event('ok'); END IF;";
        let stmts = lower_statement_body(src);
        let calls = extract_call_sites(&stmts);
        let names: Vec<&str> = calls.iter().map(|c| c.callee_display.as_str()).collect();
        assert!(names.contains(&"is_valid"));
        assert!(names.contains(&"log_event"));
    }

    #[test]
    fn for_loop_body_calls_recursed() {
        let src = "FOR i IN 1..10 LOOP process_row(i); END LOOP;";
        let stmts = lower_statement_body(src);
        let calls = extract_call_sites(&stmts);
        assert!(calls.iter().any(|c| c.callee_display == "process_row"));
    }

    #[test]
    fn no_calls_in_pure_arithmetic() {
        let stmts = lower_statement_body("v := a + b * 2;");
        let calls = extract_call_sites(&stmts);
        assert!(calls.is_empty());
    }

    #[test]
    fn binary_operands_searched_for_calls() {
        let stmts = lower_statement_body("v := f(x) + g(y);");
        let calls = extract_call_sites(&stmts);
        let names: Vec<&str> = calls.iter().map(|c| c.callee_display.as_str()).collect();
        assert!(names.contains(&"f"));
        assert!(names.contains(&"g"));
    }

    #[test]
    fn callsite_serde_round_trip() {
        let stmts = lower_statement_body("v := compute(a);");
        let calls = extract_call_sites(&stmts);
        let json = serde_json::to_string(&calls[0]).unwrap();
        let back: CallSite = serde_json::from_str(&json).unwrap();
        assert_eq!(back, calls[0]);
        assert!(json.contains("\"context\":\"assignment\""));
    }

    #[test]
    fn nested_block_calls_recursed() {
        let stmts = lower_statement_body("BEGIN inner_proc(1); END;");
        let calls = extract_call_sites(&stmts);
        assert!(calls.iter().any(|c| c.callee_display == "inner_proc"));
    }

    // oracle-v4wa: the exact crash shape from the bundled public
    // fixture `corpus/synthetic/l1/pkg_error_handling.pkb`. A
    // `SELECT … FOR UPDATE;` body fragment leaves the bare token
    // `FOR UPDATE`; the text-scanner's `classify_loop` treats
    // `FOR …` as a FOR-loop, finds no `IN` and no `END LOOP`, and
    // falls back to a `BareLoop` whose `body_text` is *the same
    // string* `FOR UPDATE`. Re-lowering it yields the identical
    // non-shrinking `BareLoop` → before the depth guard this
    // recursed unbounded and aborted the whole `analyze`
    // (SIGABRT / "stack overflow"). It must now terminate and
    // report the truncation honestly (R13).
    #[test]
    fn non_shrinking_for_update_does_not_stack_overflow_and_reports_limit() {
        let stmts = vec![Statement::BareLoop {
            body_text: "FOR UPDATE".to_string(),
        }];
        let (calls, outcome) = extract_call_sites_bounded(&stmts);
        assert!(
            outcome.limit_hit,
            "the non-shrinking `FOR UPDATE` BareLoop must trip the \
             bounded depth cap, outcome={outcome:?}, calls={calls:?}"
        );
        assert!(outcome.truncated_bodies >= 1);
        // The back-compat wrapper must also simply terminate
        // (no panic / abort) rather than recurse unbounded.
        let _ = extract_call_sites(&stmts);
    }
}
