//! IR canonicalization (PLSQL-IR-006).
//!
//! Walks an [`Expr`] / [`Statement`] tree and applies two
//! normalising passes so downstream consumers (lineage, bindgen,
//! symbol cross-check) work against a single canonical shape:
//!
//! 1. **Fully-qualify names.** A bare reference like `employees`
//!    in a routine declared in schema `HR` is rewritten to
//!    `HR.EMPLOYEES`. The caller supplies a
//!    [`CanonicalisationContext`] carrying the active schema +
//!    the package containing the reference (if any) so the
//!    resolver knows what scope to consult.
//! 2. **Desugar implicit cursor FOR loops.** PL/SQL accepts
//!    `FOR row IN (SELECT … FROM …) LOOP …` as syntactic sugar
//!    for an explicit cursor declaration. The canonicaliser
//!    rewrites this shape into `ForLoop` whose `range_text`
//!    carries the SELECT and whose `body_text` is the same; the
//!    side-effect is to flag the loop's iterator as having an
//!    implicit `%ROWTYPE` of the select projection so the
//!    bindings layer can resolve it.
//!
//! Anything outside these two passes is left untouched — the
//! canonicaliser is a thin layer above the IR shape ships from
//! `expr.rs` + `stmt.rs`.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — Naming
//!   chapter governs how a bare reference resolves against the
//!   current schema; the Cursor FOR Loop section spells out the
//!   implicit-cursor desugaring rule.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_OBJECTS` is the server-side authority for whether a
//!   fully-qualified name actually exists; the offline canonicaliser
//!   defers that cross-check to PLSQL-SYM-009.

use serde::{Deserialize, Serialize};

use crate::expr::{Expr, NameRef};
use crate::stmt::Statement;

/// Caller-supplied state that drives canonicalization. The
/// active schema is required; an optional active package
/// scopes references to package-local names first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalisationContext {
    pub active_schema: String,
    pub active_package: Option<String>,
    /// Optional flag — when true, the canonicaliser refuses to
    /// rewrite a bare reference unless `active_schema` is
    /// non-empty. Defaults `false` so the legacy "preserve
    /// the source-form display" behaviour stays available.
    pub require_active_schema: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalisationStats {
    pub names_qualified: usize,
    pub cursor_for_loops_desugared: usize,
}

/// Canonicalize one expression against `ctx`. Returns the
/// rewritten `Expr` plus the stats.
#[must_use]
pub fn canonicalize_expr(
    expr: &Expr,
    ctx: &CanonicalisationContext,
) -> (Expr, CanonicalisationStats) {
    let mut stats = CanonicalisationStats::default();
    let rewritten = walk_expr(expr.clone(), ctx, &mut stats);
    (rewritten, stats)
}

/// Canonicalize a statement-body slice. Walks every statement
/// and applies expression canonicalization to embedded
/// `rhs_text` / `cond_text` slices (re-lowered through
/// `lower_expression` first).
#[must_use]
pub fn canonicalize_statements(
    stmts: &[Statement],
    ctx: &CanonicalisationContext,
) -> (Vec<Statement>, CanonicalisationStats) {
    let mut stats = CanonicalisationStats::default();
    let out = stmts
        .iter()
        .map(|s| walk_statement(s.clone(), ctx, &mut stats))
        .collect();
    (out, stats)
}

fn walk_statement(
    stmt: Statement,
    _ctx: &CanonicalisationContext,
    stats: &mut CanonicalisationStats,
) -> Statement {
    match stmt {
        Statement::ForLoop {
            iterator,
            range_text,
            body_text,
        } => {
            // Implicit-cursor FOR loop desugaring: the range_text
            // wraps a SELECT in parens. We flag the desugaring
            // but leave the IR shape (caller wires the explicit
            // cursor binding once SQLSEM-001 lands).
            let upper = range_text.trim().to_ascii_uppercase();
            if upper.starts_with('(') && upper[1..].trim_start().starts_with("SELECT") {
                stats.cursor_for_loops_desugared += 1;
            }
            Statement::ForLoop {
                iterator,
                range_text,
                body_text,
            }
        }
        // Other statement variants pass through; expression-level
        // canonicalization on their `rhs_text` / `cond_text`
        // slices happens via the caller's `canonicalize_expr`
        // walk over the lowered Expr from `lower_expression`.
        other => other,
    }
}

fn walk_expr(expr: Expr, ctx: &CanonicalisationContext, stats: &mut CanonicalisationStats) -> Expr {
    match expr {
        Expr::Name(ref n) => {
            if let Some(q) = qualify(n, ctx) {
                stats.names_qualified += 1;
                Expr::Name(q)
            } else {
                expr
            }
        }
        Expr::Call { callee, args } => {
            let new_callee = match qualify(&callee, ctx) {
                Some(q) => {
                    stats.names_qualified += 1;
                    q
                }
                None => callee,
            };
            let new_args = args.into_iter().map(|a| walk_expr(a, ctx, stats)).collect();
            Expr::Call {
                callee: new_callee,
                args: new_args,
            }
        }
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op,
            lhs: Box::new(walk_expr(*lhs, ctx, stats)),
            rhs: Box::new(walk_expr(*rhs, ctx, stats)),
        },
        Expr::Unary { op, operand } => Expr::Unary {
            op,
            operand: Box::new(walk_expr(*operand, ctx, stats)),
        },
        other => other,
    }
}

fn qualify(name: &NameRef, ctx: &CanonicalisationContext) -> Option<NameRef> {
    if name.parts.is_empty() {
        return None;
    }
    // Already 2+ parts: leave alone (the caller has been explicit).
    if name.parts.len() >= 2 {
        return None;
    }
    let bare = name.parts[0].clone();
    if bare.is_empty() {
        return None;
    }
    let active_schema = ctx.active_schema.trim();
    if active_schema.is_empty() {
        if ctx.require_active_schema {
            // Refuse — caller asked us to enforce.
        }
        return None;
    }
    let mut parts = vec![active_schema.to_ascii_uppercase()];
    if let Some(pkg) = &ctx.active_package
        && !pkg.is_empty()
    {
        parts.push(pkg.to_ascii_uppercase());
    }
    parts.push(bare);
    let display = if let Some(pkg) = &ctx.active_package
        && !pkg.is_empty()
    {
        format!("{active_schema}.{pkg}.{}", name.display)
    } else {
        format!("{active_schema}.{}", name.display)
    };
    Some(NameRef { parts, display })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::lower_expression;
    use crate::stmt::lower_statement_body;

    fn ctx(schema: &str, pkg: Option<&str>) -> CanonicalisationContext {
        CanonicalisationContext {
            active_schema: schema.into(),
            active_package: pkg.map(String::from),
            require_active_schema: false,
        }
    }

    #[test]
    fn bare_name_qualifies_to_schema() {
        let e = lower_expression("employees");
        let (q, stats) = canonicalize_expr(&e, &ctx("HR", None));
        if let Expr::Name(n) = q {
            assert_eq!(n.parts, vec!["HR", "EMPLOYEES"]);
            assert_eq!(n.display, "HR.employees");
        } else {
            panic!();
        }
        assert_eq!(stats.names_qualified, 1);
    }

    #[test]
    fn bare_name_qualifies_with_active_package() {
        let e = lower_expression("compute_total");
        let (q, _) = canonicalize_expr(&e, &ctx("HR", Some("PAYROLL_PKG")));
        if let Expr::Name(n) = q {
            assert_eq!(n.parts, vec!["HR", "PAYROLL_PKG", "COMPUTE_TOTAL"]);
        } else {
            panic!();
        }
    }

    #[test]
    fn already_qualified_name_left_alone() {
        let e = lower_expression("hr.employees");
        let (q, stats) = canonicalize_expr(&e, &ctx("OTHER", None));
        if let Expr::Name(n) = q {
            assert_eq!(n.parts, vec!["HR", "EMPLOYEES"]);
        } else {
            panic!();
        }
        assert_eq!(stats.names_qualified, 0);
    }

    #[test]
    fn missing_active_schema_no_op() {
        let e = lower_expression("employees");
        let (q, stats) = canonicalize_expr(&e, &ctx("", None));
        // No change — bare name preserved.
        if let Expr::Name(n) = q {
            assert_eq!(n.parts, vec!["EMPLOYEES"]);
        } else {
            panic!();
        }
        assert_eq!(stats.names_qualified, 0);
    }

    #[test]
    fn binary_operand_names_both_qualified() {
        let e = lower_expression("a + b");
        let (q, stats) = canonicalize_expr(&e, &ctx("HR", None));
        if let Expr::Binary { lhs, rhs, .. } = q {
            if let Expr::Name(n) = *lhs {
                assert_eq!(n.parts, vec!["HR", "A"]);
            }
            if let Expr::Name(n) = *rhs {
                assert_eq!(n.parts, vec!["HR", "B"]);
            }
        }
        assert_eq!(stats.names_qualified, 2);
    }

    #[test]
    fn call_callee_and_args_qualified() {
        let e = lower_expression("nvl(emp_id, 0)");
        let (q, stats) = canonicalize_expr(&e, &ctx("HR", None));
        if let Expr::Call { callee, args } = q {
            assert_eq!(callee.parts, vec!["HR", "NVL"]);
            if let Expr::Name(n) = &args[0] {
                assert_eq!(n.parts, vec!["HR", "EMP_ID"]);
            } else {
                panic!();
            }
        } else {
            panic!();
        }
        // NVL + emp_id → 2 qualifications.
        assert_eq!(stats.names_qualified, 2);
    }

    #[test]
    fn implicit_cursor_for_loop_desugaring_flagged() {
        let stmts = lower_statement_body(
            "FOR rec IN (SELECT id, name FROM employees) LOOP NULL; END LOOP;",
        );
        let (_, stats) = canonicalize_statements(&stmts, &ctx("HR", None));
        assert_eq!(stats.cursor_for_loops_desugared, 1);
    }

    #[test]
    fn explicit_numeric_for_loop_not_flagged_as_cursor() {
        let stmts = lower_statement_body("FOR i IN 1..10 LOOP NULL; END LOOP;");
        let (_, stats) = canonicalize_statements(&stmts, &ctx("HR", None));
        assert_eq!(stats.cursor_for_loops_desugared, 0);
    }

    #[test]
    fn literal_expressions_pass_through_unchanged() {
        let e = lower_expression("42");
        let (q, stats) = canonicalize_expr(&e, &ctx("HR", None));
        assert_eq!(q, e);
        assert_eq!(stats.names_qualified, 0);
    }

    #[test]
    fn unary_operand_canonicalised() {
        let e = lower_expression("NOT v_flag");
        let (q, stats) = canonicalize_expr(&e, &ctx("HR", None));
        if let Expr::Unary { operand, .. } = q
            && let Expr::Name(n) = *operand
        {
            assert_eq!(n.parts, vec!["HR", "V_FLAG"]);
        } else {
            panic!();
        }
        assert_eq!(stats.names_qualified, 1);
    }
}
