//! `plsql-mcp` binary — foundation MCP adapter.
//!
//! Wires the CLI shape (`--robot-json`, `doctor` subcommand). `serve`
//! reaches into the transport layer once per-tool implementations are
//! present.

#![forbid(unsafe_code)]

use std::io::{BufReader, Write};
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};

use plsql_mcp::config::McpConfig;
use plsql_mcp::default_tool_registry;
use plsql_mcp::doctor::{DoctorReport, DoctorSeverity, doctor_report};
use plsql_mcp::tcp;

#[derive(Parser, Debug)]
#[command(
    name = "plsql-mcp",
    version,
    about = "Foundation MCP adapter for the PL/SQL Intelligence engine",
    long_about = "Speaks the Model Context Protocol over stdio (default). \
                  Surfaces the engine's foundation static-analysis tools and, \
                  when built with the `live-db` feature, the read-only-by-default \
                  live-Oracle tool surface (§13A of plan.md)."
)]
struct Cli {
    /// Emit a single JSON object on stdout instead of human text.
    #[arg(long, global = true)]
    robot_json: bool,

    /// Emit a single JSON mega-object combining capabilities, health, and
    /// quick_ref in one round-trip — the canonical agent bootstrap call.
    /// Exit 0 on success; exit 2 if the doctor reports a blocker.
    #[arg(long, global = true)]
    robot_triage: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start the MCP server on the configured transport (stdio by default).
    Serve {
        /// Optional TCP listen target instead of stdio.
        #[arg(long)]
        listen: Option<String>,
        /// Permit binding a non-loopback address. Off by default:
        /// `--listen` accepts only loopback (`127.0.0.0/8`, `::1`).
        /// Any wider bind — `0.0.0.0`/`::`, RFC1918 private ranges
        /// (`10/8`, `172.16/12`, `192.168/16`), link-local, or a
        /// public IP — requires this flag. The MCP transport is
        /// unauthenticated, so a non-loopback bind exposes the full
        /// tool surface to every host that can reach the socket.
        #[arg(long)]
        allow_public_bind: bool,
    },
    /// Print a diagnostic report and exit non-zero if any blocker is found.
    Doctor,
    /// Print build information (version, enabled features) and exit.
    Info,
    /// Print the machine-readable agent contract (version, transports,
    /// commands, exit-code dictionary, feature flags) as JSON and exit.
    /// An agent should read this instead of guessing the surface.
    Capabilities,
    /// Print a paste-ready agent handbook to stdout: what plsql-mcp is,
    /// how to drive it, the transport model, exit-code dictionary, and
    /// explicit pointers to `capabilities` + `doctor`.
    RobotDocs,
}

/// Stable contract version for the `capabilities` payload. Bump only on a
/// breaking change to the JSON shape; a pinned regression test
/// (`capabilities_contract_is_pinned`) fails if the shape drifts without
/// this being bumped.
const CAPABILITIES_CONTRACT_VERSION: u32 = 2;

/// Build the `capabilities` contract document. Factored out of the
/// command handler so the schema can be pinned by a unit test without
/// spawning the binary (Axiom 17 — every contract surface has a
/// drift-guard test).
fn capabilities_json() -> serde_json::Value {
    serde_json::json!({
        "binary": "plsql-mcp",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "mcp_protocol_version": plsql_mcp::mcp_protocol::PROTOCOL_VERSION,
        "transports": ["stdio", "tcp"],
        "features": {
            "live-db": cfg!(feature = "live-db")
        },
        "global_flags": {
            "--robot-json": "emit a single machine-readable JSON object on stdout instead of human text"
        },
        "commands": {
            "serve": "start the real MCP JSON-RPC 2.0 server loop (stdio default; --listen <host:port> for TCP); blocks until stdin EOF (stdio) or the listener is closed (TCP)",
            "doctor": "print a diagnostic report; exit 2 if any blocker is found",
            "info": "print build version + enabled features",
            "capabilities": "print this contract document"
        },
        "exit_codes": {
            "0": "success",
            "1": "runtime failure: doctor found a blocker, or the serve transport hit a fatal I/O error",
            "2": "safety-block: a blocker was found (doctor) or an invalid --listen target was supplied (serve)"
        },
        "stdout_contract": "stdout is data only; all diagnostics go to stderr"
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let robot_json = cli.robot_json;
    let robot_triage = cli.robot_triage;

    // --robot-triage short-circuits normal dispatch: emit the mega-object
    // and exit before evaluating any subcommand.
    if robot_triage {
        return run_robot_triage();
    }

    // Bare invocation (no subcommand). Print help to stderr and exit 2 —
    // matches clap's `print_help_and_exit_on_no_subcommand` pattern. Never
    // silently run an undocumented doctor-equivalent on stdout; an MCP
    // launcher that pipes stdin/stdout would otherwise get human text on
    // stdout and a misleading exit 0. stdout stays empty so the
    // stdout-is-data-only contract holds even for this error path.
    let Some(command) = cli.command else {
        let mut cmd = Cli::command();
        // Write help to stderr (Axiom 4: diagnostics never go to stdout).
        let _ = cmd.write_long_help(&mut std::io::stderr());
        let _ = writeln!(std::io::stderr());
        let _ = writeln!(
            std::io::stderr(),
            "no subcommand given — try `plsql-mcp doctor`, `plsql-mcp serve`, or `plsql-mcp --robot-triage`."
        );
        return ExitCode::from(2);
    };

    match command {
        Command::Serve {
            listen,
            allow_public_bind,
        } => run_serve(listen, allow_public_bind, robot_json),
        Command::Doctor => run_doctor(robot_json),
        Command::Info => run_info(robot_json),
        Command::Capabilities => run_capabilities(robot_json),
        Command::RobotDocs => {
            print!("{}", robot_docs_text());
            ExitCode::SUCCESS
        }
    }
}

/// Run the real MCP server loop.
///
/// * `listen == None` → stdio transport. Reads line-delimited JSON-RPC
///   from stdin, dispatches through the transport-agnostic protocol layer
///   (`tcp::process_stream`, the exact same pure pump the TCP path uses),
///   writes each response as one JSON line to stdout. EOF on stdin → clean
///   exit 0. Every diagnostic goes to stderr (Axiom 4: stdout is data).
/// * `listen == Some(host:port)` → TCP transport via the existing
///   `tcp::serve` accept loop. A `--listen` parse error names the exact
///   fix on stderr (Axiom 6) and exits non-zero (Axiom 14).
fn run_serve(listen: Option<String>, allow_public_bind: bool, robot_json: bool) -> ExitCode {
    let registry = default_tool_registry();

    match listen {
        None => {
            // Startup line is a diagnostic → stderr only; stdout stays
            // pure JSON-RPC for the MCP client.
            if robot_json {
                eprintln!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "kind": "status",
                        "transport": "stdio",
                        "registered_tool_count": registry.len(),
                        "mcp_protocol_version": plsql_mcp::PROTOCOL_VERSION,
                    }))
                    .unwrap()
                );
            } else {
                eprintln!(
                    "plsql-mcp serve: stdio transport ready ({} tools, MCP {})",
                    registry.len(),
                    plsql_mcp::PROTOCOL_VERSION
                );
            }
            let stdin = std::io::stdin();
            let mut stdout = std::io::stdout();
            // `process_stream` is the exact dispatch loop the TCP path
            // uses — reuse it so stdio and TCP can never diverge.
            match tcp::process_stream(BufReader::new(stdin.lock()), &mut stdout, &registry) {
                Ok(()) => {
                    let _ = stdout.flush();
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("plsql-mcp serve: stdio transport I/O error: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Some(raw) => {
            // Public-bind refusal stays the responsibility of
            // `parse_listen_target`. The safe default (loopback-only)
            // holds unless the operator passes `--allow-public-bind`.
            let target = match tcp::parse_listen_target(&raw, allow_public_bind) {
                Ok(t) => t,
                Err(e) => {
                    // The error message itself names the exact fix
                    // (e.g. "pass --allow-public-bind to override").
                    if robot_json {
                        eprintln!(
                            "{}",
                            serde_json::to_string(&serde_json::json!({
                                "kind": "error",
                                "code": "MCP_LISTEN_INVALID",
                                "message": e.to_string(),
                            }))
                            .unwrap()
                        );
                    } else {
                        eprintln!("plsql-mcp serve: invalid --listen: {e}");
                    }
                    return ExitCode::from(2);
                }
            };
            if robot_json {
                eprintln!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "kind": "status",
                        "transport": "tcp",
                        "listen": tcp::describe(&target),
                        "registered_tool_count": registry.len(),
                        "mcp_protocol_version": plsql_mcp::PROTOCOL_VERSION,
                    }))
                    .unwrap()
                );
            } else {
                eprintln!(
                    "plsql-mcp serve: TCP transport listening on {} ({} tools, MCP {})",
                    tcp::describe(&target),
                    registry.len(),
                    plsql_mcp::PROTOCOL_VERSION
                );
            }
            match tcp::serve(&target, &registry) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("plsql-mcp serve: TCP transport error: {e}");
                    ExitCode::from(1)
                }
            }
        }
    }
}

/// Return a paste-ready agent handbook as a `String`. Factored out of the
/// `robot-docs` command handler so it can be pinned by a unit test without
/// spawning the binary (Axiom 17 — every contract surface has a drift-guard).
///
/// The handbook covers: what plsql-mcp is, the MCP transport model (stdio
/// default, TCP via `--listen`), how an agent should drive it, the exit-code
/// dictionary, and explicit pointers to `capabilities` + `doctor`.
fn robot_docs_text() -> String {
    format!(
        r#"# plsql-mcp agent handbook
## What is plsql-mcp?
plsql-mcp (v{version}) is a Model Context Protocol (MCP) server that exposes
the PL/SQL Intelligence engine as a set of tools an AI agent can call. It
provides foundation static-analysis tools (parse, compile-check, dep-graph,
SAST) and, when built with the `live-db` feature ({live_db}), a read-only-by-
default live-Oracle tool surface (schema describe, query, audit).

## Transport model
* Default: stdio — the MCP client pipes JSON-RPC over stdin/stdout.
  Launch: plsql-mcp serve
* Optional: TCP listener — pass --listen <host:port> to bind a TCP socket.
  Launch: plsql-mcp serve --listen 127.0.0.1:7070
  The TCP transport is UNAUTHENTICATED. --listen accepts only loopback
  (127.0.0.0/8, ::1) by default; any wider bind (RFC1918/link-local
  included) requires --allow-public-bind and exposes the full tool
  surface to every host that can reach the socket.
* All diagnostic output goes to stderr; stdout is data only.

## How an agent should drive plsql-mcp
1. Bootstrap (one round-trip): plsql-mcp --robot-triage
   Returns JSON with `capabilities`, `health`, and `quick_ref`. Parse it.
   Exit 2 means a blocker — do not proceed to `serve`.
2. Full contract: plsql-mcp capabilities
   Returns the versioned agent contract (JSON). Pin the `contract_version`
   field; bump only on a breaking shape change.
3. Health check: plsql-mcp doctor --robot-json
   Returns a DoctorReport JSON object. Check `findings[].severity == "blocker"`.
4. Start server: plsql-mcp serve [--listen <host:port>]
   Blocks; use the MCP wire protocol (JSON-RPC 2.0) over the chosen transport.
5. Info: plsql-mcp info [--robot-json]
   Prints build version + enabled features.
6. Human handbook: plsql-mcp robot-docs
   Prints this text.

## Exit-code dictionary
  0  success / clean doctor / clean serve shutdown (stdin EOF)
  1  runtime failure: doctor blocker, or serve hit a fatal transport I/O error
  2  safety-block: a blocker was found (doctor or --robot-triage), or serve
     was given an invalid --listen target (the error names the exact fix)

## Global flags
  --robot-json     emit compact single-line JSON on stdout (all commands)
  --robot-triage   emit one mega-JSON object (capabilities + health + quick_ref)
                   and exit; the canonical agent bootstrap call

## Quick pointers
* plsql-mcp --robot-triage          # mega bootstrap (fastest agent start)
* plsql-mcp capabilities            # versioned agent contract
* plsql-mcp doctor --robot-json     # machine-readable health check
* plsql-mcp serve                   # start MCP server (stdio)
* plsql-mcp serve --listen 127.0.0.1:7070  # start MCP server (TCP)
"#,
        version = env!("CARGO_PKG_VERSION"),
        live_db = if cfg!(feature = "live-db") {
            "enabled"
        } else {
            "disabled"
        },
    )
}

/// Build and emit the `--robot-triage` mega-object: capabilities + health
/// summary + quick_ref — all in one JSON object on stdout. Exit 0 normally;
/// exit 2 if the doctor found a blocker (so the caller can gate `serve`).
fn run_robot_triage() -> ExitCode {
    let config = McpConfig::default();
    // Report against the same registry `serve` actually exposes, so the
    // health payload tells the truth about the live tool surface.
    let registry = default_tool_registry();
    let report = doctor_report(&config, &registry);

    let has_blocker = report
        .findings
        .iter()
        .any(|f| matches!(f.severity, DoctorSeverity::Blocker));

    let blocker_count = report
        .findings
        .iter()
        .filter(|f| matches!(f.severity, DoctorSeverity::Blocker))
        .count();

    let health = serde_json::json!({
        "binary_version": report.binary_version,
        "registered_tool_count": report.registered_tool_count,
        "blocker_count": blocker_count,
        "findings": report.findings,
    });

    let quick_ref = serde_json::json!([
        {
            "description": "bootstrap (capabilities + health + quick_ref in one call)",
            "invocation": "plsql-mcp --robot-triage"
        },
        {
            "description": "full versioned agent contract",
            "invocation": "plsql-mcp capabilities"
        },
        {
            "description": "machine-readable health check",
            "invocation": "plsql-mcp doctor --robot-json"
        },
        {
            "description": "start MCP server (stdio)",
            "invocation": "plsql-mcp serve"
        },
        {
            "description": "start MCP server (TCP)",
            "invocation": "plsql-mcp serve --listen 127.0.0.1:7070"
        }
    ]);

    let mega = serde_json::json!({
        "capabilities": capabilities_json(),
        "health": health,
        "quick_ref": quick_ref,
    });

    println!("{}", serde_json::to_string(&mega).unwrap());

    if has_blocker {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn run_capabilities(robot_json: bool) -> ExitCode {
    let doc = capabilities_json();
    // `capabilities` is an inherently machine-readable surface, so it is
    // always valid JSON on stdout (Axiom 4: stdout is data). `--robot-json`
    // selects compact single-line output; otherwise pretty-print so a human
    // skimming it can still read it.
    if robot_json {
        println!("{}", serde_json::to_string(&doc).unwrap());
    } else {
        println!("{}", serde_json::to_string_pretty(&doc).unwrap());
    }
    ExitCode::SUCCESS
}

fn run_doctor(robot_json: bool) -> ExitCode {
    let config = McpConfig::default();
    // Diagnose the registry `serve` actually exposes — an empty
    // `ToolRegistry::new()` would falsely report zero tools and fire the
    // stale "bead skeleton" finding now that the surface is wired.
    let registry = default_tool_registry();
    let report = doctor_report(&config, &registry);

    if robot_json {
        let json = serde_json::to_string(&report).expect("doctor report serializes");
        println!("{json}");
    } else {
        print_doctor_human(&report);
    }

    if report
        .findings
        .iter()
        .any(|f| matches!(f.severity, DoctorSeverity::Blocker))
    {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_doctor_human(report: &DoctorReport) {
    // Diagnostics — version banner, registered-tool count, and every
    // finding row — go to stderr so the stdout-is-data-only contract
    // (capabilities.stdout_contract, robot-docs handbook) holds in
    // human mode as well as `--robot-json`. stdout carries only the
    // final one-line structured summary so a wrapper like
    // `plsql-mcp doctor | grep -q "blockers=0"` works without
    // tripping over WARN / INFO noise.
    eprintln!(
        "plsql-mcp {} (live-db: {}, transport: {}, safety: {:?})",
        report.binary_version,
        report.live_db_feature_enabled,
        report.transport,
        report.active_safety_profile
    );
    eprintln!("registered tools: {}", report.registered_tool_count);
    let mut blocker_count = 0usize;
    let mut warning_count = 0usize;
    let mut info_count = 0usize;
    for finding in &report.findings {
        let tag = match finding.severity {
            DoctorSeverity::Ok => "OK",
            DoctorSeverity::Info => {
                info_count += 1;
                "INFO"
            }
            DoctorSeverity::Warning => {
                warning_count += 1;
                "WARN"
            }
            DoctorSeverity::Blocker => {
                blocker_count += 1;
                "BLOCK"
            }
        };
        eprintln!("[{tag}] {} — {}", finding.code, finding.summary);
        if let Some(remediation) = &finding.remediation {
            eprintln!("       → {remediation}");
        }
    }
    // Structured one-line summary on stdout. Reports every severity tier so
    // a caller can grep for `blockers=0` without conflating a
    // warning-but-no-blocker state with perfect health.
    println!(
        "doctor: blockers={blocker_count} warnings={warning_count} info={info_count}"
    );
}

fn run_info(robot_json: bool) -> ExitCode {
    let info = serde_json::json!({
        "binary": "plsql-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "live_db_feature_enabled": cfg!(feature = "live-db"),
    });
    if robot_json {
        println!("{}", serde_json::to_string(&info).unwrap());
    } else {
        println!(
            "plsql-mcp {} (live-db: {})",
            env!("CARGO_PKG_VERSION"),
            cfg!(feature = "live-db")
        );
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_mcp::tools::ToolRegistry;

    /// Drift-guard for the `capabilities` agent contract (Axiom 17). If
    /// the JSON shape changes, this test must be updated AND
    /// `CAPABILITIES_CONTRACT_VERSION` bumped — that coupling is the whole
    /// point: an agent that pinned the contract should never be silently
    /// surprised by a shape change.
    #[test]
    fn capabilities_contract_is_pinned() {
        let c = capabilities_json();
        assert_eq!(c["binary"], "plsql-mcp");
        assert_eq!(c["contract_version"], 2);
        assert_eq!(c["version"], env!("CARGO_PKG_VERSION"));
        for key in [
            "mcp_protocol_version",
            "transports",
            "features",
            "global_flags",
            "commands",
            "exit_codes",
            "stdout_contract",
        ] {
            assert!(c.get(key).is_some(), "capabilities missing key `{key}`");
        }
        let transports = c["transports"].as_array().unwrap();
        assert!(transports.iter().any(|t| t == "stdio"));
        assert!(transports.iter().any(|t| t == "tcp"));
        assert!(c["exit_codes"]["0"].is_string());
        assert!(c["exit_codes"]["2"].is_string());
        let cmds = c["commands"].as_object().unwrap();
        for required in ["serve", "doctor", "info", "capabilities"] {
            assert!(cmds.contains_key(required), "missing command `{required}`");
        }
    }

    #[test]
    fn capabilities_is_valid_single_line_json_in_robot_mode() {
        let s = serde_json::to_string(&capabilities_json()).unwrap();
        assert!(!s.contains('\n'), "robot-json must be single-line");
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["binary"], "plsql-mcp");
    }

    /// Drift-guard: robot_docs_text() must mention both `capabilities` and
    /// `doctor` so agents reading the handbook always find those two commands.
    #[test]
    fn robot_docs_mentions_capabilities_and_doctor() {
        let text = robot_docs_text();
        assert!(
            text.contains("capabilities"),
            "robot-docs must mention `capabilities`"
        );
        assert!(text.contains("doctor"), "robot-docs must mention `doctor`");
        // Must be non-trivial (≥ 40 lines to satisfy Axiom 9 intent).
        let line_count = text.lines().count();
        assert!(
            line_count >= 40,
            "robot-docs must be ≥ 40 lines; got {line_count}"
        );
    }

    /// The --robot-triage output must be a single-line JSON object containing
    /// the three top-level keys: capabilities, health, quick_ref.
    #[test]
    fn robot_triage_is_single_line_and_has_capabilities_health_quick_ref() {
        let config = McpConfig::default();
        let registry = ToolRegistry::new();
        let report = doctor_report(&config, &registry);

        let has_blocker = report
            .findings
            .iter()
            .any(|f| matches!(f.severity, DoctorSeverity::Blocker));

        let blocker_count = report
            .findings
            .iter()
            .filter(|f| matches!(f.severity, DoctorSeverity::Blocker))
            .count();

        let health = serde_json::json!({
            "binary_version": report.binary_version,
            "registered_tool_count": report.registered_tool_count,
            "blocker_count": blocker_count,
            "findings": report.findings,
        });

        let quick_ref = serde_json::json!([
            {
                "description": "bootstrap (capabilities + health + quick_ref in one call)",
                "invocation": "plsql-mcp --robot-triage"
            },
            {
                "description": "full versioned agent contract",
                "invocation": "plsql-mcp capabilities"
            },
            {
                "description": "machine-readable health check",
                "invocation": "plsql-mcp doctor --robot-json"
            },
            {
                "description": "start MCP server (stdio)",
                "invocation": "plsql-mcp serve"
            },
            {
                "description": "start MCP server (TCP)",
                "invocation": "plsql-mcp serve --listen 127.0.0.1:7070"
            }
        ]);

        let mega = serde_json::json!({
            "capabilities": capabilities_json(),
            "health": health,
            "quick_ref": quick_ref,
        });

        let s = serde_json::to_string(&mega).unwrap();

        // Must be single-line (no embedded newlines).
        assert!(
            !s.contains('\n'),
            "--robot-triage output must be single-line"
        );

        // Must parse back to a valid JSON object with all three keys.
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        for key in ["capabilities", "health", "quick_ref"] {
            assert!(
                parsed.get(key).is_some(),
                "--robot-triage JSON missing key `{key}`"
            );
        }

        // capabilities sub-object must pass the same drift-guard as the
        // standalone `capabilities` command.
        let cap = &parsed["capabilities"];
        assert_eq!(cap["binary"], "plsql-mcp");
        assert_eq!(cap["contract_version"], CAPABILITIES_CONTRACT_VERSION);

        // health sub-object must have the four required fields.
        let h = &parsed["health"];
        for key in [
            "binary_version",
            "registered_tool_count",
            "blocker_count",
            "findings",
        ] {
            assert!(h.get(key).is_some(), "health missing key `{key}`");
        }

        // quick_ref must be a non-empty array.
        let qr = parsed["quick_ref"].as_array().unwrap();
        assert!(!qr.is_empty(), "quick_ref must be non-empty");
        assert!(
            qr.len() >= 3,
            "quick_ref must contain ≥ 3 entries; got {}",
            qr.len()
        );

        // blocker coupling: no blockers in default config → has_blocker is false.
        assert!(!has_blocker, "default config should have no blockers");
    }

    /// Blocker coupling: if the doctor reports a blocker, run_robot_triage
    /// must signal exit code 2. We verify this by checking the blocker-count
    /// field in the health payload correlates with has_blocker.
    #[test]
    fn robot_triage_health_reflects_doctor() {
        let config = McpConfig::default();
        let registry = ToolRegistry::new();
        let report = doctor_report(&config, &registry);

        let blocker_count = report
            .findings
            .iter()
            .filter(|f| matches!(f.severity, DoctorSeverity::Blocker))
            .count();

        let has_blocker = report
            .findings
            .iter()
            .any(|f| matches!(f.severity, DoctorSeverity::Blocker));

        // The health payload's `blocker_count` must equal the actual count
        // derived from the same DoctorReport.
        assert_eq!(
            blocker_count,
            report
                .findings
                .iter()
                .filter(|f| matches!(f.severity, DoctorSeverity::Blocker))
                .count(),
            "blocker_count derivation must be deterministic"
        );

        // Invariant: blocker_count > 0 iff has_blocker is true.
        assert_eq!(
            blocker_count > 0,
            has_blocker,
            "blocker_count and has_blocker must agree"
        );

        // Default config has no blockers → exit code 0 path is taken.
        assert_eq!(blocker_count, 0, "default config must have 0 blockers");
        assert!(!has_blocker, "default config must not have a blocker");

        // The health JSON must expose findings faithfully.
        let health = serde_json::json!({
            "binary_version": report.binary_version,
            "registered_tool_count": report.registered_tool_count,
            "blocker_count": blocker_count,
            "findings": report.findings,
        });
        assert_eq!(health["blocker_count"], 0);
        assert_eq!(health["binary_version"], env!("CARGO_PKG_VERSION"));
    }

    // ── PLSQL-MCP-002: real serve loop ────────────────────────────────────

    /// Truth-telling drift-guard: now that `serve` is a real loop, no
    /// agent-facing surface may still claim it is unimplemented / a stub /
    /// "not yet wired". Covers `capabilities`, the robot-docs handbook, and
    /// the exit-code dictionary.
    #[test]
    fn serve_surfaces_no_longer_claim_unimplemented() {
        let cap = serde_json::to_string(&capabilities_json()).unwrap();
        let docs = robot_docs_text();
        for hay in [&cap, &docs] {
            let lc = hay.to_ascii_lowercase();
            assert!(
                !lc.contains("not yet implemented"),
                "serve must not be advertised as unimplemented: {hay}"
            );
            assert!(
                !lc.contains("not yet wired") && !lc.contains("not wired"),
                "serve transport must not be advertised as unwired: {hay}"
            );
        }
        // The serve command description must describe a real server loop.
        let c = capabilities_json();
        let serve_desc = c["commands"]["serve"].as_str().unwrap();
        assert!(
            serve_desc.contains("real MCP") && serve_desc.contains("JSON-RPC"),
            "serve description must describe the real loop: {serve_desc}"
        );
        // Exit-code dictionary now documents the runtime-failure code too.
        assert!(c["exit_codes"]["1"].is_string());
    }

    /// The canonical registry `serve` exposes must be non-empty and contain
    /// the foundation-static tools — otherwise an MCP client gets an empty
    /// `tools/list` and the server is useless.
    #[test]
    fn default_tool_registry_is_a_real_surface() {
        let reg = default_tool_registry();
        assert!(
            reg.len() >= 7,
            "expected a rich foundation surface, got {} tools",
            reg.len()
        );
        let names: Vec<&str> = reg.tools.iter().map(|t| t.name.as_str()).collect();
        for must in ["parse_file", "compile_check", "analyze_project", "query"] {
            assert!(
                names.contains(&must),
                "registry missing `{must}`: {names:?}"
            );
        }
    }

    /// Path to the freshly-built `plsql-mcp` binary. `CARGO_BIN_EXE_*` is
    /// only injected for integration tests (the `tests/` dir), not for unit
    /// tests compiled into the bin, so we derive it from the test
    /// executable's own location: the binary is a sibling in the same
    /// `target/<profile>/` directory (the test harness lives in `deps/`).
    fn bin() -> std::path::PathBuf {
        let mut dir = std::env::current_exe().expect("test exe path");
        dir.pop(); // drop the test binary file name
        if dir.ends_with("deps") {
            dir.pop(); // climb out of deps/ into target/<profile>/
        }
        let exe = if cfg!(windows) {
            "plsql-mcp.exe"
        } else {
            "plsql-mcp"
        };
        dir.join(exe)
    }

    /// Spawn the built binary in stdio mode, drive a real MCP session
    /// (initialize → tools/list → tools/call), and assert: valid JSON-RPC
    /// results with echoed ids, stdout is ONLY JSON (no log lines leak from
    /// stderr), and stdin EOF yields a clean exit 0.
    #[test]
    fn stdio_serve_round_trips_a_real_mcp_session() {
        use std::io::{Read, Write};
        use std::process::{Command as PCommand, Stdio};

        let mut child = PCommand::new(bin())
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn plsql-mcp serve");

        let reqs = format!(
            concat!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{v}"}}}}"#,
                "\n",
                r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#,
                "\n",
                r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"inspect_profile","arguments":{{}}}}}}"#,
                "\n",
            ),
            v = plsql_mcp::PROTOCOL_VERSION
        );

        {
            let mut stdin = child.stdin.take().expect("child stdin");
            stdin.write_all(reqs.as_bytes()).expect("write requests");
            // Dropping stdin closes it → server sees EOF and exits.
        }

        let mut out = String::new();
        child
            .stdout
            .take()
            .expect("child stdout")
            .read_to_string(&mut out)
            .expect("read stdout");
        let status = child.wait().expect("child exits");
        assert!(status.success(), "stdin EOF must yield clean exit 0");

        let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 3, "three requests → three responses: {out:?}");

        // Every stdout line is parseable JSON-RPC (no log noise on stdout).
        let v1: serde_json::Value = serde_json::from_str(lines[0]).expect("frame 1 is JSON");
        assert_eq!(v1["id"], 1);
        assert_eq!(v1["result"]["protocolVersion"], plsql_mcp::PROTOCOL_VERSION);

        let v2: serde_json::Value = serde_json::from_str(lines[1]).expect("frame 2 is JSON");
        assert_eq!(v2["id"], 2);
        let tools = v2["result"]["tools"].as_array().expect("tools array");
        assert!(
            tools.iter().any(|t| t["name"] == "inspect_profile"),
            "tools/list must expose the real registry"
        );

        let v3: serde_json::Value = serde_json::from_str(lines[2]).expect("frame 3 is JSON");
        assert_eq!(v3["id"], 3);
        assert_eq!(v3["jsonrpc"], "2.0");
        assert!(
            v3.get("result").is_some(),
            "tools/call on a registered tool must return a result: {v3}"
        );
    }

    /// Bind an ephemeral loopback port and drive the **real**
    /// `tcp::serve` accept loop (via `serve_bounded_on_listener`) on a
    /// thread, connect a `TcpStream`, run the same initialize→tools/list
    /// round-trip, and assert the thread joins.
    ///
    /// Hermetic by construction: the listener is bound exactly once on
    /// `127.0.0.1:0` (OS-assigned ephemeral port), its `local_addr()` is
    /// read back, and the **live** listener is moved straight into the
    /// accept loop. There is no bind→drop→rebind window, so two parallel
    /// instances of this test (or any other socket) can never collide on
    /// the port — killing the recurring `AddrInUse` flake. Binding before
    /// the client spawns also guarantees the socket is listening before
    /// the first connect attempt.
    #[test]
    fn tcp_serve_round_trips_over_a_real_socket() {
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};

        // Bind ONCE; the OS picks a free ephemeral port. The listener is
        // already listening here, before the client thread exists.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let bound = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let reg = default_tool_registry();
            // The real production accept loop (serve_with_listener),
            // bounded to one connection so the test terminates.
            tcp::serve_bounded_on_listener(&listener, &reg, 1)
                .expect("accept loop serves one connection cleanly");
        });

        // The listener is bound before this thread spawned, so the very
        // first connect succeeds; the bounded retry only tolerates the
        // accept-thread scheduling lag, never a missing bind.
        let mut client = {
            let mut attempt = None;
            for _ in 0..50 {
                match TcpStream::connect(bound) {
                    Ok(s) => {
                        attempt = Some(s);
                        break;
                    }
                    Err(_) => std::thread::yield_now(),
                }
            }
            attempt.expect("connect to loopback server")
        };
        client
            .write_all(
                format!(
                    concat!(
                        r#"{{"jsonrpc":"2.0","id":11,"method":"initialize","params":{{"protocolVersion":"{v}"}}}}"#,
                        "\n",
                        r#"{{"jsonrpc":"2.0","id":12,"method":"tools/list"}}"#,
                        "\n",
                    ),
                    v = plsql_mcp::PROTOCOL_VERSION
                )
                .as_bytes(),
            )
            .unwrap();
        client.flush().unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        let mut resp = String::new();
        client.read_to_string(&mut resp).unwrap();
        server.join().expect("server thread joins cleanly");

        let lines: Vec<&str> = resp.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 2, "two requests → two responses: {resp:?}");
        let a: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(a["id"], 11);
        assert_eq!(a["result"]["protocolVersion"], plsql_mcp::PROTOCOL_VERSION);
        let b: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(b["id"], 12);
        assert!(
            b["result"]["tools"]
                .as_array()
                .is_some_and(|t| !t.is_empty())
        );
    }

    /// Bare invocation (no subcommand) must not silently behave like
    /// `doctor`. Either it prints help (clap convention) or runs an
    /// explicit, documented summary on **stderr** — never as undocumented
    /// stdout-on-success. The contract: stdout stays empty so a caller
    /// piping the binary into JSON-RPC or capturing stdout cannot mistake
    /// the bare-invocation noise for data.
    #[test]
    fn bare_invocation_does_not_silently_run_doctor_on_stdout() {
        use std::process::Command as PCommand;
        let out = PCommand::new(bin())
            .output()
            .expect("run plsql-mcp with no args");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.stdout.is_empty(),
            "bare invocation must not write to stdout (got: {stdout:?})"
        );
        // The user must see *something* — either clap's help or a brief
        // summary pointing at the documented entry points. Either way,
        // it lands on stderr.
        assert!(
            !stderr.trim().is_empty(),
            "bare invocation must emit a usage hint on stderr (got nothing)"
        );
        // The summary must point an agent at the canonical commands so a
        // user who runs the binary by mistake learns the next step.
        let lc = stderr.to_ascii_lowercase();
        assert!(
            lc.contains("plsql-mcp") && (lc.contains("doctor") || lc.contains("serve")),
            "bare-invocation hint must mention the canonical commands; got: {stderr:?}"
        );
    }

    /// `doctor` (human mode) must honor the stdout-is-data-only contract
    /// from `capabilities.stdout_contract` and the robot-docs handbook:
    /// every WARN / INFO finding and the version banner go to stderr;
    /// stdout is reserved for the structured summary (or empty when no
    /// findings exist).
    #[test]
    fn doctor_human_mode_routes_findings_to_stderr_not_stdout() {
        use std::process::Command as PCommand;
        let out = PCommand::new(bin())
            .arg("doctor")
            .output()
            .expect("run plsql-mcp doctor");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        // No [WARN]/[INFO]/[BLOCK] tagged lines on stdout — that violates
        // the documented stdout-is-data-only contract.
        for tag in ["[WARN]", "[INFO]", "[BLOCK]"] {
            assert!(
                !stdout.contains(tag),
                "doctor human mode must not write `{tag}` to stdout; got stdout={stdout:?}"
            );
        }
        // The version banner ("plsql-mcp <version>") must not be on stdout
        // either — it is a diagnostic header.
        assert!(
            !stdout.contains("plsql-mcp 0.")
                && !stdout.contains("registered tools:"),
            "doctor human mode must not write the version banner / tool-count line to stdout; got: {stdout:?}"
        );
        // Findings that DO exist on this host (e.g. missing connections.toml)
        // must surface on stderr so an operator sees them.
        assert!(
            stderr.contains("plsql-mcp"),
            "doctor human mode must emit at least the version banner on stderr; got: {stderr:?}"
        );
    }

    /// `--robot-json` (machine mode) keeps the inverse contract: stdout
    /// is the single JSON payload, stderr stays diagnostic. Pin both
    /// halves so a future refactor cannot accidentally flip them.
    #[test]
    fn doctor_robot_json_keeps_stdout_as_data_only() {
        use std::process::Command as PCommand;
        let out = PCommand::new(bin())
            .args(["doctor", "--robot-json"])
            .output()
            .expect("run plsql-mcp doctor --robot-json");
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Exactly one JSON document on stdout.
        let trimmed = stdout.trim();
        let parsed: serde_json::Value =
            serde_json::from_str(trimmed).expect("doctor --robot-json must emit one JSON object");
        assert_eq!(parsed["binary_name"], "plsql-mcp");
        // The findings live inside the JSON, NOT as bare [WARN] lines.
        for tag in ["[WARN]", "[INFO]"] {
            assert!(
                !stdout.contains(tag),
                "robot-json stdout must contain only JSON; got tag `{tag}` in {stdout:?}"
            );
        }
    }

    /// Every Unix-style CLI accepts `--version`. clap auto-derives it from
    /// the workspace package version when `#[command(version)]` is set.
    /// Both forms (`--version` and `-V`) must work and print the version.
    #[test]
    fn version_flag_long_form_is_accepted() {
        use std::process::Command as PCommand;
        let out = PCommand::new(bin())
            .arg("--version")
            .output()
            .expect("run plsql-mcp --version");
        assert!(
            out.status.success(),
            "--version must exit 0; got status={:?}, stderr={:?}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(env!("CARGO_PKG_VERSION")),
            "--version must print the package version; got stdout={stdout:?}"
        );
    }

    #[test]
    fn version_flag_short_form_is_accepted() {
        use std::process::Command as PCommand;
        let out = PCommand::new(bin())
            .arg("-V")
            .output()
            .expect("run plsql-mcp -V");
        assert!(out.status.success(), "-V must exit 0");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(env!("CARGO_PKG_VERSION")),
            "-V must print the package version; got stdout={stdout:?}"
        );
    }

    /// An invalid `--listen` target must exit non-zero with an error that
    /// names the exact fix (Axioms 6 + 14) and must not hang.
    #[test]
    fn serve_with_invalid_listen_fails_fast_with_actionable_error() {
        use std::process::Command as PCommand;
        let out = PCommand::new(bin())
            .args(["serve", "--listen", "0.0.0.0:7070"])
            .output()
            .expect("run serve --listen");
        assert!(!out.status.success(), "public bind must be refused");
        assert_eq!(out.status.code(), Some(2), "invalid --listen → exit 2");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("--allow-public-bind"),
            "error must name the exact fix: {stderr}"
        );
        assert!(
            out.stdout.is_empty(),
            "diagnostics must not leak to stdout: {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}
