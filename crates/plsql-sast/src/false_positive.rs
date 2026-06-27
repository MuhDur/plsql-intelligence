//! False-positive measurement harness.
//!
//! A *negative corpus* is PL/SQL known to be clean for a given
//! rule set. By definition **every** finding produced on it is a
//! false positive, so the FP rate is simply
//! `units_with_findings / units`. This module turns that into a
//! measurable, CI-enforceable gate.
//!
//! The harness is generic over the corpus (callers build
//! [`NegativeCase`]s from real files). A small built-in corpus
//! of representative safe constructs is provided so the shipped
//! rule pack carries its own FP regression test.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{CompletenessSnapshot, Finding, Rule, ScanUnit, run_scan};
use plsql_ir::{FactStore, FlowEnv};

#[cfg(test)]
fn corpus_provenance() -> plsql_ir::FactProvenance {
    plsql_ir::FactProvenance {
        component: "corpus".into(),
        component_version: "0".into(),
        run_id: String::new(),
    }
}

/// One known-clean unit: its logical id, source path, and the
/// Layer-2 inputs a scan would see for it.
pub struct NegativeCase<'a> {
    pub unit_logical_id: &'a str,
    pub source_file: &'a str,
    pub flow: &'a FlowEnv,
    pub facts: &'a FactStore,
    pub completeness: CompletenessSnapshot,
}

/// Measured false-positive outcome over a negative corpus.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FalsePositiveReport {
    pub units: usize,
    pub units_with_findings: usize,
    /// Every finding here is, by construction, a false positive.
    pub false_positives: Vec<Finding>,
    /// Per-rule false-positive counts (sorted by rule id).
    pub by_rule: BTreeMap<String, usize>,
}

impl FalsePositiveReport {
    /// `units_with_findings / units` in `[0.0, 1.0]`; `0.0` for
    /// an empty corpus (vacuously clean).
    #[must_use]
    pub fn fp_rate(&self) -> f64 {
        if self.units == 0 {
            0.0
        } else {
            self.units_with_findings as f64 / self.units as f64
        }
    }

    /// True iff the rule set produced zero findings on the
    /// negative corpus — the gate CI should assert.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.false_positives.is_empty()
    }
}

/// Run `rules` over every negative case and tally false
/// positives. Deterministic: `false_positives` follows each
/// per-unit [`run_scan`] order; `by_rule` is a sorted map.
#[must_use]
pub fn measure_false_positives(
    rules: &[Box<dyn Rule>],
    corpus: &[NegativeCase<'_>],
) -> FalsePositiveReport {
    let mut report = FalsePositiveReport {
        units: corpus.len(),
        ..FalsePositiveReport::default()
    };
    for case in corpus {
        let units = [ScanUnit {
            unit_logical_id: case.unit_logical_id,
            source_file: case.source_file,
            flow: case.flow,
        }];
        let scan = run_scan(rules, &units, case.facts, &case.completeness);
        if !scan.findings.is_empty() {
            report.units_with_findings += 1;
        }
        for f in scan.findings {
            *report.by_rule.entry(f.rule_id.clone()).or_insert(0) += 1;
            report.false_positives.push(f);
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Sec001ExecuteImmediateInjection, Sec002DbmsSqlParse, Sec006GrantToPublic};

    /// Built-in negative corpus: representative *safe* PL/SQL.
    /// The shipped rule pack must produce ZERO findings here.
    #[test]
    fn shipped_rules_have_zero_false_positives_on_safe_corpus() {
        let no_sources = plsql_ir::TaintSources {
            user_input_names: vec![],
            bind_names: vec![],
        };

        // Case 1: dynamic SQL built from a pure string literal —
        // SEC001 must treat it as provably safe.
        let env_literal = plsql_ir::analyze_flow(
            &plsql_ir::lower_statement_body("dyn := 'SELECT 1 FROM dual';"),
            &no_sources,
        );
        let mut facts_literal = FactStore::default();
        facts_literal.push(plsql_ir::mint_fact(
            corpus_provenance(),
            plsql_ir::FactPayload::DynamicSqlEvidence { site: "DYN".into() },
        ));
        plsql_ir::emit_flow_env_facts(
            &mut facts_literal,
            &corpus_provenance(),
            "safe.literal",
            &env_literal,
        );

        // Case 2: dynamic SQL built only from constants + a
        // non-user expression (no taint source) — out of SEC001
        // scope (that is SEC002's remit), so SEC001 must stay
        // silent rather than flag a non-tainted concatenation.
        let env_const = plsql_ir::analyze_flow(
            &plsql_ir::lower_statement_body(
                "dyn := 'SELECT * FROM t WHERE d=' || to_char(sysdate);",
            ),
            &no_sources,
        );
        let mut facts_const = FactStore::default();
        facts_const.push(plsql_ir::mint_fact(
            corpus_provenance(),
            plsql_ir::FactPayload::DynamicSqlEvidence { site: "DYN".into() },
        ));
        plsql_ir::emit_flow_env_facts(
            &mut facts_const,
            &corpus_provenance(),
            "safe.const",
            &env_const,
        );

        // Case 3: privileges granted only to named roles, never
        // PUBLIC — SEC006 must stay silent.
        let env_priv = FlowEnv::default();
        let mut facts_priv = FactStore::default();
        for (g, p, o) in [
            ("REPORTING", "SELECT", "HR.EMPLOYEES"),
            ("APP_ROLE", "EXECUTE", "HR.PKG"),
        ] {
            facts_priv.push(plsql_ir::mint_fact(
                corpus_provenance(),
                plsql_ir::FactPayload::Privilege {
                    grantee: g.into(),
                    privilege: p.into(),
                    on: o.into(),
                },
            ));
        }

        let with_cat = CompletenessSnapshot {
            catalog_available: true,
            ..CompletenessSnapshot::default()
        };
        let corpus = vec![
            NegativeCase {
                unit_logical_id: "safe.literal",
                source_file: "safe1.sql",
                flow: &env_literal,
                facts: &facts_literal,
                completeness: CompletenessSnapshot::default(),
            },
            NegativeCase {
                unit_logical_id: "safe.const",
                source_file: "safe2.sql",
                flow: &env_const,
                facts: &facts_const,
                completeness: CompletenessSnapshot::default(),
            },
            NegativeCase {
                unit_logical_id: "safe.priv",
                source_file: "safe3.sql",
                flow: &env_priv,
                facts: &facts_priv,
                completeness: with_cat,
            },
        ];

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(Sec001ExecuteImmediateInjection),
            Box::new(Sec002DbmsSqlParse),
            Box::new(Sec006GrantToPublic),
        ];
        let report = measure_false_positives(&rules, &corpus);

        assert_eq!(report.units, 3);
        assert!(
            report.is_clean(),
            "shipped rules false-positived on the safe corpus: {:?}",
            report.false_positives
        );
        assert_eq!(report.fp_rate(), 0.0);
    }

    #[test]
    fn harness_counts_a_planted_false_positive() {
        // A deliberately over-eager rule fires on clean input —
        // the harness must measure it, proving the gate works.
        struct AlwaysFires;
        impl Rule for AlwaysFires {
            fn id(&self) -> &'static str {
                "FP-TEST"
            }
            fn default_severity(&self) -> crate::Severity {
                crate::Severity::Low
            }
            fn description(&self) -> &'static str {
                "fires on everything"
            }
            fn scan(&self, ctx: &crate::ScanContext<'_>) -> crate::RuleOutput {
                crate::RuleOutput::default().finding(crate::finding(
                    self.id(),
                    crate::Severity::Low,
                    "noise",
                    ctx.source_file,
                    1,
                    (0, 1),
                ))
            }
        }
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let corpus = vec![
            NegativeCase {
                unit_logical_id: "a",
                source_file: "a.sql",
                flow: &env,
                facts: &facts,
                completeness: CompletenessSnapshot::default(),
            },
            NegativeCase {
                unit_logical_id: "b",
                source_file: "b.sql",
                flow: &env,
                facts: &facts,
                completeness: CompletenessSnapshot::default(),
            },
        ];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(AlwaysFires)];
        let r = measure_false_positives(&rules, &corpus);
        assert_eq!(r.units_with_findings, 2);
        assert_eq!(r.fp_rate(), 1.0);
        assert_eq!(r.by_rule.get("FP-TEST"), Some(&2));
        assert!(!r.is_clean());
    }

    #[test]
    fn empty_corpus_is_vacuously_clean() {
        let r = measure_false_positives(&[], &[]);
        assert!(r.is_clean());
        assert_eq!(r.fp_rate(), 0.0);
        assert_eq!(r.units, 0);
    }

    #[test]
    fn report_round_trips_through_json() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let corpus = vec![NegativeCase {
            unit_logical_id: "a",
            source_file: "a.sql",
            flow: &env,
            facts: &facts,
            completeness: CompletenessSnapshot::default(),
        }];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec006GrantToPublic)];
        let r = measure_false_positives(&rules, &corpus);
        let j = serde_json::to_string(&r).unwrap();
        let back: FalsePositiveReport = serde_json::from_str(&j).unwrap();
        assert_eq!(back, r);
    }
}
