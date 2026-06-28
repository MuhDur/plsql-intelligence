//! Concrete SAST rules. Each `Rule` impl is a pure function of
//! its [`ScanContext`](crate::ScanContext); the harness gates and
//! drives them.

use plsql_ir::{FactId, FactKind, FactPayload, StringShape, TaintKind};

use crate::{
    CompletenessRequirement, Finding, Rule, RuleOutput, ScanContext, Severity, SkipReason, finding,
};

/// **SEC001 — `EXECUTE IMMEDIATE` SQL injection.**
///
/// For every dynamic-SQL site the analysis recorded
/// (`FactKind::DynamicSqlEvidence`), decide soundly:
///
/// * the SQL string is **taint-reachable** from an uncleansed
///   source → `Critical` injection finding;
/// * an [`Opacity`](plsql_ir::FactPayload::Opacity) fact covers
///   the site (DBMS_SQL / db-link / wrapped) → **skip**
///   `OpaqueConstruct` — we cannot prove safety, and silence
///   would be a false negative (R13);
/// * the flow pass produced **no string evidence at all** for
///   the site → **skip** `MissingFlowFacts` (R13);
/// * the string is a pure literal / empty → provably safe, no
///   finding and no skip.
///
/// Non-tainted concatenated SQL is intentionally *not* flagged
/// here — that is SEC002's remit; SEC001 asserts only proven
/// taint-to-sink, keeping precision high.
///
/// /oracle: `SECURITY-OPTIONS-REFERENCE.md` (SQL injection /
/// dynamic SQL) + `DATABASE-REFERENCE.md` (`EXECUTE IMMEDIATE`,
/// `DBMS_ASSERT` cleansers).
pub struct Sec001ExecuteImmediateInjection;

fn normalise_flow_fact_name(name: &str) -> String {
    name.trim().to_ascii_uppercase()
}

fn fact_unit_matches(fact_unit: &str, ctx_unit: &str) -> bool {
    fact_unit.trim().eq_ignore_ascii_case(ctx_unit.trim())
}

fn taint_fact_for<'a>(
    ctx: &'a ScanContext<'_>,
    site: &str,
) -> Option<(&'a FactId, &'a [TaintKind])> {
    let wanted = normalise_flow_fact_name(site);
    ctx.facts.by_kind(FactKind::Taint).find_map(|fact| {
        let FactPayload::Taint {
            unit_logical_id,
            name,
            kinds,
        } = &fact.payload
        else {
            return None;
        };
        if fact_unit_matches(unit_logical_id, ctx.unit_logical_id)
            && name.eq_ignore_ascii_case(&wanted)
        {
            Some((&fact.id, kinds.as_slice()))
        } else {
            None
        }
    })
}

fn string_shape_fact_for<'a>(
    ctx: &'a ScanContext<'_>,
    site: &str,
) -> Option<(&'a FactId, &'a StringShape)> {
    let wanted = normalise_flow_fact_name(site);
    ctx.facts.by_kind(FactKind::StringShape).find_map(|fact| {
        let FactPayload::StringShape {
            unit_logical_id,
            name,
            shape,
        } = &fact.payload
        else {
            return None;
        };
        if fact_unit_matches(unit_logical_id, ctx.unit_logical_id)
            && name.eq_ignore_ascii_case(&wanted)
        {
            Some((&fact.id, shape))
        } else {
            None
        }
    })
}

impl Rule for Sec001ExecuteImmediateInjection {
    fn id(&self) -> &'static str {
        "SEC001"
    }

    fn default_severity(&self) -> Severity {
        Severity::Critical
    }

    fn description(&self) -> &'static str {
        "Tainted value reaches an EXECUTE IMMEDIATE dynamic-SQL sink (SQL injection)"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::DynamicSqlEvidence]
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();

        // Sites the analysis flagged opaque — we must not assert
        // safety on these.
        let opaque: std::collections::BTreeSet<&str> = ctx
            .facts
            .by_kind(FactKind::Opacity)
            .filter_map(|f| match &f.payload {
                FactPayload::Opacity {
                    target_logical_id, ..
                } => Some(target_logical_id.as_str()),
                _ => None,
            })
            .collect();

        for fact in ctx.facts.by_kind(FactKind::DynamicSqlEvidence) {
            let FactPayload::DynamicSqlEvidence { site } = &fact.payload else {
                continue;
            };

            if opaque.contains(site.as_str()) {
                out = out.skip(ctx.skip(
                    self.id(),
                    SkipReason::OpaqueConstruct,
                    &format!("dynamic-SQL site `{site}` is opaque; cannot prove safety"),
                ));
                continue;
            }

            if let Some((taint_fact_id, kinds)) = taint_fact_for(ctx, site) {
                let kinds = kinds
                    .iter()
                    .map(|k| format!("{k:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let f: Finding = finding(
                    self.id(),
                    self.default_severity(),
                    &format!(
                        "Uncleansed tainted value ({kinds}) reaches EXECUTE IMMEDIATE at `{site}` \
                         (TaintFact {})",
                        taint_fact_id.0
                    ),
                    ctx.source_file_for_fact(fact),
                    0,
                    (0, 0),
                );
                out = out.finding(Finding {
                    remediation: Some(
                        "Bind user input with USING placeholders, or validate via \
                         DBMS_ASSERT before concatenation."
                            .to_string(),
                    ),
                    ..f
                });
                continue;
            }

            match string_shape_fact_for(ctx, site).map(|(_, shape)| shape) {
                Some(StringShape::Literal { .. }) | Some(StringShape::Empty) => {
                    // Provably constant — safe, nothing to report.
                }
                Some(StringShape::InterpolatedWithFix { .. }) | Some(StringShape::FullyOpaque) => {
                    // Built from runtime expressions but no taint
                    // reached it — out of SEC001's scope (SEC002).
                }
                None => {
                    out = out.skip(ctx.skip(
                        self.id(),
                        SkipReason::MissingFlowFacts,
                        &format!("no StringShape/Taint facts for dynamic-SQL site `{site}`"),
                    ));
                }
            }
        }
        out
    }
}

/// **SEC006 — `GRANT … TO PUBLIC`**.
///
/// Granting any privilege to the `PUBLIC` role exposes the object
/// to every database account and is a textbook privilege-sprawl /
/// escalation finding. Object privileges (SELECT/EXECUTE/…) and
/// especially powerful system privileges to PUBLIC are flagged
/// `High`; the report's confidence stamp stays `High` because the
/// evidence is a definitive catalog/DDL fact, not a heuristic.
///
/// /oracle: `SECURITY-OPTIONS-REFERENCE.md` — least-privilege /
/// PUBLIC-grant guidance; `LOW-LEVEL-CATALOGS.md` —
/// `DBA_TAB_PRIVS.GRANTEE = 'PUBLIC'`.
pub struct Sec006GrantToPublic;

impl Rule for Sec006GrantToPublic {
    fn id(&self) -> &'static str {
        "SEC006"
    }

    fn default_severity(&self) -> Severity {
        Severity::High
    }

    fn description(&self) -> &'static str {
        "Privilege granted to PUBLIC exposes the object to every database account"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::Privilege]
    }

    fn minimum_completeness(&self) -> CompletenessRequirement {
        // Privilege facts come from the catalog / DDL extraction;
        // without a catalog there is nothing sound to assert.
        CompletenessRequirement {
            requires_catalog: true,
            ..CompletenessRequirement::default()
        }
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::Privilege) {
            let FactPayload::Privilege {
                grantee,
                privilege,
                on,
            } = &fact.payload
            else {
                continue;
            };
            if !grantee.eq_ignore_ascii_case("PUBLIC") {
                continue;
            }
            let msg = format!(
                "`GRANT {privilege} ON {on} TO PUBLIC` exposes {on} to every database account"
            );
            let f: Finding = finding(
                self.id(),
                self.default_severity(),
                &msg,
                ctx.source_file_for_fact(fact),
                // Privilege facts are catalog/DDL-derived and
                // carry no source span; point at the unit rather
                // than fabricate a precise location.
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(format!(
                    "Revoke from PUBLIC and grant {privilege} on {on} to a specific role."
                )),
                ..f
            });
        }
        out
    }
}

/// **SEC002 — `DBMS_SQL.PARSE` opaque dynamic SQL.**
///
/// `DBMS_SQL` builds a statement through a cursor handle, so the
/// SQL text rarely survives as a single traceable value — the
/// analyzer records it as [`Opacity`](plsql_ir::FactPayload::Opacity)
/// rather than a taint path. SEC001 cannot reason about these
/// sites, so they would otherwise be a *silent* blind spot. SEC002
/// surfaces every `DBMS_SQL`-attributed opaque site as a `Medium`
/// finding (the *injection* is unproven, but the surface is
/// unanalysable and must be reviewed) with a `Medium`-confidence
/// stamp — this is a "review-queue", not a "must-fix", signal.
///
/// /oracle: `SECURITY-OPTIONS-REFERENCE.md` (dynamic SQL attack
/// surface) + `DATABASE-REFERENCE.md` (`DBMS_SQL` package).
pub struct Sec002DbmsSqlParse;

impl Rule for Sec002DbmsSqlParse {
    fn id(&self) -> &'static str {
        "SEC002"
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn description(&self) -> &'static str {
        "DBMS_SQL dynamic SQL is opaque to taint analysis — manual injection review required"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::Opacity]
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::Opacity) {
            let FactPayload::Opacity {
                target_logical_id,
                reason,
            } = &fact.payload
            else {
                continue;
            };
            if !reason.to_ascii_uppercase().contains("DBMS_SQL") {
                continue;
            }
            let evidence = string_shape_fact_for(ctx, target_logical_id)
                .map(|(id, shape)| format!("StringShapeFact {} ({shape:?})", id.0))
                .unwrap_or_else(|| format!("OpacityFact {}", fact.id.0));
            let f: Finding = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "DBMS_SQL dynamic SQL at `{target_logical_id}` is opaque to taint \
                     analysis ({reason}); injection cannot be ruled out automatically \
                     ({evidence})"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            // Heuristic surface, not a proven taint path — drop
            // confidence so the report files it under review,
            // not must-fix.
            let mut downgraded = f;
            downgraded.confidence = plsql_core::Confidence {
                level: plsql_core::ConfidenceLevel::Medium,
                explanation: Some(
                    "opaque DBMS_SQL surface; injection unproven but unanalysable".to_string(),
                ),
            };
            downgraded.remediation = Some(
                "Prefer EXECUTE IMMEDIATE with USING binds, or bind every DBMS_SQL value \
                 via DBMS_SQL.BIND_VARIABLE; manually review this site."
                    .to_string(),
            );
            out = out.finding(downgraded);
        }
        out
    }
}

/// **PERF001 — cursor `FOR` loop is a BULK COLLECT candidate.**
///
/// A row-by-row cursor `FOR` loop incurs one context switch per
/// row. Fetching the result set with `BULK COLLECT … LIMIT` is
/// typically an order of magnitude faster. Fires on every
/// `CursorForLoop` fact (the kcjx substrate already excluded
/// numeric-range loops, so every fact here is a real cursor loop).
/// Advisory severity — it is a performance smell, not a defect.
///
/// /oracle: `DATABASE-REFERENCE.md` PL/SQL Language Reference —
/// "Bulk SQL and Bulk Binding" / `BULK COLLECT`.
pub struct Perf001CursorForLoopBulkCollect;

impl Rule for Perf001CursorForLoopBulkCollect {
    fn id(&self) -> &'static str {
        "PERF001"
    }

    fn default_severity(&self) -> Severity {
        Severity::Low
    }

    fn description(&self) -> &'static str {
        "Cursor FOR loop fetches row-by-row; consider BULK COLLECT for set-based fetch"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::CursorForLoop]
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::CursorForLoop) {
            let FactPayload::CursorForLoop {
                unit_logical_id,
                loop_var,
                ..
            } = &fact.payload
            else {
                continue;
            };
            let f: Finding = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "Cursor FOR loop `{loop_var}` in `{unit_logical_id}` fetches row-by-row; \
                     BULK COLLECT … LIMIT avoids per-row context switches"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Replace the cursor FOR loop with `BULK COLLECT INTO <collection> LIMIT n` \
                     and process the collection in batches."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **PERF002 — row-by-row DML in a cursor `FOR` loop (FORALL
/// candidate).**
///
/// A cursor `FOR` loop whose body issues `INSERT`/`UPDATE`/
/// `DELETE`/`MERGE` per row is the classic `FORALL` anti-pattern:
/// one SQL→PL/SQL context switch per row in *both* directions.
/// Strictly narrower than PERF001 — it fires only on the
/// `has_body_dml` subset — so it carries a slightly higher
/// severity and a `FORALL` remediation.
///
/// /oracle: `DATABASE-REFERENCE.md` PL/SQL Language Reference —
/// "FORALL Statement".
pub struct Perf002CursorForLoopForall;

impl Rule for Perf002CursorForLoopForall {
    fn id(&self) -> &'static str {
        "PERF002"
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn description(&self) -> &'static str {
        "Row-by-row DML inside a cursor FOR loop; consider FORALL bulk DML"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::CursorForLoop]
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::CursorForLoop) {
            let FactPayload::CursorForLoop {
                unit_logical_id,
                loop_var,
                has_body_dml,
            } = &fact.payload
            else {
                continue;
            };
            if !has_body_dml {
                continue;
            }
            let f: Finding = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "Cursor FOR loop `{loop_var}` in `{unit_logical_id}` performs row-by-row \
                     DML; a FORALL bulk bind eliminates per-row context switches"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Collect the driving rows with BULK COLLECT, then apply the DML with \
                     `FORALL i IN 1..coll.COUNT` instead of per-row statements."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **STYLE001 — routine has no instrumentation (opt-in, per house
/// policy).**
///
/// Some shops require every executable unit to emit at least one
/// logging / tracing / audit / error-signal call so production
/// incidents are diagnosable. This rule fires on the
/// `MissingInstrumentation` fact (the kcjx substrate already
/// gated it to units with a `BEGIN` body and zero recognized
/// markers, and skipped specs). It is `Info`-severity and intended
/// to run only when the consumer opts the STYLE family in — the
/// rule itself never asserts a hard violation.
///
/// /oracle: `DATABASE-REFERENCE.md` PL/SQL Language Reference —
/// `DBMS_APPLICATION_INFO` / instrumentation guidance.
pub struct Style001MissingInstrumentation;

impl Rule for Style001MissingInstrumentation {
    fn id(&self) -> &'static str {
        "STYLE001"
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn description(&self) -> &'static str {
        "Routine body has no instrumentation/logging call (opt-in house-policy rule)"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::MissingInstrumentation]
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::MissingInstrumentation) {
            let FactPayload::MissingInstrumentation { unit_logical_id } = &fact.payload else {
                continue;
            };
            let f: Finding = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "`{unit_logical_id}` has an executable body but no recognized \
                     instrumentation/logging call (house-policy STYLE001)"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Add a logging/tracing call (e.g. DBMS_APPLICATION_INFO, your logger \
                     package, or a structured audit insert) so production incidents in this \
                     unit are diagnosable; or exclude this unit from the STYLE001 policy."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **SEC003 — hardcoded credential.**
///
/// A secret embedded as a string literal in source (an
/// `IDENTIFIED BY '…'` DDL, an assignment to a
/// password/secret/token-named target, or a `password => '…'`
/// named argument) is a textbook credential-leak: it lands in
/// version control, logs, and bundles. Fires on the
/// `HardcodedCredential` fact (the substrate already required a
/// literal to directly follow the marker, so this is a definitive
/// syntactic signal — `High` confidence, `Critical` severity).
///
/// /oracle: `SECURITY-OPTIONS-REFERENCE.md` — credential
/// management / least-exposure; `DATABASE-REFERENCE.md` — proxy
/// auth / external auth / wallets as the non-hardcoded paths.
pub struct Sec003HardcodedCredentials;

impl Rule for Sec003HardcodedCredentials {
    fn id(&self) -> &'static str {
        "SEC003"
    }

    fn default_severity(&self) -> Severity {
        Severity::Critical
    }

    fn description(&self) -> &'static str {
        "Hardcoded credential: a secret is embedded as a string literal in source"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::HardcodedCredential]
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::HardcodedCredential) {
            let FactPayload::HardcodedCredential {
                unit_logical_id,
                marker,
            } = &fact.payload
            else {
                continue;
            };
            let f: Finding = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "Hardcoded credential in `{unit_logical_id}`: a string literal follows \
                     `{marker}` — the secret is committed to source"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Remove the literal secret: use an Oracle wallet / external password store, \
                     proxy or external authentication, or inject the secret at deploy time — \
                     never commit it to source."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **SEC004 — invoker's rights (`AUTHID CURRENT_USER`).**
///
/// An `AUTHID CURRENT_USER` unit resolves privileges, roles, and
/// unqualified name resolution against the *caller* at run time,
/// not the owner. That is a deliberate pattern in some designs but
/// materially widens the trust/attack surface (a low-privilege
/// caller may invoke owner code that now runs with the caller's —
/// or an attacker-influenced — context). Advisory: it is often
/// intentional, so SEC004 is `Medium` and frames it as
/// review-required, not a hard defect.
///
/// /oracle: `DATABASE-REFERENCE.md` PL/SQL Language Reference —
/// "Invoker's Rights and Definer's Rights (AUTHID Property)";
/// `SECURITY-OPTIONS-REFERENCE.md` — least-privilege guidance.
pub struct Sec004InvokerRights;

impl Rule for Sec004InvokerRights {
    fn id(&self) -> &'static str {
        "SEC004"
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn description(&self) -> &'static str {
        "Unit declares AUTHID CURRENT_USER (invoker's rights) — review the widened trust surface"
    }

    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::InvokerRights]
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::InvokerRights) {
            let FactPayload::InvokerRights { unit_logical_id } = &fact.payload else {
                continue;
            };
            let f: Finding = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "`{unit_logical_id}` declares `AUTHID CURRENT_USER`; privilege and name \
                     resolution defer to the caller at run time — confirm this is intended and \
                     the caller context is trusted"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "If owner-context execution is intended use `AUTHID DEFINER`; if invoker's \
                     rights are required, document why and ensure callers cannot escalate via \
                     unqualified name resolution (schema-qualify all references)."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **SEC007 — function returns a REF CURSOR.** Hands an open
/// cursor to the caller: resource-ownership ambiguity and, when
/// the cursor wrapped dynamic SQL, an injection-amplification
/// surface. `Medium`, advisory (sometimes a deliberate API shape).
///
/// /oracle: `DATABASE-REFERENCE.md` — cursor variables / REF
/// CURSOR; `SECURITY-OPTIONS-REFERENCE.md` — dynamic-SQL surface.
pub struct Sec007RefCursorReturn;

impl Rule for Sec007RefCursorReturn {
    fn id(&self) -> &'static str {
        "SEC007"
    }
    fn default_severity(&self) -> Severity {
        Severity::Medium
    }
    fn description(&self) -> &'static str {
        "Function returns a REF CURSOR — caller-owned open cursor / dynamic-SQL surface"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::RefCursorReturn]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::RefCursorReturn) {
            let FactPayload::RefCursorReturn { unit_logical_id } = &fact.payload else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "`{unit_logical_id}` returns a REF CURSOR; the caller owns an open cursor — \
                     ensure it cannot be opened from unsanitized dynamic SQL and that callers \
                     close it"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Prefer returning a concrete collection/record type; if a REF CURSOR is \
                     required, open it only from static SQL or DBMS_ASSERT-validated text and \
                     document caller close responsibility."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **QUAL007 — DML inside a FUNCTION.** Side-effecting functions
/// break purity, are unsafe in SQL / parallel / replication
/// contexts, and surprise callers. `Medium`.
///
/// /oracle: `DATABASE-REFERENCE.md` — function purity / `WNDS`
/// (writes-no-database-state) expectations.
pub struct Qual007DmlInFunction;

impl Rule for Qual007DmlInFunction {
    fn id(&self) -> &'static str {
        "QUAL007"
    }
    fn default_severity(&self) -> Severity {
        Severity::Medium
    }
    fn description(&self) -> &'static str {
        "Function performs row-level DML — side effects break purity and SQL/parallel safety"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::DmlInFunction]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::DmlInFunction) {
            let FactPayload::DmlInFunction { unit_logical_id } = &fact.payload else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "Function `{unit_logical_id}` performs row-level DML; side-effecting \
                     functions are unsafe in SQL, parallel query, and replication"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Move the DML into a procedure the caller invokes explicitly; keep the \
                     function read-only (no INSERT/UPDATE/DELETE/MERGE)."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **QUAL003 — unbounded `BULK COLLECT`.** A `BULK COLLECT INTO`
/// with no `LIMIT` materializes the entire result set into PGA —
/// an unbounded-memory hazard on large tables. `Medium`.
///
/// /oracle: `DATABASE-REFERENCE.md` — "Limiting Rows for a Bulk
/// FETCH Operation with the LIMIT Clause".
pub struct Qual003UnboundedBulkCollect;

impl Rule for Qual003UnboundedBulkCollect {
    fn id(&self) -> &'static str {
        "QUAL003"
    }
    fn default_severity(&self) -> Severity {
        Severity::Medium
    }
    fn description(&self) -> &'static str {
        "BULK COLLECT without LIMIT materializes the whole result set into PGA (unbounded memory)"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::UnboundedBulkCollect]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::UnboundedBulkCollect) {
            let FactPayload::UnboundedBulkCollect { unit_logical_id } = &fact.payload else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "Unbounded BULK COLLECT in `{unit_logical_id}`: no LIMIT clause — the entire \
                     result set is loaded into PGA memory"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Fetch in batches: `LOOP FETCH cur BULK COLLECT INTO coll LIMIT n; EXIT WHEN \
                     coll.COUNT = 0; … END LOOP;`."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **QUAL005 — deprecated / legacy construct.** `dbms_job`, the
/// legacy `(+)` outer-join operator, and the legacy `WORK`
/// transaction-control keyword have modern replacements and are
/// commonly policy-flagged. `Low` (advisory / modernization).
///
/// /oracle: `DATABASE-REFERENCE.md` — DBMS_SCHEDULER vs DBMS_JOB;
/// ANSI joins vs `(+)`.
pub struct Qual005DeprecatedFeature;

impl Rule for Qual005DeprecatedFeature {
    fn id(&self) -> &'static str {
        "QUAL005"
    }
    fn default_severity(&self) -> Severity {
        Severity::Low
    }
    fn description(&self) -> &'static str {
        "Deprecated/legacy construct with a modern replacement"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::DeprecatedFeature]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::DeprecatedFeature) {
            let FactPayload::DeprecatedFeature {
                unit_logical_id,
                feature,
            } = &fact.payload
            else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!("`{unit_logical_id}` uses a deprecated/legacy construct: {feature}"),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Migrate to the modern replacement (DBMS_SCHEDULER, ANSI JOIN syntax, plain \
                     COMMIT/ROLLBACK)."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **QUAL008 — `DETERMINISTIC` misuse.** A function marked
/// `DETERMINISTIC` whose body is in fact non-deterministic (DML,
/// query, SYSDATE/SYSTIMESTAMP, DBMS_RANDOM, sequence) returns
/// stale/incorrect results from result-cache / function-based
/// index / MV contexts. `Medium`.
///
/// /oracle: `DATABASE-REFERENCE.md` — DETERMINISTIC clause
/// semantics and the correctness contract it asserts.
pub struct Qual008DeterministicMisuse;

impl Rule for Qual008DeterministicMisuse {
    fn id(&self) -> &'static str {
        "QUAL008"
    }
    fn default_severity(&self) -> Severity {
        Severity::Medium
    }
    fn description(&self) -> &'static str {
        "Function declared DETERMINISTIC but its body is non-deterministic"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::DeterministicMisuse]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::DeterministicMisuse) {
            let FactPayload::DeterministicMisuse {
                unit_logical_id,
                construct,
            } = &fact.payload
            else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "`{unit_logical_id}` is declared DETERMINISTIC but its body uses {construct} \
                     — the determinism contract is violated"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Remove the DETERMINISTIC keyword, or remove the non-deterministic construct \
                     so the function genuinely returns the same output for the same inputs."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **QUAL006 — mutating-table trigger.** A `FOR EACH ROW` trigger
/// that queries or DMLs its own base table raises ORA-04091 at run
/// time (and the workarounds — compound triggers / autonomous txns
/// — have correctness traps). `High`.
///
/// /oracle: `DATABASE-REFERENCE.md` — Mutating-Table Restriction;
/// compound trigger guidance.
pub struct Qual006MutatingTableTrigger;

impl Rule for Qual006MutatingTableTrigger {
    fn id(&self) -> &'static str {
        "QUAL006"
    }
    fn default_severity(&self) -> Severity {
        Severity::High
    }
    fn description(&self) -> &'static str {
        "Row-level trigger references its own base table (ORA-04091 mutating-table hazard)"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::MutatingTableTrigger]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::MutatingTableTrigger) {
            let FactPayload::MutatingTableTrigger {
                unit_logical_id,
                table,
            } = &fact.payload
            else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "Row-level trigger `{unit_logical_id}` references its own base table \
                     `{table}` — ORA-04091 mutating-table error at run time"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Restructure with a compound trigger (collect in BEFORE STATEMENT / apply \
                     in AFTER STATEMENT) or move the logic out of the row-level trigger."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **QUAL002 — error logged then swallowed.** An exception handler
/// that instruments/logs but neither re-raises nor signals records
/// the failure yet lets the program continue as if nothing went
/// wrong — diagnosable but still a swallowed error. `Medium`.
///
/// /oracle: `DATABASE-REFERENCE.md` — exception propagation; log
/// *and* re-raise is the canonical guidance.
pub struct Qual002LogWithoutReraise;

impl Rule for Qual002LogWithoutReraise {
    fn id(&self) -> &'static str {
        "QUAL002"
    }
    fn default_severity(&self) -> Severity {
        Severity::Medium
    }
    fn description(&self) -> &'static str {
        "Exception handler logs but does not re-raise — the error is recorded then swallowed"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::LogWithoutReraise]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::LogWithoutReraise) {
            let FactPayload::LogWithoutReraise { unit_logical_id } = &fact.payload else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "`{unit_logical_id}` logs an exception but does not re-raise — the failure \
                     is swallowed and execution continues"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "After logging, `RAISE;` (or RAISE_APPLICATION_ERROR) so the caller sees \
                     the failure; only swallow when the recovery is deliberate and documented."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **DEP001 — cross-schema write.** A DML statement whose target
/// is schema-qualified to a schema other than the unit's own
/// widens the write blast radius and the privilege surface.
/// `Medium`, review-required.
///
/// /oracle: `SECURITY-OPTIONS-REFERENCE.md` — least-privilege /
/// cross-schema write surface; `DATABASE-REFERENCE.md` — object
/// privileges.
pub struct Dep001CrossSchemaWrite;

impl Rule for Dep001CrossSchemaWrite {
    fn id(&self) -> &'static str {
        "DEP001"
    }
    fn default_severity(&self) -> Severity {
        Severity::Medium
    }
    fn description(&self) -> &'static str {
        "DML writes to an object in a different schema (cross-schema write surface)"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::CrossSchemaWrite]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::CrossSchemaWrite) {
            let FactPayload::CrossSchemaWrite {
                unit_logical_id,
                target,
            } = &fact.payload
            else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "`{unit_logical_id}` writes to `{target}` in another schema — confirm the \
                     grant is intended and minimally scoped"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Write through an owned API in the target schema (definer's-rights \
                     procedure) instead of direct cross-schema DML, and grant only that."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **SEC005 — sensitive object exposed via PUBLIC SYNONYM.** A
/// public synonym is resolvable by every database account; routing
/// a credential/PII/finance object through one widens its reach
/// and the attack surface. `High`, review-required (the underlying
/// grant still gates access, but the exposure is a smell worth a
/// deliberate decision).
///
/// /oracle: `SECURITY-OPTIONS-REFERENCE.md` — least-exposure /
/// PUBLIC visibility; `LOW-LEVEL-CATALOGS.md` — `ALL_SYNONYMS`
/// (`OWNER = 'PUBLIC'`).
pub struct Sec005SensitivePublicSynonym;

impl Rule for Sec005SensitivePublicSynonym {
    fn id(&self) -> &'static str {
        "SEC005"
    }
    fn default_severity(&self) -> Severity {
        Severity::High
    }
    fn description(&self) -> &'static str {
        "Sensitive object exposed through a PUBLIC SYNONYM (visible to every account)"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::SensitivePublicSynonym]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::SensitivePublicSynonym) {
            let FactPayload::SensitivePublicSynonym {
                unit_logical_id,
                synonym,
                target,
            } = &fact.payload
            else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "PUBLIC SYNONYM `{synonym}` in `{unit_logical_id}` exposes sensitive object \
                     `{target}` to every database account"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Drop the public synonym and use a private synonym (or schema-qualified \
                     reference) granted only to the roles that need it; keep sensitive objects \
                     off PUBLIC."
                        .to_string(),
                ),
                ..f
            });
        }
        out
    }
}

/// **PERF003 — `IS NULL` on an indexed column.** A B-tree index
/// does not store all-NULL keys, so `WHERE col IS NULL` cannot use
/// an index on `col` — the query silently full-scans despite the
/// index existing. Fires on the `IsNullOnIndexedColumn` fact (the
/// substrate already correlated an in-source `CREATE INDEX` with
/// the `IS NULL` predicate; catalog-only indexes are out of that
/// source-level scope). `Medium`.
///
/// /oracle: `DATABASE-REFERENCE.md` — B-tree index NULL semantics;
/// function-based / NVL-bearing indexes as the workaround.
pub struct Perf003IsNullOnIndexedColumn;

impl Rule for Perf003IsNullOnIndexedColumn {
    fn id(&self) -> &'static str {
        "PERF003"
    }
    fn default_severity(&self) -> Severity {
        Severity::Medium
    }
    fn description(&self) -> &'static str {
        "IS NULL on an indexed column cannot use the B-tree index (silent full scan)"
    }
    fn required_facts(&self) -> &'static [FactKind] {
        &[FactKind::IsNullOnIndexedColumn]
    }
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::IsNullOnIndexedColumn) {
            let FactPayload::IsNullOnIndexedColumn {
                unit_logical_id,
                column,
            } = &fact.payload
            else {
                continue;
            };
            let f = finding(
                self.id(),
                self.default_severity(),
                &format!(
                    "`{unit_logical_id}` has `{column} IS NULL` but `{column}` is indexed — a \
                     plain B-tree index cannot serve IS NULL, so this silently full-scans"
                ),
                ctx.source_file_for_fact(fact),
                0,
                (0, 0),
            );
            out = out.finding(Finding {
                remediation: Some(
                    "Add a function-based index covering the NULL case (e.g. on `NVL(col,\
                     sentinel)`) and query with the same expression, or restructure to avoid \
                     the IS NULL predicate on the indexed column."
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
    use plsql_ir::{
        FactProvenance, FactStore, FlowEnv, FlowQuery, TaintSources, emit_flow_env_facts, mint_fact,
    };
    use serde::Serialize;

    fn prov() -> FactProvenance {
        FactProvenance {
            component: "test".to_string(),
            component_version: "0".to_string(),
            run_id: String::new(),
            source_logical_id: None,
            source_file: None,
        }
    }

    fn priv_fact(grantee: &str, privilege: &str, on: &str) -> plsql_ir::Fact {
        mint_fact(
            prov(),
            FactPayload::Privilege {
                grantee: grantee.to_string(),
                privilege: privilege.to_string(),
                on: on.to_string(),
            },
        )
    }

    #[test]
    fn flags_grant_to_public() {
        let mut facts = FactStore::default();
        facts.push(priv_fact("PUBLIC", "SELECT", "HR.EMPLOYEES"));
        facts.push(priv_fact("REPORTING", "SELECT", "HR.EMPLOYEES"));
        let env = FlowEnv::default();
        let units = [ScanUnit {
            unit_logical_id: "hr",
            source_file: "hr.sql",
            flow: &env,
        }];
        let snap = CompletenessSnapshot {
            catalog_available: true,
            ..CompletenessSnapshot::default()
        };
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec006GrantToPublic)];
        let r = run_scan(&rules, &units, &facts, &snap);
        assert_eq!(r.findings.len(), 1, "only the PUBLIC grant is flagged");
        assert_eq!(r.findings[0].rule_id, "SEC006");
        assert_eq!(r.findings[0].severity, Severity::High);
        assert!(r.findings[0].message.contains("PUBLIC"));
        assert!(
            r.findings[0]
                .remediation
                .as_ref()
                .unwrap()
                .contains("Revoke")
        );
    }

    #[test]
    fn case_insensitive_public_grantee() {
        let mut facts = FactStore::default();
        facts.push(priv_fact("public", "EXECUTE", "APP.PKG"));
        let env = FlowEnv::default();
        let units = [ScanUnit {
            unit_logical_id: "u",
            source_file: "u.sql",
            flow: &env,
        }];
        let snap = CompletenessSnapshot {
            catalog_available: true,
            ..CompletenessSnapshot::default()
        };
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec006GrantToPublic)];
        let r = run_scan(&rules, &units, &facts, &snap);
        assert_eq!(r.findings.len(), 1);
    }

    #[test]
    fn no_facts_means_harness_gates_rule_not_a_false_finding() {
        let facts = FactStore::default();
        let env = FlowEnv::default();
        let units = [ScanUnit {
            unit_logical_id: "u",
            source_file: "u.sql",
            flow: &env,
        }];
        let snap = CompletenessSnapshot {
            catalog_available: true,
            ..CompletenessSnapshot::default()
        };
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec006GrantToPublic)];
        let r = run_scan(&rules, &units, &facts, &snap);
        assert!(r.findings.is_empty());
        assert_eq!(
            r.rules_gated, 1,
            "no Privilege facts -> required-facts gate"
        );
    }

    #[test]
    fn without_catalog_rule_is_gated_never_run_blind() {
        let mut facts = FactStore::default();
        facts.push(priv_fact("PUBLIC", "SELECT", "HR.T"));
        let env = FlowEnv::default();
        let units = [ScanUnit {
            unit_logical_id: "u",
            source_file: "u.sql",
            flow: &env,
        }];
        let snap = CompletenessSnapshot {
            catalog_available: false,
            ..CompletenessSnapshot::default()
        };
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec006GrantToPublic)];
        let r = run_scan(&rules, &units, &facts, &snap);
        assert!(r.findings.is_empty());
        assert_eq!(r.rules_gated, 1);
        assert_eq!(r.skipped[0].reason, crate::SkipReason::PreconditionUnmet);
    }

    // ---- SEC001 ------------------------------------------------

    fn dynsql_fact(site: &str) -> plsql_ir::Fact {
        mint_fact(
            prov(),
            FactPayload::DynamicSqlEvidence {
                site: site.to_string(),
            },
        )
    }

    fn opacity_fact(target: &str) -> plsql_ir::Fact {
        mint_fact(
            prov(),
            FactPayload::Opacity {
                target_logical_id: target.to_string(),
                reason: "DBMS_SQL".to_string(),
            },
        )
    }

    fn shape_fact(unit: &str, name: &str, shape: StringShape) -> plsql_ir::Fact {
        mint_fact(
            prov(),
            FactPayload::StringShape {
                unit_logical_id: unit.to_string(),
                name: name.trim().to_ascii_uppercase(),
                shape,
            },
        )
    }

    #[derive(Debug, Serialize, PartialEq, Eq)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum SastEquivalenceRow {
        Finding {
            rule_id: String,
            severity: Severity,
            message: String,
            file: String,
            line: u32,
            byte_span: (u32, u32),
            confidence: String,
            remediation: Option<String>,
        },
        Skip {
            rule_id: String,
            unit: String,
            reason: crate::SkipReason,
            detail: String,
        },
    }

    #[derive(Debug, Serialize, PartialEq, Eq)]
    struct SastReportProjection {
        rules_run: usize,
        rules_gated: usize,
        rows: Vec<SastEquivalenceRow>,
    }

    fn project_report(report: &crate::ScanReport) -> SastReportProjection {
        let mut rows = Vec::new();
        rows.extend(
            report
                .findings
                .iter()
                .map(|finding| SastEquivalenceRow::Finding {
                    rule_id: finding.rule_id.clone(),
                    severity: finding.severity,
                    message: finding.message.clone(),
                    file: finding.location.file.clone(),
                    line: finding.location.line,
                    byte_span: finding.location.byte_span,
                    confidence: format!("{:?}", finding.confidence.level),
                    remediation: finding.remediation.clone(),
                }),
        );
        rows.extend(report.skipped.iter().map(|skip| SastEquivalenceRow::Skip {
            rule_id: skip.rule_id.clone(),
            unit: skip.unit.clone(),
            reason: skip.reason,
            detail: skip.detail.clone(),
        }));
        SastReportProjection {
            rules_run: report.rules_run,
            rules_gated: report.rules_gated,
            rows,
        }
    }

    fn dynamic_sql_sites(facts: &FactStore) -> Vec<&str> {
        facts
            .by_kind(FactKind::DynamicSqlEvidence)
            .filter_map(|fact| match &fact.payload {
                FactPayload::DynamicSqlEvidence { site } => Some(site.as_str()),
                _ => None,
            })
            .collect()
    }

    fn opaque_sites(facts: &FactStore) -> std::collections::BTreeSet<&str> {
        facts
            .by_kind(FactKind::Opacity)
            .filter_map(|fact| match &fact.payload {
                FactPayload::Opacity {
                    target_logical_id, ..
                } => Some(target_logical_id.as_str()),
                _ => None,
            })
            .collect()
    }

    fn legacy_sec001_output(ctx: &ScanContext<'_>) -> RuleOutput {
        let rule = Sec001ExecuteImmediateInjection;
        let mut out = RuleOutput::default();
        let opaque = opaque_sites(ctx.facts);

        for site in dynamic_sql_sites(ctx.facts) {
            if opaque.contains(site) {
                out = out.skip(ctx.skip(
                    rule.id(),
                    crate::SkipReason::OpaqueConstruct,
                    &format!("dynamic-SQL site `{site}` is opaque; cannot prove safety"),
                ));
                continue;
            }

            let taint = ctx.flow.taint_of(site);
            if taint.is_tainted {
                let Some((taint_fact_id, _)) = taint_fact_for(ctx, site) else {
                    out = out.skip(ctx.skip(
                        rule.id(),
                        crate::SkipReason::MissingFlowFacts,
                        &format!("no TaintFact for dynamic-SQL site `{site}`"),
                    ));
                    continue;
                };
                let kinds = taint
                    .kinds
                    .iter()
                    .map(|k| format!("{k:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let f = finding(
                    rule.id(),
                    rule.default_severity(),
                    &format!(
                        "Uncleansed tainted value ({kinds}) reaches EXECUTE IMMEDIATE at `{site}` \
                         (TaintFact {})",
                        taint_fact_id.0
                    ),
                    ctx.source_file,
                    0,
                    (0, 0),
                );
                out = out.finding(Finding {
                    remediation: Some(
                        "Bind user input with USING placeholders, or validate via \
                         DBMS_ASSERT before concatenation."
                            .to_string(),
                    ),
                    ..f
                });
                continue;
            }

            match ctx.flow.string_shape_of(site) {
                Some(StringShape::Literal { .. }) | Some(StringShape::Empty) => {}
                Some(StringShape::InterpolatedWithFix { .. }) | Some(StringShape::FullyOpaque) => {}
                None => {
                    out = out.skip(ctx.skip(
                        rule.id(),
                        crate::SkipReason::MissingFlowFacts,
                        &format!("no StringShape/Taint facts for dynamic-SQL site `{site}`"),
                    ));
                }
            }
        }
        out
    }

    fn legacy_sec002_output(ctx: &ScanContext<'_>) -> RuleOutput {
        let rule = Sec002DbmsSqlParse;
        let mut out = RuleOutput::default();
        for fact in ctx.facts.by_kind(FactKind::Opacity) {
            let FactPayload::Opacity {
                target_logical_id,
                reason,
            } = &fact.payload
            else {
                continue;
            };
            if !reason.to_ascii_uppercase().contains("DBMS_SQL") {
                continue;
            }
            let evidence = ctx
                .flow
                .string_shape_of(target_logical_id)
                .and_then(|shape| {
                    string_shape_fact_for(ctx, target_logical_id)
                        .map(|(id, _)| format!("StringShapeFact {} ({shape:?})", id.0))
                })
                .unwrap_or_else(|| format!("OpacityFact {}", fact.id.0));
            let f = finding(
                rule.id(),
                rule.default_severity(),
                &format!(
                    "DBMS_SQL dynamic SQL at `{target_logical_id}` is opaque to taint \
                     analysis ({reason}); injection cannot be ruled out automatically \
                     ({evidence})"
                ),
                ctx.source_file,
                0,
                (0, 0),
            );
            let mut downgraded = f;
            downgraded.confidence = plsql_core::Confidence {
                level: plsql_core::ConfidenceLevel::Medium,
                explanation: Some(
                    "opaque DBMS_SQL surface; injection unproven but unanalysable".to_string(),
                ),
            };
            downgraded.remediation = Some(
                "Prefer EXECUTE IMMEDIATE with USING binds, or bind every DBMS_SQL value \
                 via DBMS_SQL.BIND_VARIABLE; manually review this site."
                    .to_string(),
            );
            out = out.finding(downgraded);
        }
        out
    }

    fn legacy_scan(unit: &ScanUnit<'_>, facts: &FactStore) -> crate::ScanReport {
        let ctx = ScanContext::new(
            unit.unit_logical_id,
            unit.source_file,
            FlowQuery::new(unit.flow),
            facts,
        );
        let mut report = crate::ScanReport::default();
        for (rule_id, required_kind, output) in [
            (
                "SEC001",
                FactKind::DynamicSqlEvidence,
                legacy_sec001_output(&ctx),
            ),
            ("SEC002", FactKind::Opacity, legacy_sec002_output(&ctx)),
        ] {
            if facts.by_kind(required_kind).next().is_none() {
                report.skipped.push(crate::RuleSkippedDiagnostic {
                    rule_id: rule_id.to_string(),
                    unit: "<analysis-run>".to_string(),
                    reason: crate::SkipReason::MissingFlowFacts,
                    detail: format!("no facts of required kind {required_kind:?}"),
                });
                report.rules_gated += 1;
            } else {
                report.rules_run += 1;
                report.findings.extend(output.findings);
                report.skipped.extend(output.skipped);
            }
        }
        report
    }

    struct SastEquivalenceCase {
        name: &'static str,
        unit: &'static str,
        source: &'static str,
        user_input_names: &'static [&'static str],
        dynamic_site: Option<&'static str>,
        opacity: Option<(&'static str, &'static str)>,
    }

    fn build_sast_case(case: &SastEquivalenceCase) -> (FlowEnv, FactStore) {
        let stmts = plsql_ir::lower_statement_body(case.source);
        let env = plsql_ir::analyze_flow(
            &stmts,
            &TaintSources {
                user_input_names: case
                    .user_input_names
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect(),
                bind_names: vec![],
            },
        );
        let mut facts = FactStore::default();
        if let Some(site) = case.dynamic_site {
            facts.push(dynsql_fact(site));
        }
        if let Some((target, reason)) = case.opacity {
            facts.push(opacity_reason(target, reason));
        }
        emit_flow_env_facts(&mut facts, &prov(), case.unit, &env);
        (env, facts)
    }

    #[test]
    fn public_synthetic_sast_outputs_match_legacy_flow_oracle() -> Result<(), serde_json::Error> {
        // Coverage mirrors corpus/synthetic/l2: no-bind EXECUTE IMMEDIATE,
        // static EXECUTE IMMEDIATE, DBMS_ASSERT-sanitized EXECUTE IMMEDIATE,
        // and DBMS_SQL opacity. Keep this floor monotone until the release-wide
        // coverage_index seed lands.
        const EXPECTED_EQUIVALENCE_CASES: usize = 4;
        let cases = [
            SastEquivalenceCase {
                name: "pkg_execute_immediate.update_no_binds",
                unit: "pkg_execute_immediate_demo.update_no_binds",
                source: "v_sql := 'UPDATE invoices SET amount = ' || p_amount || ' WHERE id = ' || p_id;",
                user_input_names: &["p_amount", "p_id"],
                dynamic_site: Some("V_SQL"),
                opacity: None,
            },
            SastEquivalenceCase {
                name: "pkg_execute_immediate.truncate_static_string",
                unit: "pkg_execute_immediate_demo.truncate_static_string",
                source: "v_sql := 'TRUNCATE TABLE staging_invoices';",
                user_input_names: &[],
                dynamic_site: Some("V_SQL"),
                opacity: None,
            },
            SastEquivalenceCase {
                name: "pkg_dbms_assert.delete_from_table",
                unit: "pkg_dbms_assert_demo.delete_from_table",
                source: "v_sql := 'DELETE FROM ' || dbms_assert.sql_object_name(p_table);",
                user_input_names: &["p_table"],
                dynamic_site: Some("V_SQL"),
                opacity: None,
            },
            SastEquivalenceCase {
                name: "dbms_sql.parse_opaque_surface",
                unit: "pkg_execute_immediate_demo.dbms_sql_surface",
                source: "sql_text := 'SELECT * FROM ' || p_table;",
                user_input_names: &["p_table"],
                dynamic_site: None,
                opacity: Some(("SQL_TEXT", "dynamic SQL via DBMS_SQL.PARSE")),
            },
        ];
        assert_eq!(cases.len(), EXPECTED_EQUIVALENCE_CASES);

        for case in cases {
            let (env, facts) = build_sast_case(&case);
            let units = [ScanUnit {
                unit_logical_id: case.unit,
                source_file: "corpus/synthetic/l2/equivalence.sql",
                flow: &env,
            }];
            let legacy_unit = ScanUnit {
                unit_logical_id: case.unit,
                source_file: "corpus/synthetic/l2/equivalence.sql",
                flow: &env,
            };
            let fact_rules: Vec<Box<dyn Rule>> = vec![
                Box::new(Sec001ExecuteImmediateInjection),
                Box::new(Sec002DbmsSqlParse),
            ];
            let fact_report = run_scan(
                &fact_rules,
                &units,
                &facts,
                &CompletenessSnapshot::default(),
            );
            let legacy_report = legacy_scan(&legacy_unit, &facts);

            let fact_bytes = serde_json::to_string(&project_report(&fact_report))?;
            let legacy_bytes = serde_json::to_string(&project_report(&legacy_report))?;
            assert_eq!(fact_bytes, legacy_bytes, "case {}", case.name);
        }
        Ok(())
    }

    #[test]
    fn sec001_flags_tainted_value_reaching_execute_immediate() {
        let stmts = plsql_ir::lower_statement_body("dyn := p_user || ' x';");
        let env = plsql_ir::analyze_flow(
            &stmts,
            &plsql_ir::TaintSources {
                user_input_names: vec!["P_USER".to_string()],
                bind_names: vec![],
            },
        );
        let mut facts = FactStore::default();
        facts.push(dynsql_fact("DYN"));
        emit_flow_env_facts(&mut facts, &prov(), "hr.proc", &env);
        let taint_fact_id = facts
            .by_kind(FactKind::Taint)
            .find_map(|fact| match &fact.payload {
                FactPayload::Taint {
                    unit_logical_id,
                    name,
                    ..
                } if unit_logical_id == "hr.proc" && name == "DYN" => Some(fact.id.0.clone()),
                _ => None,
            })
            .expect("flow projection must emit a DYN taint fact");
        let empty_env = FlowEnv::default();
        let units = [ScanUnit {
            unit_logical_id: "hr.proc",
            source_file: "hr.sql",
            flow: &empty_env,
        }];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec001ExecuteImmediateInjection)];
        let r = run_scan(&rules, &units, &facts, &CompletenessSnapshot::default());
        assert_eq!(r.findings.len(), 1, "tainted DYN -> injection");
        assert_eq!(r.findings[0].rule_id, "SEC001");
        assert_eq!(r.findings[0].severity, Severity::Critical);
        assert!(
            r.findings[0].message.contains(&taint_fact_id),
            "finding must cite the stable TaintFact id"
        );
        assert!(
            r.findings[0]
                .remediation
                .as_ref()
                .unwrap()
                .contains("USING")
        );
    }

    #[test]
    fn sec001_opaque_site_is_skipped_not_asserted_safe() {
        let env = FlowEnv::default();
        let mut facts = FactStore::default();
        facts.push(dynsql_fact("BLOB_SQL"));
        facts.push(opacity_fact("BLOB_SQL"));
        let units = [ScanUnit {
            unit_logical_id: "u",
            source_file: "u.sql",
            flow: &env,
        }];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec001ExecuteImmediateInjection)];
        let r = run_scan(&rules, &units, &facts, &CompletenessSnapshot::default());
        assert!(r.findings.is_empty());
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].reason, crate::SkipReason::OpaqueConstruct);
    }

    #[test]
    fn sec001_no_flow_evidence_is_skipped_r13() {
        let env = FlowEnv::default();
        let mut facts = FactStore::default();
        facts.push(dynsql_fact("UNKNOWN_SITE"));
        let units = [ScanUnit {
            unit_logical_id: "u",
            source_file: "u.sql",
            flow: &env,
        }];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec001ExecuteImmediateInjection)];
        let r = run_scan(&rules, &units, &facts, &CompletenessSnapshot::default());
        assert!(r.findings.is_empty(), "no evidence != false positive");
        assert_eq!(r.skipped[0].reason, crate::SkipReason::MissingFlowFacts);
    }

    #[test]
    fn sec001_literal_sql_is_safe_no_finding_no_skip() {
        let stmts = plsql_ir::lower_statement_body("dyn := 'SELECT 1 FROM dual';");
        let env = plsql_ir::analyze_flow(
            &stmts,
            &plsql_ir::TaintSources {
                user_input_names: vec![],
                bind_names: vec![],
            },
        );
        let mut facts = FactStore::default();
        facts.push(dynsql_fact("DYN"));
        emit_flow_env_facts(&mut facts, &prov(), "u", &env);
        let empty_env = FlowEnv::default();
        let units = [ScanUnit {
            unit_logical_id: "u",
            source_file: "u.sql",
            flow: &empty_env,
        }];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec001ExecuteImmediateInjection)];
        let r = run_scan(&rules, &units, &facts, &CompletenessSnapshot::default());
        assert!(r.findings.is_empty(), "constant SQL is safe");
        assert!(r.skipped.is_empty(), "literal is provably safe, no skip");
    }

    #[test]
    fn sec001_required_facts_gate_when_no_dynamic_sql() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let units = [ScanUnit {
            unit_logical_id: "u",
            source_file: "u.sql",
            flow: &env,
        }];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec001ExecuteImmediateInjection)];
        let r = run_scan(&rules, &units, &facts, &CompletenessSnapshot::default());
        assert_eq!(r.rules_gated, 1);
        assert!(r.findings.is_empty());
    }

    // ---- SEC002 ------------------------------------------------

    fn opacity_reason(target: &str, reason: &str) -> plsql_ir::Fact {
        mint_fact(
            prov(),
            FactPayload::Opacity {
                target_logical_id: target.to_string(),
                reason: reason.to_string(),
            },
        )
    }

    #[test]
    fn sec002_flags_dbms_sql_opaque_site_as_review() {
        let env = FlowEnv::default();
        let mut facts = FactStore::default();
        facts.push(opacity_reason("sql_text", "dynamic SQL via DBMS_SQL.PARSE"));
        let shape = shape_fact(
            "hr.proc",
            "sql_text",
            StringShape::InterpolatedWithFix {
                literal_prefix: "select * from ".to_string(),
                literal_suffix: String::new(),
            },
        );
        let shape_fact_id = shape.id.0.clone();
        facts.push(shape);
        let units = [ScanUnit {
            unit_logical_id: "hr.proc",
            source_file: "hr.sql",
            flow: &env,
        }];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec002DbmsSqlParse)];
        let r = run_scan(&rules, &units, &facts, &CompletenessSnapshot::default());
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].rule_id, "SEC002");
        assert_eq!(r.findings[0].severity, Severity::Medium);
        assert!(
            r.findings[0].message.contains(&shape_fact_id),
            "review finding must cite the stable StringShapeFact id"
        );
        assert_eq!(
            r.findings[0].confidence.level,
            plsql_core::ConfidenceLevel::Medium,
            "DBMS_SQL surface is review-queue, not must-fix"
        );
    }

    #[test]
    fn sec002_ignores_non_dbms_sql_opacity() {
        let env = FlowEnv::default();
        let mut facts = FactStore::default();
        facts.push(opacity_reason("x", "remote object via DB link"));
        let units = [ScanUnit {
            unit_logical_id: "u",
            source_file: "u.sql",
            flow: &env,
        }];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec002DbmsSqlParse)];
        let r = run_scan(&rules, &units, &facts, &CompletenessSnapshot::default());
        assert!(r.findings.is_empty(), "only DBMS_SQL opacity is SEC002");
    }

    #[test]
    fn sec002_required_facts_gate_without_opacity() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let units = [ScanUnit {
            unit_logical_id: "u",
            source_file: "u.sql",
            flow: &env,
        }];
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(Sec002DbmsSqlParse)];
        let r = run_scan(&rules, &units, &facts, &CompletenessSnapshot::default());
        assert_eq!(r.rules_gated, 1);
        assert!(r.findings.is_empty());
    }

    // --- PERF001 / PERF002 / STYLE001 (kcjx-fact-backed) ---

    fn cfl_fact(unit: &str, loop_var: &str, has_body_dml: bool) -> plsql_ir::Fact {
        mint_fact(
            prov(),
            FactPayload::CursorForLoop {
                unit_logical_id: unit.to_string(),
                loop_var: loop_var.to_string(),
                has_body_dml,
            },
        )
    }

    fn mi_fact(unit: &str) -> plsql_ir::Fact {
        mint_fact(
            prov(),
            FactPayload::MissingInstrumentation {
                unit_logical_id: unit.to_string(),
            },
        )
    }

    fn one_unit_scan(rule: Box<dyn Rule>, facts: &FactStore) -> crate::ScanReport {
        let env = FlowEnv::default();
        let units = [ScanUnit {
            unit_logical_id: "hr.pkg.p",
            source_file: "hr.sql",
            flow: &env,
        }];
        run_scan(&[rule], &units, facts, &CompletenessSnapshot::default())
    }

    #[test]
    fn perf001_flags_every_cursor_for_loop() {
        let mut facts = FactStore::default();
        facts.push(cfl_fact("hr.pkg.p", "r", false));
        facts.push(cfl_fact("hr.pkg.p", "rec", true));
        let rep = one_unit_scan(Box::new(Perf001CursorForLoopBulkCollect), &facts);
        assert_eq!(rep.findings.len(), 2, "PERF001 fires on every cursor loop");
        assert!(
            rep.findings
                .iter()
                .all(|f| f.rule_id.as_str().eq("PERF001"))
        );
        assert!(rep.findings[0].message.contains("BULK COLLECT"));
    }

    #[test]
    fn perf002_only_flags_cursor_loops_with_body_dml() {
        let mut facts = FactStore::default();
        facts.push(cfl_fact("hr.pkg.p", "r_nodml", false));
        facts.push(cfl_fact("hr.pkg.p", "r_dml", true));
        let rep = one_unit_scan(Box::new(Perf002CursorForLoopForall), &facts);
        assert_eq!(rep.findings.len(), 1, "only the has_body_dml loop");
        assert_eq!(rep.findings[0].rule_id, "PERF002");
        assert!(rep.findings[0].message.contains("FORALL"));
        assert!(rep.findings[0].message.contains("r_dml"));
    }

    #[test]
    fn style001_flags_missing_instrumentation() {
        let mut facts = FactStore::default();
        facts.push(mi_fact("hr.pkg.silent"));
        let rep = one_unit_scan(Box::new(Style001MissingInstrumentation), &facts);
        assert_eq!(rep.findings.len(), 1);
        assert_eq!(rep.findings[0].rule_id, "STYLE001");
        assert_eq!(rep.findings[0].severity, Severity::Info);
        assert!(rep.findings[0].message.contains("hr.pkg.silent"));
    }

    #[test]
    fn perf_and_style_rules_gate_without_their_facts() {
        // R13: no CursorForLoop / MissingInstrumentation facts ⇒
        // each rule is gated (typed), never a silent pass.
        let facts = FactStore::default();
        for rule in [
            Box::new(Perf001CursorForLoopBulkCollect) as Box<dyn Rule>,
            Box::new(Perf002CursorForLoopForall),
            Box::new(Style001MissingInstrumentation),
        ] {
            let rep = one_unit_scan(rule, &facts);
            assert_eq!(rep.rules_gated, 1);
            assert!(rep.findings.is_empty());
        }
    }

    // --- SEC003 hardcoded credential ---

    fn cred_fact(unit: &str, marker: &str) -> plsql_ir::Fact {
        mint_fact(
            prov(),
            FactPayload::HardcodedCredential {
                unit_logical_id: unit.to_string(),
                marker: marker.to_string(),
            },
        )
    }

    #[test]
    fn sec003_flags_hardcoded_credential_as_critical() {
        let mut facts = FactStore::default();
        facts.push(cred_fact("hr.admin", "identified by"));
        let rep = one_unit_scan(Box::new(Sec003HardcodedCredentials), &facts);
        assert_eq!(rep.findings.len(), 1);
        assert_eq!(rep.findings[0].rule_id, "SEC003");
        assert_eq!(rep.findings[0].severity, Severity::Critical);
        assert!(rep.findings[0].message.contains("identified by"));
        assert!(
            rep.findings[0]
                .remediation
                .as_ref()
                .unwrap()
                .contains("wallet")
        );
    }

    #[test]
    fn sec003_gates_without_credential_facts() {
        let facts = FactStore::default();
        let rep = one_unit_scan(Box::new(Sec003HardcodedCredentials), &facts);
        assert_eq!(rep.rules_gated, 1);
        assert!(rep.findings.is_empty());
    }

    // --- SEC004 invoker's rights ---

    #[test]
    fn sec004_flags_invoker_rights() {
        let mut facts = FactStore::default();
        facts.push(mint_fact(
            prov(),
            FactPayload::InvokerRights {
                unit_logical_id: "hr.pkg".to_string(),
            },
        ));
        let rep = one_unit_scan(Box::new(Sec004InvokerRights), &facts);
        assert_eq!(rep.findings.len(), 1);
        assert_eq!(rep.findings[0].rule_id, "SEC004");
        assert_eq!(rep.findings[0].severity, Severity::Medium);
        assert!(rep.findings[0].message.contains("AUTHID CURRENT_USER"));
    }

    #[test]
    fn sec004_gates_without_invoker_rights_facts() {
        let facts = FactStore::default();
        let rep = one_unit_scan(Box::new(Sec004InvokerRights), &facts);
        assert_eq!(rep.rules_gated, 1);
        assert!(rep.findings.is_empty());
    }

    // --- SEC007 / QUAL007 / QUAL003 ---

    #[test]
    fn sec007_qual007_qual003_fire_on_their_facts() {
        let mut facts = FactStore::default();
        facts.push(mint_fact(
            prov(),
            FactPayload::RefCursorReturn {
                unit_logical_id: "hr.f".into(),
            },
        ));
        facts.push(mint_fact(
            prov(),
            FactPayload::DmlInFunction {
                unit_logical_id: "hr.g".into(),
            },
        ));
        facts.push(mint_fact(
            prov(),
            FactPayload::UnboundedBulkCollect {
                unit_logical_id: "hr.p".into(),
            },
        ));
        let s7 = one_unit_scan(Box::new(Sec007RefCursorReturn), &facts);
        assert_eq!(s7.findings.len(), 1);
        assert_eq!(s7.findings[0].rule_id, "SEC007");
        let q7 = one_unit_scan(Box::new(Qual007DmlInFunction), &facts);
        assert_eq!(q7.findings.len(), 1);
        assert_eq!(q7.findings[0].rule_id, "QUAL007");
        let q3 = one_unit_scan(Box::new(Qual003UnboundedBulkCollect), &facts);
        assert_eq!(q3.findings.len(), 1);
        assert_eq!(q3.findings[0].rule_id, "QUAL003");
    }

    #[test]
    fn sec007_qual007_qual003_gate_without_facts() {
        let facts = FactStore::default();
        for rule in [
            Box::new(Sec007RefCursorReturn) as Box<dyn Rule>,
            Box::new(Qual007DmlInFunction),
            Box::new(Qual003UnboundedBulkCollect),
        ] {
            let rep = one_unit_scan(rule, &facts);
            assert_eq!(rep.rules_gated, 1);
            assert!(rep.findings.is_empty());
        }
    }

    // --- QUAL005 / QUAL008 ---

    #[test]
    fn qual005_qual008_fire_and_gate() {
        let mut facts = FactStore::default();
        facts.push(mint_fact(
            prov(),
            FactPayload::DeprecatedFeature {
                unit_logical_id: "hr.p".into(),
                feature: "dbms_job (deprecated)".into(),
            },
        ));
        facts.push(mint_fact(
            prov(),
            FactPayload::DeterministicMisuse {
                unit_logical_id: "hr.f".into(),
                construct: "SYSDATE".into(),
            },
        ));
        let q5 = one_unit_scan(Box::new(Qual005DeprecatedFeature), &facts);
        assert_eq!(q5.findings.len(), 1);
        assert_eq!(q5.findings[0].rule_id, "QUAL005");
        assert!(q5.findings[0].message.contains("dbms_job"));
        let q8 = one_unit_scan(Box::new(Qual008DeterministicMisuse), &facts);
        assert_eq!(q8.findings.len(), 1);
        assert_eq!(q8.findings[0].rule_id, "QUAL008");
        assert!(q8.findings[0].message.contains("SYSDATE"));

        let empty = FactStore::default();
        for rule in [
            Box::new(Qual005DeprecatedFeature) as Box<dyn Rule>,
            Box::new(Qual008DeterministicMisuse),
        ] {
            let rep = one_unit_scan(rule, &empty);
            assert_eq!(rep.rules_gated, 1);
            assert!(rep.findings.is_empty());
        }
    }

    // --- QUAL006 / QUAL002 / DEP001 ---

    #[test]
    fn qual006_qual002_dep001_fire_and_gate() {
        let mut facts = FactStore::default();
        facts.push(mint_fact(
            prov(),
            FactPayload::MutatingTableTrigger {
                unit_logical_id: "hr.trg".into(),
                table: "employees".into(),
            },
        ));
        facts.push(mint_fact(
            prov(),
            FactPayload::LogWithoutReraise {
                unit_logical_id: "hr.p".into(),
            },
        ));
        facts.push(mint_fact(
            prov(),
            FactPayload::CrossSchemaWrite {
                unit_logical_id: "hr.p".into(),
                target: "fin.ledger".into(),
            },
        ));
        let q6 = one_unit_scan(Box::new(Qual006MutatingTableTrigger), &facts);
        assert_eq!(q6.findings.len(), 1);
        assert_eq!(q6.findings[0].rule_id, "QUAL006");
        assert!(q6.findings[0].message.contains("employees"));
        let q2 = one_unit_scan(Box::new(Qual002LogWithoutReraise), &facts);
        assert_eq!(q2.findings.len(), 1);
        assert_eq!(q2.findings[0].rule_id, "QUAL002");
        let d1 = one_unit_scan(Box::new(Dep001CrossSchemaWrite), &facts);
        assert_eq!(d1.findings.len(), 1);
        assert_eq!(d1.findings[0].rule_id, "DEP001");
        assert!(d1.findings[0].message.contains("fin.ledger"));

        let empty = FactStore::default();
        for rule in [
            Box::new(Qual006MutatingTableTrigger) as Box<dyn Rule>,
            Box::new(Qual002LogWithoutReraise),
            Box::new(Dep001CrossSchemaWrite),
        ] {
            let rep = one_unit_scan(rule, &empty);
            assert_eq!(rep.rules_gated, 1);
            assert!(rep.findings.is_empty());
        }
    }

    // --- SEC005 ---

    #[test]
    fn sec005_fires_and_gates() {
        let mut facts = FactStore::default();
        facts.push(mint_fact(
            prov(),
            FactPayload::SensitivePublicSynonym {
                unit_logical_id: "hr.ddl".into(),
                synonym: "emp_pwd".into(),
                target: "hr.employee_passwords".into(),
            },
        ));
        let rep = one_unit_scan(Box::new(Sec005SensitivePublicSynonym), &facts);
        assert_eq!(rep.findings.len(), 1);
        assert_eq!(rep.findings[0].rule_id, "SEC005");
        assert_eq!(rep.findings[0].severity, Severity::High);
        assert!(rep.findings[0].message.contains("employee_passwords"));

        let empty = FactStore::default();
        let g = one_unit_scan(Box::new(Sec005SensitivePublicSynonym), &empty);
        assert_eq!(g.rules_gated, 1);
        assert!(g.findings.is_empty());
    }

    // --- PERF003 ---

    #[test]
    fn perf003_fires_and_gates() {
        let mut facts = FactStore::default();
        facts.push(mint_fact(
            prov(),
            FactPayload::IsNullOnIndexedColumn {
                unit_logical_id: "hr.q".into(),
                column: "deleted_at".into(),
            },
        ));
        let rep = one_unit_scan(Box::new(Perf003IsNullOnIndexedColumn), &facts);
        assert_eq!(rep.findings.len(), 1);
        assert_eq!(rep.findings[0].rule_id, "PERF003");
        assert!(rep.findings[0].message.contains("deleted_at"));

        let empty = FactStore::default();
        let g = one_unit_scan(Box::new(Perf003IsNullOnIndexedColumn), &empty);
        assert_eq!(g.rules_gated, 1);
        assert!(g.findings.is_empty());
    }
}
