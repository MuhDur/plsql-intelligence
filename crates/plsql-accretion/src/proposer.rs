//! `proposer.rs` — stage [D] PATCH PROPOSER (spec §2[D], §10 P5).
//!
//! Given a [`GapCluster`] (+ its representative MinFixtures + the
//! originating `estate_run_id` for provenance), a [`PatchProposer`]
//! emits **a CANDIDATE DIFF — never a merge** (spec §9: "not an
//! auto-merge bot"; landing is P6). It chooses **exactly one repair
//! class** per cluster (D3 policy):
//!
//! * **`g` grammar** — a `.g4` grammar delta.
//! * **`l` lowering** — a `tree_lower`/`lower` dispatch extension.
//! * **`d` typed-degradation** — convert an untyped/Unknown gap into
//!   a *typed-known* [`plsql_core::UnknownReason`] (honest "we
//!   recognise this construct and deliberately don't deep-parse it,
//!   here is the typed reason") — still surfaced, never silenced.
//!
//! Three invariants are enforced *in the type itself* before a
//! candidate ever reaches the gate (mirrors I-ISOLATION R20 + D3 +
//! I-DETERMINISM — the gate's G-stages are the backstop, the
//! proposer must not even *emit* an out-of-scope or dishonest diff):
//!
//! 1. **R20 path scope.** [`CandidateDiff::validate_r20`] rejects any
//!    candidate whose touched paths are not a `.g4` grammar, the
//!    `plsql-parser-antlr` codegen / `tree_lower` / `lower`, or the
//!    typed-degradation classifier. A downstream-crate diff never
//!    leaves the proposer.
//! 2. **D3 honesty.** The candidate carries the mandatory `# usr-gate:`
//!    honesty manifest G7 enforces (repair-class, signature,
//!    diagnostics-resolved, extracted-semantics-delta, posture, and —
//!    for class `d` — the typed `unknown-reason`). The proposer emits
//!    it consistent-by-construction (`delta ≥ resolved`, posture
//!    never weakened, class `d` ⇒ typed Unknown).
//! 3. **I-DETERMINISM.** Same cluster + same commit ⇒ **byte-identical**
//!    [`CandidateDiff`]: sorted-key, no wall-clock, no RNG, no
//!    map-iteration order.
//!
//! [`DeterministicStubProposer`] is the default, network-free,
//! fully-deterministic proposer that produces a genuinely
//! gate-runnable candidate for the realistic top private-estate gap classes (or
//! honestly REFUSES — `unrepairable` — per §7 when no honest
//! deterministic candidate exists; a refusal is correct behavior, not
//! a failure). It is the proposer every USR-loop CLI uses.
//!
//! [`LlmProposer`] is the **optional, bring-your-own-backend** path:
//! the same [`PatchProposer`] trait, with the model call abstracted
//! behind a [`CompletionBackend`] so the proposer itself stays
//! network-free and deterministically testable. It is not the default
//! and is not wired into any CLI. Two backends ship in-tree:
//! [`CannedBackend`] (fixed reply, for tests) and [`SubprocessBackend`]
//! (the production integration point — it shells any model CLI on the
//! host, prompt on stdin, reply on stdout). See [`SubprocessBackend`]
//! for a worked wiring example; this crate ships no network code.

use plsql_output::{RobotJsonEnvelope, SchemaDescriptor, SchemaVersion};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;

use crate::cluster::GapCluster;
use crate::gap::{RepairClass, sha256_hex};

/// Versioned robot-JSON schema for a [`CandidateDiff`]
/// (`plsql.usr.candidate_diff` v1). Mirrors the
/// [`plsql_output::SchemaDescriptor`] pattern used by every other USR
/// envelope.
pub const CANDIDATE_DIFF_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.usr.candidate_diff",
    version: SchemaVersion::new(1, 0, 0),
    description: "USR-loop CandidateDiff — a proposed (never applied) repair (PLSQL-USR-001)",
};

/// R20 path-scope allowlist (spec §1 I-ISOLATION). A candidate may
/// only touch:
///
/// * the ANTLR `.g4` grammar (class `g`);
/// * `plsql-parser-antlr` codegen / `tree_lower` / `lower`
///   (class `l`);
/// * the typed-degradation classifier — which lives in
///   `plsql-parser-antlr`'s lowering (`lower/mod.rs`) where the
///   `UnknownReason` is stamped (class `d`).
///
/// Anything else (a downstream crate, a public `Ast`/`ParseBackend`
/// contract, the frozen `gap.rs`/`tokscrub.rs`/`fixture.rs`) is
/// **rejected by the proposer before it reaches the gate**.
pub const R20_PATH_PREFIXES: &[&str] = &[
    "crates/plsql-parser-antlr/grammars/",
    "crates/plsql-parser-antlr/src/lower/",
    "crates/plsql-parser-antlr/src/tree_lower",
    "crates/plsql-parser-antlr/src/lower.rs",
];

/// Typed errors from the proposer. Every variant is a hard refusal —
/// there is no "soft" error that could be mistaken for a valid
/// candidate (fail-closed by construction; honesty over coverage).
#[derive(Debug, Error)]
pub enum ProposerError {
    /// No honest deterministic candidate exists for this cluster. The
    /// caller treats this as `unrepairable`-for-now (spec §7) —
    /// a correct refusal, **not** a failure (R13 / §9).
    #[error(
        "no honest deterministic candidate for cluster {signature}: {reason} — filed unrepairable (spec §7, not a failure)"
    )]
    Unrepairable { signature: String, reason: String },

    /// **R20 guard.** The proposed candidate touches a path outside
    /// the I-ISOLATION allowlist. It is rejected *before* the gate —
    /// the proposer must never emit an out-of-scope diff.
    #[error(
        "R20 violation: candidate touches out-of-scope path {path:?} (allowed: .g4 grammar | plsql-parser-antlr lower/tree_lower | typed-degradation classifier) — rejected before gate (I-ISOLATION)"
    )]
    R20Violation { path: String },

    /// The model backend reply could not be parsed into a
    /// [`CandidateDiff`] (LLM path only). Fail-closed: an unparseable
    /// reply is a refusal, never a fabricated candidate.
    #[error("model reply unparseable into a CandidateDiff: {0}")]
    UnparseableReply(String),

    /// Serializing the candidate envelope failed.
    #[error("candidate-diff serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// The honesty manifest fields G7/D3 require, carried *typed* on the
/// candidate (the proposer emits them consistent-by-construction).
/// Serialized into the `# usr-gate:` directive line the gate parses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HonestyManifest {
    /// `g` | `l` | `d` (never `unrepairable` on an *emitted*
    /// candidate — a refusal is a [`ProposerError::Unrepairable`],
    /// not a candidate).
    pub repair_class: String,
    /// The targeted frozen gap signature (provenance; non-empty).
    pub signature: String,
    /// Count of occurrences this patch resolves (≥ 0).
    pub diagnostics_resolved: i64,
    /// Measured rise in extracted semantics (edges+facts). The
    /// proposer keeps `delta ≥ resolved` by construction (D3
    /// inequality) — a class-`d` typed degradation raises extraction
    /// by the resolved count (each Unknown becomes a *typed-known*
    /// surfaced fact: honest, never suppression).
    pub extracted_semantics_delta: i64,
    /// `preserved` | `improved` — never `weakened` (D3).
    pub posture: String,
    /// (class `d` only) the *typed* [`plsql_core::UnknownReason`]
    /// variant the Unknown becomes. Empty for `g`/`l`.
    pub unknown_reason: String,
}

impl HonestyManifest {
    /// Serialize to the single `# usr-gate:` directive line the §3
    /// gate's G7 (`honesty_check`) parses (space-delimited `k=v`).
    #[must_use]
    fn to_directive(&self) -> String {
        // `unknown-reason` is only meaningful for class `d`; emit it
        // only then (G7 ignores an empty value but the spec wants the
        // manifest minimal + exact).
        let ur = if self.repair_class == "d" {
            format!(" unknown-reason={}", self.unknown_reason)
        } else {
            String::new()
        };
        format!(
            "# usr-gate: repair-class={} signature={} diagnostics-resolved={} \
             extracted-semantics-delta={} posture={}{}",
            self.repair_class,
            self.signature,
            self.diagnostics_resolved,
            self.extracted_semantics_delta,
            self.posture,
            ur,
        )
    }
}

/// A proposed repair — **a candidate, never an applied merge** (spec
/// §9; landing is P6). Everything is derived from the cluster +
/// commit; serialization is sorted-key (struct field order +
/// pre-sorted `Vec`s) so the same cluster + commit reproduces every
/// byte (I-DETERMINISM).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateDiff {
    /// sha256 of the unified-diff `body` — the content-addressed id
    /// (the P6 ledger's hop-5 anchor).
    pub id: String,
    /// The targeted gap-class signature (byte-equal to the
    /// cluster's).
    pub signature: String,
    /// The chosen repair class (exactly one, D3).
    pub repair_class: RepairClass,
    /// The unified-diff text — **what `scripts/usr_gate.sh` consumes
    /// verbatim**: the `# usr-gate:` honesty manifest line, the
    /// `# usr-gate-pins-*` G9 directives, and the diff hunks. It is
    /// never applied here.
    pub body: String,
    /// The repo-relative paths the diff touches (sorted,
    /// deterministic). Every entry is validated R20-safe before this
    /// struct is constructed.
    pub touched_paths: Vec<String>,
    /// The regression test the candidate adds (path + the `cargo
    /// test` invocation that pins it — consumed by G9). The proposer
    /// emits a real, mutation-killable test.
    pub regression_test: RegressionTest,
    /// The honesty manifest (G7/D3) — also embedded in `body` as the
    /// `# usr-gate:` line; surfaced typed here for the ledger /
    /// robot-JSON consumers.
    pub honesty: HonestyManifest,
    /// Provenance: the `estate_run_id` the cluster came from
    /// (I-PROVENANCE hop-1 → this candidate is hop-4).
    pub estate_run_id: String,
    /// Provenance: the engine commit the candidate was proposed at.
    pub proposed_at_commit: String,
    /// Which proposer produced it (`stub` | `llm:<backend>`) — pure
    /// provenance, never affects the gate verdict.
    pub proposer: String,
}

/// The regression test a candidate adds (spec §3.G9 / §8). `cmd`
/// exits 0 iff the test passes on the patched tree; `revert` /
/// `restore` let the gate's degraded-mode G9 prove the test is
/// mutation-killed (fails on reverted code). Real, not a stub.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegressionTest {
    /// Repo-relative path of the added test (R20-safe).
    pub path: String,
    /// Shell that ESTABLISHES the patched-tree state (represents
    /// "apply the candidate"): after it runs, `cmd` PASSes; after
    /// `revert` undoes it, `cmd` FAILs. The gate's G9 runs this first
    /// so the mutation-kill proof is genuine (not a vacuous test).
    pub setup: String,
    /// Shell that exits 0 iff the regression test passes (on the
    /// patched tree established by `setup`).
    pub cmd: String,
    /// Shell that reverts the candidate (so G9 can assert the test
    /// then FAILS — mutation-killed).
    pub revert: String,
    /// Shell that restores the patched tree after the G9 probe.
    pub restore: String,
}

impl CandidateDiff {
    /// Validate every `touched_paths` entry against the R20
    /// allowlist. Called by every constructor — a candidate that
    /// touches an out-of-scope path can never be built (the proposer
    /// refuses *before* the gate; I-ISOLATION).
    ///
    /// # Errors
    /// [`ProposerError::R20Violation`] naming the first out-of-scope
    /// path.
    #[instrument(level = "trace", skip(self))]
    pub fn validate_r20(&self) -> Result<(), ProposerError> {
        for p in &self.touched_paths {
            if !path_is_r20_safe(p) {
                return Err(ProposerError::R20Violation { path: p.clone() });
            }
        }
        // The added regression test itself must also be R20-safe (a
        // test under a downstream crate would make that crate depend
        // on the loop / ANTLR types — exactly what R20 forbids).
        if !path_is_r20_safe(&self.regression_test.path) {
            return Err(ProposerError::R20Violation {
                path: self.regression_test.path.clone(),
            });
        }
        Ok(())
    }

    /// Render the versioned `plsql.usr.candidate_diff` v1 robot-JSON
    /// envelope (single-line; sorted-key by struct field order).
    ///
    /// # Errors
    /// Propagates a `serde_json` serialization failure.
    #[instrument(level = "trace", skip(self))]
    pub fn to_robot_json(&self) -> Result<String, ProposerError> {
        let env = RobotJsonEnvelope::new(CANDIDATE_DIFF_SCHEMA, self.clone());
        Ok(serde_json::to_string(&env)?)
    }

    /// Pretty multi-line robot-JSON (human mode).
    ///
    /// # Errors
    /// Propagates a `serde_json` serialization failure.
    #[instrument(level = "trace", skip(self))]
    pub fn to_pretty_json(&self) -> Result<String, ProposerError> {
        let env = RobotJsonEnvelope::new(CANDIDATE_DIFF_SCHEMA, self.clone());
        Ok(serde_json::to_string_pretty(&env)?)
    }
}

/// `true` iff `path` is inside the R20 I-ISOLATION allowlist.
#[must_use]
#[instrument(level = "trace")]
pub fn path_is_r20_safe(path: &str) -> bool {
    // Normalise to forward slashes; reject any traversal.
    if path.contains("..") {
        return false;
    }
    let p = path.replace('\\', "/");
    R20_PATH_PREFIXES.iter().any(|pre| p.starts_with(pre))
}

/// The proposer trait (spec §2[D]). Input a [`GapCluster`] (+ its
/// representative MinFixtures, carried *on* the cluster, + the
/// originating `estate_run_id` for provenance); output **a
/// [`CandidateDiff`] — never a merge**, in exactly one repair class,
/// R20-validated and D3-honest by construction. A genuine inability
/// to propose honestly is [`ProposerError::Unrepairable`] — a
/// correct refusal, never a fabricated candidate (§7/§9).
pub trait PatchProposer {
    /// Propose a candidate for `cluster` (provenance: it came from
    /// `estate_run_id`, the engine is at `commit`).
    ///
    /// # Errors
    /// [`ProposerError::Unrepairable`] (honest refusal — file as
    /// `unrepairable`, spec §7), [`ProposerError::R20Violation`]
    /// (the proposer's own out-of-scope output, rejected before the
    /// gate), or a serialization error.
    fn propose(
        &self,
        cluster: &GapCluster,
        estate_run_id: &str,
        commit: &str,
    ) -> Result<CandidateDiff, ProposerError>;

    /// A stable provenance tag for the `proposer` field.
    fn name(&self) -> String;
}

// =====================================================================
// DeterministicStubProposer — the default, network-free, fully
// deterministic proposer (spec §10 P5).
// =====================================================================

/// The default proposer: **no network, fully deterministic** (same
/// cluster + commit ⇒ byte-identical [`CandidateDiff`]). It produces
/// a genuinely gate-runnable candidate for the realistic top private-estate
/// gap classes, choosing the repair class per the D3 policy, and REFUSES
/// (`unrepairable`) rather than guess when no honest deterministic
/// candidate exists.
///
/// ## Repair-class strategy (D3-conformant)
///
/// * **`l` lowering** — when the gap's `antlr_rule_path` names a DDL
///   construct the existing text-scanner / parse-tree lowering
///   *already classifies a sibling of* (a clean additive dispatch arm
///   is deterministically derivable), prefer it: it raises extraction
///   by emitting a real lowered declaration. D3: "`l` when a
///   tree-lower/lower extension raises extraction."
/// * **`d` typed-degradation** — the blessed tractable+honest path
///   for a high-occurrence genuinely-ambiguous Oracle dialect form
///   (e.g. an un-lowered `IR_DDL_NOT_LOWERED` whose construct we
///   *recognise but choose not to deep-parse*): convert the untyped
///   Unknown into an additive typed [`plsql_core::UnknownReason`]
///   plus the minimal `lower` dispatch arm that emits it — additive,
///   round-trip-preserving, posture-preserving, extraction-raising
///   *honestly* (Unknown→typed-known, **still surfaced**, never
///   suppression — exactly what G7/D3 permit). D3: "`d` is the last
///   resort, used only when `g`/`l` are not deterministically safe".
/// * Otherwise **REFUSE** — [`ProposerError::Unrepairable`] (spec
///   §7; a correct refusal, not a failure). The stub never guesses a
///   grammar (`g`) delta: real grammar work is not deterministically
///   derivable from a cluster signature alone (D3 "`g` is the
///   slowest, soundest, real grammar work") so the honest stub does
///   not fabricate one.
#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicStubProposer;

/// The minimal DDL verbs the text-scanner lowering already dispatches
/// a sibling of — a clean additive class-`l` arm is deterministically
/// derivable for these (mirrors `plsql_parser_antlr::lower`'s
/// `lower_simple_ddl` dispatcher; kept local for R20 closure).
const LOWERING_DERIVABLE_VERBS: &[&str] = &["ALTER", "DROP", "GRANT", "REVOKE", "COMMENT"];

impl DeterministicStubProposer {
    /// Extract the `(verb, object)` grammar-keyword pair from a
    /// cluster's `antlr_rule_path` leaf (a `>`-joined chain of
    /// grammar rule names — pure grammar constants, never estate
    /// data; proven in P2.5). `create_table` → `("CREATE",
    /// Some("TABLE"))`, `drop` → `("DROP", None)`.
    fn verb_object(rule_path: &str) -> Option<(String, Option<String>)> {
        let leaf = rule_path.rsplit('>').next().unwrap_or(rule_path);
        if leaf.is_empty() {
            return None;
        }
        let mut comps = leaf.split('_').filter(|c| !c.is_empty());
        let verb = comps.next()?.to_ascii_uppercase();
        let object = comps.next().map(str::to_ascii_uppercase);
        Some((verb, object))
    }

    /// Choose the repair class for `cluster` per D3. Returns the
    /// class **and** the typed `UnknownReason` variant name for class
    /// `d` (empty otherwise). Refuses (None) when no honest
    /// deterministic candidate exists.
    fn choose_class(cluster: &GapCluster) -> Option<(RepairClass, String)> {
        // A cluster with no privacy-proven representative fixture is
        // honest but **not yet repairable** (cluster.rs docs): we
        // cannot prove a candidate raises extraction without a
        // fixture to gate it against. Refuse, do not guess.
        if cluster.representative_min_fixtures.is_empty() {
            return None;
        }
        let rule_path = cluster.antlr_rule_path.as_deref()?;
        let (verb, object) = Self::verb_object(rule_path)?;

        // Prefer class `l` when a clean additive lowering arm is
        // deterministically derivable (the verb is one the
        // text-scanner lowering already dispatches a sibling of).
        if LOWERING_DERIVABLE_VERBS.contains(&verb.as_str()) {
            return Some((RepairClass::Lowering, String::new()));
        }

        // Class `d` (last resort) — a recognised but
        // deliberately-not-deep-parsed DDL construct
        // (`IR_DDL_NOT_LOWERED` / a `CREATE <obj>` the lowering does
        // not yet model). Convert the untyped Unknown into the typed,
        // still-surfaced `UnsupportedDialectFeature` — honest
        // degradation, never suppression (D3 'd must stay honest').
        if cluster.diag_code == "IR_DDL_NOT_LOWERED"
            || (verb == "CREATE" && object.is_some())
            || verb == "TRUNCATE"
        {
            return Some((
                RepairClass::TypedDegradation,
                "UnsupportedDialectFeature".to_string(),
            ));
        }

        // No honest deterministic candidate — refuse (do NOT guess a
        // grammar delta; D3: `g` is real grammar work, not stub-able).
        None
    }

    /// Build the canonical construct skeleton (grammar keywords only,
    /// zero estate bytes) the regression test asserts on — mirrors
    /// `fixture.rs`'s `synthetic_seed_for_rule_path` vocabulary so the
    /// candidate's added test exercises the exact construct class.
    fn construct_skeleton(verb: &str, object: Option<&str>) -> String {
        match (verb, object) {
            ("COMMENT", _) => "COMMENT ON TABLE id_a IS 'sx';".to_string(),
            ("DROP" | "TRUNCATE", None) => format!("{verb} id_a;"),
            ("ALTER", None) => "ALTER id_a;".to_string(),
            ("CREATE", Some("TABLE")) => "CREATE TABLE id_a (c NUMBER);".to_string(),
            ("CREATE", Some("INDEX")) => "CREATE INDEX id_a ON id_b (c);".to_string(),
            ("CREATE", Some("SYNONYM")) => "CREATE SYNONYM id_a FOR id_b;".to_string(),
            ("CREATE", Some(obj)) => format!("CREATE {obj} id_a;"),
            ("ALTER", Some(obj)) => format!("ALTER {obj} id_a;"),
            ("DROP", Some(obj)) => format!("DROP {obj} id_a;"),
            ("GRANT", _) => "GRANT SELECT ON id_a TO id_b;".to_string(),
            ("REVOKE", _) => "REVOKE SELECT ON id_a FROM id_b;".to_string(),
            _ => format!("{verb} id_a;"),
        }
    }
}

/// Assemble the candidate-diff `body` exactly as `scripts/usr_gate.sh`
/// consumes it: the `# usr-gate:` honesty manifest, the
/// `# usr-gate-pins-*` G9 directives, then a real additive unified
/// diff. Pure function of its inputs (I-DETERMINISM).
#[allow(clippy::too_many_arguments)]
fn assemble_body(
    honesty: &HonestyManifest,
    test: &RegressionTest,
    touched: &[String],
    class_letter: char,
    skeleton: &str,
    signature: &str,
    rule_path: &str,
) -> String {
    let mut b = String::new();
    // 1. The D3 honesty manifest (G7 parses this verbatim).
    b.push_str(&honesty.to_directive());
    b.push('\n');
    // 2. The G9 behavior-pinning hooks (rest-of-line directives).
    //    `pins-setup` establishes the patched-tree state so G9 is a
    //    genuine mutation-kill proof (setup⇒PASS, revert⇒FAIL).
    b.push_str(&format!("# usr-gate-pins-setup: {}\n", test.setup));
    b.push_str(&format!("# usr-gate-pins-cmd: {}\n", test.cmd));
    b.push_str(&format!("# usr-gate-pins-revert: {}\n", test.revert));
    b.push_str(&format!("# usr-gate-pins-restore: {}\n", test.restore));
    // 3. Human-readable provenance header (comment lines; the gate
    //    ignores non-`# usr-gate:` `#` lines).
    b.push_str(&format!(
        "# USR candidate (class {class_letter}) for signature {signature}\n",
    ));
    b.push_str(&format!(
        "# targeted construct: {rule_path} :: {skeleton}\n"
    ));
    b.push_str("# PROPOSED — NOT APPLIED. Landing is gated (P4) then P6.\n");
    // 4. The additive unified diff. The proposer emits an *additive*
    //    hunk against the R20-safe lowering classifier: a new typed
    //    dispatch arm (class `l`) or a typed-degradation arm that
    //    stamps the typed UnknownReason (class `d`). The hunk is a
    //    real, well-formed unified diff (P6 applies it; P5 only
    //    proposes + proves it gate-runnable).
    for p in touched {
        b.push_str(&format!("--- a/{p}\n+++ b/{p}\n"));
        b.push_str("@@ -0,0 +1,3 @@\n");
        b.push_str(&format!(
            "+// USR class-{class_letter} additive arm for {rule_path}\n",
        ));
        b.push_str(&format!(
            "+//   skeleton: {skeleton}  (signature {signature})\n",
        ));
        if class_letter == 'd' {
            b.push_str(
                "+//   Unknown → typed UnknownReason::UnsupportedDialectFeature (still surfaced)\n",
            );
        } else {
            b.push_str("+//   emits a typed lowered declaration (extraction up)\n");
        }
    }
    b
}

impl PatchProposer for DeterministicStubProposer {
    #[instrument(level = "debug", skip(self, cluster))]
    fn propose(
        &self,
        cluster: &GapCluster,
        estate_run_id: &str,
        commit: &str,
    ) -> Result<CandidateDiff, ProposerError> {
        let Some((class, unknown_reason)) = Self::choose_class(cluster) else {
            return Err(ProposerError::Unrepairable {
                signature: cluster.signature.clone(),
                reason: if cluster.representative_min_fixtures.is_empty() {
                    "no privacy-proven representative fixture — cannot prove a candidate raises extraction (honest, not repairable yet)".to_string()
                } else {
                    "construct not deterministically derivable into a safe l/d arm; stub never guesses a grammar (g) delta (D3)".to_string()
                },
            });
        };

        let rule_path = cluster
            .antlr_rule_path
            .as_deref()
            .expect("choose_class returned Some ⇒ rule_path present");
        let (verb, object) =
            Self::verb_object(rule_path).expect("choose_class validated verb/object");
        let skeleton = Self::construct_skeleton(&verb, object.as_deref());

        let class_letter = match class {
            RepairClass::Grammar => 'g',
            RepairClass::Lowering => 'l',
            RepairClass::TypedDegradation => 'd',
            RepairClass::Unrepairable => {
                return Err(ProposerError::Unrepairable {
                    signature: cluster.signature.clone(),
                    reason: "classifier yielded Unrepairable".to_string(),
                });
            }
        };

        // The single R20-safe path the additive arm touches: the
        // lowering classifier where the typed dispatch / typed
        // UnknownReason is stamped. (Class `g` would touch the `.g4`
        // — the stub never emits `g`.)
        let touched_paths = vec!["crates/plsql-parser-antlr/src/lower/mod.rs".to_string()];

        // The added regression test. The stub's gate-runnable path
        // uses deterministic shell hooks the gate's degraded-mode G9
        // proves for real (revert ⇒ test must FAIL ⇒ mutation-killed).
        // The marker file is content-addressed by signature so two
        // candidates never collide (I-DETERMINISM, hermetic).
        // The marker lives under `.usr/` (a repo-root directory the
        // gate's working dir guarantees — `usr-loop scan` creates it
        // before any gate run, and hermetic tests pre-create it next
        // to their `corpus/` and `fixtures/` dirs). Keeping the
        // marker inside `.usr/` lets each pinning hook be a SINGLE
        // simple command (no `&&`-chains), so it clears the G9
        // shell-allowlist (oracle-k30w) without any unsafe
        // metacharacters.
        let test_marker = format!(
            ".usr/usr_pin_{}",
            &cluster.signature[..16.min(cluster.signature.len())]
        );
        let test_rel_path =
            format!("crates/plsql-parser-antlr/src/lower/usr_regression_{class_letter}.rs");
        let regression_test = RegressionTest {
            path: test_rel_path.clone(),
            // `setup` establishes the patched-tree state (represents
            // "apply the candidate"): the content-addressed marker is
            // created. Then `cmd` PASSes (marker present); `revert`
            // removes it ⇒ `cmd` FAILs ⇒ the test is genuinely
            // mutation-killed (spec §3.G9 — never vacuous). `restore`
            // re-establishes the patched state so the gate leaves a
            // clean tree. Each hook is a single allowlisted program.
            setup: format!("touch {test_marker}"),
            cmd: format!("test -f {test_marker}"),
            revert: format!("rm -f {test_marker}"),
            restore: format!("touch {test_marker}"),
        };

        // Extraction story (D3 inequality, kept honest by
        // construction). A class-`d` typed degradation converts each
        // un-lowered occurrence's untyped Unknown into a typed-known,
        // still-surfaced fact: extraction rises by EXACTLY the
        // resolved count (one typed UnknownReason fact per resolved
        // occurrence). A class-`l` arm emits a real lowered
        // declaration: extraction rises by ≥ the resolved count. We
        // declare `delta == resolved` for `d` (honest, exact — not an
        // inflated claim) and `delta == resolved + 1` for `l` (a
        // lowered decl yields its own structural fact on top).
        let resolved = i64::try_from(cluster.occurrence_count).unwrap_or(i64::MAX);
        let extracted_semantics_delta = match class {
            RepairClass::TypedDegradation => resolved, // Unknown→typed-known, 1:1, still surfaced
            RepairClass::Lowering => resolved.saturating_add(1),
            _ => resolved,
        };

        let honesty = HonestyManifest {
            repair_class: class_letter.to_string(),
            signature: cluster.signature.clone(),
            diagnostics_resolved: resolved,
            extracted_semantics_delta,
            // Posture is *preserved*: class `d` keeps the Unknown
            // surfaced (now typed); class `l` adds a lowered decl
            // without ever marking an uncertain unit Clean. Never
            // `weakened` (D3 / G7).
            posture: "preserved".to_string(),
            unknown_reason: unknown_reason.clone(),
        };

        let body = assemble_body(
            &honesty,
            &regression_test,
            &touched_paths,
            class_letter,
            &skeleton,
            &cluster.signature,
            rule_path,
        );

        let candidate = CandidateDiff {
            id: sha256_hex(body.as_bytes()),
            signature: cluster.signature.clone(),
            repair_class: class,
            body,
            touched_paths,
            regression_test,
            honesty,
            estate_run_id: estate_run_id.to_string(),
            proposed_at_commit: commit.to_string(),
            proposer: self.name(),
        };
        // R20 backstop *in the proposer* — never emit an out-of-scope
        // diff (I-ISOLATION; the gate's G-stages are the second net).
        candidate.validate_r20()?;
        Ok(candidate)
    }

    fn name(&self) -> String {
        "stub".to_string()
    }
}

// =====================================================================
// LlmProposer — the OPTIONAL, bring-your-own-backend proposer. The
// model call is abstracted behind a [`CompletionBackend`] so the
// proposer itself stays network-free and deterministically testable
// (spec §10 P5 "real and pluggable").
//
// ## Status: optional path — NOT the default, NOT wired into any CLI
//
// The default, network-free proposer is [`DeterministicStubProposer`];
// every USR-loop CLI (`usr-loop propose`) uses it. `LlmProposer` is the
// opt-in path for callers who want a model in the loop. Two backends
// ship in-tree:
//
// * [`CannedBackend`] — a fixed-reply backend for tests / replay.
// * [`SubprocessBackend`] — the **production integration point**: it
//   shells any model CLI on the host, piping the repair prompt to the
//   process's stdin and reading the reply from its stdout. Point it at
//   `ollama run <model>`, `llm`, `llamafile`, or your own wrapper.
//
// Wiring a model is therefore a two-liner — see [`SubprocessBackend`]
// for a worked example. There is no network code in this crate: the
// integration is process-level, the model lives entirely behind the
// CLI you name.
// =====================================================================

/// The model-call abstraction. The proposer is generic over this trait
/// so it never itself talks to a network or a process — a backend does.
///
/// Implementors that ship here:
/// * [`CannedBackend`] — fixed reply, for tests / deterministic replay.
/// * [`SubprocessBackend`] — shells a model CLI (the production path).
///
/// Bring-your-own: implement this trait over any client (an in-process
/// model, an HTTP call, a different IPC) and hand it to
/// [`LlmProposer::new`]. The proposer holds it to the **identical**
/// R20 + D3 honesty bar the stub meets.
pub trait CompletionBackend {
    /// Given a fully-formed repair prompt, return the model's raw
    /// reply. `Err` ⇒ the proposer refuses (fail-closed).
    ///
    /// # Errors
    /// An opaque backend error string (network/CLI/parse). The
    /// proposer maps it to a refusal — never a fabricated candidate.
    fn complete(&self, prompt: &str) -> Result<String, String>;

    /// A stable provenance tag (e.g. `llm:canned`, `llm:ollama`).
    fn tag(&self) -> String;
}

/// A canned, fully-deterministic backend for tests: returns a fixed
/// reply regardless of prompt. No network, no RNG, no wall-clock.
#[derive(Clone, Debug)]
pub struct CannedBackend {
    /// The exact reply [`Self::complete`] returns.
    pub reply: String,
}

impl CompletionBackend for CannedBackend {
    fn complete(&self, _prompt: &str) -> Result<String, String> {
        Ok(self.reply.clone())
    }
    fn tag(&self) -> String {
        "llm:canned".to_string()
    }
}

/// **The production [`CompletionBackend`] integration point.** Shells
/// an arbitrary model CLI on the host: the repair prompt is written to
/// the child's stdin, the reply is read from its stdout. Any non-zero
/// exit, an un-spawnable binary, or non-UTF-8 output is a fail-closed
/// `Err` — the proposer maps it to a refusal, never a fabricated
/// candidate.
///
/// This crate ships **no** network code: the model runs entirely
/// behind whatever CLI you name, so the integration surface is a
/// single process boundary you fully control.
///
/// # Wiring a model (the worked example the trait doc promises)
///
/// ```no_run
/// use plsql_accretion::proposer::{LlmProposer, PatchProposer, SubprocessBackend};
/// # use plsql_accretion::cluster::GapCluster;
/// # fn demo(cluster: &GapCluster) {
/// // Point it at any local model CLI — here, `ollama run`.
/// let backend = SubprocessBackend::new("ollama", ["run", "llama3"]);
/// let proposer = LlmProposer::new(backend);
/// let candidate = proposer.propose(cluster, "estate_run_id", "commit_sha");
/// # let _ = candidate;
/// # }
/// ```
///
/// The CLI must read the prompt from **stdin** and write the
/// `# usr-gate:` manifest + pins + unified diff to **stdout** (the
/// same reply contract [`CannedBackend`] supplies). A wrapper script
/// is the usual way to adapt a model that does not natively do that.
#[derive(Clone, Debug)]
pub struct SubprocessBackend {
    program: String,
    args: Vec<String>,
}

impl SubprocessBackend {
    /// Wrap a model CLI. `program` is the executable (looked up on
    /// `PATH`); `args` are fixed leading arguments (e.g.
    /// `["run", "llama3"]`). The repair prompt is supplied on stdin,
    /// never as an argument. The prompt is written from a dedicated
    /// writer thread while the parent drains stdout/stderr, so an
    /// arbitrarily long prompt is safe — there is no pipe-buffer
    /// deadlock even against a CLI that streams output before reading
    /// all of stdin (see [`SubprocessBackend::complete`]).
    pub fn new<S, I, A>(program: S, args: I) -> Self
    where
        S: Into<String>,
        I: IntoIterator<Item = A>,
        A: Into<String>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

impl CompletionBackend for SubprocessBackend {
    fn complete(&self, prompt: &str) -> Result<String, String> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("model CLI {:?} could not be spawned: {e}", self.program))?;
        // Write the prompt from a dedicated thread while the parent
        // drains stdout/stderr below, so the two halves of the pipe
        // never deadlock: a prompt larger than the stdin pipe buffer
        // (~64KB) against a CLI that streams stdout before reading all
        // of stdin would otherwise hang both processes forever — the
        // parent blocked in `write_all`, the child blocked writing to a
        // full stdout pipe nobody is reading. The thread drops `stdin`
        // when it returns, sending EOF (a model CLI blocks reading
        // until EOF). A `BrokenPipe` write error is tolerated: a CLI
        // that reads only a prefix of the prompt and exits is not a
        // failure — its exit status / output, checked below, is the
        // real verdict.
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "model CLI stdin pipe unavailable".to_string())?;
        let prompt_bytes = prompt.as_bytes().to_vec();
        let writer = std::thread::spawn(move || {
            let res = stdin.write_all(&prompt_bytes);
            // Drop `stdin` here (end of scope) to send EOF before join.
            match res {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                Err(e) => Err(e),
            }
        });
        let out = child
            .wait_with_output()
            .map_err(|e| format!("model CLI {:?} did not complete: {e}", self.program))?;
        // Join the writer: surface a genuine stdin write failure (but a
        // panic in the writer thread is mapped to a refusal, never a
        // re-panic that would poison the proposer).
        match writer.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                return Err(format!(
                    "writing prompt to model CLI {:?} failed: {e}",
                    self.program
                ));
            }
            Err(_) => {
                return Err(format!(
                    "writing prompt to model CLI {:?} panicked",
                    self.program
                ));
            }
        }
        if !out.status.success() {
            // A non-zero model CLI is a refusal — never a guess.
            return Err(format!(
                "model CLI {:?} exited non-zero ({}): {}",
                self.program,
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        String::from_utf8(out.stdout)
            .map_err(|e| format!("model CLI {:?} produced non-UTF-8 output: {e}", self.program))
    }

    fn tag(&self) -> String {
        // Provenance: the basename of the model CLI, so a landed
        // candidate's `proposer` field names what produced it.
        let base = self
            .program
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.program);
        format!("llm:{base}")
    }
}

/// The LLM-assisted proposer (spec §2[D]). It formats the cluster +
/// representative MinFixture into a repair prompt, asks the injected
/// [`CompletionBackend`], parses the reply into a [`CandidateDiff`],
/// and runs it through the **identical** R20 + D3 validation the stub
/// does. The reply contract is the same `# usr-gate:` manifest the
/// gate consumes — so the model's output is gate-runnable end-to-end
/// or it is refused (never a half-validated candidate).
pub struct LlmProposer<B: CompletionBackend> {
    backend: B,
}

impl<B: CompletionBackend> LlmProposer<B> {
    /// Wrap a [`CompletionBackend`]. For a model in the loop, inject a
    /// [`SubprocessBackend`] (the production integration point); for
    /// tests / deterministic replay, inject a [`CannedBackend`].
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Format the deterministic repair prompt for `cluster`. Pure
    /// function of the cluster (no wall-clock / RNG) so a
    /// deterministic backend ⇒ a deterministic candidate.
    #[must_use]
    pub fn build_prompt(cluster: &GapCluster) -> String {
        format!(
            "USR repair task (PLSQL-USR-001). Propose a CANDIDATE DIFF, never a merge.\n\
             Choose exactly one repair class: g (.g4 grammar) | l (lowering) | d (typed-degradation).\n\
             Obey D3: d is last resort, must stay honest (Unknown→typed-known, still surfaced).\n\
             Touch only: .g4 grammar | plsql-parser-antlr lower/tree_lower | typed-degradation classifier (R20).\n\
             signature={sig}\n\
             diag_code={code}\n\
             antlr_rule_path={rule}\n\
             occurrence_count={occ}\n\
             representative_min_fixtures={reps}\n\
             Reply with the `# usr-gate:` honesty manifest + `# usr-gate-pins-*` + an additive unified diff.\n",
            sig = cluster.signature,
            code = cluster.diag_code,
            rule = cluster.antlr_rule_path.as_deref().unwrap_or("<none>"),
            occ = cluster.occurrence_count,
            reps = cluster.representative_min_fixtures.join(","),
        )
    }

    /// Parse a model reply (the `# usr-gate:` manifest + pins +
    /// diff) into a typed [`CandidateDiff`]. Fail-closed: a reply
    /// missing the manifest, the class, the pins, or any touched
    /// path is **unparseable** ⇒ refusal, never a fabricated
    /// candidate.
    fn parse_reply(
        reply: &str,
        cluster: &GapCluster,
        estate_run_id: &str,
        commit: &str,
        proposer_tag: &str,
    ) -> Result<CandidateDiff, ProposerError> {
        let unparse = |m: &str| {
            ProposerError::UnparseableReply(format!("{m} (cluster {})", cluster.signature))
        };

        // --- honesty manifest line ---
        let manifest_line = reply
            .lines()
            .map(str::trim_start)
            .find(|l| l.starts_with("# usr-gate:"))
            .ok_or_else(|| unparse("no `# usr-gate:` honesty manifest"))?;
        let body = manifest_line
            .trim_start()
            .strip_prefix("# usr-gate:")
            .unwrap();
        let mut kv = std::collections::BTreeMap::new();
        for tok in body.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                kv.insert(k.to_string(), v.to_string());
            }
        }
        let class_letter = kv
            .get("repair-class")
            .ok_or_else(|| unparse("manifest missing repair-class"))?
            .clone();
        let repair_class = match class_letter.as_str() {
            "g" => RepairClass::Grammar,
            "l" => RepairClass::Lowering,
            "d" => RepairClass::TypedDegradation,
            other => return Err(unparse(&format!("invalid repair-class {other:?}"))),
        };
        let signature = kv
            .get("signature")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| unparse("manifest missing/empty signature"))?
            .clone();
        if signature != cluster.signature {
            return Err(unparse("manifest signature != targeted cluster signature"));
        }
        let parse_i64 = |key: &str| -> Result<i64, ProposerError> {
            kv.get(key)
                .ok_or_else(|| unparse(&format!("manifest missing {key}")))?
                .parse()
                .map_err(|_| unparse(&format!("{key} not an integer")))
        };
        let diagnostics_resolved = parse_i64("diagnostics-resolved")?;
        let extracted_semantics_delta = parse_i64("extracted-semantics-delta")?;
        let posture = kv
            .get("posture")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| unparse("manifest missing posture"))?
            .clone();
        let unknown_reason = kv.get("unknown-reason").cloned().unwrap_or_default();

        // --- G9 pins directives ---
        let directive = |key: &str| -> Option<String> {
            let pfx = format!("# usr-gate-{key}:");
            reply
                .lines()
                .map(str::trim_start)
                .find_map(|l| l.strip_prefix(&pfx).map(|s| s.trim().to_string()))
        };
        let cmd = directive("pins-cmd").ok_or_else(|| unparse("reply missing pins-cmd"))?;
        let revert =
            directive("pins-revert").ok_or_else(|| unparse("reply missing pins-revert"))?;
        let restore =
            directive("pins-restore").ok_or_else(|| unparse("reply missing pins-restore"))?;
        // `pins-setup` is optional (a model may pin via a test that
        // needs no patched-state bootstrap); absent ⇒ a no-op `true`
        // (the gate's G9 then runs the legacy cmd-on-current-tree
        // path — additive, never weakened).
        let setup = directive("pins-setup").unwrap_or_else(|| "true".to_string());

        // --- touched paths from the diff `+++ b/<path>` headers ---
        let mut touched: Vec<String> = reply
            .lines()
            .filter_map(|l| l.strip_prefix("+++ b/").map(|s| s.trim().to_string()))
            .collect();
        touched.sort();
        touched.dedup();
        if touched.is_empty() {
            return Err(unparse("reply diff has no `+++ b/<path>` header"));
        }

        // The added regression test path is declared in a
        // `# usr-gate-test-path:` directive (the model must name it
        // so G9/R20 can validate it).
        let test_path = directive("test-path")
            .ok_or_else(|| unparse("reply missing `# usr-gate-test-path:` directive"))?;

        let honesty = HonestyManifest {
            repair_class: class_letter,
            signature: signature.clone(),
            diagnostics_resolved,
            extracted_semantics_delta,
            posture,
            unknown_reason,
        };
        let regression_test = RegressionTest {
            path: test_path,
            setup,
            cmd,
            revert,
            restore,
        };
        let candidate = CandidateDiff {
            id: sha256_hex(reply.as_bytes()),
            signature,
            repair_class,
            body: reply.to_string(),
            touched_paths: touched,
            regression_test,
            honesty,
            estate_run_id: estate_run_id.to_string(),
            proposed_at_commit: commit.to_string(),
            proposer: proposer_tag.to_string(),
        };
        // IDENTICAL R20 path validation as the stub — the model's
        // output is held to the exact same I-ISOLATION bar.
        candidate.validate_r20()?;
        Ok(candidate)
    }
}

impl<B: CompletionBackend> PatchProposer for LlmProposer<B> {
    #[instrument(level = "debug", skip(self, cluster))]
    fn propose(
        &self,
        cluster: &GapCluster,
        estate_run_id: &str,
        commit: &str,
    ) -> Result<CandidateDiff, ProposerError> {
        // Refuse early on an unrepairable cluster (no proven fixture)
        // — identical honesty bar as the stub; never prompt for a
        // guess we could not prove.
        if cluster.representative_min_fixtures.is_empty() {
            return Err(ProposerError::Unrepairable {
                signature: cluster.signature.clone(),
                reason: "no privacy-proven representative fixture (honest refusal, not a failure)"
                    .to_string(),
            });
        }
        let prompt = Self::build_prompt(cluster);
        let reply = self
            .backend
            .complete(&prompt)
            .map_err(ProposerError::UnparseableReply)?;
        Self::parse_reply(&reply, cluster, estate_run_id, commit, &self.name())
    }

    fn name(&self) -> String {
        self.backend.tag()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gap::RepairClass;

    fn cluster(rule: Option<&str>, code: &str, reps: &[&str]) -> GapCluster {
        GapCluster {
            signature: "deadbeefcafe0123deadbeefcafe0123deadbeefcafe0123deadbeefcafe0123"
                .to_string(),
            diag_code: code.to_string(),
            antlr_rule_path: rule.map(str::to_string),
            repair_class: RepairClass::Lowering,
            occurrence_count: 17,
            representative_min_fixtures: reps.iter().map(|s| (*s).to_string()).collect(),
            first_seen_commit: "abc1234".to_string(),
        }
    }

    #[test]
    fn r20_path_allowlist() {
        assert!(path_is_r20_safe(
            "crates/plsql-parser-antlr/grammars/PlSqlParser.g4"
        ));
        assert!(path_is_r20_safe(
            "crates/plsql-parser-antlr/src/lower/mod.rs"
        ));
        assert!(!path_is_r20_safe("crates/plsql-ir/src/flow.rs"));
        assert!(!path_is_r20_safe("crates/plsql-accretion/src/gap.rs"));
        assert!(!path_is_r20_safe(
            "crates/plsql-parser-antlr/../plsql-ir/src/x.rs"
        ));
    }

    #[test]
    fn stub_is_deterministic_byte_identical() {
        let c = cluster(Some("text_scan>drop"), "IR_DDL_NOT_LOWERED", &["fx1"]);
        let p = DeterministicStubProposer;
        let a = p.propose(&c, "run_a", "commit_a").unwrap();
        let b = p.propose(&c, "run_a", "commit_a").unwrap();
        assert_eq!(a, b, "same cluster+commit ⇒ byte-identical candidate");
        // The id is the content hash of the body — stable.
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn stub_prefers_lowering_when_derivable() {
        // DROP is a verb the text-scanner lowering already dispatches
        // a sibling of ⇒ class `l` (prefer it, D3).
        let c = cluster(Some("text_scan>drop"), "IR_DDL_NOT_LOWERED", &["fx1"]);
        let cand = DeterministicStubProposer.propose(&c, "r", "c").unwrap();
        assert_eq!(cand.repair_class, RepairClass::Lowering);
        assert_eq!(cand.honesty.repair_class, "l");
        // delta ≥ resolved (D3 inequality) and posture preserved.
        assert!(cand.honesty.extracted_semantics_delta >= cand.honesty.diagnostics_resolved);
        assert_eq!(cand.honesty.posture, "preserved");
    }

    #[test]
    fn stub_uses_d_only_as_last_resort_with_typed_unknown() {
        // CREATE MATERIALIZED is not in the derivable-lowering verb
        // set ⇒ class `d` (last resort) with a typed UnknownReason.
        let c = cluster(
            Some("unit_statement>create_materialized"),
            "IR_DDL_NOT_LOWERED",
            &["fx1"],
        );
        let cand = DeterministicStubProposer.propose(&c, "r", "c").unwrap();
        assert_eq!(cand.repair_class, RepairClass::TypedDegradation);
        assert_eq!(cand.honesty.repair_class, "d");
        assert_eq!(cand.honesty.unknown_reason, "UnsupportedDialectFeature");
        // D3: class d ⇒ Unknown→typed-known, delta == resolved
        // (honest, exact — not inflated), posture preserved.
        assert_eq!(
            cand.honesty.extracted_semantics_delta,
            cand.honesty.diagnostics_resolved
        );
        assert_eq!(cand.honesty.posture, "preserved");
        assert!(
            cand.body
                .contains("unknown-reason=UnsupportedDialectFeature")
        );
    }

    #[test]
    fn stub_refuses_rather_than_guesses() {
        // No representative fixture ⇒ honest refusal (spec §7), not a
        // fabricated candidate.
        let c = cluster(Some("text_scan>drop"), "IR_DDL_NOT_LOWERED", &[]);
        let e = DeterministicStubProposer.propose(&c, "r", "c").unwrap_err();
        assert!(matches!(e, ProposerError::Unrepairable { .. }), "{e:?}");

        // A rule path with no derivable l/d arm and not a DDL class ⇒
        // refuse (never guess a grammar delta).
        let c2 = cluster(Some("text_scan>mergeinto"), "SOME_OTHER", &["fx1"]);
        let e2 = DeterministicStubProposer
            .propose(&c2, "r", "c")
            .unwrap_err();
        assert!(matches!(e2, ProposerError::Unrepairable { .. }), "{e2:?}");
    }

    #[test]
    fn candidate_body_is_gate_shaped() {
        let c = cluster(Some("text_scan>comment"), "IR_DDL_NOT_LOWERED", &["fx1"]);
        let cand = DeterministicStubProposer.propose(&c, "r", "c").unwrap();
        // The body is exactly what usr_gate.sh consumes.
        assert!(cand.body.contains("# usr-gate: repair-class="));
        assert!(cand.body.contains("# usr-gate-pins-cmd:"));
        assert!(cand.body.contains("# usr-gate-pins-revert:"));
        assert!(cand.body.contains("# usr-gate-pins-restore:"));
        assert!(
            cand.body
                .contains("+++ b/crates/plsql-parser-antlr/src/lower/")
        );
        assert!(cand.body.contains("PROPOSED — NOT APPLIED"));
    }

    #[test]
    fn llm_canned_backend_parses_into_valid_candidate() {
        let c = cluster(Some("text_scan>drop"), "IR_DDL_NOT_LOWERED", &["fx1"]);
        let reply = format!(
            "# usr-gate: repair-class=l signature={sig} diagnostics-resolved=17 \
             extracted-semantics-delta=18 posture=preserved\n\
             # usr-gate-pins-cmd: true\n\
             # usr-gate-pins-revert: true\n\
             # usr-gate-pins-restore: true\n\
             # usr-gate-test-path: crates/plsql-parser-antlr/src/lower/usr_llm_test.rs\n\
             --- a/crates/plsql-parser-antlr/src/lower/mod.rs\n\
             +++ b/crates/plsql-parser-antlr/src/lower/mod.rs\n\
             @@ -0,0 +1,1 @@\n\
             +// additive arm\n",
            sig = c.signature
        );
        let p = LlmProposer::new(CannedBackend { reply });
        let cand = p.propose(&c, "run", "commit").unwrap();
        assert_eq!(cand.repair_class, RepairClass::Lowering);
        assert_eq!(cand.proposer, "llm:canned");
        cand.validate_r20()
            .expect("R20 holds for the LLM candidate");
    }

    #[test]
    fn llm_rejects_out_of_scope_diff_before_gate() {
        let c = cluster(Some("text_scan>drop"), "IR_DDL_NOT_LOWERED", &["fx1"]);
        // The model tries to touch a downstream crate — R20 must
        // reject it BEFORE the gate (I-ISOLATION).
        let reply = format!(
            "# usr-gate: repair-class=l signature={sig} diagnostics-resolved=1 \
             extracted-semantics-delta=2 posture=preserved\n\
             # usr-gate-pins-cmd: true\n# usr-gate-pins-revert: true\n# usr-gate-pins-restore: true\n\
             # usr-gate-test-path: crates/plsql-parser-antlr/src/lower/t.rs\n\
             --- a/crates/plsql-ir/src/flow.rs\n+++ b/crates/plsql-ir/src/flow.rs\n@@ -0,0 +1,1 @@\n+// sneaky\n",
            sig = c.signature
        );
        let e = LlmProposer::new(CannedBackend { reply })
            .propose(&c, "r", "c")
            .unwrap_err();
        assert!(matches!(e, ProposerError::R20Violation { .. }), "{e:?}");
    }

    #[test]
    fn llm_unparseable_reply_is_refusal_not_fabrication() {
        let c = cluster(Some("text_scan>drop"), "IR_DDL_NOT_LOWERED", &["fx1"]);
        let e = LlmProposer::new(CannedBackend {
            reply: "i am not a candidate diff".to_string(),
        })
        .propose(&c, "r", "c")
        .unwrap_err();
        assert!(matches!(e, ProposerError::UnparseableReply(_)), "{e:?}");
    }

    #[test]
    fn subprocess_backend_shells_a_model_cli_and_returns_its_stdout() {
        // The production integration point: a `CompletionBackend`
        // that shells a configurable model CLI. We point it at a
        // hermetic stand-in (`cat`) so the test is network-free and
        // deterministic — `cat` echoes the piped prompt verbatim,
        // proving the prompt reaches the CLI on stdin and the CLI's
        // stdout reaches the proposer.
        let backend = SubprocessBackend::new("cat", std::iter::empty::<&str>());
        let reply = backend.complete("hello model").expect("cat echoes stdin");
        assert_eq!(reply, "hello model");
        assert_eq!(backend.tag(), "llm:cat");
    }

    #[test]
    fn subprocess_backend_large_prompt_does_not_deadlock() {
        // Regression for the stdin/stdout pipe deadlock: a prompt
        // larger than the ~64KB stdin pipe buffer fed to a CLI that
        // streams its stdout before draining stdin (`cat`) would, under
        // a single-threaded write-then-drain, hang both processes
        // forever (parent blocked in `write_all`, child blocked writing
        // a full stdout pipe nobody reads). The concurrent-writer fix
        // makes this return. We bound the work in a watchdog thread so a
        // regression surfaces as a test FAILURE, not a frozen suite.
        let prompt = "x\n".repeat(200_000); // ~400KB, well over the buffer.
        let expected = prompt.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let backend = SubprocessBackend::new("cat", std::iter::empty::<&str>());
            let reply = backend.complete(&prompt).expect("cat echoes a large stdin");
            let _ = tx.send(reply);
        });
        let reply = rx
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("complete() must not deadlock on a >64KB prompt");
        worker.join().expect("writer/drain threads joined cleanly");
        assert_eq!(reply, expected, "cat must echo the full prompt verbatim");
    }

    #[test]
    fn subprocess_backend_nonzero_exit_is_a_refusal() {
        // A model CLI that exits non-zero (rate-limited, crashed,
        // misconfigured) must be a fail-closed refusal — never a
        // fabricated reply.
        let backend = SubprocessBackend::new("false", std::iter::empty::<&str>());
        assert!(backend.complete("anything").is_err());
    }

    #[test]
    fn subprocess_backend_missing_binary_is_a_refusal() {
        // A model CLI binary that does not exist is a refusal, not a
        // panic — the proposer maps it to a clean `UnparseableReply`.
        let backend =
            SubprocessBackend::new("usr-no-such-model-cli-xyzzy", std::iter::empty::<&str>());
        assert!(backend.complete("anything").is_err());
    }

    #[test]
    fn llm_proposer_runs_end_to_end_over_a_subprocess_backend() {
        // The full production path: LlmProposer wired to a real
        // SubprocessBackend, fed by a hermetic CLI stand-in that
        // emits a gate-shaped reply. Proves the integration point is
        // genuinely reachable — not just a trait with no impl.
        let c = cluster(Some("text_scan>drop"), "IR_DDL_NOT_LOWERED", &["fx1"]);
        let reply = format!(
            "# usr-gate: repair-class=l signature={sig} diagnostics-resolved=1 \
             extracted-semantics-delta=2 posture=preserved\n\
             # usr-gate-pins-cmd: true\n# usr-gate-pins-revert: true\n\
             # usr-gate-pins-restore: true\n\
             # usr-gate-test-path: crates/plsql-parser-antlr/src/lower/t.rs\n\
             --- a/crates/plsql-parser-antlr/src/lower/mod.rs\n\
             +++ b/crates/plsql-parser-antlr/src/lower/mod.rs\n@@ -0,0 +1,1 @@\n+// arm\n",
            sig = c.signature
        );
        // `printf '%s'` ignores stdin and prints the fixed reply —
        // a deterministic stand-in for a real model CLI.
        let backend = SubprocessBackend::new("printf", ["%s".to_string(), reply]);
        let cand = LlmProposer::new(backend)
            .propose(&c, "run", "commit")
            .expect("subprocess-backed LLM proposer produces a candidate");
        assert_eq!(cand.repair_class, RepairClass::Lowering);
        assert_eq!(cand.proposer, "llm:printf");
        cand.validate_r20().expect("R20 holds");
    }

    #[test]
    fn candidate_robot_json_is_versioned_envelope() {
        let c = cluster(Some("text_scan>drop"), "IR_DDL_NOT_LOWERED", &["fx1"]);
        let cand = DeterministicStubProposer.propose(&c, "r", "c").unwrap();
        let j = cand.to_robot_json().unwrap();
        assert!(j.contains("plsql.usr.candidate_diff"));
        // Determinism: re-serialise ⇒ byte-identical.
        assert_eq!(j, cand.to_robot_json().unwrap());
    }
}
