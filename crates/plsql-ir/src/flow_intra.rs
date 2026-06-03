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
//! Taint is *use-def transitive*: an RHS that references a local
//! already tainted earlier in the body inherits that taint, so
//! laundering through intermediates (`v_tmp := p_user;
//! v_sql := v_tmp;`) cannot escape the analysis. The walk is
//! iterated to a fixpoint over the finite taint lattice so a name
//! tainted only on a later pass (e.g. across a loop back-edge) is
//! still captured.
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
        // Taint kinds accumulate (union) across the branch arms a may-analysis
        // folds into one env. `cleansed_by` also accumulates, but ONLY for
        // reporting: the alarm reads `kinds` (live, uncleansed taint), so a
        // cleanser recorded on one arm cannot mask a live kind contributed by a
        // sibling arm. (Under the former "tainted-but-cleansed" model this union
        // was a fail-open at branch joins — oracle-qm3q.26; the live-kinds model
        // from oracle-qm3q.1 makes the join sound without needing CFG-precise
        // path-intersection of cleansers.)
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
///
/// Taint propagates transitively through assignments: an RHS that
/// references a previously-tainted *local* (`v_sql := v_tmp` after
/// `v_tmp := p_user`) inherits that local's live taint, so
/// multi-hop laundering through intermediate variables cannot
/// escape the analysis. Because branches and loops can re-read a
/// name that is only tainted on a later pass, `walk` is iterated to
/// a fixpoint over the (finite) taint lattice before the env is
/// returned.
///
/// Back-compat wrapper over [`analyze_flow_bounded`]: the per-pass
/// re-lowering recursion is depth-guarded so a non-shrinking
/// malformed body (e.g. the bare token `FOR UPDATE` that a
/// `SELECT … FOR UPDATE;` fragment leaves behind, which classifies
/// as a `BareLoop` whose `body_text` re-lowers to the *identical*
/// `BareLoop`) terminates instead of overflowing the stack /
/// aborting the process (R13). Callers that need to surface the
/// typed degradation (`outcome.limit_hit`) should call
/// [`analyze_flow_bounded`] directly.
#[must_use]
pub fn analyze_flow(stmts: &[Statement], sources: &TaintSources) -> FlowEnv {
    analyze_flow_bounded(stmts, sources).0
}

/// Depth-bounded variant of [`analyze_flow`]. Returns the flow
/// environment plus a [`RecursionOutcome`] recording whether (and
/// how often) a nested re-lowered body was abandoned at the
/// recursion-depth cap rather than walked unbounded. The caller is
/// responsible for emitting an honest typed diagnostic when
/// `outcome.limit_hit` (R13 — never silently truncate, never
/// stack-overflow on a non-shrinking malformed slice).
#[must_use]
pub fn analyze_flow_bounded(
    stmts: &[Statement],
    sources: &TaintSources,
) -> (FlowEnv, crate::RecursionOutcome) {
    let mut env = FlowEnv::default();
    let mut outcome = crate::RecursionOutcome::default();
    // Iterate to a fixpoint: `merge_into` is monotone (it only ever
    // unions kinds/cleansers and joins value-sets upward), so the
    // finite lattice guarantees the env stops growing. The cap is a
    // belt-and-suspenders bound (never expected to bind) so a
    // pathological body can never spin forever.
    const MAX_PASSES: usize = 64;
    for _ in 0..MAX_PASSES {
        let before = env.clone();
        // Re-accumulate the truncation outcome each pass over a
        // *fresh* outcome so the count reflects one pass, not the
        // sum across passes; the env still folds monotonically.
        let mut pass_outcome = crate::RecursionOutcome::default();
        walk(stmts, sources, &mut env, 0, &mut pass_outcome);
        outcome.absorb(pass_outcome);
        if env == before {
            break;
        }
    }
    (env, outcome)
}

fn walk(
    stmts: &[Statement],
    sources: &TaintSources,
    env: &mut FlowEnv,
    depth: usize,
    outcome: &mut crate::RecursionOutcome,
) {
    // Recurse into a re-lowered control-flow body only while we
    // have depth budget left. At the cap we record the truncation
    // and stop descending — never silently drop, never recurse
    // unbounded (which stack-overflows on a non-shrinking malformed
    // slice such as the bare `FOR UPDATE` token). Mirrors
    // `calls.rs::walk_call_sites` / `dml_edges.rs::walk_table_accesses`.
    macro_rules! recurse_body {
        ($text:expr) => {{
            if depth + 1 >= crate::MAX_RELOWER_DEPTH {
                outcome.note_truncated();
            } else {
                let lowered = crate::lower_statement_body($text);
                walk(&lowered, sources, env, depth + 1, outcome);
            }
        }};
    }
    for s in stmts {
        match s {
            Statement::Assignment { target, rhs_text } => {
                let rhs_expr = crate::expr::lower_expression(rhs_text);
                // Read the live env (use-def aware) so taint already
                // accumulated on a referenced local flows into the RHS.
                let flow = expr_flow(&rhs_expr, sources, env);
                env.merge_into(target, flow);
            }
            Statement::If {
                arms,
                else_body_text,
            } => {
                for arm in arms {
                    recurse_body!(&arm.body_text);
                }
                if let Some(eb) = else_body_text {
                    recurse_body!(eb);
                }
            }
            Statement::ForLoop { body_text, .. }
            | Statement::WhileLoop { body_text, .. }
            | Statement::BareLoop { body_text } => {
                recurse_body!(body_text);
            }
            Statement::NestedBlock { body_text } => {
                // Anonymous `BEGIN … END` / `DECLARE … END` sub-block: a
                // value laundered through it (`BEGIN v_sql := p_user; END;`)
                // must still taint `v_sql`, or the FLOW-001 pass fails open
                // for that name and SEC001 misses the injection. Strip the
                // wrapper and re-lower the inner statements, mirroring
                // `calls.rs::walk_call_sites` / `dml_edges.rs`. Only recurse
                // when the stripped slice differs from the original so the
                // depth-guarded `recurse_body!` cannot spin on a non-stripping
                // slice (the cap already bounds a non-shrinking one). A block
                // with no strippable wrapper carries no recoverable
                // assignment, so it is left untouched.
                let inner = crate::calls::strip_block_wrapper(body_text);
                if inner != body_text.as_str() {
                    recurse_body!(inner);
                }
            }
            _ => {}
        }
    }
}

/// Compute the `ValueFlow` of an expression. Taint flows from any
/// referenced source name OR any previously-tainted local recorded
/// in `env` (use-def transitivity); a `DBMS_ASSERT.*` call cleanses.
fn expr_flow(expr: &Expr, sources: &TaintSources, env: &FlowEnv) -> ValueFlow {
    let mut flow = ValueFlow::default();
    collect_expr_flow(expr, sources, env, &mut flow);
    flow
}

/// Is `path` (an already-upper-cased dotted call path) a *validating*
/// `DBMS_ASSERT` entry point — i.e. one that actually rejects unsafe input
/// and so cleanses the taint of its argument?
///
/// Two prior gaps, both fixed here (oracle-rwjl.4):
///
/// 1. **`DBMS_ASSERT.NOOP` is NOT a sanitizer.** Oracle documents NOOP as an
///    identity pass-through that performs no validation and returns its
///    argument unchanged. The old `path.starts_with("DBMS_ASSERT.")` guard
///    matched it uniformly, so `EXECUTE IMMEDIATE DBMS_ASSERT.NOOP(p_user)`
///    was reported clean — a SQL-injection fail-open in the flagship SEC001
///    rule. NOOP (and any unrecognized DBMS_ASSERT entry point) must fall
///    through to the transparent branch so its argument's taint reaches the
///    sink and still alarms.
/// 2. **A schema prefix made a real sanitizer transparent.** `starts_with`
///    failed to match `SYS.DBMS_ASSERT.SIMPLE_SQL_NAME(...)`, so a genuinely
///    cleansed value over-reported. We now tolerate an optional leading
///    schema segment.
///
/// The allowlist mirrors the validating set enumerated in
/// `plsql-symbols/src/dynamic_sql.rs` (which lists NOOP separately, only for
/// textual detection — never as a validator).
fn is_dbms_assert_sanitizer(path: &str) -> bool {
    const VALIDATORS: &[&str] = &[
        "SIMPLE_SQL_NAME",
        "QUALIFIED_SQL_NAME",
        "SCHEMA_NAME",
        "ENQUOTE_NAME",
        "SQL_OBJECT_NAME",
        "ENQUOTE_LITERAL",
    ];
    let segs: Vec<&str> = path.split('.').collect();
    // Match `[schema.]DBMS_ASSERT.<fn>`: the trailing two segments must be
    // `DBMS_ASSERT` then a validating function. NOOP (or any unknown entry
    // point) deliberately fails this test and falls through to transparent.
    match segs.as_slice() {
        [.., "DBMS_ASSERT", func] => VALIDATORS.contains(func),
        _ => false,
    }
}

fn collect_expr_flow(expr: &Expr, sources: &TaintSources, env: &FlowEnv, flow: &mut ValueFlow) {
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
            // Use-def transitivity: a reference to a previously-assigned
            // local inherits that local's accumulated flow, so taint
            // laundered through an intermediate variable
            // (`v_tmp := p_user; v_sql := v_tmp;`) still reaches the sink.
            // Only LIVE kinds carry the alarm; `cleansed_by` is unioned for
            // reporting (a recorded cleanser never masks a live kind — see
            // `flags_alarm`). String shape is preserved only when the parent
            // has none yet.
            if let Some(prev) = env.get(head) {
                for k in &prev.taint.kinds {
                    if !flow.taint.kinds.contains(k) {
                        flow.taint.kinds.push(*k);
                    }
                }
                for c in &prev.taint.cleansed_by {
                    if !flow.taint.cleansed_by.contains(c) {
                        flow.taint.cleansed_by.push(*c);
                    }
                }
                if flow.string_shape.is_none() {
                    flow.string_shape = prev.string_shape.clone();
                }
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
            if is_dbms_assert_sanitizer(&path) {
                // A `DBMS_ASSERT.*` call SANITIZES its argument: the value it
                // returns is safe to interpolate. The cleansing therefore binds to
                // the call's *argument subtree*, NOT to the enclosing expression.
                // Compute the args in an ISOLATED sub-flow and drop their taint
                // (kinds + cleansers) — it is consumed by the sanitizer — so the
                // call contributes nothing injectable to the parent. Only taint
                // that flows AROUND the call (e.g. a concatenated sibling) reaches
                // the parent and can still alarm.
                //
                // The old code pushed `DbmsAssert` onto the *shared* parent flow
                // and recursed the args into it, so a cleanse on one operand zeroed
                // the alarm for an unrelated sibling — e.g.
                // `DBMS_ASSERT.ENQUOTE_LITERAL('x') || p_user` came out
                // {UserInput, cleansed:DbmsAssert} → flags_alarm=false (fail-open).
                let mut sanitized = ValueFlow::default();
                for a in args {
                    collect_expr_flow(a, sources, env, &mut sanitized);
                }
                // The sanitizer CONSUMES its argument's live taint: record the
                // cleanser (for reporting) and DROP the kinds — they are no longer
                // injectable. `kinds` holds only *live* (uncleansed) taint, so the
                // dropped kinds simply never enter the enclosing `flow`. Only taint
                // that flows AROUND the call (a concatenated sibling) reaches it.
                if !sanitized.taint.kinds.is_empty()
                    && !flow.taint.cleansed_by.contains(&TaintCleanser::DbmsAssert)
                {
                    flow.taint.cleansed_by.push(TaintCleanser::DbmsAssert);
                }
                // Carry forward only non-taint shape info; the result is clean.
                if flow.string_shape.is_none() {
                    flow.string_shape = sanitized.string_shape;
                }
            } else {
                // A non-sanitizing call is transparent to taint: its arguments'
                // taint flows through to the enclosing expression.
                for a in args {
                    collect_expr_flow(a, sources, env, flow);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr_flow(lhs, sources, env, flow);
            collect_expr_flow(rhs, sources, env, flow);
        }
        Expr::Unary { operand, .. } => collect_expr_flow(operand, sources, env, flow),
        Expr::Raw { .. } => {
            // The recognizer could not lower this sub-expression (an
            // unrecognized shape like a SQL `CASE` expression, an
            // unbalanced/unterminated fragment, or a depth-limit-collapsed
            // concat tail). Any user-tainted operand inside it is invisible to
            // this collector, so treating the value as clean would be a silent
            // taint fail-open (R13). Fail CLOSED: mark the value Unanalyzable so
            // a downstream dynamic-SQL sink flags it, and force the string shape
            // opaque so it can never be mistaken for a provably-constant literal.
            if !flow.taint.kinds.contains(&TaintKind::Unanalyzable) {
                flow.taint.kinds.push(TaintKind::Unanalyzable);
            }
            if flow.string_shape.is_none() {
                flow.string_shape = Some(StringShape::FullyOpaque);
            }
        }
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
    fn unlowerable_case_expression_rhs_fails_closed_as_unanalyzable() {
        // oracle-qo1v.2: a SQL CASE expression on an assignment RHS is not a
        // recognized Expr shape, so it lowers to Expr::Raw and the user-tainted
        // operand (p_user) inside it is invisible to the taint collector. The
        // old catch-all dropped it silently (taint fail-open). The collector now
        // fails CLOSED: the value is marked Unanalyzable (raises the alarm so a
        // downstream EXECUTE IMMEDIATE is flagged) and forced to an opaque string
        // shape so it can never be read as a provably-constant literal.
        let s = lower_statement_body("v_sql := CASE WHEN cond THEN p_user ELSE 'x' END;");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env.get("v_sql").expect("v_sql flow recorded");
        assert!(
            f.taint.kinds.contains(&TaintKind::Unanalyzable),
            "un-lowerable CASE RHS must be marked Unanalyzable: {:?}",
            f.taint
        );
        assert!(f.taint.flags_alarm(), "fail closed: must raise the alarm");
        assert!(
            matches!(f.string_shape, Some(StringShape::FullyOpaque)),
            "un-lowerable value must not be mistaken for a constant literal: {:?}",
            f.string_shape
        );
    }

    #[test]
    fn dbms_assert_call_cleanses_its_argument() {
        // DBMS_ASSERT.* sanitizes its argument: the result is a clean value with no
        // alarm. The arg's taint is consumed by the sanitizer, so the result no
        // longer carries the UserInput kind (we dropped the old "tainted-but-
        // cleansed" representation, which let an unrelated cleanser mask a
        // concatenated sibling — see the fail-open regression below).
        let s = lower_statement_body("v_safe := DBMS_ASSERT.SIMPLE_SQL_NAME(p_user_table);");
        let env = analyze_flow(&s, &src(&["p_user_table"]));
        let f = env.get("v_safe").unwrap();
        assert!(!f.taint.flags_alarm(), "sanitized value must not alarm");
        assert!(
            !f.taint.kinds.contains(&TaintKind::UserInput),
            "the sanitizer consumes the argument's taint"
        );
    }

    #[test]
    fn dbms_assert_does_not_cleanse_a_concatenated_sibling() {
        // SEC001 fail-open regression: a DBMS_ASSERT cleanse on ONE operand must
        // NOT zero the injection alarm for tainted input concatenated ALONGSIDE it.
        // `DBMS_ASSERT.ENQUOTE_LITERAL('x') || p_user` interpolates raw p_user.
        let s =
            lower_statement_body("v_sql := DBMS_ASSERT.ENQUOTE_LITERAL('x') || p_user;");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env.get("v_sql").unwrap();
        assert!(
            f.taint.kinds.contains(&TaintKind::UserInput),
            "the uncleansed sibling p_user must remain tainted"
        );
        assert!(
            f.taint.cleansed_by.is_empty(),
            "the sibling assert's cleanser must not leak onto the whole expression"
        );
        assert!(
            f.taint.flags_alarm(),
            "raw user input concatenated with a sanitized literal must still alarm"
        );
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
    fn branch_merge_sibling_cleanse_does_not_mask_live_kind() {
        // Regression for oracle-qm3q.26 (cleanser-union fail-open across a
        // branch join). One arm sanitises `v` with DBMS_ASSERT; the OTHER arm
        // assigns raw `p_user`. `merge_into` unions the cleanser from the THEN
        // arm with the live UserInput kind from the ELSE arm — but because
        // `kinds` tracks only LIVE (uncleansed) taint and `flags_alarm` no
        // longer depends on `cleansed_by`, the uncleansed ELSE path still
        // alarms. (Under the old "tainted-but-cleansed" model the recorded
        // DbmsAssert cleanser would have masked the live ELSE-path kind — a
        // SEC001 fail-open.)
        let s = lower_statement_body(
            "IF c THEN v := DBMS_ASSERT.SIMPLE_SQL_NAME(p_user); ELSE v := p_user; END IF;",
        );
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env.get("v").unwrap();
        assert!(
            f.taint.kinds.contains(&TaintKind::UserInput),
            "the uncleansed ELSE-path UserInput kind must survive the branch join"
        );
        assert!(
            f.taint.cleansed_by.contains(&TaintCleanser::DbmsAssert),
            "the THEN-path cleanser is still recorded for reporting"
        );
        assert!(
            f.taint.flags_alarm(),
            "a sibling cleanse on one branch must NOT mask the live kind on the other"
        );
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

    #[test]
    fn two_hop_local_laundering_propagates_taint() {
        // Regression for oracle-qm3q.20 (transitive intra-procedural taint).
        // `v_tmp` launders `p_user`; `v_sql := v_tmp` must inherit the taint so
        // an EXECUTE IMMEDIATE built from v_sql is still flagged. Before the
        // use-def fix, expr_flow only consulted the static `sources` set and
        // never the live env, so v_sql came out clean (a SEC001 false negative).
        let s = lower_statement_body("v_tmp := p_user; v_sql := v_tmp;");
        let env = analyze_flow(&s, &src(&["p_user"]));
        assert!(
            env.get("v_tmp")
                .unwrap()
                .taint
                .kinds
                .contains(&TaintKind::UserInput),
            "the first hop is tainted from the source"
        );
        let sql = env.get("v_sql").unwrap();
        assert!(
            sql.taint.kinds.contains(&TaintKind::UserInput),
            "taint laundered through v_tmp must reach v_sql"
        );
        assert!(sql.taint.flags_alarm(), "the laundered value still alarms");
    }

    #[test]
    fn n_hop_local_laundering_propagates_taint() {
        // Deeper chain: p_user -> a -> b -> c. Each hop must carry the taint
        // forward through the live env.
        let s = lower_statement_body("v_a := p_user; v_b := v_a; v_c := v_b;");
        let env = analyze_flow(&s, &src(&["p_user"]));
        for name in ["v_a", "v_b", "v_c"] {
            assert!(
                env.get(name)
                    .unwrap()
                    .taint
                    .kinds
                    .contains(&TaintKind::UserInput),
                "{name} must be tainted along the laundering chain"
            );
        }
    }

    #[test]
    fn cleansed_local_then_reused_stays_clean() {
        // The dual of laundering: once a local is sanitised by DBMS_ASSERT,
        // reusing it must NOT resurrect a live UserInput kind. The transitive
        // env-consult inherits cleansed_by (for reporting) but no live kind,
        // because the sanitiser already drained the kinds it consumed.
        let s = lower_statement_body(
            "v_tmp := DBMS_ASSERT.SIMPLE_SQL_NAME(p_user); v_sql := v_tmp;",
        );
        let env = analyze_flow(&s, &src(&["p_user"]));
        let sql = env.get("v_sql").unwrap();
        assert!(
            !sql.taint.kinds.contains(&TaintKind::UserInput),
            "a reused sanitised local carries no live taint"
        );
        assert!(
            !sql.taint.flags_alarm(),
            "reusing a sanitised value must not alarm"
        );
        assert!(
            sql.taint.cleansed_by.contains(&TaintCleanser::DbmsAssert),
            "the cleanser is carried forward for reporting"
        );
    }

    #[test]
    fn taint_laundered_through_local_into_concatenation_alarms() {
        // Combine transitivity with the sibling-cleanse guard: stage raw user
        // input in a local, then concatenate it into a dynamic-SQL string.
        let s = lower_statement_body(
            "v_t := p_user; v_sql := 'SELECT * FROM ' || v_t;",
        );
        let env = analyze_flow(&s, &src(&["p_user"]));
        let sql = env.get("v_sql").unwrap();
        assert!(
            sql.taint.kinds.contains(&TaintKind::UserInput),
            "laundered taint concatenated into SQL must remain tainted"
        );
        assert!(sql.taint.flags_alarm());
    }

    // oracle-rwjl.3: a verb-prefixed local (`return_val`) used to be swallowed
    // by classify() (→ Statement::Return), dropping the assignment from
    // flow_intra::walk so taint laundered through it never reached the sink.
    // Now it is a real Assignment, so v_sql inherits p_user's taint.
    #[test]
    fn verb_prefixed_local_laundering_propagates_taint() {
        let s = lower_statement_body("return_val := p_user; v_sql := return_val;");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let rv = env
            .get("return_val")
            .expect("the verb-prefixed local must be recorded as an assignment");
        assert!(
            rv.taint.kinds.contains(&TaintKind::UserInput),
            "return_val must inherit p_user's taint"
        );
        let sql = env.get("v_sql").unwrap();
        assert!(
            sql.taint.kinds.contains(&TaintKind::UserInput),
            "taint laundered through the verb-prefixed local must reach v_sql"
        );
        assert!(sql.taint.flags_alarm());
    }

    // oracle-rwjl.4: DBMS_ASSERT.NOOP is Oracle's documented identity
    // pass-through — it performs NO validation, so it must NOT cleanse. Raw
    // user input wrapped in NOOP and concatenated into dynamic SQL must still
    // alarm (the old uniform `starts_with("DBMS_ASSERT.")` reported it clean —
    // a SEC001 fail-open).
    #[test]
    fn dbms_assert_noop_is_not_a_sanitizer() {
        let s = lower_statement_body("v_sql := 'SELECT * FROM ' || DBMS_ASSERT.NOOP(p_user);");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env.get("v_sql").unwrap();
        assert!(
            f.taint.kinds.contains(&TaintKind::UserInput),
            "NOOP performs no validation; its argument's taint must survive"
        );
        assert!(
            f.taint.flags_alarm(),
            "user input wrapped in DBMS_ASSERT.NOOP must still alarm"
        );
    }

    // oracle-rwjl.4 (direct, not just concatenated): a bare NOOP wrap is also
    // transparent.
    #[test]
    fn dbms_assert_noop_direct_assignment_stays_tainted() {
        let s = lower_statement_body("v_sql := DBMS_ASSERT.NOOP(p_user);");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env.get("v_sql").unwrap();
        assert!(
            f.taint.kinds.contains(&TaintKind::UserInput),
            "NOOP does not consume taint"
        );
        assert!(f.taint.flags_alarm());
    }

    // oracle-rwjl.4: a REAL validating sanitizer with a SYS schema prefix must
    // still be recognised as a cleanser (the old `starts_with` missed the
    // prefix and over-reported a genuinely safe value).
    #[test]
    fn sys_prefixed_dbms_assert_sanitizer_cleanses() {
        let s = lower_statement_body("v_safe := SYS.DBMS_ASSERT.SIMPLE_SQL_NAME(p_tab);");
        let env = analyze_flow(&s, &src(&["p_tab"]));
        let f = env.get("v_safe").unwrap();
        assert!(
            !f.taint.flags_alarm(),
            "a schema-prefixed real sanitizer must still cleanse"
        );
        assert!(
            !f.taint.kinds.contains(&TaintKind::UserInput),
            "the sanitizer consumes the argument's taint"
        );
    }

    // oracle-lokg.2: the exact crash shape from the bundled public
    // fixture. A `SELECT … FOR UPDATE;` body fragment leaves the bare
    // token `FOR UPDATE`; the text-scanner's `classify_loop` treats
    // `FOR …` as a FOR-loop, finds no word-bounded `IN` and no
    // `END LOOP`, and falls back to a `BareLoop` whose `body_text` is
    // *the same string* `FOR UPDATE`. Re-lowering it yields the
    // identical non-shrinking `BareLoop` → before the depth guard
    // `walk` recursed unbounded and aborted the whole `analyze_flow`
    // (SIGABRT / "stack overflow"; MAX_PASSES=64 bounds only the OUTER
    // fixpoint, not the per-pass recursion). It must now terminate and
    // report the truncation honestly (R13).
    #[test]
    fn non_shrinking_for_update_does_not_stack_overflow_and_reports_limit() {
        let stmts = vec![Statement::BareLoop {
            body_text: "FOR UPDATE".to_string(),
        }];
        let (env, outcome) = analyze_flow_bounded(&stmts, &src(&[]));
        assert!(
            outcome.limit_hit,
            "the non-shrinking `FOR UPDATE` BareLoop must trip the \
             bounded depth cap, outcome={outcome:?}"
        );
        assert!(outcome.truncated_bodies >= 1);
        // No assignment can be recovered from the malformed fragment.
        assert!(env.is_empty());
        // The back-compat wrapper must also simply terminate
        // (no panic / abort) rather than recurse unbounded.
        let _ = analyze_flow(&stmts, &src(&[]));
    }

    // oracle-lokg.2: the same shape arrived at via the lowering path
    // (not a hand-built `Statement`), proving the end-to-end public API
    // `analyze_flow(&lower_statement_body("FOR UPDATE"), …)` terminates.
    #[test]
    fn analyze_flow_over_lowered_for_update_terminates() {
        let stmts = lower_statement_body("FOR UPDATE");
        let env = analyze_flow(&stmts, &TaintSources::default());
        // We do not assert the env contents — only that the call
        // returned at all (before the guard this aborted the process).
        let _ = env.is_empty();
    }

    // oracle-lokg.2: a genuinely deep linear nesting chain must
    // terminate at the depth cap with a clean typed truncation outcome
    // instead of overflowing the stack. Each level is a `BareLoop`
    // wrapping the next, so the re-lowered slice shrinks one level per
    // recursion — but without the cap a sufficiently deep chain would
    // overflow the native stack. DEPTH is set well above
    // `MAX_RELOWER_DEPTH` (128) so the cap is guaranteed to fire while
    // keeping the per-level re-lowering scan cheap; the same guard
    // bounds the recursion to 128 frames no matter how deep the input.
    #[test]
    fn deep_nested_loop_chain_degrades_to_limit_not_overflow() {
        const DEPTH: usize = 1_000;
        // Compile-time invariant: DEPTH must exceed the cap so the
        // truncation is guaranteed to fire.
        const _: () = assert!(DEPTH > crate::MAX_RELOWER_DEPTH);
        // Build the chain with a single linear pass (no quadratic
        // string re-allocation): DEPTH `LOOP ` openers, the innermost
        // assignment, then DEPTH ` END LOOP;` closers.
        let mut body = String::with_capacity(DEPTH * 16 + 32);
        for _ in 0..DEPTH {
            body.push_str("LOOP ");
        }
        body.push_str("v_x := p_user; ");
        for _ in 0..DEPTH {
            body.push_str("END LOOP; ");
        }
        let stmts = lower_statement_body(&body);
        let (_, outcome) = analyze_flow_bounded(&stmts, &src(&["p_user"]));
        assert!(
            outcome.limit_hit,
            "a {DEPTH}-deep nested LOOP chain must trip the depth cap, \
             outcome={outcome:?}"
        );
    }

    // oracle-hrzg.2: taint laundered through an anonymous BEGIN…END
    // sub-block must still reach the assigned name. Before the
    // NestedBlock arm in `walk`, the `_ => {}` catch-all dropped the
    // sub-block entirely, so `v_sql` came back UNtainted (FLOW-001
    // fail-open → SEC001 misses the injection once wired).
    #[test]
    fn nested_begin_block_launders_taint_into_assignment() {
        let s = lower_statement_body("BEGIN v_sql := p_user; END;");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env
            .get("v_sql")
            .expect("the nested-block assignment to v_sql must be recorded");
        assert!(
            f.taint.kinds.contains(&TaintKind::UserInput),
            "taint laundered through a BEGIN…END sub-block must reach v_sql"
        );
        assert!(f.taint.flags_alarm(), "the laundered value still alarms");
    }

    // oracle-hrzg.2: the same, via a DECLARE…END wrapper (the other
    // anonymous-block shape the classifier emits as NestedBlock).
    #[test]
    fn nested_declare_block_launders_taint_into_assignment() {
        let s = lower_statement_body("DECLARE v_x NUMBER; BEGIN v_sql := p_user; END;");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env
            .get("v_sql")
            .expect("the DECLARE-wrapped assignment to v_sql must be recorded");
        assert!(
            f.taint.kinds.contains(&TaintKind::UserInput),
            "taint laundered through a DECLARE…END sub-block must reach v_sql"
        );
        assert!(f.taint.flags_alarm());
    }

    // oracle-hrzg.2: a deeply nested chain of anonymous blocks must
    // terminate at the MAX_RELOWER_DEPTH cap (honest typed truncation)
    // rather than overflowing the stack — same posture as the loop-chain
    // guard. Each level wraps the next in `BEGIN … END;` so the stripped
    // slice shrinks one level per recursion.
    #[test]
    fn deep_nested_block_chain_degrades_to_limit_not_overflow() {
        const DEPTH: usize = 1_000;
        const _: () = assert!(DEPTH > crate::MAX_RELOWER_DEPTH);
        let mut body = String::with_capacity(DEPTH * 12 + 32);
        for _ in 0..DEPTH {
            body.push_str("BEGIN ");
        }
        body.push_str("v_x := p_user; ");
        for _ in 0..DEPTH {
            body.push_str("END; ");
        }
        let stmts = lower_statement_body(&body);
        let (_, outcome) = analyze_flow_bounded(&stmts, &src(&["p_user"]));
        assert!(
            outcome.limit_hit,
            "a {DEPTH}-deep nested BEGIN chain must trip the depth cap, \
             outcome={outcome:?}"
        );
    }

    // oracle-hrzg.5: a parenthesised concatenation operand
    // `'SELECT … ' || (p_user)` must keep p_user's taint — the paren
    // group is unwrapped before the `||` split. Before the
    // `recognise_paren_group` recognizer, `(p_user)` lowered to
    // `Raw{UnrecognizedShape}`, contributing zero taint, and the byte-
    // identical un-parenthesised form alarmed while this one did not
    // (SEC001 fail-open on a no-obfuscation code shape).
    #[test]
    fn parenthesised_concat_operand_keeps_taint() {
        let s = lower_statement_body("v_sql := 'SELECT * FROM ' || (p_user);");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env.get("v_sql").unwrap();
        assert!(
            f.taint.kinds.contains(&TaintKind::UserInput),
            "a parenthesised tainted operand must remain tainted"
        );
        assert!(f.taint.flags_alarm());
    }

    // oracle-hrzg.5: a whole-RHS parenthesised group
    // `('SELECT …' || p_user)` is unwrapped first, then the inner `||`
    // splits normally so the taint survives.
    #[test]
    fn whole_rhs_paren_group_keeps_taint() {
        let s = lower_statement_body("v_sql := ('SELECT * FROM ' || p_user);");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env.get("v_sql").unwrap();
        assert!(
            f.taint.kinds.contains(&TaintKind::UserInput),
            "a whole-RHS parenthesised group must preserve inner taint"
        );
        assert!(f.taint.flags_alarm());
    }

    // oracle-hrzg.5: a bare `(p_user)` group is a Name, so it taints
    // identically to the un-parenthesised reference.
    #[test]
    fn bare_paren_group_is_tainted_name() {
        let s = lower_statement_body("v_sql := (p_user);");
        let env = analyze_flow(&s, &src(&["p_user"]));
        let f = env.get("v_sql").unwrap();
        assert!(f.taint.kinds.contains(&TaintKind::UserInput));
        assert!(f.taint.flags_alarm());
    }
}
