//! `gate.rs` — the typed §3 conformance-gate runner (PLSQL-USR-001,
//! Phase P4). **The safety rail.**
//!
//! This module does two things, both fail-closed:
//!
//! 1. **Typed runner** ([`run_gate`]): shells the content-pinned
//!    `scripts/usr_gate.sh`, parses every `GATE Gn: PASS|FAIL
//!    <evidence>` line into a typed [`GateStageVerdict`], and folds
//!    them into a [`GateOutcome`]. ANY non-PASS, any unparseable
//!    line, any missing stage, a missing script, or a **sha
//!    mismatch** → [`GateOutcome::Reject`] (never default-pass). No
//!    partial credit: 8/9 is REJECT. Determinism: the same candidate
//!    + commit yields an identical verdict.
//!
//! 2. **Check primitives** ([`roundtrip_check`], [`honesty_check`],
//!    [`residue_check`], [`baseline_cmp`]): the real Rust-level
//!    checks the gate script invokes via the `usr-gate-rs` helper
//!    binary (`src/bin/usr_gate_rs.rs`). They are public so the
//!    adversarial self-test exercises the *same code path* the gate
//!    runs — the bar is identical, only the input set is scoped.
//!
//! ## sha-pin / immutability (mirrors compliance `☖ STAKE-RUBRIC`)
//!
//! The gate script is content-pinned: its `sha256` is committed in
//! [`GATE_SHA256_PATH`] (`crates/plsql-accretion/gate.sha256`).
//! [`verify_gate_sha`] recomputes the on-disk sha and ABORTS with a
//! distinct [`GateError::ShaMismatch`] (never "pass") on any drift.
//! Changing the gate REQUIRES a deliberate, human-reviewed commit
//! that bumps that committed sha — the bar never moves silently.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// All nine stage ids, in their strict order (spec §3).
pub const GATE_STAGES: [&str; 9] = ["G1", "G2", "G3", "G4", "G5", "G6", "G7", "G8", "G9"];

/// Distinct process exit code the gate script uses for an I-PRIVACY
/// abort (spec §1/§7 — the run aborts, nothing is persisted).
pub const PRIVACY_ABORT_EXIT: i32 = 9;

/// Repo-relative path to the content-pinned gate script.
pub const GATE_SCRIPT_REL: &str = "scripts/usr_gate.sh";

/// Repo-relative path to the committed sha256 manifest pinning the
/// gate script body.
pub const GATE_SHA256_PATH: &str = "crates/plsql-accretion/gate.sha256";

/// Typed errors from the gate runner. Every variant is a hard
/// REJECT/ABORT — there is no "soft" gate error that could be
/// mistaken for a pass (fail-closed by construction).
#[derive(Debug, Error)]
pub enum GateError {
    /// The gate script is missing or unreadable. Fail-closed: a gate
    /// that cannot run rejects.
    #[error("gate script missing/unreadable at {0}")]
    ScriptMissing(PathBuf),

    /// The committed sha manifest is missing/unreadable.
    #[error("gate sha manifest missing/unreadable at {0}")]
    ShaManifestMissing(PathBuf),

    /// **Immutability guard.** The on-disk gate script sha ≠ the
    /// committed pin. The run ABORTS — this is never a pass; changing
    /// the gate requires a deliberate human sha bump.
    #[error(
        "gate sha mismatch: on-disk={on_disk} pinned={pinned} — gate tampered; deliberate human sha bump required (☖ STAKE-RUBRIC)"
    )]
    ShaMismatch { on_disk: String, pinned: String },

    /// The gate process could not be spawned.
    #[error("gate process spawn failed: {0}")]
    Spawn(String),

    /// A `GATE Gn:` line was neither PASS nor FAIL, or unparseable.
    #[error("unparseable gate line: {0:?}")]
    UnparseableLine(String),
}

/// One stage's parsed verdict.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GateStageVerdict {
    /// Stage id, e.g. `"G7"`.
    pub stage: String,
    /// `true` iff the stage line was `PASS`.
    pub passed: bool,
    /// The verbatim `<evidence>` text the stage printed.
    pub evidence: String,
}

/// The folded outcome of a full gate run. Fail-closed: only
/// [`GateOutcome::Accept`] when ALL nine stages PASSed and the
/// process exited 0.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum GateOutcome {
    /// All nine stages PASSed, process exit 0. The ONLY accept path.
    Accept { stages: Vec<GateStageVerdict> },
    /// At least one stage was not PASS, or a stage was missing, or an
    /// unparseable line, or non-zero exit. The candidate becomes a
    /// quarantined bead (spec §7). `failing_stage` is the FIRST
    /// non-PASS (fail-closed stops there).
    Reject {
        failing_stage: Option<String>,
        stages: Vec<GateStageVerdict>,
        exit_code: i32,
    },
    /// I-PRIVACY fail-safe: G8 detected an estate-byte leak. The run
    /// aborted; nothing was persisted (spec §1/§7). Distinct from
    /// `Reject` so callers can wire the alert + drop in-memory state.
    PrivacyAbort { stages: Vec<GateStageVerdict> },
}

impl GateOutcome {
    /// `true` iff this is the unique accept path.
    #[must_use]
    pub fn is_accept(&self) -> bool {
        matches!(self, GateOutcome::Accept { .. })
    }

    /// The first non-PASS stage id, if any.
    #[must_use]
    pub fn failing_stage(&self) -> Option<&str> {
        match self {
            GateOutcome::Accept { .. } => None,
            GateOutcome::Reject { failing_stage, .. } => failing_stage.as_deref(),
            GateOutcome::PrivacyAbort { .. } => Some("G8"),
        }
    }
}

/// Compute `sha256:<hex>` of a byte slice (workspace convention).
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(7 + digest.len() * 2);
    out.push_str("sha256:");
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Verify the on-disk gate script sha against the committed pin.
///
/// # Errors
/// [`GateError::ScriptMissing`] / [`GateError::ShaManifestMissing`]
/// if either file is unreadable; [`GateError::ShaMismatch`] (a hard
/// ABORT, never a pass) on any content drift.
pub fn verify_gate_sha(repo_root: &Path) -> Result<String, GateError> {
    let script = repo_root.join(GATE_SCRIPT_REL);
    let manifest = repo_root.join(GATE_SHA256_PATH);
    let body = std::fs::read(&script).map_err(|_| GateError::ScriptMissing(script.clone()))?;
    let on_disk = sha256_hex(&body);
    let pinned_raw = std::fs::read_to_string(&manifest)
        .map_err(|_| GateError::ShaManifestMissing(manifest.clone()))?;
    // The manifest is `<sha256:hex>  scripts/usr_gate.sh` (shasum
    // style) or a bare `sha256:hex`; take the first whitespace token.
    let pinned = pinned_raw
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    if on_disk != pinned {
        return Err(GateError::ShaMismatch { on_disk, pinned });
    }
    Ok(on_disk)
}

/// Parse one `GATE Gn: PASS|FAIL <evidence>` line. Returns `None`
/// for non-`GATE ` lines (the gate also prints a final summary).
fn parse_gate_line(line: &str) -> Option<Result<GateStageVerdict, GateError>> {
    let rest = line.strip_prefix("GATE ")?;
    // `Gn: PASS evidence...`  (n in 0..=9; G0 is a pre-flight error)
    let (stage, after) = rest.split_once(": ")?;
    if stage == "G0" || !stage.starts_with('G') {
        return None;
    }
    if let Some(ev) = after.strip_prefix("PASS ") {
        Some(Ok(GateStageVerdict {
            stage: stage.to_string(),
            passed: true,
            evidence: ev.to_string(),
        }))
    } else if let Some(ev) = after.strip_prefix("FAIL ") {
        Some(Ok(GateStageVerdict {
            stage: stage.to_string(),
            passed: false,
            evidence: ev.to_string(),
        }))
    } else if after == "PASS" {
        Some(Ok(GateStageVerdict {
            stage: stage.to_string(),
            passed: true,
            evidence: String::new(),
        }))
    } else if after.starts_with("ABORT ") {
        // G8 abort marker — not a stage verdict line, swallow.
        None
    } else {
        Some(Err(GateError::UnparseableLine(line.to_string())))
    }
}

/// Run the content-pinned gate against `candidate` and fold the
/// result into a typed [`GateOutcome`]. **Fail-closed**: sha
/// mismatch, missing script, unparseable line, missing stage, any
/// non-PASS, or non-zero exit ⇒ `Reject`/`PrivacyAbort` — never
/// `Accept`. Determinism: same candidate + same commit ⇒ identical
/// outcome (the gate itself is deterministic; this runner adds no
/// wall-clock / RNG / map-order).
///
/// `env` is appended to the child env (used by the self-test to
/// scope the *inputs* — never the checks — so G1–G6 run fast).
///
/// # Errors
/// [`GateError`] only for conditions that cannot even produce a
/// verdict (sha mismatch, missing script, spawn failure). Every
/// other failure is a typed `Reject`/`PrivacyAbort` (fail-closed).
pub fn run_gate(
    repo_root: &Path,
    candidate: &Path,
    env: &[(&str, &str)],
) -> Result<GateOutcome, GateError> {
    // sha-pin FIRST — a tampered gate never runs (immutability).
    verify_gate_sha(repo_root)?;

    let script = repo_root.join(GATE_SCRIPT_REL);
    if !script.is_file() {
        return Err(GateError::ScriptMissing(script));
    }

    let mut cmd = Command::new("bash");
    cmd.arg(&script)
        .arg(candidate)
        .current_dir(repo_root)
        // Deterministic child env: no inherited LANG/locale surprises
        // in numeric/string compares.
        .env("LC_ALL", "C");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|e| GateError::Spawn(e.to_string()))?;
    let exit_code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);

    let mut stages: Vec<GateStageVerdict> = Vec::with_capacity(9);
    let mut first_fail: Option<String> = None;
    for line in stdout.lines() {
        match parse_gate_line(line) {
            None => {}
            Some(Ok(v)) => {
                if !v.passed && first_fail.is_none() {
                    first_fail = Some(v.stage.clone());
                }
                stages.push(v);
            }
            Some(Err(e)) => return Err(e),
        }
    }

    // I-PRIVACY abort: distinct exit code 9 (spec §1/§7).
    if exit_code == PRIVACY_ABORT_EXIT {
        return Ok(GateOutcome::PrivacyAbort { stages });
    }

    // No partial credit: ALL nine present AND all PASS AND exit 0.
    let present: BTreeSet<&str> = stages.iter().map(|s| s.stage.as_str()).collect();
    let all_present = GATE_STAGES.iter().all(|g| present.contains(g));
    let all_pass = stages.iter().all(|s| s.passed) && stages.len() == GATE_STAGES.len();

    if exit_code == 0 && all_present && all_pass {
        return Ok(GateOutcome::Accept { stages });
    }

    // Fail-closed: identify the first non-PASS, else the first
    // missing stage (a missing stage is itself a REJECT — never a
    // pass by omission).
    let failing_stage = first_fail.or_else(|| {
        GATE_STAGES
            .iter()
            .find(|g| !present.contains(*g))
            .map(|g| (*g).to_string())
    });
    Ok(GateOutcome::Reject {
        failing_stage,
        stages,
        exit_code,
    })
}

// =====================================================================
// Check primitives — the REAL Rust-level checks the gate invokes via
// the `usr-gate-rs` helper. Public so the self-test runs the same
// code path the gate runs (the bar is identical).
// =====================================================================

/// PL/SQL source extensions the gate's round-trip / residue scans
/// read in place (mirrors the engine; kept local for R20 closure
/// minimalism).
const GATE_SRC_EXTS: &[&str] = &[
    "sql", "pls", "plsql", "pks", "pkb", "prc", "fnc", "trg", "tps", "tpb", "plb", "bdy", "spec",
    "typ",
];

/// Recursively collect source files under `dir` (sorted —
/// I-DETERMINISM, no fs-iteration-order in any verdict).
fn collect_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                stack.push(p);
            } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if GATE_SRC_EXTS.contains(&ext) {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

/// **G2** — lossless round-trip over `corpus_dir` + every prior
/// MinFixture in `fixtures_dir`. `Ok(report)` iff
/// `reconstruct(tape)==input` byte-for-byte for 100% of inputs;
/// `Err(first-mismatch)` on the first divergence (one mismatch =
/// FAIL, spec §3.G2).
///
/// # Errors
/// Returns `Err` describing the first non-round-tripping file (the
/// fail-closed evidence the gate prints).
pub fn roundtrip_check(corpus_dir: &Path, fixtures_dir: &Path) -> Result<String, String> {
    use plsql_core::FileId;
    use plsql_parser::{ParseOptions, parse_with_backend};
    use plsql_parser_antlr::Antlr4RustBackend;

    let mut files = collect_sources(corpus_dir);
    if fixtures_dir.is_dir() {
        files.extend(collect_sources(fixtures_dir));
    }
    files.sort();
    if files.is_empty() {
        // A round-trip stage with zero inputs cannot make a real
        // claim — fail-closed (never a vacuous pass).
        return Err(format!(
            "no round-trip inputs found under {} or {} — cannot make a real lossless claim",
            corpus_dir.display(),
            fixtures_dir.display()
        ));
    }
    let backend = Antlr4RustBackend::new();
    let mut checked = 0usize;
    for f in &files {
        let Ok(src) = std::fs::read_to_string(f) else {
            continue;
        };
        let r = parse_with_backend(&src, FileId::new(0), &backend, &ParseOptions::default());
        let recon = r.cst.reconstruct();
        if recon != src {
            return Err(format!(
                "round-trip mismatch in {} ({} bytes in, {} bytes out)",
                f.display(),
                src.len(),
                recon.len()
            ));
        }
        checked += 1;
    }
    Ok(format!(
        "lossless round-trip 100% over {checked} inputs ({} corpus + {} prior MinFixtures)",
        collect_sources(corpus_dir).len(),
        if fixtures_dir.is_dir() {
            collect_sources(fixtures_dir).len()
        } else {
            0
        }
    ))
}

/// The candidate-diff honesty manifest (D3). Parsed from `# usr-gate:`
/// directive lines in the candidate diff.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HonestyManifest {
    pub repair_class: String,
    pub signature: String,
    pub diagnostics_resolved: i64,
    pub extracted_semantics_delta: i64,
    pub posture: String,
    pub unknown_reason: String,
}

/// Parse the D3 honesty manifest out of a candidate diff.
fn parse_honesty(candidate_text: &str) -> Result<HonestyManifest, String> {
    let mut m = HonestyManifest::default();
    let mut seen_any = false;
    for raw in candidate_text.lines() {
        let line = raw.trim_start();
        let Some(body) = line.strip_prefix("# usr-gate:") else {
            continue;
        };
        seen_any = true;
        for kv in body.split_whitespace() {
            let Some((k, v)) = kv.split_once('=') else {
                continue;
            };
            match k {
                "repair-class" => m.repair_class = v.to_string(),
                "signature" => m.signature = v.to_string(),
                "diagnostics-resolved" => {
                    m.diagnostics_resolved = v
                        .parse()
                        .map_err(|_| format!("diagnostics-resolved not an integer: {v:?}"))?;
                }
                "extracted-semantics-delta" => {
                    m.extracted_semantics_delta = v
                        .parse()
                        .map_err(|_| format!("extracted-semantics-delta not an integer: {v:?}"))?;
                }
                "posture" => m.posture = v.to_string(),
                "unknown-reason" => m.unknown_reason = v.to_string(),
                // golden-delta is consumed by G4, ignored here.
                _ => {}
            }
        }
    }
    if !seen_any {
        return Err(
            "no '# usr-gate:' honesty manifest in candidate (D3) — undeclared claim is suppression-by-omission".into(),
        );
    }
    Ok(m)
}

/// **G7** — anti-gaming + honesty. Enforces the D3 inequality:
/// diagnostics fall IFF extraction rose by ≥ the resolved count;
/// posture never weakened; class `d` must carry a typed
/// `UnknownReason`. `Ok(evidence)` iff honest; `Err(reason)` ⇒ FAIL
/// (suppression / posture-weakened / silenced Unknown).
///
/// # Errors
/// Returns `Err` with the precise honesty violation (the gate's
/// fail-closed evidence).
pub fn honesty_check(candidate_text: &str) -> Result<String, String> {
    let m = parse_honesty(candidate_text)?;
    let valid_class = matches!(m.repair_class.as_str(), "g" | "l" | "d" | "unrepairable");
    if !valid_class {
        return Err(format!(
            "repair-class {:?} not one of g|l|d|unrepairable (D3)",
            m.repair_class
        ));
    }
    if m.signature.is_empty() {
        return Err("empty targeted signature — claim not provenanced (D3)".into());
    }
    if m.posture == "weakened" || m.posture.is_empty() {
        return Err(format!(
            "completeness posture {:?} — weakened/undeclared posture is the oracle-bh4p dishonesty (G7)",
            m.posture
        ));
    }
    // The core anti-gaming inequality (spec §3.G7, D3).
    if m.diagnostics_resolved > 0 && m.extracted_semantics_delta < m.diagnostics_resolved {
        return Err(format!(
            "SUPPRESSION: diagnostics_resolved={} but extracted_semantics_delta={} (< resolved) — diagnostics fell with no commensurate extraction rise (I-NO-GAMING / oracle-bh4p)",
            m.diagnostics_resolved, m.extracted_semantics_delta
        ));
    }
    if m.repair_class == "d" && m.unknown_reason.is_empty() {
        return Err(
            "repair-class d but no typed unknown-reason — the Unknown was silenced, not typed (spec §3.G7, D3 'd is last resort, must stay honest')".into(),
        );
    }
    Ok(format!(
        "honest: class={} resolved={} extraction_delta={} posture={} (delta ≥ resolved, posture not weakened{})",
        m.repair_class,
        m.diagnostics_resolved,
        m.extracted_semantics_delta,
        m.posture,
        if m.repair_class == "d" {
            format!(", Unknown→typed {}", m.unknown_reason)
        } else {
            String::new()
        }
    ))
}

/// **G8** — privacy residue scan over the candidate diff + every
/// MinFixture in `fixtures_dir`. Uses the real ANTLR-lexer-driven
/// [`crate::tokscrub::token_verdicts`] (wordlist-free) so every
/// surviving estate-class token must be a synthetic
/// `id_`/`sx_`/numeral alias — anything else is an original-byte
/// leak. Also greps for the planted estate-identifier set.
///
/// `Ok(evidence)` ⇒ 0 surviving original bytes. `Err(leak)` ⇒ the
/// caller MUST abort the whole run with [`PRIVACY_ABORT_EXIT`]
/// (I-PRIVACY fail-safe, spec §1/§7).
///
/// # Errors
/// Returns `Err` describing the leak (the gate aborts on `Err`).
pub fn residue_check(candidate_text: &str, fixtures_dir: &Path) -> Result<String, String> {
    // The planted estate-identifier set the metamorphic privacy test
    // and the self-test use. Any survival of these EXACT tokens in a
    // persisted artifact is a leak by definition. (Kept here, not in
    // a fixture file, so the scan needs no estate access — R20/C5.)
    const ESTATE_MARKERS: &[&str] = &[
        "PRIVATE_ESTATE",
        "PRIVATEESTATE",
        "ACME_CORP",
        "ACME CORP",
        "CUSTOMER_SSN",
        "ESTATE_SECRET",
        "PLANTED_LEAK",
    ];

    let scan_one = |label: &str, text: &str| -> Result<(), String> {
        let upper = text.to_uppercase();
        for marker in ESTATE_MARKERS {
            if upper.contains(marker) {
                return Err(format!(
                    "I-PRIVACY LEAK in {label}: estate-derived identifier {marker:?} survived"
                ));
            }
        }
        Ok(())
    };

    // 1. The candidate diff body itself must carry no estate marker.
    scan_one("candidate-diff", candidate_text)?;

    // 2. Every added MinFixture: estate-marker grep + the real
    //    wordlist-free token residue proof (every estate-class token
    //    must be a synthetic alias).
    let mut fixtures_scanned = 0usize;
    if fixtures_dir.is_dir() {
        for f in collect_sources(fixtures_dir) {
            let Ok(src) = std::fs::read_to_string(&f) else {
                continue;
            };
            scan_one(&format!("fixture {}", f.display()), &src)?;
            if let Some(verdicts) = crate::tokscrub::token_verdicts(&src) {
                for v in verdicts {
                    if let crate::tokscrub::TokVerdict::EstateClass(body) = v {
                        if !is_synthetic_alias(&body) {
                            return Err(format!(
                                "I-PRIVACY LEAK in fixture {}: non-synthetic estate-class token {:?} survived (not an id_/sx_/numeral alias)",
                                f.display(),
                                body
                            ));
                        }
                    }
                }
            }
            fixtures_scanned += 1;
        }
    }
    Ok(format!(
        "0 surviving original bytes: candidate clean + {fixtures_scanned} MinFixture(s) residue-proven (wordlist-free ANTLR-lexer token scan + estate-marker grep)"
    ))
}

/// A token is a legitimate synthetic alias iff it is an
/// `id_<n>` / `sx_<n>` identifier alias or a numeral the scrubber
/// emits. (Mirrors `fixture.rs`'s synthesis vocabulary; kept local
/// so G8 needs no estate.)
fn is_synthetic_alias(body: &str) -> bool {
    let b = body.trim_matches('"').trim_matches('\'');
    if let Some(rest) = b.strip_prefix("id_").or_else(|| b.strip_prefix("sx_")) {
        return !rest.is_empty() && rest.bytes().all(|c| c.is_ascii_alphanumeric());
    }
    // Numerals / quoted synthetic strings the scrubber emits.
    !b.is_empty()
        && b.bytes()
            .all(|c| c.is_ascii_digit() || c == b'.' || c == b'-')
}

/// **G6 helper** — measure the three monotonic §0 metrics for
/// `estate` deterministically via the engine (read-in-place; no
/// estate byte is copied — AGENTS.md C5/C6). Emits the canonical
/// `edges=<n> facts=<n> ratio=<f>` line [`baseline_cmp`] consumes.
/// This is the authoritative metric source for G6 (the harness still
/// independently gates `RESULT: PASS`); decoupling from harness
/// stdout keeps G6 robust to the harness's note formatting.
///
/// # Errors
/// Returns `Err` if the engine analyze fails (fail-closed — a G6
/// that cannot measure cannot pass).
pub fn measure_estate_metrics(estate: &Path) -> Result<String, String> {
    use plsql_engine::{AnalysisRequest, analyze_project};
    let mut req = AnalysisRequest {
        project_root: estate.to_path_buf(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;
    let run = analyze_project(req).map_err(|e| format!("engine analyze failed: {e}"))?;
    let edges = run.dep_graph.edge_count();
    let facts = run.fact_store.fact_count;
    let ratio = run.completeness.extracted_semantics_ratio;
    Ok(format!(
        "measured: edges={edges} facts={facts} ratio={ratio}"
    ))
}

/// **G6 helper** — compare measured metrics against the committed
/// baseline. `baseline_json` is the committed
/// `gate_baseline.json`; `metrics_text` is the
/// [`measure_estate_metrics`] line.
/// `Ok` iff `dep_graph edges ≥ baseline` AND `facts ≥ baseline` AND
/// `extracted_semantics_ratio ≥ baseline`; `Err` on any regression.
///
/// # Errors
/// Returns `Err` if the baseline is malformed or any metric fell.
pub fn baseline_cmp(baseline_json: &str, metrics_text: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Baseline {
        dep_graph_edges: u64,
        facts: u64,
        extracted_semantics_ratio: f64,
    }
    let base: Baseline =
        serde_json::from_str(baseline_json).map_err(|e| format!("baseline json malformed: {e}"))?;
    // The harness prints `measured: ... edges=<n> facts=<n>
    // ratio=<f>` style notes. We extract conservatively: a missing
    // metric is treated as a regression (fail-closed), never assumed
    // ≥ baseline.
    let grab_u64 = |key: &str| -> Option<u64> {
        metrics_text
            .split_whitespace()
            .find_map(|tok| tok.strip_prefix(key).and_then(|v| v.parse().ok()))
    };
    let grab_f64 = |key: &str| -> Option<f64> {
        metrics_text
            .split_whitespace()
            .find_map(|tok| tok.strip_prefix(key).and_then(|v| v.parse().ok()))
    };
    let edges = grab_u64("edges=")
        .ok_or_else(|| "harness output carried no `edges=` metric (fail-closed)".to_string())?;
    let facts = grab_u64("facts=")
        .ok_or_else(|| "harness output carried no `facts=` metric (fail-closed)".to_string())?;
    let ratio = grab_f64("ratio=")
        .ok_or_else(|| "harness output carried no `ratio=` metric (fail-closed)".to_string())?;
    if edges < base.dep_graph_edges {
        return Err(format!(
            "dep_graph edges {edges} < baseline {} (regression)",
            base.dep_graph_edges
        ));
    }
    if facts < base.facts {
        return Err(format!(
            "facts {facts} < baseline {} (regression)",
            base.facts
        ));
    }
    if ratio + f64::EPSILON < base.extracted_semantics_ratio {
        return Err(format!(
            "extracted_semantics_ratio {ratio} < baseline {} (regression)",
            base.extracted_semantics_ratio
        ));
    }
    Ok(format!(
        "edges {edges}≥{} facts {facts}≥{} ratio {ratio:.4}≥{:.4} (monotonic non-regression)",
        base.dep_graph_edges, base.facts, base.extracted_semantics_ratio
    ))
}

/// **G9 helper (degraded mode)** — revert-and-assert. Applies the
/// candidate's REVERSE diff, runs its declared regression test,
/// asserts it FAILS on reverted code, then restores. A test that
/// passes on reverted code is vacuous = FAIL (spec §3.G9).
///
/// The candidate declares its pinning hooks via rest-of-line
/// directives (the command may contain spaces):
///
/// ```text
/// # usr-gate-pins-cmd: <shell that exits 0 iff the regression test passes>
/// # usr-gate-pins-revert: <shell that reverts the candidate>
/// # usr-gate-pins-restore: <shell that restores the patched tree>
/// ```
///
/// In P4 the proposer (P5) is a stub, so the self-test supplies
/// these deterministically. This is a REAL check — it actually runs
/// the revert and the test, asserting the test FAILS on reverted
/// code (mutation-killed equivalent).
///
/// # Errors
/// Returns `Err` if the test passes on reverted code (vacuous), or
/// the revert/restore could not run (fail-closed).
pub fn pins_check(repo_root: &Path, candidate_text: &str) -> Result<String, String> {
    // Rest-of-line directive: `# usr-gate-<key>: <full shell line>`
    // (the shell line may contain spaces — unlike the space-delimited
    // honesty manifest, a pins hook is one whole command).
    let directive = |key: &str| -> Option<String> {
        let prefix = format!("# usr-gate-{key}:");
        for raw in candidate_text.lines() {
            let line = raw.trim_start();
            if let Some(body) = line.strip_prefix(&prefix) {
                return Some(body.trim().to_string());
            }
        }
        None
    };
    // Self-test / stub path: explicit shell hooks (deterministic).
    let pins_cmd = directive("pins-cmd");
    let pins_revert = directive("pins-revert");
    let pins_restore = directive("pins-restore");
    // Optional: a hook that ESTABLISHES the patched-tree state before
    // the cmd→revert→cmd→restore cycle (the proposer's candidate
    // declares this so the gate can prove the test is genuinely
    // mutation-killed — `pins-setup` represents "the patch is
    // applied"; reverting it must make the pinned test FAIL). Absent
    // ⇒ the legacy direct cmd-on-current-tree path (the adversarial
    // self-test uses that, unchanged — additive, fail-closed).

    let run = |sh: &str| -> i32 {
        Command::new("bash")
            .arg("-c")
            .arg(sh)
            .current_dir(repo_root)
            .env("LC_ALL", "C")
            .status()
            .ok()
            .and_then(|s| s.code())
            .unwrap_or(-1)
    };

    if let (Some(cmd), Some(rev), Some(res)) = (pins_cmd, pins_revert, pins_restore) {
        // 0. If the candidate declares a `pins-setup`, establish the
        //    patched-tree state first. This makes G9 a REAL
        //    mutation-kill proof for an additive proposer candidate:
        //    setup(apply patch) ⇒ test PASSES; revert ⇒ test FAILS.
        //    A failing setup is fail-closed (cannot prove a pin).
        if let Some(setup) = directive("pins-setup") {
            if run(&setup) != 0 {
                return Err(
                    "pins-setup hook failed to establish the patched-tree state (fail-closed; cannot prove a real behavior pin)".into(),
                );
            }
        }
        // 1. Sanity: the test must PASS on the (patched) tree first —
        //    a test that fails even patched pins nothing.
        if run(&cmd) != 0 {
            return Err("declared pinning test fails on the patched tree — it pins nothing".into());
        }
        // 2. Revert, assert the test now FAILS (mutation-killed).
        if run(&rev) != 0 {
            return Err("pins-revert hook failed to run (fail-closed)".into());
        }
        let reverted_status = run(&cmd);
        // 3. Restore unconditionally (best-effort, then verify).
        let restore_status = run(&res);
        if restore_status != 0 {
            return Err("pins-restore hook failed — refusing to claim a pass with an unrestored tree (fail-closed)".into());
        }
        if reverted_status == 0 {
            return Err(
                "VACUOUS TEST: the declared regression test PASSES on reverted code — it does not pin the patched behavior (spec §3.G9)".into(),
            );
        }
        return Ok("revert-and-assert: declared test passes patched, FAILS reverted (mutation-killed equivalent), tree restored".into());
    }

    Err(
        "no '# usr-gate-pins-cmd / -pins-revert / -pins-restore' directives — cannot run a real behavior-pinning check (fail-closed; spec §3.G9 forbids skip-as-pass)".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pass_and_fail_lines() {
        let v = parse_gate_line("GATE G7: PASS honest: class=l")
            .unwrap()
            .unwrap();
        assert!(v.passed);
        assert_eq!(v.stage, "G7");
        let f = parse_gate_line("GATE G2: FAIL round-trip mismatch")
            .unwrap()
            .unwrap();
        assert!(!f.passed);
        assert!(parse_gate_line("some other line").is_none());
        assert!(parse_gate_line("GATE G0: FAIL preflight").is_none());
    }

    #[test]
    fn honesty_rejects_suppression() {
        let diff = "# usr-gate: repair-class=l signature=abc diagnostics-resolved=5 extracted-semantics-delta=0 posture=preserved\n";
        let e = honesty_check(diff).unwrap_err();
        assert!(e.contains("SUPPRESSION"), "{e}");
    }

    #[test]
    fn honesty_accepts_commensurate() {
        let diff = "# usr-gate: repair-class=l signature=abc diagnostics-resolved=3 extracted-semantics-delta=7 posture=improved\n";
        assert!(honesty_check(diff).is_ok());
    }

    #[test]
    fn honesty_rejects_silenced_unknown_for_class_d() {
        let diff = "# usr-gate: repair-class=d signature=abc diagnostics-resolved=0 extracted-semantics-delta=0 posture=preserved\n";
        let e = honesty_check(diff).unwrap_err();
        assert!(e.contains("silenced"), "{e}");
    }

    #[test]
    fn honesty_requires_manifest() {
        assert!(honesty_check("just a diff, no manifest\n").is_err());
    }

    #[test]
    fn baseline_cmp_detects_regression() {
        let base = r#"{"dep_graph_edges":100,"facts":200,"extracted_semantics_ratio":0.5}"#;
        let ok = "measured: edges=120 facts=250 ratio=0.6";
        assert!(baseline_cmp(base, ok).is_ok());
        let regress = "measured: edges=80 facts=250 ratio=0.6";
        assert!(baseline_cmp(base, regress).unwrap_err().contains("edges"));
        let missing = "measured: facts=250 ratio=0.6";
        assert!(baseline_cmp(base, missing).is_err());
    }

    #[test]
    fn synthetic_alias_classifier() {
        assert!(is_synthetic_alias("id_1"));
        assert!(is_synthetic_alias("sx_42a"));
        assert!(is_synthetic_alias("123"));
        assert!(!is_synthetic_alias("customer_ssn"));
        assert!(!is_synthetic_alias("ACME"));
    }
}
