//! Intra-procedural assignment + expression flow.
//!
//! Walks a lowered statement body and propagates [`ValueFlow`]
//! facts (FLOW-001) through assignments and expressions inside a
//! single routine. The pass is deliberately a *may*-analysis
//! over a flat statement list: it does not model branch joins
//! precisely (that needs a CFG, scheduled for a later pass) —
//! it conservatively merges every assignment's RHS flow into the
//! LHS via `ValueSet::join`, which is sound for taint /
//! string-shape over-approximation.
//!
//! Outputs a `FlowEnv` mapping each assigned name to its
//! accumulated `ValueFlow`. SAST consumes this to answer "does
//! tainted input reach a dynamic-SQL sink without a cleanser?".
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   assignment + parameter-mode chapters define how a value
//!   enters / moves through a routine.
//! * `LOW-LEVEL-CATALOGS.md` Supplied Package Buckets —
//!   `DBMS_ASSERT` is the cleanser that resets a name's taint.

use std::collections::BTreeMap;

use crate::expr::Expr;
use crate::flow::{StringShape, TaintCleanser, TaintKind, ValueFlow};
use crate::stmt::Statement;

/// Per-routine flow environment: name (upper-cased) → flow.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowEnv {
    map: BTreeMap<String, ValueFlow>,
}

impl FlowEnv {
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ValueFlow> {
        self.map.get(&name.to_ascii_uppercase())
    }

    /// Iterate every name (upper-cased) the environment tracks.
    /// Used by the FLOW-005 query facade to enumerate tainted
    /// names without exposing the inner map.
    pub fn iter_names(&self) -> impl Iterator<Item = String> + '_ {
        self.map.keys().cloned()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn merge_into(&mut self, name: &str, flow: ValueFlow) {
        let key = name.to_ascii_uppercase();
        let entry = self.map.entry(key).or_default();
        // Taint kinds accumulate (union); cleansers accumulate.
        for k in flow.taint.kinds {
            if !entry.taint.kinds.contains(&k) {
                entry.taint.kinds.push(k);
            }
        }
        for c in flow.taint.cleansed_by {
            if !entry.taint.cleansed_by.contains(&c) {
                entry.taint.cleansed_by.push(c);
            }
        }
        // Value set joins (lattice over-approx).
        let prev = std::mem::take(&mut entry.value_set);
        entry.value_set = prev.join(flow.value_set);
        // Constant: if both sides agree keep it, else drop to None.
        if entry.constant != flow.constant {
            entry.constant = None;
        }
        // String shape: keep the more-specific one only if equal.
        if entry.string_shape != flow.string_shape {
            entry.string_shape = flow.string_shape.or(entry.string_shape.take());
        }
    }
}

/// Names referenced inside an expression that look like
/// parameters/binds the caller flagged as tainted. The caller
/// passes the set of tainted source names (e.g. public IN
/// parameters); any reference to one taints the expression's
/// flow with `UserInput`.
#[derive(Clone, Debug, Default)]
pub struct TaintSources {
    pub user_input_names: Vec<String>,
    pub bind_names: Vec<String>,
}

/// Run intra-procedural flow over `stmts`. `sources` declares
/// which bare names are tainted on entry (public params, binds).
#[must_use]
pub fn analyze_flow(stmts: &[Statement], sources: &TaintSources) -> FlowEnv {
    let mut env = FlowEnv::default();
    walk(stmts, sources, &mut env);
    env
}

fn walk(stmts: &[Statement], sources: &TaintSources, env: &mut FlowEnv) {
    for s in stmts {
        match s {
            Statement::Assignment { target, rhs_text } => {
                let rhs_expr = crate::expr::lower_expression(rhs_text);
                let flow = expr_flow(&rhs_expr, sources);
                env.merge_into(target, flow);
            }
            Statement::If {
                arms,
                else_body_text,
            } => {
                for arm in arms {
                    walk(&crate::lower_statement_body(&arm.body_text), sources, env);
                }
                if let Some(eb) = else_body_text {
                    walk(&crate::lower_statement_body(eb), sources, env);
                }
            }
            Statement::ForLoop { body_text, .. }
            | Statement::WhileLoop { body_text, .. }
            | Statement::BareLoop { body_text } => {
                walk(&crate::lower_statement_body(body_text), sources, env);
            }
            _ => {}
        }
    }
}

/// Compute the `ValueFlow` of an expression. Taint flows from any
/// referenced source name; a `DBMS_ASSERT.*` call cleanses.
fn expr_flow(expr: &Expr, sources: &TaintSources) -> ValueFlow {
    let mut flow = ValueFlow::default();
    collect_expr_flow(expr, sources, &mut flow);
    flow
}

fn collect_expr_flow(expr: &Expr, sources: &TaintSources, flow: &mut ValueFlow) {
    match expr {
        Expr::Name(n) => {
            let head = n.parts.first().map(String::as_str).unwrap_or_default();
            if sources
                .user_input_names
                .iter()
                .any(|s| s.eq_ignore_ascii_case(head))
                && !flow.taint.kinds.contains(&TaintKind::UserInput)
            {
                flow.taint.kinds.push(TaintKind::UserInput);
            }
            if sources
                .bind_names
                .iter()
                .any(|s| s.eq_ignore_ascii_case(head))
                && !flow.taint.kinds.contains(&TaintKind::BindVariable)
            {
                flow.taint.kinds.push(TaintKind::BindVariable);
            }
        }
        Expr::BindRef(_) if !flow.taint.kinds.contains(&TaintKind::BindVariable) => {
            flow.taint.kinds.push(TaintKind::BindVariable);
        }
        Expr::StringLit(s) if flow.string_shape.is_none() => {
            flow.string_shape = Some(StringShape::Literal { value: s.clone() });
        }
        Expr::Call { callee, args } => {
            let path = callee.parts.join(".").to_ascii_uppercase();
            if path.starts_with("DBMS_ASSERT.")
                && !flow.taint.cleansed_by.contains(&TaintCleanser::DbmsAssert)
            {
                flow.taint.cleansed_by.push(TaintCleanser::DbmsAssert);
            }
            for a in args {
                collect_expr_flow(a, sources, flow);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr_flow(lhs, sources, flow);
            collect_expr_flow(rhs, sources, flow);
        }
        Expr::Unary { operand, .. } => collect_expr_flow(operand, sources, flow),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower_statement_body;

    fn src(user: &[&str]) -> TaintSources {
        TaintSources {
            user_input_names: user.iter().map(|s| s.to_string()).collect(),
            bind_names: vec![],
        }
    }

    #[test]
    fn assignment_from_constant_has_no_taint() {
        let s = lower_statement_body("v_x := 42;");
        let env = analyze_flow(&s, &src(&[]));
        assert!(!env.get("v_x").unwrap().taint.flags_alarm());
    }

    #[test]
    fn assignment_from_user_input_is_tainted() {
        let s = lower_statement_body("v_sql := p_user_table;");
        let env = analyze_flow(&s, &src(&["p_user_table"]));
        let f = env.get("v_sql").unwrap();
        assert!(f.taint.kinds.contains(&TaintKind::UserInput));
        assert!(f.taint.flags_alarm());
    }

    #[test]
    fn dbms_assert_call_cleanses_taint() {
        let s = lower_statement_body("v_safe := DBMS_ASSERT.SIMPLE_SQL_NAME(p_user_table);");
        let env = analyze_flow(&s, &src(&["p_user_table"]));
        let f = env.get("v_safe").unwrap();
        assert!(f.taint.kinds.contains(&TaintKind::UserInput));
        assert!(f.taint.cleansed_by.contains(&TaintCleanser::DbmsAssert));
        // Cleanser present → no alarm.
        assert!(!f.taint.flags_alarm());
    }

    #[test]
    fn taint_flows_through_concatenation() {
        let s = lower_statement_body("v_sql := 'SELECT * FROM ' || p_tab;");
        let env = analyze_flow(&s, &src(&["p_tab"]));
        assert!(
            env.get("v_sql")
                .unwrap()
                .taint
                .kinds
                .contains(&TaintKind::UserInput)
        );
    }

    #[test]
    fn bind_ref_is_bind_taint() {
        let s = lower_statement_body("v_x := :1;");
        let env = analyze_flow(&s, &src(&[]));
        assert!(
            env.get("v_x")
                .unwrap()
                .taint
                .kinds
                .contains(&TaintKind::BindVariable)
        );
    }

    #[test]
    fn string_literal_assignment_records_shape() {
        let s = lower_statement_body("v_msg := 'hello';");
        let env = analyze_flow(&s, &src(&[]));
        match &env.get("v_msg").unwrap().string_shape {
            Some(StringShape::Literal { value }) => assert_eq!(value, "hello"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn if_branch_assignments_both_recorded() {
        let s = lower_statement_body("IF flag THEN v_x := p_a; ELSE v_x := 0; END IF;");
        let env = analyze_flow(&s, &src(&["p_a"]));
        // May-analysis: v_x carries the union of both branches'
        // flow, so the tainted branch taints it.
        assert!(
            env.get("v_x")
                .unwrap()
                .taint
                .kinds
                .contains(&TaintKind::UserInput)
        );
    }

    #[test]
    fn loop_body_assignment_recorded() {
        let s = lower_statement_body("FOR i IN 1..10 LOOP v_acc := v_acc + p_in; END LOOP;");
        let env = analyze_flow(&s, &src(&["p_in"]));
        assert!(
            env.get("v_acc")
                .unwrap()
                .taint
                .kinds
                .contains(&TaintKind::UserInput)
        );
    }

    #[test]
    fn untainted_name_not_flagged() {
        let s = lower_statement_body("v_x := v_y + 1;");
        let env = analyze_flow(&s, &src(&["p_user"]));
        assert!(!env.get("v_x").unwrap().taint.flags_alarm());
    }

    #[test]
    fn case_insensitive_source_match() {
        let s = lower_statement_body("v_x := P_USER;");
        let env = analyze_flow(&s, &src(&["p_user"]));
        assert!(
            env.get("V_X")
                .unwrap()
                .taint
                .kinds
                .contains(&TaintKind::UserInput)
        );
    }

    #[test]
    fn empty_body_yields_empty_env() {
        let env = analyze_flow(&[], &src(&[]));
        assert!(env.is_empty());
    }
}
