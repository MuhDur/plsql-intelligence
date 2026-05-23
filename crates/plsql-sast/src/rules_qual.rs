//! Quality (`QUAL`) SAST rules backed by the `ExceptionHandler`
//! fact family.
//!
//! These rules live in their own file (not `rules.rs`) so the SAST
//! rule surface can be extended by multiple agents without
//! single-file contention. Each is a pure function of its
//! [`ScanContext`](crate::ScanContext) and depends only on the
//! `ExceptionHandler` facts the IR layer emits from
//! `scan_exception_handlers` — no AST/source access, no heuristic
//! re-parsing. A run with zero `ExceptionHandler` facts causes the
//! harness to skip the rule (R13: the gap is reported, not a silent
//! pass).
//!
//! Fact contract (`plsql_ir::FactPayload::ExceptionHandler`):
//! `{ unit_logical_id, scope, body_class }` where `scope` is
//! `others` or a named exception (lowercased) and `body_class` is
//! one of `noop` / `commit` / `rollback` / `other`.

use plsql_ir::{FactKind, FactPayload};

use crate::{Finding, Rule, RuleOutput, ScanContext, Severity, finding};

/// **QUAL001 — `WHEN OTHERS THEN NULL` (swallowed exception).**
///
/// An `OTHERS` handler whose entire body is `NULL;` silently
/// discards every error — the textbook PL/SQL anti-pattern. It
/// masks failures (including security-relevant ones) and makes the
/// program lie about its own success. The evidence is a definitive
/// syntactic fact (`scope == others` AND `body_class == noop`), so
/// confidence stays `High`.
///
/// /oracle: `DATABASE-REFERENCE.md` PL/SQL Language Reference —
/// "Exception Handler" / `OTHERS`; the canonical guidance is to
/// log and re-raise, never to swallow.
pub struct Qual001WhenOthersThenNull;

impl Rule for Qual001WhenOthersThenNull {
    fn id(&self) -> &'static str {
        "QUAL001"
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn description(&self) -> &'static str {
        "WHEN OTHERS THEN NULL swallows every exception (silently discarded error)"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::ExceptionHandler]
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::ExceptionHandler) {
            let FactPayload::ExceptionHandler {
                unit_logical_id,
                scope,
                body_class,
            } = &fact.payload
            else {
                continue;
            };
            if !scope.eq_ignore_ascii_case("others") || body_class != "noop" {
                continue;
            }
            let f: Finding = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "`WHEN OTHERS THEN NULL` in `{unit_logical_id}` silently swallows every \
                     exception"
                ),
                ctx.source_file,
                // Exception-handler facts are source-scanned and
                // carry no precise span; point at the unit.
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Log the error (with SQLCODE/SQLERRM) and re-raise, or handle a specific \
                     named exception instead of swallowing OTHERS."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **QUAL004 — transaction control inside an exception handler.**
///
/// A `COMMIT` or `ROLLBACK` inside an exception handler commits or
/// discards partially-completed work on the error path, breaking
/// the caller's atomicity assumptions (a handler that commits turns
/// a failed unit into a half-applied one; one that rolls back can
/// erase a caller-owned transaction). The `ExceptionHandler` fact's
/// `body_class` (`commit` / `rollback`) is a definitive syntactic
/// signal, so confidence stays `High`.
///
/// Scope note: this rule covers transaction control *in an
/// exception handler*, which is what the `ExceptionHandler` fact
/// family supports today. Detecting COMMIT/ROLLBACK in trigger
/// bodies generally needs a separate statement-pattern fact family
/// and is intentionally out of scope here (not silently claimed).
///
/// /oracle: `DATABASE-REFERENCE.md` PL/SQL Language Reference —
/// transaction control + autonomous transactions; committing in a
/// handler is called out as an atomicity hazard.
pub struct Qual004TxnControlInHandler;

impl Rule for Qual004TxnControlInHandler {
    fn id(&self) -> &'static str {
        "QUAL004"
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn description(&self) -> &'static str {
        "COMMIT/ROLLBACK inside an exception handler breaks caller atomicity on the error path"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::ExceptionHandler]
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::ExceptionHandler) {
            let FactPayload::ExceptionHandler {
                unit_logical_id,
                scope,
                body_class,
            } = &fact.payload
            else {
                continue;
            };
            let verb = match body_class.as_str() {
                "commit" => "COMMIT",
                "rollback" => "ROLLBACK",
                _ => continue,
            };
            let f: Finding = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "`{verb}` inside the `WHEN {scope}` exception handler of \
                     `{unit_logical_id}` breaks caller atomicity on the error path"
                ),
                ctx.source_file,
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Let the exception propagate and leave transaction control to the \
                     outermost caller; if isolation is genuinely required use a documented \
                     AUTONOMOUS_TRANSACTION, not a handler-level COMMIT/ROLLBACK."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompletenessSnapshot, ScanUnit, run_scan};
    use plsql_ir::{FactProvenance, FactStore, FlowEnv, mint_fact};

    fn prov() -> FactProvenance {
        FactProvenance {
            component: "test".to_string(),
            component_version: "0".to_string(),
            run_id: String::new(),
        }
    }

    fn handler_fact(unit: &str, scope: &str, body_class: &str) -> plsql_ir::Fact {
        mint_fact(
            prov(),
            FactPayload::ExceptionHandler {
                unit_logical_id: unit.to_string(),
                scope: scope.to_string(),
                body_class: body_class.to_string(),
            },
        )
    }

    fn scan_with(rule: Box<dyn Rule>, facts: &FactStore) -> crate::ScanReport {
        let env = FlowEnv::default();
        let units = [ScanUnit {
            unit_logical_id: "pkg.proc",
            source_file: "pkg.sql",
            flow: &env,
        }];
        let snap = CompletenessSnapshot::default();
        run_scan(&[rule], &units, facts, &snap)
    }

    #[test]
    fn qual001_flags_when_others_then_null() {
        let mut facts = FactStore::default();
        facts.push(handler_fact("pkg.proc", "others", "noop"));
        let r = scan_with(Box::new(Qual001WhenOthersThenNull), &facts);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].rule_id, "QUAL001");
        assert_eq!(r.findings[0].severity, Severity::Medium);
        assert!(r.findings[0].message.contains("swallows"));
        assert!(
            r.findings[0]
                .remediation
                .as_ref()
                .unwrap()
                .contains("re-raise")
        );
    }

    #[test]
    fn qual001_ignores_named_noop_and_others_with_real_body() {
        let mut facts = FactStore::default();
        // Named exception with empty body is NOT QUAL001 (it caught a
        // specific, expected condition deliberately).
        facts.push(handler_fact("pkg.proc", "no_data_found", "noop"));
        // OTHERS that actually does something is fine here.
        facts.push(handler_fact("pkg.proc", "others", "other"));
        let r = scan_with(Box::new(Qual001WhenOthersThenNull), &facts);
        assert!(r.findings.is_empty(), "got {:?}", r.findings);
    }

    #[test]
    fn qual001_case_insensitive_scope() {
        let mut facts = FactStore::default();
        facts.push(handler_fact("pkg.proc", "OTHERS", "noop"));
        let r = scan_with(Box::new(Qual001WhenOthersThenNull), &facts);
        assert_eq!(r.findings.len(), 1);
    }

    #[test]
    fn qual004_flags_commit_and_rollback_in_handler() {
        let mut facts = FactStore::default();
        facts.push(handler_fact("pkg.proc", "others", "commit"));
        facts.push(handler_fact("pkg.proc", "dup_val_on_index", "rollback"));
        let r = scan_with(Box::new(Qual004TxnControlInHandler), &facts);
        assert_eq!(r.findings.len(), 2);
        assert!(r.findings.iter().all(|f| f.rule_id == "QUAL004"));
        assert!(r.findings.iter().any(|f| f.message.contains("COMMIT")));
        assert!(r.findings.iter().any(|f| f.message.contains("ROLLBACK")));
    }

    #[test]
    fn qual004_ignores_noop_and_other_bodies() {
        let mut facts = FactStore::default();
        facts.push(handler_fact("pkg.proc", "others", "noop"));
        facts.push(handler_fact("pkg.proc", "others", "other"));
        let r = scan_with(Box::new(Qual004TxnControlInHandler), &facts);
        assert!(r.findings.is_empty(), "got {:?}", r.findings);
    }

    #[test]
    fn rules_skip_when_no_exception_handler_facts() {
        // R13: with zero ExceptionHandler facts the harness must skip
        // (typed), never silently "pass".
        let facts = FactStore::default();
        let r1 = scan_with(Box::new(Qual001WhenOthersThenNull), &facts);
        assert!(r1.findings.is_empty());
        assert!(
            !r1.skipped.is_empty(),
            "QUAL001 must record a typed skip with no facts"
        );
        let r4 = scan_with(Box::new(Qual004TxnControlInHandler), &facts);
        assert!(r4.findings.is_empty());
        assert!(!r4.skipped.is_empty());
    }
}
