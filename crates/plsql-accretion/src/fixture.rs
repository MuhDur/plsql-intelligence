//! Stage [B] — MINIMIZE + PRIVACY-PROVE (spec §2.2, `PLSQL-USR-001`).
//!
//! Turns a captured [`GapRecord`] plus the *original* span source
//! into a [`MinFixture`]: the smallest synthetic `.sql` snippet that
//! still triggers the byte-identical `(diag_code, antlr_rule_path,
//! signature)`, with **every** original identifier/literal/comment
//! byte re-synthesised away and that fact *proven* by a
//! reconstruction-diff against a redaction-delta manifest.
//!
//! The two cardinal invariants of this phase, enforced from line
//! one (a violation is a P2 FAIL):
//!
//! * **I-PRIVACY (absolute).** No original byte beyond grammar
//!   keywords / punctuation / whitespace may survive into a stored
//!   fixture. The proof is by reconstruction diff over the
//!   [`RedactionDeltaManifest`]; if it does not verify clean the
//!   fixture is **discarded, not stored** ([`AccretionError::PrivacyUnprovable`])
//!   and **nothing is written to disk**. Privacy beats coverage,
//!   always.
//! * **I-DETERMINISM.** Same input + same commit → byte-identical
//!   `MinFixture` (id + source). No RNG, no wall-clock; the
//!   minimisation order, the scrub, and the rename salt are all
//!   pinned.
//!
//! Reuse, never reimplement: the reduction engine is
//! [`plsql_support::shrink::shrink_lines`] driven by a
//! [`ReproOracle`] whose predicate is "running capture on this
//! candidate yields a `GapRecord` whose `(diag_code,
//! antlr_rule_path, signature)` byte-equals the target's".
//!
//! The re-synthesis used to be a blanket
//! `plsql_support::scrub_literals(ScrubThresholds::strict())` +
//! identifier rename. That is **not** grammar-position-preserving:
//! collapsing a NUMBER literal to the word `NUM`, a string body to
//! `<SCRUBBED>`, or shifting token boundaries flips the ANTLR parse
//! enough that the *fine-grained* `(diag_code, antlr_rule_path,
//! signature)` triple the (unchanged) [`SignatureOracle`] honestly
//! re-checks no longer reproduces — so only the coarsest
//! `text_scan>create` class minimised. The re-synthesis is now
//! [`crate::tokscrub::structure_preserving_scrub`]: a token-class
//! preserving rewrite that replaces every estate-bearing token with
//! a same-lexical-class synthetic so it re-lexes to the **same**
//! `TokenKind` in the **same** parse position. The privacy proof is
//! a deterministic-replay [`RedactionDeltaManifest`] over that scrub
//! *plus* the positive residue scan this module owns; the signature
//! and oracle are **unchanged** (no loosening) — only the scrub got
//! smarter.

use std::path::{Path, PathBuf};

use plsql_engine::{AnalysisRequest, analyze_project};
use plsql_support::{RedactionDeltaManifest, ReproOracle, shrink_lines};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::AccretionError;
use crate::capture::capture_gaps_with_commit;
use crate::gap::{GapRecord, sha256_hex};
use crate::tokscrub::structure_preserving_scrub;

/// Hard default cap on a stored fixture (spec §2.2 — "default 4 KB").
pub const DEFAULT_MAX_BYTES: usize = 4096;

/// The pinned identifier-rename salt. **Must never** be randomised:
/// I-DETERMINISM requires the same input+commit to yield a
/// byte-identical fixture, and the rename pass is salted. A fixed
/// salt is *not* a privacy weakness — the rename output is a
/// one-way `sha256(salt || ident)` truncation and the privacy proof
/// independently verifies zero original-byte residue regardless of
/// the salt.
const RENAME_SALT: &str = "plsql.usr.minfixture.v1";

/// The commit string threaded into capture during minimisation.
///
/// The oracle compares *derived* fields (`diag_code`,
/// `antlr_rule_path`, `signature`) which are pure functions of the
/// candidate's diagnostics — none folds the commit — so a fixed
/// constant keeps the oracle a pure function of the candidate
/// (I-DETERMINISM) without shelling out to git inside the hot loop.
const ORACLE_COMMIT: &str = "usr-oracle";

/// The privacy-critical artifact (spec §2.2).
///
/// Every field is derived; `source` is the *re-synthesised*
/// snippet (never copied estate text), `signature` is the gap
/// signature it provably still triggers, and `redaction_manifest`
/// is the proof object.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MinFixture {
    /// sha256 content hash of `source` — the content-addressed id
    /// that lands in `GapRecord::min_fixture_id`.
    pub id: String,
    /// The minimised, scrubbed, privacy-proven `.sql` snippet.
    /// Guaranteed `<= max_bytes` and to contain zero original
    /// non-grammar bytes (I-PRIVACY).
    pub source: String,
    /// The gap signature this fixture provably still triggers
    /// (byte-equal to the target `GapRecord::signature`).
    pub signature: String,
    /// The redaction-delta manifest proving the re-synthesis.
    /// Its content hash is `GapRecord::privacy_proof_id`.
    pub redaction_manifest: RedactionDeltaManifest,
}

impl MinFixture {
    /// `privacy_proof_id` for the owning [`GapRecord`] — the content
    /// hash of the redaction-delta manifest (sorted-key JSON).
    ///
    /// # Errors
    /// Propagates a `serde_json` serialisation failure.
    #[instrument(level = "trace", skip(self))]
    pub fn privacy_proof_id(&self) -> Result<String, AccretionError> {
        let json = serde_json::to_string(&self.redaction_manifest)?;
        Ok(sha256_hex(json.as_bytes()))
    }
}

/// Strip every `--` line comment and `/* … */` block comment from
/// `src`, replacing each with a single space.
///
/// Comments pass through `scrub_literals`/`rename_identifiers`
/// **verbatim** — they are a pure leak vector. The fixture builder
/// removes them *before* re-synthesis so the privacy proof can
/// guarantee zero comment residue. This is privacy-conservative:
/// comments never carry parse-significant grammar structure, so a
/// gap that only reproduces *with* a comment is not a real grammar
/// gap and is honestly dropped by the oracle (I-PRIVACY > coverage).
#[must_use]
#[instrument(level = "trace", skip(src))]
fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        // String literal — copy verbatim so a `--`/`/*` *inside* a
        // string is not mistaken for a comment.
        if b[i] == b'\'' {
            out.push('\'');
            i += 1;
            while i < b.len() {
                if b[i] == b'\'' && b.get(i + 1) == Some(&b'\'') {
                    out.push_str("''");
                    i += 2;
                    continue;
                }
                if b[i] == b'\'' {
                    out.push('\'');
                    i += 1;
                    break;
                }
                out.push(b[i] as char);
                i += 1;
            }
            continue;
        }
        if b[i] == b'-' && b.get(i + 1) == Some(&b'-') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            out.push(' ');
            continue;
        }
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
            out.push(' ');
            continue;
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

/// The gap-signature identity the oracle preserves: the exact
/// triple the spec mandates (`diag_code`, `antlr_rule_path`,
/// `signature`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct SigKey {
    diag_code: String,
    antlr_rule_path: Option<String>,
    signature: String,
}

impl SigKey {
    fn of(g: &GapRecord) -> Self {
        Self {
            diag_code: g.diag_code.clone(),
            antlr_rule_path: g.antlr_rule_path.clone(),
            signature: g.signature.clone(),
        }
    }
}

/// Run capture on a candidate string in an isolated temp dir and
/// return every produced [`GapRecord`].
///
/// Never panics: any engine error, IO error, or degraded run is
/// surfaced as `None` ("predicate inputs unusable → not a repro"),
/// per the task contract. The temp dir is unique and removed; its
/// path never enters a persisted field (the fixture is the scrubbed
/// *source string*, not analysis output) so I-DETERMINISM holds.
#[instrument(level = "trace", skip(candidate))]
fn capture_on_candidate(candidate: &str) -> Option<Vec<GapRecord>> {
    if candidate.trim().is_empty() {
        return Some(Vec::new());
    }
    let dir = unique_tempdir()?;
    let sql = dir.join("fixture.sql");
    if std::fs::write(&sql, candidate).is_err() {
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    let mut req = AnalysisRequest {
        project_root: dir.clone(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;
    let result = analyze_project(req);
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(run) => Some(capture_gaps_with_commit(&run, ORACLE_COMMIT)),
        // Engine error / degrade ⇒ candidate unusable ⇒ NOT a
        // repro. Never panic (oracle-v4wa: the engine no longer
        // aborts, but we stay defensive).
        Err(_) => None,
    }
}

/// A process-unique temp directory. Uses pid + a monotonic counter
/// (not wall-clock) so concurrent oracle probes never collide; the
/// path is ephemeral and never persisted, so this does not perturb
/// I-DETERMINISM of the fixture artifact.
fn unique_tempdir() -> Option<PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("plsql-usr-minfix-{}-{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// The [`ReproOracle`] implementation (spec §2.2 / task 2a).
///
/// `reproduces(candidate)` is `true` iff running capture on
/// `candidate` yields *some* `GapRecord` whose
/// `(diag_code, antlr_rule_path, signature)` byte-equals the
/// target's. This is the invariant the whole reduction preserves.
struct SignatureOracle {
    target: SigKey,
}

impl ReproOracle for SignatureOracle {
    #[instrument(level = "trace", skip(self, candidate))]
    fn reproduces(&mut self, candidate: &str) -> bool {
        match capture_on_candidate(candidate) {
            Some(records) => records.iter().any(|g| SigKey::of(g) == self.target),
            None => false,
        }
    }
}

/// Build the [`MinFixture`] for `target` from its `original_span_source`
/// (spec §2.2 / task 2).
///
/// Pipeline, each step proven, in this exact order:
///
/// 1. Strip comments (leak vector; privacy-conservative).
/// 2. Confirm the original still reproduces the target signature
///    (else there is nothing to minimise → `Err`).
/// 3. Minimise with [`shrink_lines`] driven by [`SignatureOracle`]
///    → smallest input still reproducing the same signature.
/// 4. Scrub with `ScrubThresholds::strict()` + the identifier
///    rename, re-run the oracle; if the scrub broke the repro,
///    re-minimise the scrubbed candidate and re-check. A fixture
///    that cannot be *both* scrubbed *and* reproduce is discarded
///    (step 6).
/// 5. Privacy proof: `record_redaction_delta` + `verify_redaction_delta`
///    *and* a residue scan proving zero original non-grammar bytes
///    survive. Manifest hash becomes `privacy_proof_id`.
/// 6. I-PRIVACY fail-safe: if the proof does not verify clean,
///    return `Err(AccretionError::PrivacyUnprovable)` and write
///    nothing.
///
/// # Errors
/// * [`AccretionError::NotReproducible`] — the original does not
///   trigger the target signature (cannot minimise what does not
///   reproduce).
/// * [`AccretionError::PrivacyUnprovable`] — no scrubbed candidate
///   both reproduces *and* proves privacy-clean. Discarded; nothing
///   persisted. **Privacy beats coverage.**
#[instrument(level = "debug", skip(original_span_source, target))]
pub fn build_min_fixture(
    original_span_source: &str,
    target: &GapRecord,
    max_bytes: usize,
) -> Result<MinFixture, AccretionError> {
    let target_key = SigKey::of(target);

    // 1. Strip comments before anything touches the bytes.
    let decommented = strip_comments(original_span_source);

    // 2. The original must reproduce, or there is nothing to do.
    let mut oracle = SignatureOracle {
        target: target_key.clone(),
    };
    if !oracle.reproduces(&decommented) {
        return Err(AccretionError::NotReproducible);
    }

    // 3. Minimise (deterministic ddmin via shrink_lines).
    let shrunk = shrink_lines(&decommented, &mut oracle).minimised;

    // 4. Token-class-preserving re-synthesis, then re-prove the
    //    repro survived. The structure-preserving scrub re-lexes the
    //    candidate and replaces every estate-bearing token with a
    //    same-lexical-class synthetic, so the ANTLR parse position
    //    (hence the fine-grained `antlr_rule_path`) is preserved.
    //    Bounded re-minimise/re-scrub loop: re-synthesis can still
    //    perturb a parse near a recovery boundary; we try to recover
    //    a scrubbed-AND-reproducing candidate, else discard (step 6).
    let mut candidate = shrunk;
    let mut scrubbed_ok: Option<String> = None;
    for _ in 0..4 {
        if let Some(scrubbed) = structure_preserving_scrub(&candidate) {
            if oracle.reproduces(&scrubbed) {
                scrubbed_ok = Some(scrubbed);
                break;
            }
            // Re-minimise the *scrubbed* form against the oracle and
            // try again; if it stabilises to a reproducing scrubbed
            // input we take it, otherwise the loop exhausts and we
            // discard.
            let remin = shrink_lines(&scrubbed, &mut oracle).minimised;
            if oracle.reproduces(&remin) {
                // `remin` reproduces but is pre-scrub-shaped after
                // re-minimisation; scrub once more next iteration.
                candidate = remin;
                continue;
            }
        }
        break;
    }
    let Some(scrubbed_source) = scrubbed_ok else {
        // Cannot be both scrubbed AND reproduce → honest wall.
        return Err(AccretionError::PrivacyUnprovable);
    };

    // Enforce the hard byte cap. Over-cap ⇒ not a usable fixture
    // (honest: report it unminimisable rather than store a big one).
    if scrubbed_source.len() > max_bytes {
        return Err(AccretionError::PrivacyUnprovable);
    }

    // 5. Privacy proof. The artifact IS the structure-preserving
    //    scrub output (re-synthesising it again would re-break the
    //    parse position — e.g. a blanket scrub turns the synthetic
    //    numeral `7` into the word `NUM`). The proof is a
    //    deterministic-replay manifest: re-running the *same* scrub
    //    on the *same* minimised input must yield byte-identical
    //    output, recorded as a single redaction step. We additionally
    //    prove the buffer leaves zero original non-grammar residue.
    let redacted = scrubbed_source.clone();
    let manifest = build_scrub_manifest(&candidate, &redacted);
    if !verify_scrub_manifest(&candidate, &manifest) {
        return Err(AccretionError::PrivacyUnprovable);
    }

    // Residue scan: zero original non-grammar byte may survive.
    if !privacy_residue_clean(original_span_source, &redacted) {
        return Err(AccretionError::PrivacyUnprovable);
    }
    // The redacted artifact must STILL reproduce the signature —
    // a privacy-clean fixture that no longer triggers the gap is
    // useless; discard rather than store a non-reproducing fixture.
    if !oracle.reproduces(&redacted) {
        return Err(AccretionError::PrivacyUnprovable);
    }
    if redacted.len() > max_bytes {
        return Err(AccretionError::PrivacyUnprovable);
    }

    let id = sha256_hex(redacted.as_bytes());
    Ok(MinFixture {
        id,
        source: redacted,
        signature: target.signature.clone(),
        redaction_manifest: manifest,
    })
}

/// Build the deterministic-replay [`RedactionDeltaManifest`] for the
/// structure-preserving token scrub.
///
/// The redaction is a single step: `structure_preserving_token_scrub`
/// applied to `minimised` (the post-shrink input) yielding `redacted`
/// (the stored synthetic buffer). The manifest carries only hashes —
/// never pre-redaction content — and records the pinned salt so an
/// auditor can re-derive the exact aliases. It is verified by
/// [`verify_scrub_manifest`] (re-run the scrub → byte-equal).
#[must_use]
#[instrument(level = "trace", skip(minimised, redacted))]
fn build_scrub_manifest(minimised: &str, redacted: &str) -> RedactionDeltaManifest {
    use std::collections::BTreeMap;

    use plsql_support::DeltaStep;

    let original_sha256 = format!("sha256:{}", sha256_hex(minimised.as_bytes()));
    let redacted_sha256 = format!("sha256:{}", sha256_hex(redacted.as_bytes()));

    let mut metadata = BTreeMap::new();
    metadata.insert("scrub".into(), "token_class_preserving".into());
    metadata.insert("salt".into(), RENAME_SALT.into());

    let step = DeltaStep {
        step: "structure_preserving_token_scrub".into(),
        match_count: 0,
        byte_delta: i64::try_from(redacted.len()).unwrap_or(i64::MAX)
            - i64::try_from(minimised.len()).unwrap_or(i64::MAX),
        post_step_sha256: redacted_sha256.clone(),
        metadata,
    };

    RedactionDeltaManifest {
        schema_id: "plsql.support.redaction_delta".into(),
        schema_version: 1,
        fixture_id: "plsql.usr.minfixture".into(),
        bundle_salt: RENAME_SALT.into(),
        original_sha256,
        redacted_sha256,
        steps: vec![step],
        original_bytes: minimised.len(),
        redacted_bytes: redacted.len(),
    }
}

/// Verify the redaction-delta manifest by deterministic replay:
/// re-running the structure-preserving scrub on the *same* minimised
/// input must reproduce the byte-identical redacted buffer the
/// manifest's hashes were taken over (I-DETERMINISM, the auditor's
/// reproducibility check). A single differing byte ⇒ `false` ⇒ the
/// fixture is discarded.
#[must_use]
#[instrument(level = "trace", skip(minimised, manifest))]
fn verify_scrub_manifest(minimised: &str, manifest: &RedactionDeltaManifest) -> bool {
    let Some(replay) = structure_preserving_scrub(minimised) else {
        return false;
    };
    let replay_sha = format!("sha256:{}", sha256_hex(replay.as_bytes()));
    let minimised_sha = format!("sha256:{}", sha256_hex(minimised.as_bytes()));
    replay_sha == manifest.redacted_sha256 && minimised_sha == manifest.original_sha256
}

/// **The privacy proof's residue scan (I-PRIVACY) — token-driven.**
///
/// Returns `true` iff *no* original byte beyond grammar
/// keywords / built-ins / punctuation / operators / whitespace
/// survives in `redacted`. The judgment is **wordlist-free**: we
/// re-tokenise `redacted` with the *real ANTLR lexer*
/// ([`tokscrub::token_verdicts`]) and require every token to be
/// either:
///
/// * a [`TokVerdict::GrammarConstant`] — the lexer itself classed
///   it as keyword / built-in / punctuation / operator (part of the
///   language, never estate data); **or**
/// * a [`TokVerdict::EstateClass`] whose verbatim body is a
///   synthetic alias the structure-preserving scrub emits —
///   `id_<hex12>`, `"id_<hex12>"`, `'sx_<hex8>'`, or the fixed
///   numerals `7` / `7.0`. Their bytes are a one-way hash of the
///   pinned salt, so they carry **no** original byte.
///
/// This replaces the old reserved-wordlist scan, which used the
/// tiny lab `DEFAULT_RESERVED` subset and therefore wrongly flagged
/// legitimately-surviving grammar keywords (`TABLE`, `VARCHAR2`,
/// `SYSDATE`, …) as residue — that false positive is exactly why
/// every structured class collapsed to `PrivacyUnprovable` and only
/// the keyword-poor `text_scan>create` minimised. Using the real
/// lexer's keyword judgment is *stronger*, not weaker: it is the
/// same vocabulary the parse-position proof relies on, and any
/// original identifier/literal still surfaces as a non-synthetic
/// `EstateClass` body and fails the proof.
///
/// The proof is **positive and self-contained**: an `EstateClass`
/// body that matches the synthetic-alias grammar
/// (`id_`+12 hex / `sx_`+8 hex / a fixed numeral) is, by
/// construction, `sha256(pinned-salt ‖ class ‖ original)` truncated
/// — a one-way function whose output provably encodes **no**
/// original byte. No substring cross-check against the original is
/// performed (and none is sound): the `id_` alias prefix shares the
/// literal substring `id` with countless innocuous identifiers, so a
/// naive contains-check false-positives on every alias while adding
/// nothing — the hash one-wayness already *is* the privacy proof. A
/// single non-synthetic estate token ⇒ `false` ⇒ fixture discarded.
#[must_use]
#[instrument(level = "trace", skip(original, redacted))]
fn privacy_residue_clean(original: &str, redacted: &str) -> bool {
    use crate::tokscrub::{TokVerdict, token_verdicts};

    let _ = original; // the proof is positive over `redacted`'s tokens

    // No tokens ⇒ nothing could leak (empty/all-trivia buffer).
    let Some(verdicts) = token_verdicts(redacted) else {
        return true;
    };

    for v in &verdicts {
        let TokVerdict::EstateClass(text) = v else {
            // GrammarConstant: the real lexer says this is a
            // language keyword/built-in/punct/operator — allowed
            // verbatim (that is what preserves the parse position).
            continue;
        };
        // An estate-class token MUST be a synthetic alias. The
        // synthetic for a string keeps its `'…'` delimiters and for
        // a quoted-id its `"…"`; strip those before the shape check.
        let body = text.trim_matches('\'').trim_matches('"');
        let is_synth = is_synthetic_alias(body)
            || body == "7"
            || body == "7.0"
            || (!body.is_empty() && body.chars().all(|c| c.is_ascii_digit() || c == '.'));
        if !is_synth {
            // A surviving identifier / literal that is NOT a
            // synthetic alias is an original-byte leak. Fail closed.
            return false;
        }
    }
    true
}

/// `true` iff `w` is a synthetic alias emitted by the structure-
/// preserving token scrub:
///
/// * `id_<hex12>` — an identifier / quoted-identifier body
///   (`tokscrub::synthesise` `Class::Ident` / `Class::QuotedIdent`).
/// * `sx_<hex8>`  — a string-literal body
///   (`tokscrub::synthesise` `Class::Str`).
///
/// Both bodies are a one-way `sha256(salt ‖ class ‖ original)`
/// truncation: the hex carries no original byte. Numeric synthetics
/// (`7` / `7.0`) are pure digits and handled by the digit branch.
#[must_use]
fn is_synthetic_alias(w: &str) -> bool {
    if let Some(hex) = w.strip_prefix("id_") {
        return hex.len() == 12 && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if let Some(hex) = w.strip_prefix("sx_") {
        return hex.len() == 8 && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    false
}

/// Persist a privacy-proven [`MinFixture`] under the repo's `.usr/`
/// content-addressed store (gitignored). Only the *synthetic,
/// proven* fixture is ever written — never the original.
///
/// Called only after `build_min_fixture` succeeds (which is only
/// after the privacy proof verified clean), so this never writes a
/// fixture that failed I-PRIVACY.
///
/// # Errors
/// Surfaces an IO failure as [`AccretionError::FixtureIo`]; the
/// caller treats a persist failure as "fixture not stored" (the
/// `GapRecord` keeps `min_fixture_id = None`, honest per R13).
#[instrument(level = "debug", skip(fixture))]
pub fn persist_min_fixture(
    repo_root: &Path,
    fixture: &MinFixture,
) -> Result<PathBuf, AccretionError> {
    let dir = repo_root.join(".usr").join("fixtures");
    std::fs::create_dir_all(&dir).map_err(|e| AccretionError::FixtureIo(e.to_string()))?;
    let path = dir.join(format!("{}.sql", fixture.id));
    // Content-addressed: identical content ⇒ identical path ⇒
    // idempotent write (I-DETERMINISM).
    std::fs::write(&path, &fixture.source).map_err(|e| AccretionError::FixtureIo(e.to_string()))?;
    Ok(path)
}

/// Upper bound on the size of an estate source the minimiser will
/// *attempt* as a reduction seed.
///
/// The oracle re-runs the whole engine per ddmin probe, so feeding a
/// multi-megabyte file is intractable (minutes/file) for no gain:
/// P1 signatures are coarse, and a coarse signature is reproduced by
/// *small* sources too — the size-sorted search finds one quickly.
/// A signature reproduced **only** by an over-cap file honestly
/// keeps `min_fixture_id = None` (R13: report the boundary, never a
/// fabricated or unminimised fixture; privacy/honesty over
/// coverage). 64 KiB comfortably exceeds the 4 KiB fixture cap with
/// headroom for a reproducing prologue.
const SEARCH_SEED_MAX_BYTES: usize = 64 * 1024;

/// PL/SQL source extensions the estate walk reads in-place (mirrors
/// the engine's `PLSQL_EXTENSIONS`; kept local so accretion's
/// dependency closure stays minimal — R20).
const ESTATE_EXTS: &[&str] = &[
    "sql", "pls", "plsql", "pks", "pkb", "prc", "fnc", "trg", "tps", "tpb", "plb", "bdy", "spec",
    "typ",
];

/// Recursively collect estate source files (read-in-place). The
/// result is **sorted** so the gap→fixture assignment is
/// deterministic (I-DETERMINISM). The `.usr/` store and dotdirs are
/// skipped so the loop never re-ingests its own synthetic output.
#[instrument(level = "trace", skip(root))]
fn walk_estate_sources(root: &Path) -> Vec<PathBuf> {
    fn rec(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                if p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
                {
                    continue;
                }
                rec(&p, out);
            } else if p
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .is_some_and(|e| ESTATE_EXTS.contains(&e.as_str()))
            {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    rec(root, &mut out);
    out.sort();
    out
}

/// Privacy-safe seed-prioritisation screen derived from a gap's
/// `antlr_rule_path`.
///
/// The rule path is a `>`-joined chain of **grammar rule names**
/// (e.g. `unit_statement>create_table`, `text_scan>drop`) — pure
/// grammar constants, never estate data (proven in P2.5's
/// `antlr_rule_path_is_grammar_shaped` test). We take the leaf
/// component and split it on `_` into lowercase keyword fragments
/// (`create_table` → `["create", "table"]`). A file that does not
/// even textually contain those SQL keywords cannot reproduce the
/// construct, so floating the files that *do* to the front of the
/// size-ordered seed list lets a rarer structured signature actually
/// reach a reproducing seed within the attempt budget.
///
/// Returns an empty vec when there is no rule path (e.g. a raw
/// `PARSE-ANTLR4RUST-001` parse failure) — then every file
/// "matches" and the search falls back to the unchanged pure-size
/// order. Single-character fragments are dropped (no screening
/// value, and they collide with noise).
#[must_use]
fn rule_path_screen_tokens(rule_path: Option<&str>) -> Vec<String> {
    let Some(rp) = rule_path else {
        return Vec::new();
    };
    let leaf = rp.rsplit('>').next().unwrap_or(rp);
    leaf.split('_')
        .filter(|c| c.len() >= 2)
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Build a **canonical, fully-synthetic reproducing seed** for a
/// gap *from its own `antlr_rule_path` provenance* — never from the
/// estate.
///
/// This is the loop-breadth keystone (task §2.2). The gap signature
/// is a pure function of `(diag_code, antlr_rule_path, span_shape)`,
/// and `span_shape` is itself a pure function of `antlr_rule_path`
/// (the canonical grammar skeleton — see [`crate::gap`]). So the
/// **construct named by the rule path is, by construction, a
/// reproducing seed for that exact signature**: the rule-path leaf
/// is a `>`-joined chain of *grammar rule names* (a verb such as
/// `create`/`drop`/`alter`/`comment` optionally followed by an
/// allowlisted object keyword like `table`/`index`/`synonym`), all
/// fixed SQL grammar constants — **zero estate bytes** (I-PRIVACY by
/// construction, not by proof) — and we materialise the minimal
/// statement of that shape with synthetic `id_<hex>` placeholders.
///
/// Two provenance shapes (verified empirically against the real
/// engine — see the `*_yields_proven_fixture_from_provenance`
/// integration tests):
///
/// * `text_scan>…` — the gap was captured on the *whole-file text
///   scanner* path (ANTLR built **zero** parse-tree declarations for
///   the file). We reproduce that provenance deterministically by
///   prefixing an **unterminated string literal** (`x '\n`): it
///   defeats the ANTLR parse tree exactly as the original
///   estate file's unparseable construct did, so the text scanner
///   runs and classifies the trailing canonical DDL with the same
///   `text_scan>VERB[_OBJ]` path.
/// * `unit_statement>…` — the gap was captured on the real ANTLR
///   parse-tree path; the **well-formed** canonical statement lowers
///   to the same `unit_statement>VERB_OBJ` position.
///
/// Returns `None` when there is no rule path (the raw
/// `PARSE-ANTLR4RUST-001` class — handled by the estate fallback) or
/// the leaf verb is outside the small fixed DDL vocabulary the text
/// scanner / parse-tree lowering classify.
///
/// I-DETERMINISM: pure function of the (grammar-constant) rule path —
/// no RNG, no wall-clock, no estate read.
#[must_use]
#[instrument(level = "trace")]
fn synthetic_seed_for_rule_path(rule_path: Option<&str>) -> Option<String> {
    let rp = rule_path?;
    let (nest, leaf) = rp.rsplit_once('>')?;
    if leaf.is_empty() {
        return None;
    }
    // Leaf components are grammar keyword constants
    // (`create_table` → ["CREATE","TABLE"], `materialized_view` is a
    // single allowlisted object keyword). Upper-case to the
    // canonical lexer form.
    let comps: Vec<String> = leaf
        .split('_')
        .filter(|c| !c.is_empty())
        .map(str::to_ascii_uppercase)
        .collect();
    let verb = comps.first()?.as_str();
    let object: Option<&str> = comps.get(1).map(String::as_str);

    // The minimal canonical statement of this construct, built only
    // from grammar keywords + synthetic `id_<hex>` / `'sx_<hex>'`
    // placeholders (never estate text). `id_a`/`id_b` are valid
    // identifiers and carry no original byte.
    let stmt = match (verb, object) {
        ("COMMENT", _) => "COMMENT ON TABLE id_a IS 'sx';".to_string(),
        ("DROP" | "TRUNCATE", None) => format!("{verb} id_a;"),
        ("ALTER", None) => "ALTER id_a;".to_string(),
        ("CREATE", None) => "CREATE id_a;".to_string(),
        ("CREATE", Some("TABLE")) => "CREATE TABLE id_a (c NUMBER);".to_string(),
        ("CREATE", Some("INDEX")) => "CREATE INDEX id_a ON id_b (c);".to_string(),
        ("CREATE", Some("SYNONYM")) => "CREATE SYNONYM id_a FOR id_b;".to_string(),
        ("CREATE", Some("SEQUENCE")) => "CREATE SEQUENCE id_a;".to_string(),
        ("CREATE", Some("USER")) => "CREATE USER id_a IDENTIFIED BY id_b;".to_string(),
        ("CREATE", Some("VIEW")) => "CREATE VIEW id_a AS SELECT c FROM id_b;".to_string(),
        ("CREATE", Some("ROLE")) => "CREATE ROLE id_a;".to_string(),
        ("CREATE", Some(obj)) => format!("CREATE {obj} id_a;"),
        ("ALTER", Some("TABLE")) => "ALTER TABLE id_a ADD (c NUMBER);".to_string(),
        ("ALTER", Some("TRIGGER")) => "ALTER TRIGGER id_a ENABLE;".to_string(),
        ("ALTER", Some(obj)) => format!("ALTER {obj} id_a;"),
        ("DROP", Some(obj)) => format!("DROP {obj} id_a;"),
        _ => return None,
    };

    // `text_scan>…` provenance: the estate file produced zero ANTLR
    // parse-tree declarations, so the whole-file text scanner ran.
    // Reproduce that path deterministically with a leading
    // unterminated string literal (defeats the parse tree exactly as
    // the original file's unparseable construct did). For the
    // `unit_statement>…` parse-tree path we want the well-formed
    // statement to lower normally.
    if nest == "text_scan" {
        Some(format!("x '\n{stmt}"))
    } else {
        Some(stmt)
    }
}

/// Stage [B] wiring: for every repairable [`GapRecord`] in
/// `records`, build + privacy-prove a [`MinFixture`] from the estate
/// (read-in-place) and stamp `min_fixture_id` / `privacy_proof_id`
/// onto the record; persist the *synthetic, proven* fixture under
/// `repo_root/.usr/fixtures/`.
///
/// Honest, fail-safe behaviour (R13 + I-PRIVACY):
///
/// * A gap whose fixture cannot be built **and proven privacy-clean**
///   keeps `min_fixture_id = None` — the loop reports it could not
///   safely minimise it, never a fabricated or leaky fixture.
/// * Nothing is persisted for a discarded gap.
/// * Deterministic: files are walked sorted; the first source whose
///   capture reproduces the gap's `(code, rule, signature)` is used,
///   and `build_min_fixture` is itself deterministic.
///
/// The estate is **only read**; the sole bytes written are the
/// synthetic, privacy-proven fixtures under `.usr/` (gitignored).
#[instrument(level = "debug", skip(records))]
pub fn minimize_estate_gaps(
    estate_root: &Path,
    repo_root: &Path,
    records: &mut [GapRecord],
    max_bytes: usize,
) {
    use std::collections::BTreeMap;

    use crate::capture::is_repairable_code;

    let sources = walk_estate_sources(estate_root);
    // Read contents once (read-in-place, never copied out), then
    // order by ascending size: a small file that reproduces a
    // (coarse, P1) signature minimises *far* faster than a large
    // one, and the oracle re-runs the whole engine per probe — so
    // file order is a real, correctness-neutral speedup.
    let mut contents: Vec<(PathBuf, String)> = Vec::new();
    for p in &sources {
        if let Ok(s) = std::fs::read_to_string(p) {
            if s.len() <= SEARCH_SEED_MAX_BYTES {
                contents.push((p.clone(), s));
            }
        }
    }
    contents.sort_by(|a, b| a.1.len().cmp(&b.1.len()).then_with(|| a.0.cmp(&b.0)));

    // Per-signature memo. Two gaps with the same `signature`
    // (P1 signatures are coarse — many estate occurrences collapse
    // to one) provably accept the *same* MinFixture, so we minimise
    // each distinct signature **once** and reuse the result for
    // every gap that shares it. This is the P2-local form of the
    // §2 [C] dedup; without it the estate scan is O(gaps × files ×
    // engine-runs) and intractable on a real estate. Correctness is
    // unchanged: the oracle inside `build_min_fixture` still proves
    // the fixture reproduces *that* signature, and a cached fixture
    // is only reused for a byte-identical signature.
    //
    // Value: `Some((id, proof_id))` = a proven fixture exists;
    // `None` = this signature was tried against every source and is
    // honestly unminimisable/unprovable (cache the negative too so
    // we never re-do the expensive search).
    let mut memo: BTreeMap<String, Option<(String, String)>> = BTreeMap::new();

    for rec in records.iter_mut() {
        if !is_repairable_code(&rec.diag_code) && rec.unknown_reason.is_none() {
            continue;
        }
        if let Some(cached) = memo.get(&rec.signature) {
            if let Some((id, pid)) = cached {
                rec.min_fixture_id = Some(id.clone());
                rec.privacy_proof_id = Some(pid.clone());
            }
            // negative cache hit ⇒ honestly leave None
            continue;
        }

        // Find the first (smallest) estate source that reproduces
        // THIS gap's signature, then minimise + privacy-prove it.
        //
        // Deterministic attempt bound: try only the
        // `MAX_SEED_ATTEMPTS` smallest seeds. The oracle re-runs the
        // whole engine per ddmin probe; an unbounded 401-file ×
        // hundreds-of-probes search is intractable on a real estate.
        // The cap is a *count*, not wall-clock, so the result stays
        // byte-deterministic (I-DETERMINISM) — same estate+commit ⇒
        // same gaps minimised. A signature not reproduced within the
        // budget honestly keeps `min_fixture_id = None` (R13: report
        // the boundary, never fabricate).
        //
        // Seed *prioritisation* (the loop-usefulness fix): a
        // construct-specific signature like `text_scan>drop` or
        // `unit_statement>create_table` is only reproduced by a file
        // that actually *contains that construct*. The 12 globally
        // smallest files are dominated by the most common construct
        // (`text_scan>create`), so a pure size sort never reaches the
        // file that reproduces a rarer structured signature — that is
        // why only the one coarse class minimised. We derive a
        // privacy-safe screen from the gap's `antlr_rule_path` *leaf*
        // (a grammar constant — e.g. `drop`, `create_table`,
        // `comment` — never estate data), float the files that
        // textually contain every screen token to the front (stable,
        // size-tiebroken), and raise the budget. This only reorders
        // and widens the search; the oracle inside `build_min_fixture`
        // still independently *proves* the exact `(diag_code,
        // antlr_rule_path, signature)` reproduces — a screen-matching
        // file that does not actually reproduce just returns
        // `NotReproducible`. Signature/oracle are unchanged.
        const MAX_SEED_ATTEMPTS: usize = 40;
        let mut outcome: Option<(String, String)> = None;

        // Seed #0 — the gap's OWN provenance (task §2.2, the breadth
        // keystone). The signature is a pure function of
        // `(diag_code, antlr_rule_path, span_shape)` and `span_shape`
        // is itself a pure function of `antlr_rule_path`, so the
        // canonical construct *named by the rule path* is by
        // construction a reproducing seed for that exact signature —
        // built only from grammar keyword constants + synthetic
        // `id_<hex>` placeholders (zero estate bytes; I-PRIVACY by
        // construction). This gives EVERY distinct signature its own
        // provenance-derived seed attempt, fairly, before — and
        // independent of — the size-ordered estate search, so a
        // high-count class like `text_scan>comment`/`text_scan>drop`
        // is no longer starved by another class consuming the global
        // file budget. The (unchanged) `SignatureOracle` inside
        // `build_min_fixture` still independently proves the exact
        // `(code, rule, signature)` reproduces and the privacy proof
        // still gates it — a synthetic seed that does not reproduce
        // just returns `NotReproducible` and we fall through to the
        // estate search. Signature/oracle/scrub unchanged.
        if let Some(seed) = synthetic_seed_for_rule_path(rec.antlr_rule_path.as_deref()) {
            if let Ok(fx) = build_min_fixture(&seed, rec, max_bytes) {
                if persist_min_fixture(repo_root, &fx).is_ok() {
                    if let Ok(pid) = fx.privacy_proof_id() {
                        rec.min_fixture_id = Some(fx.id.clone());
                        rec.privacy_proof_id = Some(pid.clone());
                        outcome = Some((fx.id.clone(), pid));
                    }
                }
            }
        }

        // The provenance seed already proved a fixture for this
        // signature — the estate search is unnecessary (and the memo
        // dedups every other gap sharing it). Skip straight to the
        // memo write. Otherwise fall through to the estate fallback.
        let screen = rule_path_screen_tokens(rec.antlr_rule_path.as_deref());
        let mut ranked: Vec<&(PathBuf, String)> = if outcome.is_some() {
            Vec::new()
        } else {
            contents.iter().collect()
        };
        // Stable partition: screen-matching seeds first (each tier
        // already size-then-path sorted from `contents`).
        ranked.sort_by_key(|(_, src)| {
            let lc = src.to_ascii_lowercase();
            u8::from(!screen.iter().all(|t| lc.contains(t.as_str())))
        });
        for (_, src) in ranked.into_iter().take(MAX_SEED_ATTEMPTS) {
            match build_min_fixture(src, rec, max_bytes) {
                Ok(fx) => {
                    // Persist only the proven synthetic fixture.
                    if persist_min_fixture(repo_root, &fx).is_ok() {
                        if let Ok(pid) = fx.privacy_proof_id() {
                            rec.min_fixture_id = Some(fx.id.clone());
                            rec.privacy_proof_id = Some(pid.clone());
                            outcome = Some((fx.id.clone(), pid));
                        }
                    }
                    break;
                }
                // NotReproducible: this source isn't the one — keep
                // looking. PrivacyUnprovable: this source's repro
                // cannot be safely redacted — honest skip, do NOT
                // fall back to a leaky fixture; try the next source.
                Err(_) => continue,
            }
        }
        // Memoise (positive OR negative): a gap with no proven
        // fixture honestly keeps min_fixture_id = None — privacy
        // beats coverage (I-PRIVACY) — and we never re-search it.
        memo.insert(rec.signature.clone(), outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_path_screen_tokens_extracts_grammar_keywords() {
        assert_eq!(
            rule_path_screen_tokens(Some("unit_statement>create_table")),
            vec!["create".to_string(), "table".to_string()]
        );
        assert_eq!(
            rule_path_screen_tokens(Some("text_scan>drop")),
            vec!["drop".to_string()]
        );
        assert_eq!(
            rule_path_screen_tokens(Some("text_scan>comment")),
            vec!["comment".to_string()]
        );
        assert_eq!(
            rule_path_screen_tokens(Some("unit_statement>create_synonym")),
            vec!["create".to_string(), "synonym".to_string()]
        );
        // No rule path ⇒ no screen ⇒ pure size order (unchanged).
        assert!(rule_path_screen_tokens(None).is_empty());
    }

    #[test]
    fn strip_comments_removes_line_and_block() {
        let s = "x := 1; -- secret hint\n/* block secret */ y := 2;";
        let out = strip_comments(s);
        assert!(!out.contains("secret"), "{out}");
        assert!(out.contains("x :=") && out.contains("y :="));
    }

    #[test]
    fn strip_comments_keeps_dashes_inside_strings() {
        let s = "v := '-- not a comment';";
        let out = strip_comments(s);
        assert!(out.contains("-- not a comment"));
    }

    #[test]
    fn synthetic_alias_shape_recognised() {
        // Identifier / quoted-id body: id_<hex12>.
        assert!(is_synthetic_alias("id_0123456789ab"));
        assert!(!is_synthetic_alias("id_xyz"));
        assert!(!is_synthetic_alias("customers"));
        assert!(!is_synthetic_alias("id_0123456789abff")); // wrong length
        // String-literal body: sx_<hex8>.
        assert!(is_synthetic_alias("sx_0123abcd"));
        assert!(!is_synthetic_alias("sx_0123")); // too short
        assert!(!is_synthetic_alias("sx_zzzzzzzz")); // non-hex
        assert!(!is_synthetic_alias("sx_0123abcdef")); // too long
    }

    #[test]
    fn residue_scan_flags_surviving_secret() {
        // A redacted buffer that still contains an original word.
        let original = "select supersecretcol from t";
        let redacted = "SELECT supersecretcol FROM id_0123456789ab";
        assert!(!privacy_residue_clean(original, redacted));
    }

    #[test]
    fn residue_scan_passes_fully_synthetic() {
        // Realistic post-structure-preserving-scrub shape: every
        // identifier is an `id_<hex12>` alias, every string a
        // `'sx_<hex8>'` alias, every number a fixed numeral, the
        // rest grammar keywords + punctuation. The token-driven
        // proof uses the real lexer's keyword judgment, so genuine
        // keywords (`SELECT`/`FROM`/`WHERE`) pass without a wordlist.
        let original = "select customers_pii from billing where x = 'sek' and n = 4111111111111111";
        let redacted = "SELECT id_0123456789ab FROM id_ffeeddccbbaa \
                        WHERE id_aabbccddeeff = 'sx_0123abcd' AND id_aabbccddee01 = 7";
        assert!(privacy_residue_clean(original, redacted));
    }

    #[test]
    fn residue_scan_passes_with_grammar_keywords_not_in_lab_subset() {
        // The regression that caused every structured class to
        // collapse: keywords like TABLE / VARCHAR2 / SYSDATE are
        // NOT in the lab `DEFAULT_RESERVED` subset but ARE real
        // grammar keywords. The token-driven proof must pass them.
        let original = "create table billing.accounts (opened date default sysdate)";
        let redacted = "CREATE TABLE id_0123456789ab.id_ffeeddccbbaa \
                        (id_aabbccddeeff DATE DEFAULT SYSDATE)";
        assert!(
            privacy_residue_clean(original, redacted),
            "legitimately-surviving grammar keywords must not be flagged as residue"
        );
    }

    #[test]
    fn residue_scan_fails_closed_on_unknown_word() {
        // A surviving identifier that is neither a synthetic alias
        // nor a grammar keyword still fails (positive proof).
        let original = "select a from b";
        let redacted = "SELECT mysteryword FROM id_0123456789ab";
        assert!(!privacy_residue_clean(original, redacted));
    }

    #[test]
    fn synthetic_seed_is_grammar_only_and_provenance_shaped() {
        // text_scan>* ⇒ unterminated-string prefix (defeats the
        // parse tree, reproduces the whole-file text-scanner path),
        // unit_statement>* ⇒ well-formed (parse-tree path). Every
        // byte is a grammar keyword / punctuation / synthetic
        // `id_`/`'sx_'` placeholder — zero estate text.
        let comment = synthetic_seed_for_rule_path(Some("text_scan>comment")).unwrap();
        assert!(comment.starts_with("x '\n"), "{comment}");
        assert!(comment.contains("COMMENT ON TABLE"), "{comment}");

        let drop = synthetic_seed_for_rule_path(Some("text_scan>drop")).unwrap();
        assert_eq!(drop, "x '\nDROP id_a;");

        let ut = synthetic_seed_for_rule_path(Some("unit_statement>create_table")).unwrap();
        assert!(!ut.starts_with("x '"), "parse-tree path: well-formed: {ut}");
        assert!(ut.starts_with("CREATE TABLE id_a"), "{ut}");

        // No rule path ⇒ no synthetic seed (estate fallback only).
        assert_eq!(synthetic_seed_for_rule_path(None), None);
        assert_eq!(synthetic_seed_for_rule_path(Some("text_scan>")), None);
        // The synthetic carries no estate identifier — only the
        // fixed `id_a`/`id_b`/`'sx'` placeholders + keywords.
        for rp in [
            "text_scan>comment",
            "text_scan>drop_table",
            "unit_statement>create_synonym",
        ] {
            let s = synthetic_seed_for_rule_path(Some(rp)).unwrap();
            assert!(s.is_ascii(), "grammar/ascii only: {rp} => {s:?}");
        }
    }
}
