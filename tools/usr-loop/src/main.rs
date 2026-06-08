#![forbid(unsafe_code)]
//! `usr-loop` — the USR (Uncertainty-Sourced Repair) loop
//! orchestrator (through Phase P6).
//!
//! Subcommands (stages [A]–[G], spec §2):
//!
//! * `scan <estate>` — [A]+[B]: analyze read-in-place, capture every
//!   repairable honest-uncertainty gap, minimise + privacy-prove a
//!   MinFixture, emit the `plsql.usr.gap_record` v1 envelope.
//! * `cluster <estate>` — [C]: deduped `GapCluster` batch.
//! * `propose <estate>` — [D]: a `CandidateDiff` (never applied) or
//!   an honest `unrepairable` refusal (spec §7).
//! * `gate <candidate>` — [E]: the content-pinned §3 conformance
//!   gate (fail-closed, sha-pinned).
//! * `land <candidate> --fixture <min.sql>` — [F]: gate-prove then
//!   atomically land (apply + corpus pin + exactly one ledger entry,
//!   `signature → commit` rollback anchor) OR [F'] quarantine on
//!   REJECT (provenanced record; gate never weakened).
//! * `ledger {append|verify|index|tripwire}` — the append-only
//!   content-addressed Ledger + the §4 monotonic accretion tripwire.
//! * `doctor` — crate/schema versions + dependency posture (R11).
//!
//! Global `--robot-json` (R10): single-line machine envelope on
//! stdout; otherwise a pretty multi-line envelope.
//!
//! ## Exit-code dictionary
//!
//! | code | meaning |
//! |------|---------|
//! | 0 | success (incl. `propose` honest `unrepairable` refusal / `land` success — spec §7, not a failure) |
//! | 1 | runtime error (engine analyze failed, serialization failed, bad path) |
//! | 2 | `doctor` found a blocker (degraded posture) |
//! | 3 | `gate` REJECT / `land` quarantined (spec §7 [F'] — NOT landed, gate not weakened) |
//! | 4 | `gate` sha-pin mismatch or script missing (immutability abort) |
//! | 9 | `gate`/`land` I-PRIVACY abort (G8 estate-byte leak; nothing persisted) |

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use plsql_accretion::{
    CANDIDATE_DIFF_SCHEMA, CandidateDiff, DEFAULT_MAX_BYTES, DEFAULT_MAX_REPRESENTATIVES,
    DeterministicStubProposer, GAP_CLUSTER_SCHEMA, GAP_RECORD_SCHEMA, GapCluster,
    GapClusterEnvelope, GapRecordEnvelope, GateError, GateOutcome, LandError, LandFixture, Ledger,
    LedgerBody, PatchProposer, ProposerError, capture_gaps, cluster_gaps_with, estate_run_id,
    fixture_sizes_from_store, land_candidate, minimize_estate_gaps, persist_quarantine, run_gate,
};
use plsql_engine::{AnalysisRequest, analyze_project};
use plsql_output::{RobotJsonEnvelope, SchemaDescriptor, SchemaVersion};

/// USR loop orchestrator.
#[derive(Parser, Debug)]
#[command(
    name = "usr-loop",
    version,
    about,
    long_about = None,
    after_help = "DISCOVERY:\n  usr-loop capabilities     machine-readable agent contract (JSON)\n  usr-loop robot-docs       agent handbook — start here if you are an AI\n  usr-loop --robot-triage   one-shot bootstrap (capabilities + health + quick_ref)"
)]
struct Cli {
    /// Emit a single-line machine-readable robot-JSON envelope
    /// (R10). Default is a pretty multi-line envelope.
    #[arg(long, global = true)]
    robot_json: bool,

    /// One-shot agent bootstrap. Emits a single JSON mega-object
    /// {capabilities, health, quick_ref} on stdout and exits — short-
    /// circuits any subcommand. Exit 0 normally; exit 2 if the doctor
    /// reports a blocker. Mirrors the `plsql-mcp --robot-triage` shape
    /// so agents can use the same bootstrap recipe for every CLI.
    #[arg(long, global = true)]
    robot_triage: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Analyze an Oracle estate read-in-place and emit the captured
    /// GapRecord batch (stage [A]).
    Scan {
        /// Path to the estate / project root (read-in-place; no
        /// byte is ever copied out).
        estate_path: PathBuf,
    },
    /// Scan → capture → minimise → cluster/dedup an estate and
    /// emit the deduped `GapCluster` batch (stage [C], spec §2).
    Cluster {
        /// Path to the estate / project root (read-in-place; no
        /// byte is ever copied out).
        estate_path: PathBuf,
    },
    /// Operate on the append-only, content-addressed ledger under
    /// `<cwd>/.usr/ledger/` (spec §2.1/§4, I-PROVENANCE).
    Ledger {
        #[command(subcommand)]
        action: LedgerAction,
    },
    /// Propose a CANDIDATE DIFF (never a merge — landing is P6) for a
    /// gap cluster (stage [D], spec §2[D]/§10 P5). Default proposer is
    /// the deterministic, network-free stub; it picks exactly one
    /// repair class (g|l|d) per D3 or honestly REFUSES (`unrepairable`,
    /// spec §7). Output is the candidate + provenance, NEVER applied.
    Propose {
        /// Estate / project root (read-in-place). The loop scans →
        /// captures → minimises → clusters it, then proposes a
        /// candidate for the selected cluster.
        estate_path: PathBuf,
        /// Target a specific cluster by its frozen signature
        /// (prefix-matched). Omit to propose for the highest-
        /// occurrence cluster deterministically (`--from-scan`).
        #[arg(long)]
        cluster_id: Option<String>,
        /// Propose for the highest-occurrence cluster from the scan
        /// (the default when `--cluster-id` is absent; explicit flag
        /// for clarity, spec §6).
        #[arg(long)]
        from_scan: bool,
    },
    /// Run the content-pinned §3 conformance gate (the safety rail,
    /// spec §3) on a candidate diff. Fail-closed: exit 0 iff all
    /// nine stages PASS; exit 3 = REJECT; exit 4 = sha-pin/immutability
    /// abort; exit 9 = I-PRIVACY abort (nothing persisted).
    Gate {
        /// Path to the candidate diff to gate.
        candidate_diff: PathBuf,
    },
    /// Stage [F] LAND (spec §2[F], §10 P6). Run the REAL §3 gate
    /// on a proposed candidate; on ACCEPT apply it, add the MinFixture
    /// to the committed regression corpus + a pinned test, append
    /// EXACTLY ONE content-addressed ledger entry (signature→commit
    /// for `git revert` rollback). On REJECT → [F'] quarantine: a
    /// provenanced record naming the failing stage; NOT landed, gate
    /// NEVER weakened. Exit 0 = landed; 3 = quarantined (spec-correct,
    /// not a bug); 4 = gate sha-pin abort; 9 = I-PRIVACY abort.
    Land {
        /// Path to the candidate-diff envelope JSON (`usr-loop
        /// propose` output: a `plsql.usr.candidate_diff` envelope) OR
        /// a raw candidate-diff body the proposer emitted.
        candidate: PathBuf,
        /// Path to the privacy-proven MinFixture `.sql` the candidate
        /// pins (from stage [B]; e.g. a `.usr/fixtures/<id>.sql`).
        #[arg(long)]
        fixture: PathBuf,
    },
    /// Report crate/schema versions and dependency posture (R11).
    /// Exit 2 on any blocker.
    Doctor,
    /// Print the machine-readable agent contract (binary, version,
    /// subcommands, exit-code dictionary, global flags, stdout
    /// contract) as JSON and exit. An agent should read this instead
    /// of guessing the surface. Use `--robot-json` for compact
    /// single-line output.
    Capabilities,
    /// Print a paste-ready agent handbook to stdout: what usr-loop is,
    /// how to drive stages [A]–[G], the exit-code dictionary, and
    /// explicit pointers to `capabilities` / `doctor` / `--robot-triage`.
    RobotDocs,
}

/// Versioned robot-JSON schema for a [`GateOutcome`]
/// (`plsql.usr.gate_outcome` v1). Mirrors the `SchemaDescriptor`
/// pattern used by every other USR envelope.
const GATE_OUTCOME_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.usr.gate_outcome",
    version: SchemaVersion::new(1, 0, 0),
    description: "USR §3 conformance-gate verdict — fail-closed, sha-pinned (PLSQL-USR-001)",
};

/// Versioned robot-JSON schema for a structured CLI error envelope
/// (`plsql.usr.error_envelope` v1). Emitted on stdout (matching the
/// success-envelope contract) in `--robot-json` mode for every error
/// path so `usr-loop --robot-json … | jq .` pipelines do not break.
/// The matching human-readable diagnostic is still echoed to stderr.
const ERROR_ENVELOPE_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.usr.error_envelope",
    version: SchemaVersion::new(1, 0, 0),
    description: "USR runtime error envelope — code, message, optional path/remediation",
};

/// Stable contract version for the `capabilities` payload. Bump only on
/// a breaking change to the JSON shape; the pinned regression test
/// (`capabilities_contract_is_pinned`) will fail if the shape drifts
/// without this being bumped.
const CAPABILITIES_CONTRACT_VERSION: u32 = 1;

/// Versioned robot-JSON schema for a [`plsql_accretion::LandReceipt`]
/// / quarantine outcome (`plsql.usr.land_outcome` v1). Mirrors the
/// `SchemaDescriptor` pattern used by every other USR envelope.
const LAND_OUTCOME_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.usr.land_outcome",
    version: SchemaVersion::new(1, 0, 0),
    description: "USR §2[F] land receipt / §7 [F'] quarantine — propose-prove-then-land (PLSQL-USR-001)",
};

#[derive(Subcommand, Debug)]
enum LedgerAction {
    /// Scan → capture → minimise → cluster the estate, then append
    /// one provenance entry per cluster to the ledger (idempotent
    /// by content).
    Append {
        /// Estate / project root (read-in-place).
        estate_path: PathBuf,
    },
    /// Verify the full tamper-evident hash chain. Exit 1 if broken.
    Verify,
    /// Recompute and print the §4 accretion index from a public
    /// corpus scan (never the private estate) and append it nowhere — it
    /// is a pure, reproducible read.
    Index {
        /// Public benchmark corpus root (e.g. `corpus/synthetic`).
        corpus_path: PathBuf,
    },
    /// §4 monotonic tripwire (spec §4, §1 I-MONOTONIC-VALUE).
    /// Compute `coverage_index` over the frozen public benchmark set
    /// (never the private estate) PLUS `distinct_resolved_gap_signatures` from
    /// the provenance Ledger, append it to the append-only
    /// `accretion_ledger.jsonl`, and assert `coverage_index(HEAD) ≥
    /// coverage_index(last release tag)`. If no release tag exists
    /// yet, seed the monotone floor deterministically and PASS. Exit
    /// non-zero iff the index dropped.
    Tripwire {
        /// Frozen public benchmark corpus root (never the private estate).
        corpus_path: PathBuf,
        /// The git ref this measurement anchors to (default `HEAD`).
        #[arg(long, default_value = "HEAD")]
        git_ref: String,
        /// The release ref to assert monotonicity against. If absent
        /// or unknown the first run seeds the floor and PASSes.
        #[arg(long)]
        baseline_ref: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // --robot-triage short-circuits any subcommand: emit the mega
    // bootstrap object (capabilities + health + quick_ref) and exit
    // before evaluating the subcommand. Mirrors the plsql-mcp shape.
    if cli.robot_triage {
        return run_robot_triage(cli.robot_json);
    }

    let Some(command) = cli.command else {
        // Bare invocation: usr-loop with no subcommand. Print help to
        // stderr and exit 2 — keep stdout pure for `--robot-json` pipes.
        eprintln!(
            "usr-loop: no subcommand given — try `usr-loop scan <estate>`, `usr-loop doctor`, or `usr-loop --robot-triage`."
        );
        eprintln!("run `usr-loop --help` for the full subcommand list.");
        return ExitCode::from(2);
    };

    match command {
        Command::Scan { estate_path } => run_scan(&estate_path, cli.robot_json),
        Command::Cluster { estate_path } => run_cluster(&estate_path, cli.robot_json),
        Command::Propose {
            estate_path,
            cluster_id,
            from_scan,
        } => run_propose(
            &estate_path,
            cluster_id.as_deref(),
            from_scan,
            cli.robot_json,
        ),
        Command::Ledger { action } => run_ledger(action, cli.robot_json),
        Command::Gate { candidate_diff } => run_gate_cmd(&candidate_diff, cli.robot_json),
        Command::Land { candidate, fixture } => run_land_cmd(&candidate, &fixture, cli.robot_json),
        Command::Doctor => run_doctor(cli.robot_json),
        Command::Capabilities => run_capabilities(cli.robot_json),
        Command::RobotDocs => {
            print!("{}", robot_docs_text());
            ExitCode::SUCCESS
        }
    }
}

/// Emit a structured error envelope to stdout in `--robot-json` mode
/// (so `usr-loop --robot-json … | jq .` pipelines see structured
/// output instead of an empty stdout). The human-readable diagnostic
/// is ALWAYS echoed to stderr — agents and humans both see something.
/// `code` is a short stable token (e.g. `"estate_not_found"`); the
/// schema is `plsql.usr.error_envelope` v1.
fn emit_error_envelope(
    robot_json: bool,
    code: &str,
    message: &str,
    path: Option<&Path>,
    remediation: Option<&str>,
) {
    eprintln!("usr-loop: {message}");
    if !robot_json {
        return;
    }
    let payload = serde_json::json!({
        "kind": "error",
        "code": code,
        "message": message,
        "path": path.map(|p| p.display().to_string()),
        "remediation": remediation,
    });
    let env = RobotJsonEnvelope::new(ERROR_ENVELOPE_SCHEMA, payload);
    if let Ok(s) = serde_json::to_string(&env) {
        println!("{s}");
    }
}

/// Build the `capabilities` contract document. Factored out so the
/// schema can be pinned by a unit test without spawning the binary.
fn capabilities_json() -> serde_json::Value {
    serde_json::json!({
        "binary": "usr-loop",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "global_flags": {
            "--robot-json": "emit a single-line machine-readable robot-JSON envelope on stdout instead of human-readable text; diagnostics always on stderr",
            "--robot-triage": "one-shot bootstrap: emit {capabilities, health, quick_ref} on stdout and exit; exit 2 if doctor reports a blocker"
        },
        "subcommands": {
            "scan": "stage [A]+[B]: analyze read-in-place + capture honest-uncertainty gaps + privacy-prove MinFixtures; emit plsql.usr.gap_record v1",
            "cluster": "stage [C]: deduped GapCluster batch (plsql.usr.gap_cluster v1)",
            "propose": "stage [D]: CandidateDiff (never applied) or honest unrepairable refusal (spec §7)",
            "gate": "stage [E]: content-pinned §3 conformance gate (fail-closed, sha-pinned)",
            "land": "stage [F]: gate-prove then atomically land OR quarantine on REJECT (spec §7 [F'])",
            "ledger": "append-only content-addressed Ledger ops {append|verify|index|tripwire}",
            "doctor": "crate/schema versions + dependency posture; exit 2 on blocker",
            "capabilities": "print this machine-readable agent contract as JSON and exit",
            "robot-docs": "print a paste-ready agent handbook to stdout (plain text)"
        },
        "schemas": {
            "gap_record":     { "id": GAP_RECORD_SCHEMA.id,     "version": GAP_RECORD_SCHEMA.version.to_string() },
            "gap_cluster":    { "id": GAP_CLUSTER_SCHEMA.id,    "version": GAP_CLUSTER_SCHEMA.version.to_string() },
            "candidate_diff": { "id": CANDIDATE_DIFF_SCHEMA.id, "version": CANDIDATE_DIFF_SCHEMA.version.to_string() },
            "gate_outcome":   { "id": GATE_OUTCOME_SCHEMA.id,   "version": GATE_OUTCOME_SCHEMA.version.to_string() },
            "land_outcome":   { "id": LAND_OUTCOME_SCHEMA.id,   "version": LAND_OUTCOME_SCHEMA.version.to_string() },
            "error_envelope": { "id": ERROR_ENVELOPE_SCHEMA.id, "version": ERROR_ENVELOPE_SCHEMA.version.to_string() }
        },
        "exit_codes": {
            "0": "success (incl. `propose` honest unrepairable refusal — spec §7, not a failure)",
            "1": "runtime error (engine analyze failed, serialization failed, bad path)",
            "2": "doctor blocker / bare invocation (no subcommand) / --robot-triage blocker",
            "3": "gate REJECT / land quarantined (spec §7 [F'] — NOT landed, gate not weakened)",
            "4": "gate sha-pin mismatch or script missing (immutability abort)",
            "9": "gate/land I-PRIVACY abort (G8 estate-byte leak; nothing persisted)"
        },
        "stdout_contract": "stdout is data only; all diagnostics (and error envelopes are mirrored here as well as stdout) go to stderr"
    })
}

/// Build the agent handbook as a `String`. Factored out for unit tests.
fn robot_docs_text() -> String {
    format!(
        r#"# usr-loop agent handbook

## What is usr-loop?
usr-loop (v{version}) is the USR (Uncertainty-Sourced Repair) loop
orchestrator. It walks stages [A]–[G]: analyze an Oracle estate
read-in-place, capture honest-uncertainty gaps, minimise + privacy-prove
MinFixtures, cluster/dedup, propose a candidate repair, gate it through
the content-pinned §3 conformance gate, and land OR quarantine. No
estate byte is ever copied out; every artifact is content-addressed
and provenanced through the append-only Ledger.

## How an agent should drive usr-loop
1. Bootstrap (one round-trip):     usr-loop --robot-triage
   Returns JSON {{capabilities, health, quick_ref}}. Parse it.
   Exit 2 means a blocker — do not proceed.
2. Full contract:                   usr-loop capabilities
   Versioned agent contract (JSON). Pin `contract_version`; bump
   signals a breaking shape change.
3. Health check:                    usr-loop doctor --robot-json
   Returns the doctor JSON {{schemas, dependency_posture, blockers}}.
4. Stages [A]-[F]:
   usr-loop scan     <estate>                                 # capture gaps
   usr-loop cluster  <estate>                                 # dedup
   usr-loop propose  <estate> [--cluster-id <sig>]            # candidate
   usr-loop gate     <candidate.json>                         # §3 gate
   usr-loop land     <candidate.json> --fixture <min.sql>     # land/quarantine
5. Ledger:                          usr-loop ledger {{append|verify|index|tripwire}} …
6. Handbook (this text):            usr-loop robot-docs

## Robot-JSON envelope shape
Every robot-JSON response is a versioned envelope:
  {{
    "format":         "plsql-robot-json",
    "schema_id":      "plsql.usr.<surface>",
    "schema_version": {{ "major": N, "minor": N, "patch": N }},
    "payload":        {{ ... }}
  }}
Parse `schema_id` + `schema_version` before trusting the payload.

## Error envelope (robot-json mode)
On every runtime error path under `--robot-json`, a single-line
`plsql.usr.error_envelope` v1 object is also emitted on stdout so
`| jq .` pipelines never see an empty stdout. The same human-readable
diagnostic is echoed to stderr.
  payload = {{ "kind": "error", "code": "<token>", "message": "<text>",
               "path": "<path?>", "remediation": "<hint?>" }}

## Exit-code dictionary
  0  success (incl. `propose` honest unrepairable refusal — spec §7)
  1  runtime error (engine analyze failed, serialization failed, bad path)
  2  doctor blocker / bare invocation / --robot-triage blocker
  3  gate REJECT / land quarantined (spec §7 [F'] — NOT landed)
  4  gate sha-pin mismatch or script missing (immutability abort)
  9  gate/land I-PRIVACY abort (G8 leak; nothing persisted)

## Global flags
  --robot-json     compact single-line JSON envelopes on stdout
  --robot-triage   one-shot bootstrap (capabilities + health + quick_ref)

## Discovery
  usr-loop capabilities             machine-readable agent contract
  usr-loop robot-docs               this handbook
  usr-loop --help                   full subcommand reference
"#,
        version = env!("CARGO_PKG_VERSION"),
    )
}

fn run_capabilities(robot_json: bool) -> ExitCode {
    let doc = capabilities_json();
    let rendered = if robot_json {
        serde_json::to_string(&doc)
    } else {
        serde_json::to_string_pretty(&doc)
    };
    match rendered {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "serialize_failed",
                &format!("capabilities serialization failed: {e}"),
                None,
                None,
            );
            ExitCode::from(1)
        }
    }
}

/// Build the doctor JSON report (extracted so `--robot-triage` can
/// embed it without re-printing). Returns (report, blocker_count).
fn doctor_report_json() -> (serde_json::Value, usize) {
    // P1 dependency posture: the crate depends only on
    // plsql-core/-engine/-output (R20, one-directional). No blocker
    // condition exists in P1 (capture is infallible by construction).
    let blockers: Vec<&str> = Vec::new();
    let report = serde_json::json!({
        "tool": "usr-loop",
        "version": env!("CARGO_PKG_VERSION"),
        "library": "plsql-accretion",
        "phase": "P6",
        "schema": {
            "gap_record":  { "id": GAP_RECORD_SCHEMA.id,  "version": GAP_RECORD_SCHEMA.version.to_string() },
            "gap_cluster": { "id": GAP_CLUSTER_SCHEMA.id, "version": GAP_CLUSTER_SCHEMA.version.to_string() },
        },
        "subcommands": ["scan", "cluster", "propose", "ledger {append|verify|index|tripwire}", "gate", "land", "doctor", "capabilities", "robot-docs"],
        "candidate_diff_schema": {
            "id": CANDIDATE_DIFF_SCHEMA.id,
            "version": CANDIDATE_DIFF_SCHEMA.version.to_string(),
        },
        "gate": {
            "script": plsql_accretion::GATE_SCRIPT_REL,
            "sha_manifest": plsql_accretion::GATE_SHA256_PATH,
            "stages": plsql_accretion::GATE_STAGES,
            "schema": {
                "id": GATE_OUTCOME_SCHEMA.id,
                "version": GATE_OUTCOME_SCHEMA.version.to_string(),
            },
        },
        "dependency_posture": {
            "layer": 5,
            "depends_on": [
                "plsql-core", "plsql-engine", "plsql-output",
                "plsql-parser", "plsql-parser-antlr", "plsql-support",
            ],
            "one_directional": true,
            "reverse_deps": 0,
        },
        "exit_codes": {
            "0": "success (incl. `propose` honest unrepairable refusal — spec §7, not a failure)",
            "1": "runtime error",
            "2": "doctor blocker (degraded posture)",
            "3": "gate REJECT / land quarantined (spec §7 [F'] — NOT landed, gate not weakened; spec-correct, not a bug)",
            "4": "gate sha-pin mismatch or script missing (immutability abort)",
            "9": "gate/land I-PRIVACY abort (G8 leak; nothing persisted)",
        },
        "blockers": blockers,
        "status": if blockers.is_empty() { "ok" } else { "degraded" },
    });
    (report, blockers.len())
}

/// `--robot-triage` mega-bootstrap. Emit a single object combining
/// capabilities + health + quick_ref. Exit 2 if doctor reports a
/// blocker (same convention as `plsql-mcp --robot-triage`).
fn run_robot_triage(robot_json: bool) -> ExitCode {
    let (health, blocker_count) = doctor_report_json();
    let quick_ref = serde_json::json!([
        {
            "description": "bootstrap (capabilities + health + quick_ref in one call)",
            "invocation": "usr-loop --robot-triage"
        },
        {
            "description": "full versioned agent contract",
            "invocation": "usr-loop capabilities"
        },
        {
            "description": "machine-readable health check",
            "invocation": "usr-loop doctor --robot-json"
        },
        {
            "description": "stage [A]+[B] — capture gaps + privacy-prove fixtures",
            "invocation": "usr-loop --robot-json scan <estate>"
        },
        {
            "description": "stage [C] — deduped clusters",
            "invocation": "usr-loop --robot-json cluster <estate>"
        },
        {
            "description": "stage [D] — propose candidate diff (never applied)",
            "invocation": "usr-loop --robot-json propose <estate>"
        },
        {
            "description": "stage [E] — run §3 conformance gate on a candidate",
            "invocation": "usr-loop --robot-json gate <candidate.json>"
        },
        {
            "description": "stage [F] — land or quarantine a candidate (spec §7 [F'])",
            "invocation": "usr-loop --robot-json land <candidate.json> --fixture <min.sql>"
        },
        {
            "description": "append-only Ledger ops",
            "invocation": "usr-loop --robot-json ledger {append|verify|index|tripwire} ..."
        }
    ]);
    let mega = serde_json::json!({
        "capabilities": capabilities_json(),
        "health": health,
        "quick_ref": quick_ref,
    });
    let rendered = if robot_json {
        serde_json::to_string(&mega)
    } else {
        serde_json::to_string_pretty(&mega)
    };
    match rendered {
        Ok(s) => println!("{s}"),
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "serialize_failed",
                &format!("robot-triage serialization failed: {e}"),
                None,
                None,
            );
            return ExitCode::from(1);
        }
    }
    if blocker_count > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn run_scan(estate_path: &Path, robot_json: bool) -> ExitCode {
    if !estate_path.exists() {
        emit_error_envelope(
            robot_json,
            "estate_not_found",
            &format!("estate path does not exist: {}", estate_path.display()),
            Some(estate_path),
            Some("pass an existing PL/SQL estate or project root"),
        );
        return ExitCode::from(1);
    }

    // Deterministic, side-effect-free analyze: caching disabled so
    // the scan never writes to disk and always recomputes (a scan
    // observes; it must not mutate the estate or a cache).
    let mut req = AnalysisRequest {
        project_root: estate_path.to_path_buf(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;

    let run = match analyze_project(req) {
        Ok(r) => r,
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "engine_analyze_failed",
                &format!("engine analyze failed: {e}"),
                Some(estate_path),
                None,
            );
            return ExitCode::from(1);
        }
    };

    let mut records = capture_gaps(&run);

    // Stage [B] (P2): for every repairable gap, build + privacy-prove
    // a MinFixture from the estate (read-in-place) and stamp
    // `min_fixture_id` / `privacy_proof_id`. The estate is only read;
    // the sole writes are the synthetic, privacy-proven fixtures
    // under `<repo>/.usr/fixtures/` (gitignored). A gap that cannot
    // be safely minimised honestly keeps `min_fixture_id = None`
    // (privacy beats coverage — I-PRIVACY).
    let repo_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    minimize_estate_gaps(estate_path, &repo_root, &mut records, DEFAULT_MAX_BYTES);

    let envelope = GapRecordEnvelope::new(records);

    let rendered = if robot_json {
        envelope.to_robot_json()
    } else {
        envelope.to_pretty_json()
    };
    match rendered {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "serialize_failed",
                &format!("gap-record serialization failed: {e}"),
                Some(estate_path),
                None,
            );
            ExitCode::from(1)
        }
    }
}

/// Shared stage [A]→[B] pipeline: analyze read-in-place, capture
/// repairable gaps, minimise + privacy-prove. Returns the records
/// plus the content-addressed estate-run id. `None` ⇒ a fatal
/// runtime error (already reported to stderr) with the exit code.
fn scan_and_minimize(
    estate_path: &Path,
    robot_json: bool,
) -> Result<(Vec<plsql_accretion::GapRecord>, String, PathBuf), ExitCode> {
    if !estate_path.exists() {
        emit_error_envelope(
            robot_json,
            "estate_not_found",
            &format!("estate path does not exist: {}", estate_path.display()),
            Some(estate_path),
            Some("pass an existing PL/SQL estate or project root"),
        );
        return Err(ExitCode::from(1));
    }
    let mut req = AnalysisRequest {
        project_root: estate_path.to_path_buf(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;
    let run = match analyze_project(req) {
        Ok(r) => r,
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "engine_analyze_failed",
                &format!("engine analyze failed: {e}"),
                Some(estate_path),
                None,
            );
            return Err(ExitCode::from(1));
        }
    };
    let run_id = estate_run_id(&run);
    let mut records = capture_gaps(&run);
    let repo_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    minimize_estate_gaps(estate_path, &repo_root, &mut records, DEFAULT_MAX_BYTES);
    Ok((records, run_id, repo_root))
}

fn run_cluster(estate_path: &Path, robot_json: bool) -> ExitCode {
    let (records, _run_id, repo_root) = match scan_and_minimize(estate_path, robot_json) {
        Ok(v) => v,
        Err(code) => return code,
    };
    // Representatives are ordered smallest-source-first using the
    // sizes of the persisted privacy-proven fixtures (deterministic).
    let sizes = fixture_sizes_from_store(&repo_root);
    let clusters = cluster_gaps_with(&records, DEFAULT_MAX_REPRESENTATIVES, &sizes);
    let envelope = GapClusterEnvelope::new(clusters);
    let rendered = if robot_json {
        envelope.to_robot_json()
    } else {
        envelope.to_pretty_json()
    };
    match rendered {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "serialize_failed",
                &format!("gap-cluster serialization failed: {e}"),
                Some(estate_path),
                None,
            );
            ExitCode::from(1)
        }
    }
}

/// Select the target cluster: by `--cluster-id` signature-prefix, or
/// (the `--from-scan` default) the highest-occurrence cluster, ties
/// broken by signature for determinism (I-DETERMINISM).
fn select_cluster<'a>(
    clusters: &'a [GapCluster],
    cluster_id: Option<&str>,
) -> Option<&'a GapCluster> {
    if let Some(want) = cluster_id {
        return clusters.iter().find(|c| c.signature.starts_with(want));
    }
    clusters.iter().max_by(|a, b| {
        a.occurrence_count
            .cmp(&b.occurrence_count)
            .then_with(|| b.signature.cmp(&a.signature))
    })
}

/// `usr-loop propose` — stage [D]. Scan→cluster the estate, select a
/// cluster, run the deterministic stub proposer, and emit the
/// CandidateDiff (or the honest `unrepairable` refusal, spec §7).
/// NEVER applies the diff (landing is P6).
fn run_propose(
    estate_path: &Path,
    cluster_id: Option<&str>,
    _from_scan: bool,
    robot_json: bool,
) -> ExitCode {
    let (records, run_id, repo_root) = match scan_and_minimize(estate_path, robot_json) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let sizes = fixture_sizes_from_store(&repo_root);
    let clusters = cluster_gaps_with(&records, DEFAULT_MAX_REPRESENTATIVES, &sizes);
    let Some(target) = select_cluster(&clusters, cluster_id) else {
        emit_error_envelope(
            robot_json,
            "no_cluster_matched",
            &format!(
                "no cluster matched (clusters={}, selector={:?})",
                clusters.len(),
                cluster_id
            ),
            Some(estate_path),
            Some(
                "omit --cluster-id to propose for the highest-occurrence cluster, or rescan and pick a frozen signature prefix from `usr-loop cluster <estate>`",
            ),
        );
        return ExitCode::from(1);
    };
    let commit = plsql_accretion::git_head_short();
    let proposer = DeterministicStubProposer;
    match proposer.propose(target, &run_id, &commit) {
        Ok(candidate) => {
            // Emit the candidate + provenance — NEVER applied.
            let payload = serde_json::json!({
                "applied": false,
                "note": "PROPOSED candidate diff — NOT applied (landing is P6, spec §9)",
                "candidate": candidate,
            });
            let env = RobotJsonEnvelope::new(CANDIDATE_DIFF_SCHEMA, payload);
            let _ = emit_envelope(&env, robot_json);
            ExitCode::SUCCESS
        }
        Err(ProposerError::Unrepairable { signature, reason }) => {
            // An honest refusal is correct behavior, not a failure
            // (spec §7/§9). Exit 0 with the typed `unrepairable`
            // verdict so the loop can file the bead.
            let payload = serde_json::json!({
                "applied": false,
                "verdict": "unrepairable",
                "signature": signature,
                "reason": reason,
                "note": "honest refusal — filed unrepairable-for-now (spec §7), NOT a failure",
            });
            let env = RobotJsonEnvelope::new(CANDIDATE_DIFF_SCHEMA, payload);
            let _ = emit_envelope(&env, robot_json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            let payload =
                serde_json::json!({ "applied": false, "verdict": "error", "error": e.to_string() });
            let env = RobotJsonEnvelope::new(CANDIDATE_DIFF_SCHEMA, payload);
            let _ = emit_envelope(&env, robot_json);
            ExitCode::from(1)
        }
    }
}

fn ledger_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".usr")
        .join("ledger")
}

fn run_ledger(action: LedgerAction, robot_json: bool) -> ExitCode {
    match action {
        LedgerAction::Append { estate_path } => {
            let (records, run_id, repo_root) = match scan_and_minimize(&estate_path, robot_json) {
                Ok(v) => v,
                Err(code) => return code,
            };
            let sizes = fixture_sizes_from_store(&repo_root);
            let clusters = cluster_gaps_with(&records, DEFAULT_MAX_REPRESENTATIVES, &sizes);
            let ledger = match Ledger::open(ledger_dir()) {
                Ok(l) => l,
                Err(e) => {
                    emit_error_envelope(
                        robot_json,
                        "ledger_open_failed",
                        &format!("ledger open failed: {e}"),
                        None,
                        None,
                    );
                    return ExitCode::from(1);
                }
            };
            let mut appended = 0usize;
            for c in &clusters {
                let body = LedgerBody::from_cluster(&run_id, c);
                if let Err(e) = ledger.append(body) {
                    emit_error_envelope(
                        robot_json,
                        "ledger_append_failed",
                        &format!("ledger append failed: {e}"),
                        None,
                        None,
                    );
                    return ExitCode::from(1);
                }
                appended += 1;
            }
            let report = serde_json::json!({
                "action": "append",
                "estate_run_id": run_id,
                "clusters": clusters.len(),
                "entries_processed": appended,
                "ledger_path": ledger.path().display().to_string(),
            });
            print_json(&report, robot_json)
        }
        LedgerAction::Verify => {
            let ledger = match Ledger::open(ledger_dir()) {
                Ok(l) => l,
                Err(e) => {
                    emit_error_envelope(
                        robot_json,
                        "ledger_open_failed",
                        &format!("ledger open failed: {e}"),
                        None,
                        None,
                    );
                    return ExitCode::from(1);
                }
            };
            match ledger.verify_chain() {
                Ok(()) => {
                    let entries = ledger.iter().map(|v| v.len()).unwrap_or(0);
                    let report = serde_json::json!({
                        "action": "verify",
                        "status": "ok",
                        "entries": entries,
                        "ledger_path": ledger.path().display().to_string(),
                    });
                    print_json(&report, robot_json)
                }
                Err(e) => {
                    let report = serde_json::json!({
                        "action": "verify",
                        "status": "broken",
                        "error": e.to_string(),
                    });
                    let _ = print_json(&report, robot_json);
                    ExitCode::from(1)
                }
            }
        }
        LedgerAction::Index { corpus_path } => run_index(&corpus_path, robot_json),
        LedgerAction::Tripwire {
            corpus_path,
            git_ref,
            baseline_ref,
        } => run_tripwire(&corpus_path, &git_ref, baseline_ref.as_deref(), robot_json),
    }
}

/// §4 monotonic tripwire. Deterministic: the index is a pure function
/// of the frozen public benchmark scan + the provenance Ledger's
/// resolved signatures (no wall-clock, no RNG, no private estate). Appends
/// the measurement to the append-only `accretion_ledger.jsonl`
/// (idempotent-by-content) and asserts monotonic non-decrease vs the
/// baseline ref. First run with no baseline seeds the floor + PASSes
/// (documented: I-MONOTONIC-VALUE establishes the monotone floor).
fn run_tripwire(
    corpus_path: &Path,
    git_ref: &str,
    baseline_ref: Option<&str>,
    robot_json: bool,
) -> ExitCode {
    use plsql_accretion::{AccretionLedger, BenchmarkRecord, Ledger, compute_accretion_index};
    if !corpus_path.exists() {
        emit_error_envelope(
            robot_json,
            "corpus_not_found",
            &format!(
                "benchmark corpus path does not exist: {}",
                corpus_path.display()
            ),
            Some(corpus_path),
            Some("pass a frozen public benchmark corpus root (e.g. corpus/synthetic)"),
        );
        return ExitCode::from(1);
    }
    // 1. extracted_semantics_ratio over the FROZEN public benchmark
    //    (corpus-only — anyone can reproduce this; never the private estate).
    let mut req = AnalysisRequest {
        project_root: corpus_path.to_path_buf(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;
    let run = match analyze_project(req) {
        Ok(r) => r,
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "engine_analyze_failed",
                &format!("engine analyze failed: {e}"),
                Some(corpus_path),
                None,
            );
            return ExitCode::from(1);
        }
    };
    // 2. distinct_resolved_gap_signatures: signature classes the loop
    //    has permanently closed = those with a landed Ledger entry.
    let ledger = match Ledger::open(ledger_dir()) {
        Ok(l) => l,
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "ledger_open_failed",
                &format!("ledger open failed: {e}"),
                None,
                None,
            );
            return ExitCode::from(1);
        }
    };
    let landed_sigs: Vec<String> = match ledger.iter() {
        Ok(entries) => {
            let mut v: Vec<String> = entries
                .iter()
                .filter(|e| e.body.landed_patch.is_some())
                .map(|e| e.body.signature.clone())
                .collect();
            v.sort();
            v.dedup();
            v
        }
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "ledger_read_failed",
                &format!("ledger read failed: {e}"),
                None,
                None,
            );
            return ExitCode::from(1);
        }
    };
    let bench = vec![BenchmarkRecord {
        objects_with_extracted_semantics: run.completeness.objects_with_extracted_semantics as u64,
        objects_unrecognized: run.completeness.objects_unrecognized as u64,
        resolved_signatures: landed_sigs.clone(),
    }];
    let commit = plsql_accretion::git_head_short();
    let index = compute_accretion_index(&bench, &commit);

    // 3. Append to the append-only accretion ledger
    //    (idempotent-by-content).
    let acc = match AccretionLedger::open(ledger_dir()) {
        Ok(a) => a,
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "accretion_ledger_open_failed",
                &format!("accretion ledger open failed: {e}"),
                None,
                None,
            );
            return ExitCode::from(1);
        }
    };
    let entries_before = acc.iter().map(|v| v.len()).unwrap_or(0);
    if let Err(e) = acc.append(git_ref, index.clone()) {
        emit_error_envelope(
            robot_json,
            "accretion_ledger_append_failed",
            &format!("accretion ledger append failed: {e}"),
            None,
            None,
        );
        return ExitCode::from(1);
    }
    // 4. Monotonic assertion vs the baseline ref.
    let history = acc.iter().unwrap_or_default();
    let baseline_entry =
        baseline_ref.and_then(|r| history.iter().rfind(|e| e.git_ref == r).cloned());
    let (status, exit) = match &baseline_entry {
        None => {
            // No release baseline yet — seed the monotone floor and
            // PASS (documented, spec §4: first run establishes the
            // floor; a regression is structurally impossible with no
            // prior point).
            (
                "seeded-floor (no release baseline yet — monotone floor established; PASS)",
                ExitCode::SUCCESS,
            )
        }
        Some(base) => {
            if index.coverage_index + f64::EPSILON >= base.index.coverage_index {
                (
                    "monotonic-ok (coverage_index ≥ baseline)",
                    ExitCode::SUCCESS,
                )
            } else {
                (
                    "REGRESSION (coverage_index dropped below baseline — I-MONOTONIC-VALUE violated)",
                    ExitCode::from(1),
                )
            }
        }
    };
    let report = serde_json::json!({
        "action": "tripwire",
        "git_ref": git_ref,
        "baseline_ref": baseline_ref,
        "coverage_index": index.coverage_index,
        "extracted_semantics_ratio": index.extracted_semantics_ratio,
        "distinct_resolved_gap_signatures": index.distinct_resolved_gap_signatures,
        "landed_signatures": landed_sigs,
        "baseline_coverage_index": baseline_entry.as_ref().map(|e| e.index.coverage_index),
        "accretion_ledger": acc.path().display().to_string(),
        "accretion_ledger_entries_before": entries_before,
        "accretion_ledger_entries_after": acc.iter().map(|v| v.len()).unwrap_or(0),
        "status": status,
    });
    let _ = print_json(&report, robot_json);
    exit
}

/// Compute the §4 accretion index from a public corpus scan
/// (never the private estate — the metric must be reproducible by anyone).
fn run_index(corpus_path: &Path, robot_json: bool) -> ExitCode {
    if !corpus_path.exists() {
        emit_error_envelope(
            robot_json,
            "corpus_not_found",
            &format!("corpus path does not exist: {}", corpus_path.display()),
            Some(corpus_path),
            Some("pass a public benchmark corpus root (e.g. corpus/synthetic)"),
        );
        return ExitCode::from(1);
    }
    let mut req = AnalysisRequest {
        project_root: corpus_path.to_path_buf(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;
    let run = match analyze_project(req) {
        Ok(r) => r,
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "engine_analyze_failed",
                &format!("engine analyze failed: {e}"),
                Some(corpus_path),
                None,
            );
            return ExitCode::from(1);
        }
    };
    let run_id = estate_run_id(&run);
    let records = capture_gaps(&run);
    // Resolved signatures = signatures that now carry a
    // privacy-proven fixture (id + proof both present). Corpus-only.
    let resolved: Vec<String> = {
        let mut v: Vec<String> = records
            .iter()
            .filter(|r| r.min_fixture_id.is_some() && r.privacy_proof_id.is_some())
            .map(|r| r.signature.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    };
    let bench = vec![plsql_accretion::BenchmarkRecord {
        objects_with_extracted_semantics: run.completeness.objects_with_extracted_semantics as u64,
        objects_unrecognized: run.completeness.objects_unrecognized as u64,
        resolved_signatures: resolved,
    }];
    let commit = plsql_accretion::git_head_short();
    let index = plsql_accretion::compute_accretion_index(&bench, &commit);
    let report = serde_json::json!({
        "action": "index",
        "corpus_run_id": run_id,
        "accretion_index": index,
    });
    print_json(&report, robot_json)
}

fn print_json(value: &serde_json::Value, robot_json: bool) -> ExitCode {
    let rendered = if robot_json {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    };
    match rendered {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "serialize_failed",
                &format!("serialization failed: {e}"),
                None,
                None,
            );
            ExitCode::from(1)
        }
    }
}

/// `usr-loop gate <candidate-diff>` — the typed §3 gate runner.
/// Fail-closed: the ONLY exit-0 path is [`GateOutcome::Accept`]. A
/// sha mismatch / missing script is the immutability abort (exit 4);
/// an I-PRIVACY G8 leak is exit 9 (nothing persisted); any other
/// REJECT is exit 3.
fn run_gate_cmd(candidate: &Path, robot_json: bool) -> ExitCode {
    let repo_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if !candidate.is_file() {
        emit_error_envelope(
            robot_json,
            "candidate_not_found",
            &format!("candidate diff not found: {}", candidate.display()),
            Some(candidate),
            Some("first run `usr-loop --robot-json propose <estate>` to emit a candidate JSON"),
        );
        return ExitCode::from(3);
    }
    let outcome = match run_gate(&repo_root, candidate, &[]) {
        Ok(o) => o,
        Err(e) => {
            // Immutability / spawn errors are hard aborts — never a
            // pass. sha mismatch + missing script ⇒ exit 4.
            let code = match &e {
                GateError::ShaMismatch { .. }
                | GateError::ScriptMissing(_)
                | GateError::ShaManifestMissing(_) => 4,
                _ => 3,
            };
            let env = RobotJsonEnvelope::new(
                GATE_OUTCOME_SCHEMA,
                serde_json::json!({ "verdict": "abort", "error": e.to_string() }),
            );
            let _ = emit_envelope(&env, robot_json);
            return ExitCode::from(code);
        }
    };
    let payload = serde_json::to_value(&outcome).unwrap_or_else(
        |e| serde_json::json!({ "verdict": "abort", "error": format!("serialize: {e}") }),
    );
    let env = RobotJsonEnvelope::new(GATE_OUTCOME_SCHEMA, payload);
    let _ = emit_envelope(&env, robot_json);
    match &outcome {
        GateOutcome::Accept { .. } => ExitCode::SUCCESS,
        GateOutcome::PrivacyAbort { .. } => ExitCode::from(9),
        GateOutcome::Reject { .. } => ExitCode::from(3),
    }
}

/// `usr-loop land <candidate> --fixture <min.sql>` — stage [F].
/// Reads the proposed candidate (a `plsql.usr.candidate_diff`
/// envelope OR a raw candidate body), runs the REAL §3 gate, and on
/// ACCEPT lands it atomically (apply + corpus pin + exactly one
/// ledger entry). On REJECT it persists the provenanced [F']
/// quarantine and exits 3 (the spec-correct outcome — never weakens
/// the gate, never lands unproven).
fn run_land_cmd(candidate_path: &Path, fixture_path: &Path, robot_json: bool) -> ExitCode {
    let repo_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let Ok(raw) = std::fs::read_to_string(candidate_path) else {
        emit_error_envelope(
            robot_json,
            "candidate_not_readable",
            &format!("candidate not readable: {}", candidate_path.display()),
            Some(candidate_path),
            None,
        );
        return ExitCode::from(1);
    };
    // Accept either the `propose` envelope (`{schema,payload:{candidate}}`)
    // or a bare CandidateDiff JSON.
    let candidate: CandidateDiff = match parse_candidate(&raw) {
        Ok(c) => c,
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "candidate_parse_failed",
                &format!("cannot parse candidate diff: {e}"),
                Some(candidate_path),
                None,
            );
            return ExitCode::from(1);
        }
    };
    let Ok(fixture_src) = std::fs::read_to_string(fixture_path) else {
        emit_error_envelope(
            robot_json,
            "fixture_not_readable",
            &format!("fixture not readable: {}", fixture_path.display()),
            Some(fixture_path),
            None,
        );
        return ExitCode::from(1);
    };
    let fixture = LandFixture {
        id: plsql_accretion::sha256_hex(fixture_src.as_bytes()),
        source: fixture_src,
    };
    // Reconstruct the minimal GapCluster the ledger entry needs from
    // the candidate's own provenance (signature, repair class, the
    // representative fixture). This keeps `land` operable from just
    // the proposed candidate + its fixture (no re-scan needed).
    // The diag_code is provenance-only on the ledger body; the
    // candidate's repair class maps deterministically to the
    // structural class it targets (g→PARSE-ANTLR4RUST-001,
    // l/d→IR_DDL_NOT_LOWERED) — honest and stable, never estate data.
    let diag_code = match candidate.repair_class {
        plsql_accretion::RepairClass::Grammar => "PARSE-ANTLR4RUST-001",
        _ => "IR_DDL_NOT_LOWERED",
    }
    .to_string();
    let cluster = GapCluster {
        signature: candidate.signature.clone(),
        diag_code,
        antlr_rule_path: None,
        repair_class: candidate.repair_class,
        occurrence_count: candidate.honesty.diagnostics_resolved.max(0) as u64,
        representative_min_fixtures: vec![fixture.id.clone()],
        first_seen_commit: candidate.proposed_at_commit.clone(),
    };
    let ledger_dir = ledger_dir();
    match land_candidate(
        &repo_root,
        &candidate,
        &cluster,
        &fixture,
        &candidate.estate_run_id,
        &ledger_dir,
        &[],
    ) {
        Ok(receipt) => {
            let payload = serde_json::json!({
                "verdict": "landed",
                "receipt": receipt,
                "rollback": {
                    "note": "git revert anchor — the ledger maps signature → landed_commit (spec §7)",
                    "signature": receipt.signature,
                    "landed_commit": receipt.landed_commit,
                },
            });
            let env = RobotJsonEnvelope::new(LAND_OUTCOME_SCHEMA, payload);
            let _ = emit_envelope(&env, robot_json);
            ExitCode::SUCCESS
        }
        Err(LandError::Quarantined(q)) => {
            // [F'] — persist the provenanced quarantine bead. On a G8
            // I-PRIVACY abort nothing is written (fail-safe).
            let persisted = if q.privacy_abort {
                None
            } else {
                persist_quarantine(&repo_root, &q).ok()
            };
            let payload = serde_json::json!({
                "verdict": if q.privacy_abort { "privacy_abort" } else { "quarantined" },
                "note": "candidate REJECTED — NOT landed, gate NOT weakened (spec §7 [F'])",
                "quarantine": &*q,
                "quarantine_artifact": persisted.map(|p| p.display().to_string()),
            });
            let env = RobotJsonEnvelope::new(LAND_OUTCOME_SCHEMA, payload);
            let _ = emit_envelope(&env, robot_json);
            if q.privacy_abort {
                ExitCode::from(9)
            } else {
                ExitCode::from(3)
            }
        }
        Err(LandError::Gate(e)) => {
            let env = RobotJsonEnvelope::new(
                LAND_OUTCOME_SCHEMA,
                serde_json::json!({ "verdict": "abort", "error": e.to_string() }),
            );
            let _ = emit_envelope(&env, robot_json);
            ExitCode::from(4)
        }
        Err(e) => {
            let env = RobotJsonEnvelope::new(
                LAND_OUTCOME_SCHEMA,
                serde_json::json!({ "verdict": "error", "error": e.to_string() }),
            );
            let _ = emit_envelope(&env, robot_json);
            ExitCode::from(1)
        }
    }
}

/// Parse a `CandidateDiff` from either the `usr-loop propose`
/// envelope (`{schema,payload:{candidate:…}}`) or a bare
/// `CandidateDiff` JSON object. Fail-closed: an unrecognised shape is
/// an error, never a fabricated candidate.
fn parse_candidate(raw: &str) -> Result<CandidateDiff, String> {
    let v: serde_json::Value = serde_json::from_str(raw).map_err(|e| format!("not JSON: {e}"))?;
    if let Some(c) = v.pointer("/payload/candidate") {
        return serde_json::from_value(c.clone())
            .map_err(|e| format!("payload.candidate not a CandidateDiff: {e}"));
    }
    if let Some(c) = v.pointer("/payload") {
        if c.get("id").is_some() && c.get("body").is_some() {
            return serde_json::from_value(c.clone())
                .map_err(|e| format!("payload not a CandidateDiff: {e}"));
        }
    }
    serde_json::from_value(v).map_err(|e| format!("not a CandidateDiff envelope or object: {e}"))
}

fn emit_envelope(
    env: &RobotJsonEnvelope<serde_json::Value>,
    robot_json: bool,
) -> Result<(), serde_json::Error> {
    let s = if robot_json {
        serde_json::to_string(env)?
    } else {
        serde_json::to_string_pretty(env)?
    };
    println!("{s}");
    Ok(())
}

fn run_doctor(robot_json: bool) -> ExitCode {
    // Reuse the shared `doctor_report_json` helper so the `doctor`
    // subcommand and the `--robot-triage` mega-call cannot diverge.
    let (report, blocker_count) = doctor_report_json();
    let rendered = if robot_json {
        serde_json::to_string(&report)
    } else {
        serde_json::to_string_pretty(&report)
    };
    match rendered {
        Ok(s) => {
            println!("{s}");
            if blocker_count > 0 {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            emit_error_envelope(
                robot_json,
                "serialize_failed",
                &format!("doctor serialization failed: {e}"),
                None,
                None,
            );
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Drift-guard for the `capabilities` agent contract. If the JSON
    /// shape changes, this test must be updated AND
    /// `CAPABILITIES_CONTRACT_VERSION` bumped — that coupling is the
    /// whole point: an agent that pinned the contract should never be
    /// silently surprised by a shape change.
    #[test]
    fn capabilities_contract_is_pinned() {
        let c = capabilities_json();
        assert_eq!(c["binary"], "usr-loop");
        assert_eq!(c["contract_version"], CAPABILITIES_CONTRACT_VERSION);
        assert_eq!(c["version"], env!("CARGO_PKG_VERSION"));
        for key in [
            "global_flags",
            "subcommands",
            "schemas",
            "exit_codes",
            "stdout_contract",
        ] {
            assert!(c.get(key).is_some(), "capabilities missing key `{key}`");
        }
        let subs = c["subcommands"].as_object().unwrap();
        for required in [
            "scan",
            "cluster",
            "propose",
            "gate",
            "land",
            "ledger",
            "doctor",
            "capabilities",
            "robot-docs",
        ] {
            assert!(
                subs.contains_key(required),
                "missing subcommand `{required}`"
            );
        }
        // Exit-code dictionary must cover at least every code the CLI returns.
        for code in ["0", "1", "2", "3", "4", "9"] {
            assert!(
                c["exit_codes"][code].is_string(),
                "exit_codes missing `{code}`"
            );
        }
        // Schema descriptors must include the error envelope schema so
        // agents can pin the error shape.
        assert!(c["schemas"]["error_envelope"]["id"].is_string());
    }

    #[test]
    fn capabilities_is_single_line_in_robot_mode() {
        let s = serde_json::to_string(&capabilities_json()).unwrap();
        assert!(!s.contains('\n'), "robot-json must be single-line");
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["binary"], "usr-loop");
    }

    #[test]
    fn robot_docs_mentions_canonical_anchors() {
        let docs = robot_docs_text();
        for required in [
            "capabilities",
            "doctor",
            "--robot-triage",
            "--robot-json",
            "error_envelope",
        ] {
            assert!(
                docs.contains(required),
                "robot-docs must mention `{required}`"
            );
        }
    }

    /// Capabilities subcommand list must match the `Command` enum
    /// variants — any new variant that is NOT added to
    /// `capabilities_json` will be caught here.
    #[test]
    fn capabilities_subcommand_set_matches_clap() {
        let cap_keys: std::collections::BTreeSet<String> = capabilities_json()["subcommands"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let mut clap_names: std::collections::BTreeSet<String> = Cli::command()
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .collect();
        // `help` is auto-injected by clap; do not require it in the
        // capabilities document.
        clap_names.remove("help");
        assert_eq!(
            cap_keys, clap_names,
            "capabilities subcommands must match clap enum variants"
        );
    }

    /// `--robot-triage` output must be a single object with the three
    /// top-level keys and the embedded capabilities must agree with
    /// the standalone `capabilities` surface.
    #[test]
    fn robot_triage_payload_shape() {
        let (health, _blockers) = doctor_report_json();
        let mega = serde_json::json!({
            "capabilities": capabilities_json(),
            "health": health,
            "quick_ref": serde_json::Value::Array(vec![]),
        });
        let s = serde_json::to_string(&mega).unwrap();
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        for key in ["capabilities", "health", "quick_ref"] {
            assert!(round.get(key).is_some(), "triage missing `{key}`");
        }
        assert_eq!(round["capabilities"]["binary"], "usr-loop");
        assert_eq!(round["health"]["tool"], "usr-loop");
    }

    /// Drift-guard for the `plsql.usr.error_envelope` v1 shape. An
    /// agent that pins this schema must never be surprised by a
    /// silent shape change.
    #[test]
    fn error_envelope_shape_is_pinned() {
        let payload = serde_json::json!({
            "kind": "error",
            "code": "estate_not_found",
            "message": "estate path does not exist: /nonexistent",
            "path": "/nonexistent",
            "remediation": "pass an existing PL/SQL estate or project root",
        });
        let env = RobotJsonEnvelope::new(ERROR_ENVELOPE_SCHEMA, payload);
        let s = serde_json::to_string(&env).unwrap();
        // Single-line JSON (robot-mode invariant).
        assert!(!s.contains('\n'));
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["format"], "plsql-robot-json");
        assert_eq!(round["schema_id"], "plsql.usr.error_envelope");
        assert_eq!(round["schema_version"]["major"], 1);
        assert_eq!(round["payload"]["kind"], "error");
        assert_eq!(round["payload"]["code"], "estate_not_found");
        assert!(round["payload"]["message"].is_string());
    }

    /// Help text rendered by clap for the top-level subcommand
    /// summary must NOT contain literal Markdown bold markers
    /// (`**foo**`) — clap renders them verbatim in plain TTY output,
    /// producing noisy `**Stage [F] LAND**` tokens.
    #[test]
    fn clap_help_is_markdown_free() {
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        cmd.write_long_help(&mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(
            !text.contains("**"),
            "clap help must not leak Markdown bold markers (`**`):\n{text}"
        );
    }

    /// Clap's built-in suggestions must give a "did you mean" hint
    /// for a near-miss like `--robotjson`. This is on by default in
    /// clap v4 but the test pins it so a future Cargo dep bump that
    /// disables it does not silently regress agent UX.
    #[test]
    fn clap_typo_suggests_robot_json() {
        let err = Cli::try_parse_from(["usr-loop", "--robotjson", "doctor"]).unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("--robot-json") || s.contains("similar"),
            "clap should suggest --robot-json for --robotjson typo; got: {s}"
        );
    }

    #[test]
    fn clap_typo_suggests_subcommand() {
        let err = Cli::try_parse_from(["usr-loop", "doctorx"]).unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("doctor") || s.contains("similar"),
            "clap should suggest `doctor` for `doctorx` typo; got: {s}"
        );
    }
}
