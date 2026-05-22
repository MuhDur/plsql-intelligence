//! Taint-path + string-shape query API (PLSQL-FLOW-005).
//!
//! The SAST layer (Layer 3) and the dynamic-SQL consumers need
//! to ask "is this name tainted, by what, and was it cleansed?"
//! and "what's the string shape of this name?" — but Layer 2
//! (this crate) must not depend on Layer 3. So the query surface
//! lives here, on top of the FLOW-002 [`FlowEnv`] +
//! FLOW-003 [`InterFlowResult`], and Layer 3 consumes it.
//!
//! The API is read-only and allocation-light: every query takes
//! a name + the analysis outputs and returns a small typed
//! answer the SAST rule pack can pattern-match on.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   taint sources (bind variables, IN parameters) and the
//!   `DBMS_ASSERT` cleanser come straight from the language +
//!   supplied-package references; this module only re-projects
//!   the flow facts those passes already computed.

use serde::{Deserialize, Serialize};

use crate::flow::{StringShape, TaintCleanser, TaintKind};
use crate::flow_inter::InterFlowResult;
use crate::flow_intra::FlowEnv;

/// Answer to "is this name tainted?".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaintAnswer {
    /// True when at least one taint kind has no matching cleanser.
    pub is_tainted: bool,
    pub kinds: Vec<TaintKind>,
    pub cleansed_by: Vec<TaintCleanser>,
}

/// Read-only query facade over the flow analysis outputs.
#[derive(Clone, Copy, Debug)]
pub struct FlowQuery<'a> {
    env: &'a FlowEnv,
    inter: Option<&'a InterFlowResult>,
}

impl<'a> FlowQuery<'a> {
    #[must_use]
    pub fn new(env: &'a FlowEnv) -> Self {
        Self { env, inter: None }
    }

    /// Attach inter-procedural results so call-site result taint
    /// is folded into `taint_of`.
    #[must_use]
    pub fn with_inter(mut self, inter: &'a InterFlowResult) -> Self {
        self.inter = Some(inter);
        self
    }

    /// Taint verdict for `name`. Folds in any inter-procedural
    /// propagated-return taint whose `caller` matches `name`
    /// (the call-site assignment target).
    #[must_use]
    pub fn taint_of(&self, name: &str) -> TaintAnswer {
        let mut kinds: Vec<TaintKind> = Vec::new();
        let mut cleansed: Vec<TaintCleanser> = Vec::new();
        if let Some(f) = self.env.get(name) {
            kinds.extend(f.taint.kinds.iter().copied());
            cleansed.extend(f.taint.cleansed_by.iter().copied());
        }
        if let Some(inter) = self.inter {
            for pr in &inter.propagated_returns {
                if pr.caller.eq_ignore_ascii_case(name) {
                    for k in &pr.result_taint {
                        if !kinds.contains(k) {
                            kinds.push(*k);
                        }
                    }
                }
            }
        }
        let is_tainted = !kinds.is_empty() && cleansed.is_empty();
        TaintAnswer {
            is_tainted,
            kinds,
            cleansed_by: cleansed,
        }
    }

    /// True iff `name` carries `kind` (regardless of cleansing).
    #[must_use]
    pub fn has_taint_kind(&self, name: &str, kind: TaintKind) -> bool {
        self.taint_of(name).kinds.contains(&kind)
    }

    /// String shape of `name`, if the flow pass computed one.
    #[must_use]
    pub fn string_shape_of(&self, name: &str) -> Option<StringShape> {
        self.env.get(name).and_then(|f| f.string_shape.clone())
    }

    /// True iff `name` reaches a dynamic-SQL sink while tainted
    /// AND no cleanser fired — the canonical SAST injection
    /// predicate. `is_dynamic_sink` is supplied by the Layer 3
    /// caller (which knows the sink set) so Layer 2 stays
    /// independent of the SAST rule pack.
    #[must_use]
    pub fn taint_reaches_sink(&self, name: &str, is_dynamic_sink: bool) -> bool {
        is_dynamic_sink && self.taint_of(name).is_tainted
    }

    /// Every name in the environment that is currently tainted
    /// (uncleansed). Sorted for deterministic reports.
    #[must_use]
    pub fn tainted_names(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .env
            .iter_names()
            .filter(|n| self.taint_of(n).is_tainted)
            .collect();
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_intra::{TaintSources, analyze_flow};
    use crate::lower_statement_body;

    fn env(src: &str, user: &[&str]) -> FlowEnv {
        let stmts = lower_statement_body(src);
        analyze_flow(
            &stmts,
            &TaintSources {
                user_input_names: user.iter().map(|s| s.to_string()).collect(),
                bind_names: vec![],
            },
        )
    }

    #[test]
    fn taint_of_reports_tainted_user_input() {
        let e = env("v_sql := p_user;", &["p_user"]);
        let q = FlowQuery::new(&e);
        let a = q.taint_of("v_sql");
        assert!(a.is_tainted);
        assert!(a.kinds.contains(&TaintKind::UserInput));
    }

    #[test]
    fn taint_of_clean_name_is_not_tainted() {
        let e = env("v_x := 42;", &[]);
        let q = FlowQuery::new(&e);
        assert!(!q.taint_of("v_x").is_tainted);
    }

    #[test]
    fn cleansed_name_not_flagged() {
        let e = env("v_s := DBMS_ASSERT.SIMPLE_SQL_NAME(p_user);", &["p_user"]);
        let q = FlowQuery::new(&e);
        let a = q.taint_of("v_s");
        assert!(!a.is_tainted);
        assert!(a.cleansed_by.contains(&TaintCleanser::DbmsAssert));
    }

    #[test]
    fn has_taint_kind_ignores_cleansing() {
        let e = env("v_s := DBMS_ASSERT.SIMPLE_SQL_NAME(p_user);", &["p_user"]);
        let q = FlowQuery::new(&e);
        // Kind still present even though cleansed.
        assert!(q.has_taint_kind("v_s", TaintKind::UserInput));
    }

    #[test]
    fn string_shape_query_returns_literal() {
        let e = env("v_msg := 'hello';", &[]);
        let q = FlowQuery::new(&e);
        match q.string_shape_of("v_msg") {
            Some(StringShape::Literal { value }) => assert_eq!(value, "hello"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn taint_reaches_sink_predicate() {
        let e = env("v_sql := p_user;", &["p_user"]);
        let q = FlowQuery::new(&e);
        assert!(q.taint_reaches_sink("v_sql", true));
        assert!(!q.taint_reaches_sink("v_sql", false));
    }

    #[test]
    fn tainted_names_sorted_and_filtered() {
        let e = env("z := p_a; a := p_a; clean := 1;", &["p_a"]);
        let q = FlowQuery::new(&e);
        let names = q.tainted_names();
        assert!(names.contains(&"A".to_string()));
        assert!(names.contains(&"Z".to_string()));
        assert!(!names.contains(&"CLEAN".to_string()));
        // Sorted.
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn inter_procedural_return_taint_folded_in() {
        use crate::flow_inter::{InterFlowResult, PropagatedReturn};
        let e = env("v_x := 0;", &[]);
        let inter = InterFlowResult {
            propagated_returns: vec![PropagatedReturn {
                caller: "v_x".into(),
                callee: "tainted_fn".into(),
                result_taint: vec![TaintKind::DbLink],
            }],
            unknowns: vec![],
        };
        let q = FlowQuery::new(&e).with_inter(&inter);
        let a = q.taint_of("v_x");
        assert!(a.kinds.contains(&TaintKind::DbLink));
        assert!(a.is_tainted);
    }

    #[test]
    fn answer_serde_round_trip() {
        let e = env("v_sql := p_user;", &["p_user"]);
        let q = FlowQuery::new(&e);
        let a = q.taint_of("v_sql");
        let json = serde_json::to_string(&a).unwrap();
        let back: TaintAnswer = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }
}
