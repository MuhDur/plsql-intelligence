//! Single-hop inter-procedural parameter/return flow.
//!
//! FLOW-002 propagates taint within one routine. This pass joins
//! routines: when routine A calls routine B, the taint of A's
//! actual arguments flows into B's formal parameters, and B's
//! return taint flows back to A's call-site assignment.
//!
//! Each call edge is resolved **once** against the callee's
//! [`RoutineFlowSummary`] — a single hop. The pass does NOT follow
//! transitive chains (A→B→C) or iterate a recursive frontier;
//! multi-hop following is a future pass and is intentionally out of
//! scope here. A direct self-call (A→A) and any call whose callee
//! summary is missing (external package, db-link, dynamic dispatch)
//! are recorded as conservative [`FlowUnknownFact`]s so R13
//! reporting never silently drops the boundary.
//!
//! Routine summaries are supplied by the caller as
//! [`RoutineFlowSummary`] records (param taint sensitivity +
//! return taint) so this module stays free of a hard
//! `plsql-symbols` dependency.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference —
//!   parameter modes (IN copies in, OUT copies back, IN OUT
//!   both) define the flow direction across a call boundary.
//! * `LOW-LEVEL-CATALOGS.md` — `ALL_ARGUMENTS` is the
//!   server-side authority for a routine's formal-parameter
//!   list when the source summary is unavailable.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::flow::TaintKind;

/// Per-routine flow summary the caller supplies. `param_taints`
/// maps a 0-based parameter index to the taint kinds that param
/// propagates into the body; `returns_taint` is the taint a
/// caller should attribute to the call's result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineFlowSummary {
    pub logical_id: String,
    pub param_taints: BTreeMap<usize, Vec<TaintKind>>,
    pub returns_taint: Vec<TaintKind>,
}

/// A call site to resolve: `caller` invokes `callee` with the
/// taint kinds of each positional actual argument.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallEdgeFlow {
    pub caller: String,
    pub callee: String,
    /// Taint kinds of each positional actual argument.
    pub actual_arg_taints: Vec<Vec<TaintKind>>,
}

/// Conservative boundary record (R13). Emitted whenever the pass
/// cannot resolve a call: missing callee summary, or a direct
/// recursion (self-call).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowUnknownFact {
    pub at_caller: String,
    pub callee: String,
    pub reason: FlowUnknownReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowUnknownReason {
    /// No `RoutineFlowSummary` for the callee (external package,
    /// db-link, runtime dispatch).
    MissingCalleeSummary,
    /// Callee is the caller itself (direct recursion); the
    /// single-hop pass does not unfold the cycle.
    RecursionCycle,
}

/// Result of an inter-procedural propagation run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterFlowResult {
    /// Taint attributed to each caller's call-site result, keyed
    /// by `(caller, callee)`.
    pub propagated_returns: Vec<PropagatedReturn>,
    pub unknowns: Vec<FlowUnknownFact>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropagatedReturn {
    pub caller: String,
    pub callee: String,
    pub result_taint: Vec<TaintKind>,
}

/// Propagate taint across `call_edges` using the supplied
/// `summaries`. Each edge is resolved one hop against its callee
/// summary; a direct self-call and a missing summary surface as
/// `FlowUnknownFact` (R13).
#[must_use]
pub fn propagate_inter(
    call_edges: &[CallEdgeFlow],
    summaries: &[RoutineFlowSummary],
) -> InterFlowResult {
    let by_id: BTreeMap<&str, &RoutineFlowSummary> = summaries
        .iter()
        .map(|s| (s.logical_id.as_str(), s))
        .collect();
    let mut result = InterFlowResult::default();

    for edge in call_edges {
        resolve_edge(edge, &by_id, &mut result);
    }
    result
}

fn resolve_edge(
    edge: &CallEdgeFlow,
    by_id: &BTreeMap<&str, &RoutineFlowSummary>,
    result: &mut InterFlowResult,
) {
    // Single-hop: a callee that is the caller itself is a direct
    // recursion the pass does not unfold. (Transitive cycles A→B→A
    // are out of scope until multi-hop following lands.)
    if edge.callee == edge.caller {
        result.unknowns.push(FlowUnknownFact {
            at_caller: edge.caller.clone(),
            callee: edge.callee.clone(),
            reason: FlowUnknownReason::RecursionCycle,
        });
        return;
    }
    let Some(summary) = by_id.get(edge.callee.as_str()) else {
        result.unknowns.push(FlowUnknownFact {
            at_caller: edge.caller.clone(),
            callee: edge.callee.clone(),
            reason: FlowUnknownReason::MissingCalleeSummary,
        });
        return;
    };

    // The callee's return taint = its declared return taint, plus
    // any taint that an actual argument introduces into a
    // taint-sensitive parameter.
    let mut result_taint: Vec<TaintKind> = summary.returns_taint.clone();
    for (idx, actual) in edge.actual_arg_taints.iter().enumerate() {
        if let Some(param_kinds) = summary.param_taints.get(&idx)
            && !param_kinds.is_empty()
        {
            // Param is taint-propagating: the actual's taint flows
            // through to the result.
            for k in actual {
                if !result_taint.contains(k) {
                    result_taint.push(*k);
                }
            }
        }
    }
    result.propagated_returns.push(PropagatedReturn {
        caller: edge.caller.clone(),
        callee: edge.callee.clone(),
        result_taint,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summ(id: &str, params: &[(usize, &[TaintKind])], ret: &[TaintKind]) -> RoutineFlowSummary {
        let mut pt = BTreeMap::new();
        for (i, ks) in params {
            pt.insert(*i, ks.to_vec());
        }
        RoutineFlowSummary {
            logical_id: id.into(),
            param_taints: pt,
            returns_taint: ret.to_vec(),
        }
    }

    #[test]
    fn taint_flows_through_propagating_param_to_result() {
        let edges = vec![CallEdgeFlow {
            caller: "a".into(),
            callee: "b".into(),
            actual_arg_taints: vec![vec![TaintKind::UserInput]],
        }];
        let summaries = vec![summ("b", &[(0, &[TaintKind::UserInput])], &[])];
        let r = propagate_inter(&edges, &summaries);
        assert_eq!(r.propagated_returns.len(), 1);
        assert!(
            r.propagated_returns[0]
                .result_taint
                .contains(&TaintKind::UserInput)
        );
        assert!(r.unknowns.is_empty());
    }

    #[test]
    fn non_propagating_param_does_not_taint_result() {
        let edges = vec![CallEdgeFlow {
            caller: "a".into(),
            callee: "b".into(),
            actual_arg_taints: vec![vec![TaintKind::UserInput]],
        }];
        // b has no param_taints entry for index 0 → param is inert.
        let summaries = vec![summ("b", &[], &[])];
        let r = propagate_inter(&edges, &summaries);
        assert!(r.propagated_returns[0].result_taint.is_empty());
    }

    #[test]
    fn declared_return_taint_always_present() {
        let edges = vec![CallEdgeFlow {
            caller: "a".into(),
            callee: "b".into(),
            actual_arg_taints: vec![],
        }];
        let summaries = vec![summ("b", &[], &[TaintKind::DbLink])];
        let r = propagate_inter(&edges, &summaries);
        assert!(
            r.propagated_returns[0]
                .result_taint
                .contains(&TaintKind::DbLink)
        );
    }

    #[test]
    fn missing_summary_records_unknown() {
        let edges = vec![CallEdgeFlow {
            caller: "a".into(),
            callee: "external_pkg.proc".into(),
            actual_arg_taints: vec![],
        }];
        let r = propagate_inter(&edges, &[]);
        assert_eq!(r.unknowns.len(), 1);
        assert_eq!(
            r.unknowns[0].reason,
            FlowUnknownReason::MissingCalleeSummary
        );
    }

    #[test]
    fn direct_recursion_records_cycle_unknown() {
        let edges = vec![CallEdgeFlow {
            caller: "rec".into(),
            callee: "rec".into(),
            actual_arg_taints: vec![],
        }];
        let summaries = vec![summ("rec", &[], &[])];
        let r = propagate_inter(&edges, &summaries);
        assert_eq!(r.unknowns[0].reason, FlowUnknownReason::RecursionCycle);
    }

    #[test]
    fn multiple_taint_kinds_union_into_result() {
        let edges = vec![CallEdgeFlow {
            caller: "a".into(),
            callee: "b".into(),
            actual_arg_taints: vec![vec![TaintKind::UserInput, TaintKind::BindVariable]],
        }];
        let summaries = vec![summ("b", &[(0, &[TaintKind::UserInput])], &[])];
        let r = propagate_inter(&edges, &summaries);
        let t = &r.propagated_returns[0].result_taint;
        assert!(t.contains(&TaintKind::UserInput));
        assert!(t.contains(&TaintKind::BindVariable));
    }

    #[test]
    fn result_taint_dedupes() {
        let edges = vec![CallEdgeFlow {
            caller: "a".into(),
            callee: "b".into(),
            actual_arg_taints: vec![vec![TaintKind::UserInput]],
        }];
        // returns_taint already has UserInput; actual adds it again.
        let summaries = vec![summ(
            "b",
            &[(0, &[TaintKind::UserInput])],
            &[TaintKind::UserInput],
        )];
        let r = propagate_inter(&edges, &summaries);
        let count = r.propagated_returns[0]
            .result_taint
            .iter()
            .filter(|k| **k == TaintKind::UserInput)
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn serde_round_trip() {
        let edges = vec![CallEdgeFlow {
            caller: "a".into(),
            callee: "missing".into(),
            actual_arg_taints: vec![],
        }];
        let r = propagate_inter(&edges, &[]);
        let json = serde_json::to_string(&r).unwrap();
        let back: InterFlowResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
        assert!(json.contains("missing_callee_summary"));
    }

    #[test]
    fn chain_is_resolved_single_hop_not_transitively() {
        // The pass resolves each edge ONCE against its callee summary; it does
        // not follow A→B→C transitively. With edges a→b and b→c where c returns
        // DbLink, the result must contain two independent single-hop records and
        // c's DbLink must NOT appear on a's record (no transitive composition).
        // This pins the documented single-hop scope (replacing the former
        // `depth_cap_fires_when_chain_exceeds_max` 6==6 tautology, which faked
        // coverage of a depth cap that no input could ever trigger).
        let edges = vec![
            CallEdgeFlow {
                caller: "a".into(),
                callee: "b".into(),
                actual_arg_taints: vec![vec![TaintKind::UserInput]],
            },
            CallEdgeFlow {
                caller: "b".into(),
                callee: "c".into(),
                actual_arg_taints: vec![vec![TaintKind::UserInput]],
            },
        ];
        let summaries = vec![
            summ("b", &[(0, &[TaintKind::UserInput])], &[]),
            summ("c", &[], &[TaintKind::DbLink]),
        ];
        let r = propagate_inter(&edges, &summaries);
        assert!(r.unknowns.is_empty());
        assert_eq!(r.propagated_returns.len(), 2);

        let a_rec = r
            .propagated_returns
            .iter()
            .find(|p| p.caller == "a")
            .expect("a→b record present");
        assert!(
            a_rec.result_taint.contains(&TaintKind::UserInput),
            "a→b folds the actual's UserInput through b's propagating param"
        );
        assert!(
            !a_rec.result_taint.contains(&TaintKind::DbLink),
            "single-hop: c's DbLink must NOT transitively reach a"
        );

        let b_rec = r
            .propagated_returns
            .iter()
            .find(|p| p.caller == "b")
            .expect("b→c record present");
        assert!(
            b_rec.result_taint.contains(&TaintKind::DbLink),
            "b→c carries c's declared return taint to b"
        );
    }

    #[test]
    fn distinct_caller_callee_with_same_name_in_two_edges_is_not_a_cycle() {
        // Guard against over-eager cycle detection: a→b and b→a are two distinct
        // single-hop edges (mutual recursion at the chain level), but NEITHER is
        // a direct self-call, so both resolve and neither is flagged a cycle.
        let edges = vec![
            CallEdgeFlow {
                caller: "a".into(),
                callee: "b".into(),
                actual_arg_taints: vec![],
            },
            CallEdgeFlow {
                caller: "b".into(),
                callee: "a".into(),
                actual_arg_taints: vec![],
            },
        ];
        let summaries = vec![summ("a", &[], &[]), summ("b", &[], &[])];
        let r = propagate_inter(&edges, &summaries);
        assert!(
            r.unknowns.is_empty(),
            "mutual edges are single-hop resolvable, not direct self-recursion"
        );
        assert_eq!(r.propagated_returns.len(), 2);
    }
}
