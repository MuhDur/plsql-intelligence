//! Emit declaration / reference / call facts plus
//! privilege / dynamic-SQL / unknown facts.
//!
//! Bridges the semantic-layer extractors (calls, dml-edges,
//! privilege model, dynamic-SQL evidence, opacity reasons) and
//! the declaration table into the normalized [`Fact`] stream
//! defined by. Each emitter takes the typed
//! per-family input + a [`FactProvenance`] and pushes minted
//! facts into a [`FactStore`].
//!
//! "With evidence" (FACT-004): the privilege / dynamic-SQL /
//! opacity payloads are deliberately lightweight — the evidence a
//! consumer needs to defend the fact (the grant tuple, the
//! dynamic-SQL site text, the opacity reason) travels *in* the
//! payload string, and richer structured evidence is re-fetched
//! from the originating crate's model by `FactId`.
//!
//! Keeping emission in one module means the engine wiring layer
//! has a single call site per fact family and the
//! `FactId` derivation stays consistent.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   declaration / reference / call grammar 1:1 with the fact
//!   families.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_OBJECTS` (declarations), `ALL_DEPENDENCIES`
//!   (call edges), `ALL_IDENTIFIERS` (references) are the
//!   server-side mirrors.

use std::collections::BTreeSet;

use crate::DeclId;
use crate::calls::CallSite;
use crate::fact::{FactPayload, FactProvenance, FactStore};
use crate::table_stub::DeclLike;

/// Emit one `Declaration` fact per registered declaration.
/// Returns the count emitted (post-dedup).
pub fn emit_declaration_facts<I>(store: &mut FactStore, prov: &FactProvenance, decls: I) -> usize
where
    I: IntoIterator<Item = (DeclId, String)>,
{
    let before = store.len();
    for (decl, logical_id) in decls {
        let f = crate::fact::mint_fact(prov.clone(), FactPayload::Declaration { decl, logical_id });
        store.push(f);
    }
    store.len() - before
}

/// Emit one `Reference` fact per (from_decl, to_logical_id) pair.
pub fn emit_reference_facts<I>(store: &mut FactStore, prov: &FactProvenance, refs: I) -> usize
where
    I: IntoIterator<Item = (DeclId, String)>,
{
    let before = store.len();
    for (from_decl, to_logical_id) in refs {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::Reference {
                from_decl,
                to_logical_id,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// Emit one `DependencyEdge` fact per call site. `from_logical_id`
/// is the routine the call appeared in; the callee path is joined
/// with `.` into the edge target.
pub fn emit_call_facts(
    store: &mut FactStore,
    prov: &FactProvenance,
    from_logical_id: &str,
    calls: &[CallSite],
) -> usize {
    let before = store.len();
    for c in calls {
        let to = c.callee_parts.join(".").to_ascii_lowercase();
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::DependencyEdge {
                from_logical_id: from_logical_id.to_string(),
                to_logical_id: to,
                edge_kind: "Calls".to_string(),
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// Emit one `Privilege` fact per resolved `(grantee, privilege,
/// on)` triple. The triple *is* the evidence:
/// who can do what to which object. Returns the post-dedup count.
pub fn emit_privilege_facts<I>(store: &mut FactStore, prov: &FactProvenance, grants: I) -> usize
where
    I: IntoIterator<Item = (String, String, String)>,
{
    let before = store.len();
    for (grantee, privilege, on) in grants {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::Privilege {
                grantee,
                privilege,
                on,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// Emit one `DynamicSqlEvidence` fact per recognised dynamic-SQL
/// site. `site` carries the evidence — typically
/// the logical id of the unit plus a fragment/classification
/// summary from `DynamicSqlEvidence`.
pub fn emit_dynamic_sql_facts<I>(store: &mut FactStore, prov: &FactProvenance, sites: I) -> usize
where
    I: IntoIterator<Item = String>,
{
    let before = store.len();
    for site in sites {
        let f = crate::fact::mint_fact(prov.clone(), FactPayload::DynamicSqlEvidence { site });
        store.push(f);
    }
    store.len() - before
}

/// Emit one `Opacity` fact per `(target_logical_id, reason)` pair
///  — the "unknown" family. `reason` is the
/// evidence string (typically a stringified `UnknownReason`) so a
/// consumer can explain *why* the analyser could not see through
/// the target.
pub fn emit_unknown_facts<I>(store: &mut FactStore, prov: &FactProvenance, unknowns: I) -> usize
where
    I: IntoIterator<Item = (String, String)>,
{
    let before = store.len();
    for (target_logical_id, reason) in unknowns {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::Opacity {
                target_logical_id,
                reason,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// Convenience: emit a declaration fact for every entry a
/// `DeclLike` source yields. The trait keeps this module free of
/// a hard `plsql-symbols` dependency (which would invert the
/// layer order — symbols depends on ir, not the reverse).
pub fn emit_declarations_from<T: DeclLike>(
    store: &mut FactStore,
    prov: &FactProvenance,
    source: &T,
) -> usize {
    emit_declaration_facts(store, prov, source.iter_decls())
}

/// One detected `EXCEPTION WHEN <scope> THEN <body>` handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExceptionHandlerSite {
    pub unit_logical_id: String,
    /// Caught condition, normalized: `others` or the named
    /// exception text (lowercased, whitespace-collapsed).
    pub scope: String,
    /// `noop` (body is only `NULL;` — QUAL001 swallowed
    /// exception), `commit` / `rollback` (QUAL004 transaction
    /// control in a handler), or `other`.
    pub body_class: String,
}

/// Classify an exception-handler body for the syntactic rules.
///
/// Conservative by design (R13): only an all-`NULL;` body is
/// `noop`; `COMMIT`/`ROLLBACK` anywhere in the body is reported;
/// anything else is `other` (the rule decides what to do, this
/// never asserts safety).
#[must_use]
fn classify_handler_body(body: &str) -> &'static str {
    let norm = body.trim().to_ascii_lowercase();
    let stmts: Vec<&str> = norm
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if stmts.is_empty() || stmts.iter().all(|s| *s == "null") {
        return "noop";
    }
    if stmts
        .iter()
        .any(|s| s == &"commit" || s.starts_with("commit "))
    {
        return "commit";
    }
    if stmts
        .iter()
        .any(|s| s == &"rollback" || s.starts_with("rollback ") || s.starts_with("rollback to"))
    {
        return "rollback";
    }
    "other"
}

/// Does `src[..at]` end on a word boundary so the keyword at `at`
/// is not the tail of an identifier (e.g. `bad_exception`)?
fn keyword_boundary_before(src: &str, at: usize) -> bool {
    src[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
}

/// Scan a routine `source` for its exception section and yield one
/// [`ExceptionHandlerSite`] per `WHEN ... THEN ...` handler.
///
/// Text-level, matching this crate's existing lightweight evidence
/// approach (cf. dynamic-SQL sites). It recognizes the common
/// single `EXCEPTION ... END` section: ambiguous / unparseable
/// shapes simply yield no site rather than a wrong one (R13 — a
/// false fact is worse than a missing one).
#[must_use]
pub fn scan_exception_handlers(unit_logical_id: &str, source: &str) -> Vec<ExceptionHandlerSite> {
    let lower = source.to_ascii_lowercase();
    let Some(mut idx) = lower.find("exception") else {
        return Vec::new();
    };
    // Find a standalone `exception` keyword (word boundaries).
    loop {
        let end = idx + "exception".len();
        let boundary = keyword_boundary_before(&lower, idx)
            && lower[end..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if boundary {
            break;
        }
        match lower[end..].find("exception") {
            Some(next) => idx = end + next,
            None => return Vec::new(),
        }
    }

    let section = &lower[idx + "exception".len()..];
    let mut sites = Vec::new();
    for chunk in section.split(" when ").skip(1) {
        let Some((scope_raw, rest)) = chunk.split_once(" then ") else {
            continue;
        };
        // Body runs to the next handler / section end.
        let body = rest
            .split(" when ")
            .next()
            .unwrap_or(rest)
            .rsplit_once(" end")
            .map_or(rest, |(b, _)| b);
        let scope_norm = scope_raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let scope = if scope_norm.split_whitespace().any(|w| w == "others") {
            "others".to_string()
        } else {
            scope_norm
        };
        sites.push(ExceptionHandlerSite {
            unit_logical_id: unit_logical_id.to_string(),
            scope,
            body_class: classify_handler_body(body).to_string(),
        });
    }
    sites
}

/// Emit one `ExceptionHandler` fact per detected handler so
/// QUAL001 / QUAL004 can consume them via `by_kind` like every
/// other fact-based rule.
pub fn emit_exception_handler_facts<I>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize
where
    I: IntoIterator<Item = ExceptionHandlerSite>,
{
    let before = store.len();
    for site in sites {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::ExceptionHandler {
                unit_logical_id: site.unit_logical_id,
                scope: site.scope,
                body_class: site.body_class,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// One detected cursor `FOR` loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorForLoopSite {
    pub unit_logical_id: String,
    /// The loop record variable (`FOR <var> IN …`).
    pub loop_var: String,
    /// Body contains a row-level INSERT/UPDATE/DELETE/MERGE.
    pub has_body_dml: bool,
}

/// One routine body with no recognized instrumentation call.
/// Reports *absence* only — STYLE001 (opt-in) decides whether
/// that is a finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingInstrumentationSite {
    pub unit_logical_id: String,
}

/// Substrings that count as an instrumentation / logging /
/// tracing / error-signal call. Deliberately broad so STYLE001
/// only fires when a unit has *nothing* — a false "missing" is
/// worse than a missed one (R13).
const INSTRUMENTATION_MARKERS: &[&str] = &[
    "dbms_output.put_line",
    "dbms_application_info",
    "raise_application_error",
    "apex_debug",
    "logger.",
    "log_",
    ".log(",
    ".info(",
    ".warn(",
    ".error(",
    ".debug(",
    "audit_",
];

fn body_has_dml(body: &str) -> bool {
    // Scan *every* occurrence of each DML keyword, not just the first.
    // A first-hit-only check under-reports when an earlier occurrence is
    // the tail of an identifier (e.g. `v_last_update`, `deleted_flag`):
    // the boundary check fails on that decoy and, without a retry loop,
    // the genuine row-level `update t`/`delete from t` later in the body
    // is never reached. Mirrors `scan_dml_in_function` (line ~692) and
    // `scan_deterministic_misuse` (line ~812).
    ["insert ", "update ", "delete ", "merge "]
        .iter()
        .any(|kw| {
            body.match_indices(kw)
                .any(|(at, _)| keyword_boundary_before(body, at))
        })
}

/// Scan a routine `source` for cursor `FOR` loops, yielding one
/// [`CursorForLoopSite`] per loop. Text-level, mirroring
/// [`scan_exception_handlers`]. A numeric range loop
/// (`FOR i IN 1..10 LOOP`) is **not** a cursor loop and yields no
/// site (R13: a false fact is worse than a missing one).
#[must_use]
pub fn scan_cursor_for_loops(unit_logical_id: &str, source: &str) -> Vec<CursorForLoopSite> {
    let lower = source.to_ascii_lowercase();
    let mut sites = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("for ") {
        let at = search_from + rel;
        search_from = at + 4;
        if !keyword_boundary_before(&lower, at) {
            continue;
        }
        let after = &lower[at + 4..];
        let Some((var_raw, rest)) = after.split_once(" in ") else {
            continue;
        };
        let loop_var = var_raw.trim();
        if loop_var.is_empty() || loop_var.split_whitespace().count() != 1 {
            continue;
        }
        let Some((in_clause, body_and_more)) = rest.split_once(" loop ") else {
            continue;
        };
        // Numeric range (`1..10`) ⇒ not a cursor loop.
        if in_clause.contains("..") {
            continue;
        }
        // Cursor loop iff the iterable is a query or a cursor
        // reference: contains `select`, an opening paren, or is a
        // bare identifier (cursor name). Anything else: skip (R13).
        let ic = in_clause.trim();
        let looks_cursor =
            ic.contains("select") || ic.contains('(') || ic.split_whitespace().count() == 1;
        if !looks_cursor {
            continue;
        }
        let body = body_and_more
            .split_once(" end loop")
            .map_or(body_and_more, |(b, _)| b);
        sites.push(CursorForLoopSite {
            unit_logical_id: unit_logical_id.to_string(),
            loop_var: loop_var.to_string(),
            has_body_dml: body_has_dml(body),
        });
    }
    sites
}

/// Scan a routine `source`: if it has a body (`BEGIN`) but no
/// recognized instrumentation marker, yield a single
/// [`MissingInstrumentationSite`]. A spec with no body yields
/// nothing (R13 — we only report a unit we can see executes).
#[must_use]
pub fn scan_missing_instrumentation(
    unit_logical_id: &str,
    source: &str,
) -> Vec<MissingInstrumentationSite> {
    let lower = source.to_ascii_lowercase();
    // Scan *every* occurrence of `begin`, not just the first. A first-hit-only
    // check under-reports when an earlier occurrence is the tail/head of an
    // identifier (e.g. a `v_begin_dt` declared before the real BEGIN): the
    // boundary check fails on that decoy and, without a retry loop, the genuine
    // body-introducing BEGIN later in the source is never reached, so the
    // routine is wrongly classified as a body-less spec and silently escapes
    // STYLE001. Mirrors `body_has_dml` (oracle-j1ep.5).
    let has_body = lower
        .match_indices("begin")
        .any(|(at, _)| keyword_boundary_before(&lower, at));
    if !has_body {
        return Vec::new();
    }
    if INSTRUMENTATION_MARKERS.iter().any(|m| lower.contains(m)) {
        return Vec::new();
    }
    vec![MissingInstrumentationSite {
        unit_logical_id: unit_logical_id.to_string(),
    }]
}

/// Emit one `CursorForLoop` fact per site, mirroring
/// [`emit_exception_handler_facts`].
pub fn emit_cursor_for_loop_facts<I>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize
where
    I: IntoIterator<Item = CursorForLoopSite>,
{
    let before = store.len();
    for site in sites {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::CursorForLoop {
                unit_logical_id: site.unit_logical_id,
                loop_var: site.loop_var,
                has_body_dml: site.has_body_dml,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// Emit one `MissingInstrumentation` fact per site.
pub fn emit_missing_instrumentation_facts<I>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize
where
    I: IntoIterator<Item = MissingInstrumentationSite>,
{
    let before = store.len();
    for site in sites {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::MissingInstrumentation {
                unit_logical_id: site.unit_logical_id,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// One string literal that is, by strong syntactic context, a
/// hardcoded secret (SEC003).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HardcodedCredentialSite {
    pub unit_logical_id: String,
    /// The credential context that matched (e.g. `identified by`,
    /// `password :=`).
    pub marker: String,
}

/// Credential context markers. Each must be followed (within the
/// same statement) by a `'…'` string literal to count — a bind
/// variable or column ref is *not* a hardcoded secret (R13: a
/// false credential finding erodes trust fast).
const CREDENTIAL_MARKERS: &[&str] = &[
    "identified by",
    "password",
    "passwd",
    "pwd",
    "secret",
    "api_key",
    "apikey",
    "credential",
    "private_key",
];

/// Blank the *contents* of every `'…'` string literal (keeping the
/// quotes and the byte length) so a credential marker can never
/// self-match inside a secret value (e.g. `secret` inside
/// `'Sup3rSecret'`). Doubled `''` escapes are treated as literal
/// content. ASCII-only transform; non-ASCII bytes are left as-is.
fn mask_string_literals(lower: &str) -> String {
    let bytes = lower.as_bytes();
    let mut out = String::with_capacity(lower.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            out.push('\'');
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if bytes.get(i + 1) == Some(&b'\'') {
                        out.push_str("__");
                        i += 2;
                        continue;
                    }
                    out.push('\'');
                    i += 1;
                    break;
                }
                out.push('_');
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Scan `source` for hardcoded credentials: a credential marker
/// (in code position, never inside a literal) immediately followed
/// (same statement, before `;`) by a quoted string literal.
/// Text-level + conservative, mirroring [`scan_exception_handlers`].
#[must_use]
pub fn scan_hardcoded_credentials(
    unit_logical_id: &str,
    source: &str,
) -> Vec<HardcodedCredentialSite> {
    let lower = mask_string_literals(&source.to_ascii_lowercase());
    let mut sites = Vec::new();
    for marker in CREDENTIAL_MARKERS {
        let mut from = 0;
        while let Some(rel) = lower[from..].find(marker) {
            let at = from + rel;
            from = at + marker.len();
            // No word-boundary gate here: the credential marker is
            // frequently *part of* the secret-bearing identifier
            // (`v_password := '…'`, `l_api_key := '…'`). The
            // literal-in-same-statement constraint below is what
            // keeps this conservative (R13).
            // Look only within the rest of this statement.
            let rest = &lower[at + marker.len()..];
            let stmt = rest.split(';').next().unwrap_or(rest);
            // A quoted literal must appear, and before any obvious
            // bind/identifier-only continuation. We accept the
            // first `'` within the statement window.
            if let Some(q) = stmt.find('\'') {
                // Guard: the gap between marker and the quote must
                // be short-ish (an assignment/clause, not a whole
                // unrelated statement). 64 chars is generous for
                // `password  varchar2(30) := '…'` style.
                if q <= 64 {
                    sites.push(HardcodedCredentialSite {
                        unit_logical_id: unit_logical_id.to_string(),
                        marker: (*marker).to_string(),
                    });
                }
            }
        }
    }
    sites
}

/// Emit one `HardcodedCredential` fact per site (SEC003).
pub fn emit_hardcoded_credential_facts<I>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize
where
    I: IntoIterator<Item = HardcodedCredentialSite>,
{
    let before = store.len();
    for site in sites {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::HardcodedCredential {
                unit_logical_id: site.unit_logical_id,
                marker: site.marker,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// One unit declaring invoker's rights (`AUTHID CURRENT_USER`)
/// (SEC004).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvokerRightsSite {
    pub unit_logical_id: String,
}

/// Scan `source` for an `AUTHID CURRENT_USER` clause. Literal
/// contents are masked first so the phrase can't self-match inside
/// a string; whitespace between `authid` and `current_user` is
/// collapsed. Conservative: `AUTHID DEFINER` (or absence) yields
/// no site. At most one site per unit.
#[must_use]
pub fn scan_invoker_rights(unit_logical_id: &str, source: &str) -> Vec<InvokerRightsSite> {
    let masked = mask_string_literals(&source.to_ascii_lowercase());
    // Collapse all whitespace runs to a single space so
    // `authid\n  current_user` matches.
    let collapsed: String = masked.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.contains("authid current_user") {
        vec![InvokerRightsSite {
            unit_logical_id: unit_logical_id.to_string(),
        }]
    } else {
        Vec::new()
    }
}

/// Emit one `InvokerRights` fact per site (SEC004).
pub fn emit_invoker_rights_facts<I>(store: &mut FactStore, prov: &FactProvenance, sites: I) -> usize
where
    I: IntoIterator<Item = InvokerRightsSite>,
{
    let before = store.len();
    for site in sites {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::InvokerRights {
                unit_logical_id: site.unit_logical_id,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// One unit whose `RETURN` type is a REF CURSOR (SEC007),
/// one function with row-level DML in its body (QUAL007), or
/// one unbounded `BULK COLLECT` (QUAL003). All carry only the
/// unit id — the rule explains; the fact reports presence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitFactSite {
    pub unit_logical_id: String,
}

fn collapsed_masked(source: &str) -> String {
    mask_string_literals(&source.to_ascii_lowercase())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// SEC007: a function returning a REF CURSOR. Detects the common
/// `RETURN SYS_REFCURSOR` and explicit `RETURN REF CURSOR` forms
/// (strongly-typed named ref-cursor returns need type resolution
/// and are out of this text-level scope — R13, documented).
#[must_use]
pub fn scan_ref_cursor_return(unit_logical_id: &str, source: &str) -> Vec<UnitFactSite> {
    let c = collapsed_masked(source);
    if c.contains("return sys_refcursor") || c.contains("return ref cursor") {
        vec![UnitFactSite {
            unit_logical_id: unit_logical_id.to_string(),
        }]
    } else {
        Vec::new()
    }
}

/// QUAL007: a `FUNCTION` whose body performs row-level DML. Only
/// fires when the source is a function (the `function` keyword is
/// present as a word) and `body_has_dml` (R13: a procedure with
/// DML is normal and is not flagged here).
#[must_use]
pub fn scan_dml_in_function(unit_logical_id: &str, source: &str) -> Vec<UnitFactSite> {
    let masked = mask_string_literals(&source.to_ascii_lowercase());
    let is_function = masked
        .match_indices("function")
        .any(|(at, _)| keyword_boundary_before(&masked, at));
    if is_function && body_has_dml(&masked) {
        vec![UnitFactSite {
            unit_logical_id: unit_logical_id.to_string(),
        }]
    } else {
        Vec::new()
    }
}

/// QUAL003: a `BULK COLLECT INTO` with no `LIMIT` in the same
/// statement — unbounded PGA materialization. One site per
/// offending statement.
#[must_use]
pub fn scan_unbounded_bulk_collect(unit_logical_id: &str, source: &str) -> Vec<UnitFactSite> {
    let masked = mask_string_literals(&source.to_ascii_lowercase());
    let mut sites = Vec::new();
    let mut from = 0;
    while let Some(rel) = masked[from..].find("bulk collect into") {
        let at = from + rel;
        from = at + "bulk collect into".len();
        let stmt = masked[at..].split(';').next().unwrap_or(&masked[at..]);
        if !stmt.contains("limit") {
            sites.push(UnitFactSite {
                unit_logical_id: unit_logical_id.to_string(),
            });
        }
    }
    sites
}

fn emit_unit_facts<I, F>(store: &mut FactStore, prov: &FactProvenance, sites: I, mk: F) -> usize
where
    I: IntoIterator<Item = UnitFactSite>,
    F: Fn(String) -> FactPayload,
{
    let before = store.len();
    for site in sites {
        let f = crate::fact::mint_fact(prov.clone(), mk(site.unit_logical_id));
        store.push(f);
    }
    store.len() - before
}

/// Emit `RefCursorReturn` facts (SEC007).
pub fn emit_ref_cursor_return_facts<I: IntoIterator<Item = UnitFactSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    emit_unit_facts(store, prov, sites, |unit_logical_id| {
        FactPayload::RefCursorReturn { unit_logical_id }
    })
}

/// Emit `DmlInFunction` facts (QUAL007).
pub fn emit_dml_in_function_facts<I: IntoIterator<Item = UnitFactSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    emit_unit_facts(store, prov, sites, |unit_logical_id| {
        FactPayload::DmlInFunction { unit_logical_id }
    })
}

/// Emit `UnboundedBulkCollect` facts (QUAL003).
pub fn emit_unbounded_bulk_collect_facts<I: IntoIterator<Item = UnitFactSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    emit_unit_facts(store, prov, sites, |unit_logical_id| {
        FactPayload::UnboundedBulkCollect { unit_logical_id }
    })
}

/// A site carrying a unit id plus a short detail string (the
/// matched deprecated feature / non-deterministic construct).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailFactSite {
    pub unit_logical_id: String,
    pub detail: String,
}

/// QUAL005: well-known deprecated / legacy constructs. Conservative
/// (R13): only unambiguous, widely policy-flagged forms; literals
/// are masked so a mention in a string never matches. One site per
/// distinct feature found.
#[must_use]
pub fn scan_deprecated_features(unit_logical_id: &str, source: &str) -> Vec<DetailFactSite> {
    let m = mask_string_literals(&source.to_ascii_lowercase());
    let mut sites = Vec::new();
    let mut push = |feature: &str| {
        sites.push(DetailFactSite {
            unit_logical_id: unit_logical_id.to_string(),
            detail: feature.to_string(),
        });
    };
    if m.match_indices("dbms_job")
        .any(|(at, _)| keyword_boundary_before(&m, at))
    {
        push("dbms_job (deprecated; use DBMS_SCHEDULER)");
    }
    if m.contains("(+)") {
        push("legacy (+) outer-join operator (use ANSI JOIN)");
    }
    if m.contains("commit work") || m.contains("rollback work") {
        push("legacy `WORK` transaction-control keyword");
    }
    sites
}

/// QUAL008: a `DETERMINISTIC` function whose body contains a
/// non-deterministic construct. One site per distinct construct.
#[must_use]
pub fn scan_deterministic_misuse(unit_logical_id: &str, source: &str) -> Vec<DetailFactSite> {
    let m = mask_string_literals(&source.to_ascii_lowercase());
    let is_deterministic = m
        .match_indices("deterministic")
        .any(|(at, _)| keyword_boundary_before(&m, at));
    if !is_deterministic {
        return Vec::new();
    }
    let mut sites = Vec::new();
    let mut push = |c: &str| {
        sites.push(DetailFactSite {
            unit_logical_id: unit_logical_id.to_string(),
            detail: c.to_string(),
        });
    };
    if body_has_dml(&m) {
        push("row-level DML");
    }
    for (needle, label) in [
        ("sysdate", "SYSDATE"),
        ("systimestamp", "SYSTIMESTAMP"),
        ("current_timestamp", "CURRENT_TIMESTAMP"),
        ("dbms_random", "DBMS_RANDOM"),
        (".nextval", "sequence .NEXTVAL"),
    ] {
        if m.contains(needle) {
            push(label);
        }
    }
    sites
}

/// Emit `DeprecatedFeature` facts (QUAL005).
pub fn emit_deprecated_feature_facts<I: IntoIterator<Item = DetailFactSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    let before = store.len();
    for s in sites {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::DeprecatedFeature {
                unit_logical_id: s.unit_logical_id,
                feature: s.detail,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// Emit `DeterministicMisuse` facts (QUAL008).
pub fn emit_deterministic_misuse_facts<I: IntoIterator<Item = DetailFactSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    let before = store.len();
    for s in sites {
        let f = crate::fact::mint_fact(
            prov.clone(),
            FactPayload::DeterministicMisuse {
                unit_logical_id: s.unit_logical_id,
                construct: s.detail,
            },
        );
        store.push(f);
    }
    store.len() - before
}

/// QUAL006: a `FOR EACH ROW` trigger whose body references its own
/// base table in a query/DML (ORA-04091 mutating-table hazard).
/// R13-conservative: requires a clean `on <table>` extraction and
/// `for each row`; otherwise no fact.
#[must_use]
pub fn scan_mutating_table_trigger(unit_logical_id: &str, source: &str) -> Vec<DetailFactSite> {
    let c = collapsed_masked(source);
    if !c.contains("trigger") || !c.contains("for each row") {
        return Vec::new();
    }
    // Table is the token after the first ` on ` following `trigger`.
    let Some(trig_at) = c.find("trigger") else {
        return Vec::new();
    };
    let after = &c[trig_at..];
    let Some(on_rel) = after.find(" on ") else {
        return Vec::new();
    };
    let tail = &after[on_rel + 4..];
    let raw = tail
        .split([' ', '(', '\n', '\t'])
        .next()
        .unwrap_or("")
        .trim_end_matches(|ch: char| !(ch.is_alphanumeric() || ch == '_'));
    if raw.is_empty() {
        return Vec::new();
    }
    // Strip schema qualifier for the body-reference check.
    let table = raw.rsplit('.').next().unwrap_or(raw).to_string();
    if table.is_empty() {
        return Vec::new();
    }
    let body_refs = [
        format!("from {table}"),
        format!("update {table}"),
        format!("insert into {table}"),
        format!("delete from {table}"),
        format!("merge into {table}"),
    ];
    if body_refs.iter().any(|p| c.contains(p.as_str())) {
        vec![DetailFactSite {
            unit_logical_id: unit_logical_id.to_string(),
            detail: table,
        }]
    } else {
        Vec::new()
    }
}

/// QUAL002: an exception handler that instruments/logs but neither
/// re-raises nor signals — the error is recorded then swallowed.
/// At most one site per unit. Mirrors the lightweight exception-
/// section split used by [`scan_exception_handlers`].
#[must_use]
pub fn scan_log_without_reraise(unit_logical_id: &str, source: &str) -> Vec<InvokerRightsSite> {
    let lower = mask_string_literals(&source.to_ascii_lowercase());
    let Some(mut idx) = lower.find("exception") else {
        return Vec::new();
    };
    loop {
        let end = idx + "exception".len();
        let boundary = keyword_boundary_before(&lower, idx)
            && lower[end..]
                .chars()
                .next()
                .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
        if boundary {
            break;
        }
        match lower[end..].find("exception") {
            Some(next) => idx = end + next,
            None => return Vec::new(),
        }
    }
    let section = &lower[idx + "exception".len()..];
    for chunk in section.split(" when ").skip(1) {
        let Some((_scope, rest)) = chunk.split_once(" then ") else {
            continue;
        };
        let body = rest
            .split(" when ")
            .next()
            .unwrap_or(rest)
            .rsplit_once(" end")
            .map_or(rest, |(b, _)| b);
        let has_log = INSTRUMENTATION_MARKERS.iter().any(|m| body.contains(m));
        let has_raise = body
            .match_indices("raise")
            .any(|(at, _)| keyword_boundary_before(body, at));
        if has_log && !has_raise {
            return vec![InvokerRightsSite {
                unit_logical_id: unit_logical_id.to_string(),
            }];
        }
    }
    Vec::new()
}

/// DEP001: a DML statement whose target is schema-qualified to a
/// schema other than the unit's own (cross-schema write surface).
/// Unit schema = first dotted segment of `unit_logical_id`.
#[must_use]
pub fn scan_cross_schema_write(unit_logical_id: &str, source: &str) -> Vec<DetailFactSite> {
    let unit_schema = unit_logical_id
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let m = mask_string_literals(&source.to_ascii_lowercase());
    let mut sites = Vec::new();
    // The DELETE lead is `delete ` (not `delete from `): Oracle's `FROM` is
    // optional, so a FROM-less cross-schema `delete fin.audit where …` must be
    // scanned too, or it silently escapes DEP001 (oracle-j1ep.2). After the
    // lead we skip an optional `from ` before reading the target so both
    // `delete fin.audit` and `delete from fin.audit` resolve to `fin.audit`.
    for lead in ["insert into ", "update ", "delete ", "merge into "] {
        let mut from = 0;
        while let Some(rel) = m[from..].find(lead) {
            let at = from + rel;
            from = at + lead.len();
            if !keyword_boundary_before(&m, at) {
                continue;
            }
            let mut rest = &m[at + lead.len()..];
            if lead == "delete " {
                rest = rest.trim_start();
                if let Some(after_from) = rest.strip_prefix("from ") {
                    rest = after_from.trim_start();
                }
            }
            let target = rest
                .split([' ', '(', ';', '\n', '\t'])
                .next()
                .unwrap_or("")
                .trim();
            if let Some((schema, obj)) = target.split_once('.')
                && !schema.is_empty()
                && !obj.is_empty()
                && schema != unit_schema
                && schema.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
            {
                sites.push(DetailFactSite {
                    unit_logical_id: unit_logical_id.to_string(),
                    detail: format!("{schema}.{}", obj.split('.').next().unwrap_or(obj)),
                });
            }
        }
    }
    sites
}

/// Emit `MutatingTableTrigger` facts (QUAL006).
pub fn emit_mutating_table_trigger_facts<I: IntoIterator<Item = DetailFactSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    let before = store.len();
    for s in sites {
        store.push(crate::fact::mint_fact(
            prov.clone(),
            FactPayload::MutatingTableTrigger {
                unit_logical_id: s.unit_logical_id,
                table: s.detail,
            },
        ));
    }
    store.len() - before
}

/// Emit `LogWithoutReraise` facts (QUAL002).
pub fn emit_log_without_reraise_facts<I: IntoIterator<Item = InvokerRightsSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    let before = store.len();
    for s in sites {
        store.push(crate::fact::mint_fact(
            prov.clone(),
            FactPayload::LogWithoutReraise {
                unit_logical_id: s.unit_logical_id,
            },
        ));
    }
    store.len() - before
}

/// Emit `CrossSchemaWrite` facts (DEP001).
pub fn emit_cross_schema_write_facts<I: IntoIterator<Item = DetailFactSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    let before = store.len();
    for s in sites {
        store.push(crate::fact::mint_fact(
            prov.clone(),
            FactPayload::CrossSchemaWrite {
                unit_logical_id: s.unit_logical_id,
                target: s.detail,
            },
        ));
    }
    store.len() - before
}

/// One sensitive `CREATE PUBLIC SYNONYM` site (SEC005).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SynonymFactSite {
    pub unit_logical_id: String,
    pub synonym: String,
    pub target: String,
}

/// Substrings that mark a synonym/target name as sensitive
/// (credential / PII / finance). Conservative wordlist — a public
/// synonym on a benign object is not flagged (R13).
const SENSITIVITY_MARKERS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "credential",
    "secret",
    "token",
    "apikey",
    "api_key",
    "private_key",
    "ssn",
    "salary",
    "payroll",
    "bank",
    "account",
    "acct",
    "card",
    "tax",
    "patient",
    "medical",
    "wallet",
];

fn name_is_sensitive(name: &str) -> bool {
    SENSITIVITY_MARKERS.iter().any(|m| name.contains(m))
}

/// SEC005: a `CREATE [OR REPLACE] PUBLIC SYNONYM <syn> FOR <tgt>`
/// where the synonym or its target name matches the sensitivity
/// heuristic. Literal-masked, conservative: a non-public synonym
/// or a benign name yields no fact.
#[must_use]
pub fn scan_sensitive_public_synonym(unit_logical_id: &str, source: &str) -> Vec<SynonymFactSite> {
    let c = collapsed_masked(source);
    let mut sites = Vec::new();
    let mut from = 0;
    while let Some(rel) = c[from..].find("public synonym ") {
        let at = from + rel;
        from = at + "public synonym ".len();
        let rest = &c[at + "public synonym ".len()..];
        let Some((syn_raw, after)) = rest.split_once(" for ") else {
            continue;
        };
        let synonym = syn_raw
            .split([' ', '(', ';', '\n', '\t'])
            .next()
            .unwrap_or("")
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_string();
        let target = after
            .split([' ', '(', ';', '\n', '\t'])
            .next()
            .unwrap_or("")
            .trim_end_matches(';')
            .to_string();
        let tgt_name = target.rsplit('.').next().unwrap_or(&target);
        if synonym.is_empty() || target.is_empty() {
            continue;
        }
        if name_is_sensitive(&synonym) || name_is_sensitive(tgt_name) {
            sites.push(SynonymFactSite {
                unit_logical_id: unit_logical_id.to_string(),
                synonym,
                target,
            });
        }
    }
    sites
}

/// Emit `SensitivePublicSynonym` facts (SEC005).
pub fn emit_sensitive_public_synonym_facts<I: IntoIterator<Item = SynonymFactSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    let before = store.len();
    for s in sites {
        store.push(crate::fact::mint_fact(
            prov.clone(),
            FactPayload::SensitivePublicSynonym {
                unit_logical_id: s.unit_logical_id,
                synonym: s.synonym,
                target: s.target,
            },
        ));
    }
    store.len() - before
}

/// One `<col> IS NULL` predicate on a column the same source
/// indexes (PERF003).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsNullIndexedSite {
    pub unit_logical_id: String,
    pub column: String,
}

fn simple_ident(tok: &str) -> String {
    tok.rsplit('.')
        .next()
        .unwrap_or(tok)
        .trim_matches(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .to_string()
}

/// Columns this source declares an index on:
/// `CREATE [UNIQUE|BITMAP] INDEX <name> ON <table> ( c1, c2, … )`.
fn indexed_columns(c: &str) -> BTreeSet<String> {
    let mut cols = BTreeSet::new();
    let mut from = 0;
    while let Some(rel) = c[from..].find("index ") {
        let at = from + rel;
        from = at + "index ".len();
        // Must be a CREATE … INDEX (skip `alter index`, etc.).
        let pre = &c[..at];
        if !pre
            .trim_end()
            .rsplit([' ', '\n', '\t'])
            .next()
            .map(|w| w == "create" || w == "unique" || w == "bitmap")
            .unwrap_or(false)
        {
            continue;
        }
        let rest = &c[at + "index ".len()..];
        let Some(on_rel) = rest.find(" on ") else {
            continue;
        };
        let after_on = &rest[on_rel + 4..];
        let Some(lp) = after_on.find('(') else {
            continue;
        };
        let Some(rp) = after_on[lp..].find(')') else {
            continue;
        };
        for raw in after_on[lp + 1..lp + rp].split(',') {
            let id = simple_ident(raw.split_whitespace().next().unwrap_or(""));
            if !id.is_empty() {
                cols.insert(id);
            }
        }
    }
    cols
}

/// PERF003: a `<col> IS NULL` predicate where the same source
/// declares an index whose key list contains `col`. B-tree indexes
/// do not store all-NULL keys, so the predicate forces a full scan.
/// R13: requires BOTH the index DDL and the predicate in this
/// source; catalog-only indexes are out of this source-level scope.
/// ` is null` is not a substring of `is not null`, so negated
/// predicates never match.
#[must_use]
pub fn scan_is_null_on_indexed_column(
    unit_logical_id: &str,
    source: &str,
) -> Vec<IsNullIndexedSite> {
    let c = collapsed_masked(source);
    let indexed = indexed_columns(&c);
    if indexed.is_empty() {
        return Vec::new();
    }
    let mut flagged: BTreeSet<String> = BTreeSet::new();
    let mut from = 0;
    while let Some(rel) = c[from..].find(" is null") {
        let at = from + rel;
        from = at + " is null".len();
        // Token immediately before ` is null` is the column.
        let col = simple_ident(c[..at].rsplit([' ', '(', ',']).next().unwrap_or(""));
        if !col.is_empty() && indexed.contains(&col) {
            flagged.insert(col);
        }
    }
    flagged
        .into_iter()
        .map(|column| IsNullIndexedSite {
            unit_logical_id: unit_logical_id.to_string(),
            column,
        })
        .collect()
}

/// Emit `IsNullOnIndexedColumn` facts (PERF003).
pub fn emit_is_null_on_indexed_column_facts<I: IntoIterator<Item = IsNullIndexedSite>>(
    store: &mut FactStore,
    prov: &FactProvenance,
    sites: I,
) -> usize {
    let before = store.len();
    for s in sites {
        store.push(crate::fact::mint_fact(
            prov.clone(),
            FactPayload::IsNullOnIndexedColumn {
                unit_logical_id: s.unit_logical_id,
                column: s.column,
            },
        ));
    }
    store.len() - before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::{CallContext, CallSite};
    use crate::fact::FactKind;

    fn prov() -> FactProvenance {
        FactProvenance {
            component: "plsql-ir".into(),
            component_version: "0.1.0".into(),
            run_id: String::new(),
        }
    }

    #[test]
    fn declaration_facts_emitted_and_counted() {
        let mut store = FactStore::default();
        let n = emit_declaration_facts(
            &mut store,
            &prov(),
            vec![
                (DeclId::new(1), "hr.employees".to_string()),
                (DeclId::new(2), "hr.departments".to_string()),
            ],
        );
        assert_eq!(n, 2);
        assert_eq!(store.by_kind(FactKind::Declaration).count(), 2);
    }

    #[test]
    fn declaration_facts_dedupe_identical_entries() {
        let mut store = FactStore::default();
        emit_declaration_facts(
            &mut store,
            &prov(),
            vec![(DeclId::new(1), "hr.x".to_string())],
        );
        let n2 = emit_declaration_facts(
            &mut store,
            &prov(),
            vec![(DeclId::new(1), "hr.x".to_string())],
        );
        // Same fact id → dedup → 0 new.
        assert_eq!(n2, 0);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn reference_facts_emitted() {
        let mut store = FactStore::default();
        let n = emit_reference_facts(
            &mut store,
            &prov(),
            vec![(DeclId::new(3), "hr.audit_pkg".to_string())],
        );
        assert_eq!(n, 1);
        assert_eq!(store.by_kind(FactKind::Reference).count(), 1);
    }

    #[test]
    fn call_facts_join_callee_path() {
        let mut store = FactStore::default();
        let calls = vec![CallSite {
            callee_parts: vec!["BILLING_PKG".into(), "POST_INVOICE".into()],
            callee_display: "billing_pkg.post_invoice".into(),
            arg_count: 2,
            context: CallContext::Statement,
        }];
        let n = emit_call_facts(&mut store, &prov(), "hr.run_billing", &calls);
        assert_eq!(n, 1);
        let f = store.by_kind(FactKind::DependencyEdge).next().unwrap();
        assert!(
            matches!(
                &f.payload,
                FactPayload::DependencyEdge { from_logical_id, to_logical_id, edge_kind }
                    if from_logical_id == "hr.run_billing"
                        && to_logical_id == "billing_pkg.post_invoice"
                        && edge_kind == "Calls"
            ),
            "unexpected DependencyEdge payload: {:?}",
            f.payload
        );
    }

    #[test]
    fn mixed_families_filter_independently() {
        let mut store = FactStore::default();
        emit_declaration_facts(&mut store, &prov(), vec![(DeclId::new(1), "a".into())]);
        emit_reference_facts(&mut store, &prov(), vec![(DeclId::new(1), "b".into())]);
        emit_call_facts(
            &mut store,
            &prov(),
            "a",
            &[CallSite {
                callee_parts: vec!["C".into()],
                callee_display: "c".into(),
                arg_count: 0,
                context: CallContext::Statement,
            }],
        );
        assert_eq!(store.by_kind(FactKind::Declaration).count(), 1);
        assert_eq!(store.by_kind(FactKind::Reference).count(), 1);
        assert_eq!(store.by_kind(FactKind::DependencyEdge).count(), 1);
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn privilege_facts_emitted_and_filterable() {
        let mut store = FactStore::default();
        let n = emit_privilege_facts(
            &mut store,
            &prov(),
            vec![
                ("HR_ROLE".into(), "EXECUTE".into(), "hr.billing_pkg".into()),
                ("PUBLIC".into(), "SELECT".into(), "hr.audit_v".into()),
            ],
        );
        assert_eq!(n, 2);
        assert_eq!(store.by_kind(FactKind::Privilege).count(), 2);
        let f = store.by_kind(FactKind::Privilege).next().unwrap();
        assert!(
            matches!(
                &f.payload,
                FactPayload::Privilege { grantee, privilege, on }
                    if grantee == "HR_ROLE"
                        && privilege == "EXECUTE"
                        && on == "hr.billing_pkg"
            ),
            "unexpected Privilege payload: {:?}",
            f.payload
        );
    }

    #[test]
    fn dynamic_sql_facts_emitted() {
        let mut store = FactStore::default();
        let n = emit_dynamic_sql_facts(
            &mut store,
            &prov(),
            vec![
                "hr.run_dyn: EXECUTE IMMEDIATE <sql-like, 1 bind>".to_string(),
                "hr.run_dyn2: OPEN cur FOR <opaque>".to_string(),
            ],
        );
        assert_eq!(n, 2);
        assert_eq!(store.by_kind(FactKind::DynamicSqlEvidence).count(), 2);
    }

    #[test]
    fn unknown_facts_carry_reason_evidence() {
        let mut store = FactStore::default();
        let n = emit_unknown_facts(
            &mut store,
            &prov(),
            vec![
                ("hr.remote_call".into(), "DbLinkRemoteObject".into()),
                ("hr.wrapped_pkg".into(), "WrappedSource".into()),
            ],
        );
        assert_eq!(n, 2);
        let f = store.by_kind(FactKind::Opacity).next().unwrap();
        assert!(
            matches!(
                &f.payload,
                FactPayload::Opacity { target_logical_id, reason }
                    if target_logical_id == "hr.remote_call"
                        && reason == "DbLinkRemoteObject"
            ),
            "unexpected Opacity payload: {:?}",
            f.payload
        );
    }

    #[test]
    fn fact004_families_dedupe_and_filter_independently() {
        let mut store = FactStore::default();
        emit_privilege_facts(
            &mut store,
            &prov(),
            vec![("R".into(), "EXECUTE".into(), "o".into())],
        );
        // Identical privilege fact → same id → dedup → 0 new.
        let dup = emit_privilege_facts(
            &mut store,
            &prov(),
            vec![("R".into(), "EXECUTE".into(), "o".into())],
        );
        assert_eq!(dup, 0);
        emit_dynamic_sql_facts(&mut store, &prov(), vec!["site".into()]);
        emit_unknown_facts(&mut store, &prov(), vec![("t".into(), "r".into())]);
        assert_eq!(store.by_kind(FactKind::Privilege).count(), 1);
        assert_eq!(store.by_kind(FactKind::DynamicSqlEvidence).count(), 1);
        assert_eq!(store.by_kind(FactKind::Opacity).count(), 1);
        assert_eq!(store.len(), 3);
    }

    struct FakeDeclSource;
    impl DeclLike for FakeDeclSource {
        fn iter_decls(&self) -> Vec<(DeclId, String)> {
            vec![
                (DeclId::new(10), "hr.p1".into()),
                (DeclId::new(11), "hr.p2".into()),
            ]
        }
    }

    #[test]
    fn emit_declarations_from_trait_source() {
        let mut store = FactStore::default();
        let n = emit_declarations_from(&mut store, &prov(), &FakeDeclSource);
        assert_eq!(n, 2);
    }

    #[test]
    fn classify_handler_body_buckets() {
        assert_eq!(classify_handler_body(" NULL; "), "noop");
        assert_eq!(classify_handler_body("null;null;"), "noop");
        assert_eq!(classify_handler_body(""), "noop");
        assert_eq!(classify_handler_body("commit;"), "commit");
        assert_eq!(classify_handler_body("rollback to sp1;"), "rollback");
        assert_eq!(classify_handler_body("rollback; null;"), "rollback");
        assert_eq!(classify_handler_body("log_error(sqlerrm);"), "other");
    }

    #[test]
    fn scan_when_others_then_null_is_noop_others() {
        let src = "begin do_work; exception when others then null; end;";
        let sites = scan_exception_handlers("hr.pkg.run", src);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].scope, "others");
        assert_eq!(sites[0].body_class, "noop");
        assert_eq!(sites[0].unit_logical_id, "hr.pkg.run");
    }

    #[test]
    fn scan_named_handler_and_commit_classified() {
        let src = "BEGIN x; EXCEPTION WHEN no_data_found THEN COMMIT; WHEN OTHERS THEN raise; END;";
        let sites = scan_exception_handlers("hr.p", src);
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0].scope, "no_data_found");
        assert_eq!(sites[0].body_class, "commit");
        assert_eq!(sites[1].scope, "others");
        assert_eq!(sites[1].body_class, "other");
    }

    #[test]
    fn scan_ignores_identifier_containing_exception() {
        // `bad_exception` must not be read as the section keyword.
        let src = "declare bad_exception number; begin null; end;";
        assert!(scan_exception_handlers("hr.p", src).is_empty());
    }

    #[test]
    fn emit_exception_handler_facts_pushes_typed_facts() {
        let mut store = FactStore::default();
        let sites = scan_exception_handlers(
            "hr.pkg.run",
            "begin go; exception when others then null; end;",
        );
        let n = emit_exception_handler_facts(&mut store, &prov(), sites);
        assert_eq!(n, 1);
        assert_eq!(store.by_kind(FactKind::ExceptionHandler).count(), 1);
    }

    // --- PLSQL-SAST-FACTS-LOOP (oracle-kcjx) ---

    #[test]
    fn scan_cursor_for_loop_query_form_detected() {
        let s = scan_cursor_for_loops(
            "hr.pkg.p",
            "begin for r in (select id from emps) loop dbms_output.put_line(r.id); end loop; end;",
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].loop_var, "r");
        assert!(!s[0].has_body_dml);
    }

    #[test]
    fn scan_cursor_for_loop_with_dml_sets_flag() {
        let s = scan_cursor_for_loops(
            "hr.pkg.p",
            "begin for rec in (select * from src) loop insert into dst values (rec.a); end loop; end;",
        );
        assert_eq!(s.len(), 1);
        assert!(s[0].has_body_dml, "INSERT in body must set has_body_dml");
    }

    #[test]
    fn scan_cursor_for_loop_bare_cursor_name_detected() {
        let s = scan_cursor_for_loops(
            "hr.pkg.p",
            "begin for c in emp_cur loop go(c); end loop; end;",
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].loop_var, "c");
    }

    #[test]
    fn scan_numeric_range_loop_is_not_a_cursor_loop() {
        // R13: a numeric FOR loop must NOT produce a CursorForLoop fact.
        let s = scan_cursor_for_loops(
            "hr.pkg.p",
            "begin for i in 1..10 loop go(i); end loop; end;",
        );
        assert!(s.is_empty(), "numeric range must yield no site, got {s:?}");
    }

    #[test]
    fn scan_for_keyword_inside_identifier_ignored() {
        // `before_x` contains "for" but is not a FOR loop.
        let s = scan_cursor_for_loops("hr.pkg.p", "begin before_x := 1; end;");
        assert!(s.is_empty(), "got {s:?}");
    }

    #[test]
    fn missing_instrumentation_flagged_when_body_has_no_marker() {
        let s = scan_missing_instrumentation("hr.pkg.silent", "begin update t set a=1; end;");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].unit_logical_id, "hr.pkg.silent");
    }

    #[test]
    fn missing_instrumentation_not_flagged_when_marker_present() {
        let s = scan_missing_instrumentation(
            "hr.pkg.logged",
            "begin dbms_output.put_line('x'); update t set a=1; end;",
        );
        assert!(s.is_empty(), "instrumented body must not flag, got {s:?}");
    }

    // oracle-j1ep.5: a `_begin`-suffixed identifier in the declaration section
    // (e.g. `v_begin_dt`) appears before the real BEGIN. A first-occurrence
    // `find("begin")` lands inside that decoy whose preceding `_` fails the
    // word-boundary check, short-circuiting `has_body` to false and silently
    // skipping the STYLE001 instrumentation check. Scanning every `begin`
    // occurrence (like `body_has_dml`) finds the genuine BEGIN.
    #[test]
    fn missing_instrumentation_flagged_past_begin_suffixed_decoy() {
        let s = scan_missing_instrumentation(
            "hr.pkg.silent",
            "procedure p is v_begin_dt date; begin update t set x=1; end;",
        );
        assert_eq!(
            s.len(),
            1,
            "real BEGIN past a v_begin_dt decoy must yield one site: {s:?}"
        );
        assert_eq!(s[0].unit_logical_id, "hr.pkg.silent");
    }

    #[test]
    fn missing_instrumentation_skips_specs_without_body() {
        // No BEGIN ⇒ cannot see it executes ⇒ no fact (R13).
        let s = scan_missing_instrumentation("hr.pkg.spec", "procedure p(x in number);");
        assert!(s.is_empty(), "got {s:?}");
    }

    #[test]
    fn emit_cursor_for_loop_and_missing_instrumentation_facts_are_typed() {
        let mut store = FactStore::default();
        let cfl = scan_cursor_for_loops(
            "hr.pkg.p",
            "begin for r in (select 1 from dual) loop null; end loop; end;",
        );
        let n1 = emit_cursor_for_loop_facts(&mut store, &prov(), cfl);
        assert_eq!(n1, 1);
        assert_eq!(store.by_kind(FactKind::CursorForLoop).count(), 1);

        let mi = scan_missing_instrumentation("hr.pkg.p", "begin null; end;");
        let n2 = emit_missing_instrumentation_facts(&mut store, &prov(), mi);
        assert_eq!(n2, 1);
        assert_eq!(store.by_kind(FactKind::MissingInstrumentation).count(), 1);
    }

    // --- SEC003 hardcoded-credentials substrate ---

    #[test]
    fn scan_identified_by_literal_flagged() {
        let s =
            scan_hardcoded_credentials("hr.admin", "alter user hr identified by 'Sup3rSecret';");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].marker, "identified by");
    }

    #[test]
    fn scan_password_assignment_literal_flagged() {
        let s = scan_hardcoded_credentials("hr.pkg.connect", "begin v_password := 'hunter2'; end;");
        assert!(s.iter().any(|x| x.marker == "password"));
    }

    #[test]
    fn scan_credential_marker_with_bind_not_flagged() {
        // A bind/variable (no string literal in the statement) must
        // NOT be flagged — R13, avoid false credential findings.
        let s = scan_hardcoded_credentials("hr.pkg.connect", "begin v_password := p_input; end;");
        assert!(s.is_empty(), "bind, not a literal: {s:?}");
    }

    #[test]
    fn scan_credential_keyword_in_identifier_ignored() {
        // `old_password_hash` substring should not match without a
        // following literal in the statement.
        let s = scan_hardcoded_credentials("hr.pkg.p", "begin x := old_password_hash; end;");
        assert!(s.is_empty(), "{s:?}");
    }

    #[test]
    fn emit_hardcoded_credential_facts_typed() {
        let mut store = FactStore::default();
        let sites = scan_hardcoded_credentials("hr.admin", "alter user x identified by 'p';");
        let n = emit_hardcoded_credential_facts(&mut store, &prov(), sites);
        assert_eq!(n, 1);
        assert_eq!(store.by_kind(FactKind::HardcodedCredential).count(), 1);
    }

    // --- SEC004 invoker-rights substrate ---

    #[test]
    fn scan_authid_current_user_flagged_whitespace_insensitive() {
        let s = scan_invoker_rights(
            "hr.pkg",
            "create or replace package hr.pkg\n  authid\tcurrent_user as ...",
        );
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn scan_authid_definer_not_flagged() {
        let s = scan_invoker_rights("hr.pkg", "create package hr.pkg authid definer as ...");
        assert!(s.is_empty(), "{s:?}");
    }

    #[test]
    fn scan_authid_current_user_inside_literal_not_flagged() {
        // Masked literals ⇒ the phrase in a comment-string doesn't match.
        let s = scan_invoker_rights(
            "hr.pkg",
            "begin msg := 'note: authid current_user is risky'; end;",
        );
        assert!(s.is_empty(), "literal mention must not flag: {s:?}");
    }

    #[test]
    fn emit_invoker_rights_facts_typed() {
        let mut store = FactStore::default();
        let sites = scan_invoker_rights("hr.pkg", "package p authid current_user as end;");
        let n = emit_invoker_rights_facts(&mut store, &prov(), sites);
        assert_eq!(n, 1);
        assert_eq!(store.by_kind(FactKind::InvokerRights).count(), 1);
    }

    // --- SEC007 / QUAL007 / QUAL003 substrate ---

    #[test]
    fn scan_ref_cursor_return_detects_sys_refcursor() {
        assert_eq!(
            scan_ref_cursor_return("hr.f", "function f return sys_refcursor is begin ... end;")
                .len(),
            1
        );
        assert!(
            scan_ref_cursor_return("hr.f", "function f return number is begin ... end;").is_empty()
        );
    }

    #[test]
    fn scan_dml_in_function_only_flags_functions_with_dml() {
        assert_eq!(
            scan_dml_in_function(
                "hr.f",
                "function f return number is begin insert into log values(1); return 1; end;"
            )
            .len(),
            1
        );
        // Function without DML: clean.
        assert!(
            scan_dml_in_function("hr.f", "function f return number is begin return 1; end;")
                .is_empty()
        );
        // Procedure with DML: not QUAL007's concern.
        assert!(
            scan_dml_in_function("hr.p", "procedure p is begin delete from t; end;").is_empty()
        );
    }

    #[test]
    fn scan_dml_in_function_finds_dml_after_identifier_decoy() {
        // Regression (oracle-73t1.7): `body_has_dml` must scan *every*
        // occurrence of a DML keyword, not just the first. A declared local
        // whose name ends in the keyword (`v_last_update`, `deleted_flag`,
        // `last_inserted`) is preceded by `_`, so its boundary check fails;
        // a first-hit-only scan would stop there and miss the genuine
        // row-level DML that follows.
        assert_eq!(
            scan_dml_in_function(
                "hr.f",
                "function f(p int) return number is v_last_update date; \
                 begin update t set c = 1 where id = p; return 1; end;",
            )
            .len(),
            1,
            "decoy `v_last_update` local must not mask the genuine `update t`",
        );
        assert_eq!(
            scan_dml_in_function(
                "hr.f",
                "function f(p int) return number is deleted_flag char(1); \
                 begin delete from t where id = p; return 1; end;",
            )
            .len(),
            1,
            "decoy `deleted_flag` local must not mask the genuine `delete from t`",
        );
        assert_eq!(
            scan_dml_in_function(
                "hr.f",
                "function f(p int) return number is last_inserted int; \
                 begin insert into log values (p); return 1; end;",
            )
            .len(),
            1,
            "decoy `last_inserted` local must not mask the genuine `insert into`",
        );
        // No genuine DML behind the decoy ⇒ still clean (no false positive).
        assert!(
            scan_dml_in_function(
                "hr.f",
                "function f return number is v_last_update date; begin return 1; end;",
            )
            .is_empty(),
            "identifier-only `v_last_update` must not be read as DML",
        );
    }

    #[test]
    fn scan_unbounded_bulk_collect_flags_missing_limit() {
        assert_eq!(
            scan_unbounded_bulk_collect(
                "hr.p",
                "begin select id bulk collect into ids from huge_t; end;"
            )
            .len(),
            1
        );
        // LIMIT present in the same statement ⇒ bounded ⇒ no site.
        assert!(
            scan_unbounded_bulk_collect(
                "hr.p",
                "begin fetch c bulk collect into ids limit 100; end;"
            )
            .is_empty()
        );
    }

    #[test]
    fn emit_sec007_qual007_qual003_facts_typed() {
        let mut store = FactStore::default();
        emit_ref_cursor_return_facts(
            &mut store,
            &prov(),
            scan_ref_cursor_return("hr.f", "function f return sys_refcursor is begin end;"),
        );
        emit_dml_in_function_facts(
            &mut store,
            &prov(),
            scan_dml_in_function(
                "hr.f",
                "function f return int is begin update t set a=1; end;",
            ),
        );
        emit_unbounded_bulk_collect_facts(
            &mut store,
            &prov(),
            scan_unbounded_bulk_collect("hr.p", "begin x bulk collect into y from t; end;"),
        );
        assert_eq!(store.by_kind(FactKind::RefCursorReturn).count(), 1);
        assert_eq!(store.by_kind(FactKind::DmlInFunction).count(), 1);
        assert_eq!(store.by_kind(FactKind::UnboundedBulkCollect).count(), 1);
    }

    // --- QUAL005 / QUAL008 substrate ---

    #[test]
    fn scan_deprecated_features_detects_known_forms() {
        let s = scan_deprecated_features(
            "hr.p",
            "begin dbms_job.submit(j); select a from t1, t2 where t1.id = t2.id (+); commit work; end;",
        );
        let feats: Vec<&str> = s.iter().map(|x| x.detail.as_str()).collect();
        assert!(feats.iter().any(|f| f.contains("dbms_job")));
        assert!(feats.iter().any(|f| f.contains("(+)")));
        assert!(feats.iter().any(|f| f.contains("WORK")));
        // Clean modern code: nothing.
        assert!(scan_deprecated_features("hr.q", "begin commit; end;").is_empty());
    }

    #[test]
    fn scan_deprecated_in_literal_not_flagged() {
        let s = scan_deprecated_features("hr.p", "begin msg := 'use dbms_job here'; end;");
        assert!(s.is_empty(), "{s:?}");
    }

    #[test]
    fn scan_deterministic_misuse_requires_pragma_and_construct() {
        let s = scan_deterministic_misuse(
            "hr.f",
            "function f return date deterministic is begin return sysdate; end;",
        );
        assert!(s.iter().any(|x| x.detail == "SYSDATE"));
        // DETERMINISTIC but pure: clean.
        assert!(
            scan_deterministic_misuse(
                "hr.g",
                "function g(x int) return int deterministic is begin return x*2; end;"
            )
            .is_empty()
        );
        // Non-deterministic but NOT marked deterministic: not QUAL008.
        assert!(
            scan_deterministic_misuse(
                "hr.h",
                "function h return date is begin return sysdate; end;"
            )
            .is_empty()
        );
    }

    #[test]
    fn emit_qual005_qual008_facts_typed() {
        let mut store = FactStore::default();
        emit_deprecated_feature_facts(
            &mut store,
            &prov(),
            scan_deprecated_features("hr.p", "begin dbms_job.run(1); end;"),
        );
        emit_deterministic_misuse_facts(
            &mut store,
            &prov(),
            scan_deterministic_misuse(
                "hr.f",
                "function f return int deterministic is begin insert into log values(1); return 1; end;",
            ),
        );
        assert_eq!(store.by_kind(FactKind::DeprecatedFeature).count(), 1);
        assert_eq!(store.by_kind(FactKind::DeterministicMisuse).count(), 1);
    }

    // --- QUAL006 / QUAL002 / DEP001 substrate ---

    #[test]
    fn scan_mutating_table_trigger_flags_self_reference() {
        let s = scan_mutating_table_trigger(
            "hr.trg_emp",
            "create trigger trg_emp before insert on employees for each row \
             begin select count(*) into n from employees; end;",
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].detail, "employees");
        // No FOR EACH ROW ⇒ statement-level trigger, no ORA-04091.
        assert!(
            scan_mutating_table_trigger(
                "hr.t",
                "create trigger t after insert on employees begin null; end;"
            )
            .is_empty()
        );
    }

    #[test]
    fn scan_log_without_reraise_flags_swallowed_after_log() {
        let s = scan_log_without_reraise(
            "hr.p",
            "begin go; exception when others then dbms_output.put_line('failed'); end;",
        );
        assert_eq!(s.len(), 1);
        // Re-raises ⇒ not swallowed.
        assert!(
            scan_log_without_reraise(
                "hr.p",
                "begin go; exception when others then logger.error('x'); raise; end;"
            )
            .is_empty()
        );
    }

    #[test]
    fn scan_cross_schema_write_flags_other_schema_dml() {
        let s = scan_cross_schema_write(
            "hr.pkg.p",
            "begin insert into fin.ledger(a) values(1); update hr.local set x=1; end;",
        );
        assert_eq!(s.len(), 1, "only the fin.* write is cross-schema: {s:?}");
        assert_eq!(s[0].detail, "fin.ledger");
    }

    // oracle-j1ep.2: Oracle's `FROM` is optional in a DELETE, so a FROM-less
    // cross-schema `DELETE fin.audit_log WHERE …` must still be flagged DEP001.
    // The old hardcoded `delete from ` lead matched only the FROM form, so a
    // FROM-less cross-schema delete silently escaped the scan.
    #[test]
    fn scan_cross_schema_write_flags_from_less_delete() {
        let s = scan_cross_schema_write("hr.proc1", "begin delete fin.audit_log where id = 5; end;");
        assert_eq!(s.len(), 1, "FROM-less cross-schema delete must flag: {s:?}");
        assert_eq!(s[0].detail, "fin.audit_log");
    }

    // Both `delete fin.audit` and `delete from fin.audit` must resolve to the
    // same cross-schema target.
    #[test]
    fn scan_cross_schema_write_from_and_from_less_delete_agree() {
        let with_from = scan_cross_schema_write("hr.p", "begin delete from fin.audit; end;");
        let without_from = scan_cross_schema_write("hr.p", "begin delete fin.audit; end;");
        assert_eq!(with_from.len(), 1);
        assert_eq!(without_from.len(), 1);
        assert_eq!(with_from[0].detail, without_from[0].detail);
        assert_eq!(without_from[0].detail, "fin.audit");
    }

    #[test]
    fn emit_qual006_qual002_dep001_facts_typed() {
        let mut store = FactStore::default();
        emit_mutating_table_trigger_facts(
            &mut store,
            &prov(),
            scan_mutating_table_trigger(
                "hr.trg",
                "create trigger trg before update on accounts for each row begin update accounts set z=1; end;",
            ),
        );
        emit_log_without_reraise_facts(
            &mut store,
            &prov(),
            scan_log_without_reraise(
                "hr.p",
                "begin x; exception when others then log_error('e'); end;",
            ),
        );
        emit_cross_schema_write_facts(
            &mut store,
            &prov(),
            scan_cross_schema_write("hr.p", "begin delete from fin.audit; end;"),
        );
        assert_eq!(store.by_kind(FactKind::MutatingTableTrigger).count(), 1);
        assert_eq!(store.by_kind(FactKind::LogWithoutReraise).count(), 1);
        assert_eq!(store.by_kind(FactKind::CrossSchemaWrite).count(), 1);
    }

    // --- SEC005 substrate ---

    #[test]
    fn scan_sensitive_public_synonym_flags_credential_target() {
        let s = scan_sensitive_public_synonym(
            "hr.ddl",
            "create public synonym emp_pwd for hr.employee_passwords;",
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].synonym, "emp_pwd");
        assert_eq!(s[0].target, "hr.employee_passwords");
    }

    #[test]
    fn scan_public_synonym_benign_not_flagged() {
        let s = scan_sensitive_public_synonym(
            "hr.ddl",
            "create public synonym depts for hr.departments;",
        );
        assert!(s.is_empty(), "benign synonym must not flag: {s:?}");
    }

    #[test]
    fn scan_private_synonym_not_flagged() {
        // Only PUBLIC synonyms are in scope.
        let s = scan_sensitive_public_synonym("hr.ddl", "create synonym sal for hr.salary_tbl;");
        assert!(s.is_empty(), "{s:?}");
    }

    #[test]
    fn emit_sensitive_public_synonym_facts_typed() {
        let mut store = FactStore::default();
        let sites = scan_sensitive_public_synonym(
            "hr.ddl",
            "create or replace public synonym bank_acct for fin.bank_accounts;",
        );
        let n = emit_sensitive_public_synonym_facts(&mut store, &prov(), sites);
        assert_eq!(n, 1);
        assert_eq!(store.by_kind(FactKind::SensitivePublicSynonym).count(), 1);
    }

    // --- PERF003 substrate ---

    #[test]
    fn scan_is_null_on_indexed_column_flags_correlated_case() {
        let s = scan_is_null_on_indexed_column(
            "hr.q",
            "create index emp_dt_ix on employees(deleted_at); \
             begin select id from employees where deleted_at is null; end;",
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].column, "deleted_at");
    }

    #[test]
    fn scan_is_null_without_index_not_flagged() {
        // No CREATE INDEX in source ⇒ catalog-only; out of scope (R13).
        let s = scan_is_null_on_indexed_column(
            "hr.q",
            "begin select id from employees where deleted_at is null; end;",
        );
        assert!(s.is_empty(), "{s:?}");
    }

    #[test]
    fn scan_is_not_null_never_matches() {
        let s = scan_is_null_on_indexed_column(
            "hr.q",
            "create index ix on t(c); begin select 1 from t where c is not null; end;",
        );
        assert!(s.is_empty(), "`is not null` must not match: {s:?}");
    }

    #[test]
    fn scan_is_null_on_non_indexed_column_not_flagged() {
        let s = scan_is_null_on_indexed_column(
            "hr.q",
            "create index ix on t(a); begin select 1 from t where b is null; end;",
        );
        assert!(s.is_empty(), "b is not indexed: {s:?}");
    }

    #[test]
    fn emit_is_null_on_indexed_column_facts_typed() {
        let mut store = FactStore::default();
        let sites = scan_is_null_on_indexed_column(
            "hr.q",
            "create unique index ix on t(k); begin delete from t where k is null; end;",
        );
        let n = emit_is_null_on_indexed_column_facts(&mut store, &prov(), sites);
        assert_eq!(n, 1);
        assert_eq!(store.by_kind(FactKind::IsNullOnIndexedColumn).count(), 1);
    }
}
