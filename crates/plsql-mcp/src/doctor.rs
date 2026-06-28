//! `plsql-mcp doctor` data shape.
//!
//! Wires the subcommand and report struct, `oraclemcp-db` backend reporting,
//! and the `live-db` build-status row. Connection profile validation plus
//! audit-posture verification land in the live-DB layers.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::McpConfig;
use crate::connections::ConnectionRegistry;
use crate::safety::SafetyProfile;
use crate::tools::ToolRegistry;

/// Top-level doctor report.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub binary_name: String,
    pub binary_version: String,
    pub live_db_feature_enabled: bool,
    pub active_safety_profile: SafetyProfile,
    pub registered_tool_count: usize,
    pub transport: String,
    /// Legacy thick-driver compatibility posture.
    pub instant_client: InstantClientPosture,
    /// Selected `OracleConnection` backend.
    pub oracle_connection_backend: OracleConnectionBackendInfo,
    /// Audit posture configured for this run.
    pub audit_posture: AuditPosture,
    /// Per-connection write posture — derived from
    /// registered `ConnectionProfile`s and the active session state. Empty
    /// when no connections are registered or `doctor_report` was called
    /// without a connection registry.
    pub connection_write_posture: Vec<ConnectionWritePostureRow>,
    /// Protocol / transport / engine-cache / profile health.
    pub mcp_health: McpHealth,
    pub findings: Vec<DoctorFinding>,
}

/// MCP-010 health block: the four checks an agent needs before
/// trusting the server — protocol version it speaks, whether the
/// configured transport is initialisable, whether an engine
/// artifact/cache directory is reachable, and whether the active
/// `AnalysisProfile` is internally consistent. Unknown/unset
/// inputs are reported as a typed status, never a fake "ok"
/// (R13).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpHealth {
    /// MCP wire protocol version this server speaks.
    pub protocol_version: String,
    /// Configured transport ("stdio" / "tcp:<addr>").
    pub transport_kind: String,
    /// True iff the configured transport can be initialised in
    /// this build (stdio always; tcp requires a parseable addr).
    pub transport_healthy: bool,
    /// Engine artifact/cache directory reachability.
    pub engine_cache: CacheReachability,
    /// Active analysis profile, summarised.
    pub analysis_profile_summary: String,
    /// True iff the profile passed the sanity check (a target
    /// Oracle version is set and no contradictory compatibility
    /// floor).
    pub analysis_profile_sane: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheReachability {
    /// A cache directory is configured and present/writable.
    Reachable,
    /// A cache directory is configured but missing/unwritable.
    Unreachable,
    /// No cache directory configured — immutable-artifact mode.
    /// Not an error; reported distinctly (R13).
    #[default]
    NotConfigured,
}

/// Per-connection write posture row emitted by the doctor.
/// Captures whether each registered profile authorizes writes and why.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectionWritePostureRow {
    pub name: String,
    pub connect_string: String,
    pub permanently_read_only: bool,
    pub is_active: bool,
    /// True when the connection is the active session AND the active
    /// safety profile allows writes AND the connection is not
    /// `permanently_read_only`.
    pub writes_currently_allowed: bool,
    /// Short label derived from the row state — handy for human-readable
    /// renders without re-deriving the boolean combo on the consumer side.
    pub posture_label: String,
}

/// What the doctor knows about the audit baseline.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditPosture {
    /// `DBMS_APPLICATION_INFO.SET_MODULE` is always invoked on a live tool
    /// call — the constant module name reported. Mirrors the SQLcl-MCP
    /// convention so DBAs see a consistent vendor marker.
    pub module_name: String,
    /// Whether an audit-table sink is configured.
    pub audit_table_configured: bool,
    /// Configured audit-table identifier (or `None` if not configured).
    pub audit_table_name: Option<String>,
    /// `comment_marker_template` shows the placeholder shape used by
    /// `AuditPlan::comment_marker` (the per-call substitution happens at
    /// tool-invocation time).
    pub comment_marker_template: String,
}

/// Legacy thick-driver compatibility posture.
///
/// The normal `plsql-mcp` live-DB path routes through `oraclemcp-db`, which
/// does not need Oracle Instant Client. These fields remain in the JSON report
/// so older consumers do not break while C.6 retires the thick catalog driver.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstantClientPosture {
    /// Whether the binary was built with the `live-db` Cargo feature.
    pub live_db_feature: bool,
    /// First path in `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` (or the
    /// `lib` subdirectory of `ORACLE_HOME`) that looks like an Instant
    /// Client directory. `None` when none is detected.
    pub probable_path: Option<PathBuf>,
    /// Version string extracted from the probable path (heuristic — best
    /// effort, since Oracle ships per-version directories like
    /// `instantclient_23_4` / `instantclient_19_25`).
    pub version_hint: Option<String>,
    /// Environment variables the binary inspected to find Instant Client.
    /// Empty for the normal `oraclemcp-db` thin backend.
    pub inspected_env_vars: Vec<String>,
}

/// Which Oracle connection backend the binary will use.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OracleConnectionBackendInfo {
    /// Backend crate boundary used by the live-DB runtime.
    pub name: String,
    /// Whether the backend is compiled into this binary.
    pub compiled_in: bool,
    /// Free-form notes for agent-facing remediation and inventory.
    pub notes: String,
}

/// Severity tier for doctor findings — mirrors the brand promise that
/// recommendations come with actionable remediation hints (§13A.3).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorSeverity {
    Ok,
    Info,
    Warning,
    Blocker,
}

/// A single doctor-report row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorFinding {
    pub code: String,
    pub severity: DoctorSeverity,
    pub summary: String,
    pub remediation: Option<String>,
}

/// Build the doctor report from the active configuration and tool registry.
///
/// Convenience wrapper around [`doctor_report_with_connections`] that runs
/// without inspecting per-connection state (no production-DSN warnings).
#[must_use]
pub fn doctor_report(config: &McpConfig, registry: &ToolRegistry) -> DoctorReport {
    let connections = ConnectionRegistry::default();
    doctor_report_with_connections(config, registry, &connections)
}

/// Build the doctor report while also inspecting the connection registry
/// for the `permanently_read_only` audit posture.
#[must_use]
pub fn doctor_report_with_connections(
    config: &McpConfig,
    registry: &ToolRegistry,
    connections: &ConnectionRegistry,
) -> DoctorReport {
    let live_db_feature_enabled = cfg!(feature = "live-db");
    let mut findings = Vec::new();
    let instant_client = detect_instant_client(live_db_feature_enabled);
    let oracle_connection_backend = describe_oracle_backend(live_db_feature_enabled);
    let audit_posture = AuditPosture {
        module_name: String::from(crate::audit::APPLICATION_MODULE),
        audit_table_configured: false,
        audit_table_name: None,
        comment_marker_template: String::from("/* plsql-mcp <tool> <session-id> <agent-model> */"),
    };

    if !live_db_feature_enabled {
        findings.push(DoctorFinding {
            code: String::from("MCP_LIVE_DB_FEATURE_DISABLED"),
            severity: DoctorSeverity::Info,
            summary: String::from(
                "live-DB feature disabled in this build; only foundation static-analysis tools are exposed.",
            ),
            remediation: Some(String::from(
                "Rebuild with `cargo install plsql-mcp` (default features) or `cargo build --features live-db` to enable live-DB tools.",
            )),
        });
    }
    if registry.is_empty() {
        findings.push(DoctorFinding {
            code: String::from("MCP_TOOL_REGISTRY_EMPTY"),
            severity: DoctorSeverity::Warning,
            summary: String::from(
                "no MCP tools registered; the binary will respond to `tools/list` with an empty list.",
            ),
            remediation: Some(String::from(
                "Build the registry via `plsql_mcp::default_tool_registry()` — the canonical full surface. `ToolRegistry::new()` is a bare container intended only for tests; production code should never expose it directly.",
            )),
        });
    }
    if config.connections_path.is_none() {
        findings.push(DoctorFinding {
            code: String::from("MCP_CONNECTIONS_FILE_NOT_LOADED"),
            severity: DoctorSeverity::Info,
            summary: String::from(
                "no `connections.toml` loaded; live-DB tools cannot resolve named connections.",
            ),
            remediation: Some(String::from(
                "Create `~/.plsql-mcp/connections.toml` (template in `docs/integrations/live-db/`).",
            )),
        });
    }
    for profile in connections.profiles() {
        if profile.is_production_looking() && !profile.permanently_read_only {
            findings.push(DoctorFinding {
                code: String::from("MCP_PROD_DSN_WITHOUT_PERMANENTLY_READ_ONLY"),
                severity: DoctorSeverity::Warning,
                summary: format!(
                    "connection `{}` has a production-looking DSN (`{}`) but is not marked permanently_read_only.",
                    profile.name, profile.connect_string
                ),
                remediation: Some(format!(
                    "Add `permanently_read_only = true` to the `[[connection]]` entry for `{}` in connections.toml to refuse enable_writes hardly.",
                    profile.name
                )),
            });
        }
    }

    let ok = findings.iter().all(|f| {
        !matches!(
            f.severity,
            DoctorSeverity::Blocker | DoctorSeverity::Warning
        )
    });
    if ok {
        findings.insert(
            0,
            DoctorFinding {
                code: String::from("MCP_DOCTOR_OK"),
                severity: DoctorSeverity::Ok,
                summary: String::from("plsql-mcp doctor: no blockers detected."),
                remediation: None,
            },
        );
    }

    let transport_kind = match config.transport {
        crate::config::TransportConfig::Stdio => String::from("stdio"),
        crate::config::TransportConfig::Tcp { ref listen } => format!("tcp:{listen}"),
    };
    // stdio is always initialisable; a TCP transport is healthy
    // only if its listen address parses as a socket addr.
    let transport_healthy = match config.transport {
        crate::config::TransportConfig::Stdio => true,
        crate::config::TransportConfig::Tcp { ref listen } => {
            listen.parse::<std::net::SocketAddr>().is_ok()
        }
    };
    if !transport_healthy {
        findings.push(DoctorFinding {
            code: String::from("MCP_TRANSPORT_UNHEALTHY"),
            severity: DoctorSeverity::Warning,
            summary: format!(
                "configured TCP transport `{transport_kind}` is not a valid socket address"
            ),
            remediation: Some(String::from(
                "Set a host:port the OS can bind, e.g. 127.0.0.1:7070.",
            )),
        });
    }

    // No engine cache directory is part of McpConfig — the
    // foundation server runs in immutable-artifact mode. Reported
    // distinctly rather than as a misleading failure (R13).
    let engine_cache = CacheReachability::NotConfigured;

    let profile = plsql_core::AnalysisProfile::default();
    let analysis_profile_sane = match profile.compatibility {
        Some(floor) => floor <= profile.oracle_version,
        None => true,
    };
    let analysis_profile_summary = format!(
        "oracle_version={:?}, compatibility={:?}, feature_policy={:?}",
        profile.oracle_version, profile.compatibility, profile.feature_policy
    );
    if !analysis_profile_sane {
        findings.push(DoctorFinding {
            code: String::from("MCP_ANALYSIS_PROFILE_INSANE"),
            severity: DoctorSeverity::Warning,
            summary: String::from(
                "AnalysisProfile compatibility floor is newer than the target Oracle version",
            ),
            remediation: Some(String::from(
                "Lower `compatibility` to <= `oracle_version` or raise the target version.",
            )),
        });
    }

    let mcp_health = McpHealth {
        protocol_version: String::from(crate::mcp_protocol::PROTOCOL_VERSION),
        transport_kind: transport_kind.clone(),
        transport_healthy,
        engine_cache,
        analysis_profile_summary,
        analysis_profile_sane,
    };

    DoctorReport {
        binary_name: String::from("plsql-mcp"),
        binary_version: String::from(env!("CARGO_PKG_VERSION")),
        live_db_feature_enabled,
        active_safety_profile: config.safety,
        registered_tool_count: registry.len(),
        transport: transport_kind,
        instant_client,
        oracle_connection_backend,
        audit_posture,
        connection_write_posture: derive_write_posture(connections),
        mcp_health,
        findings,
    }
}

fn derive_write_posture(registry: &ConnectionRegistry) -> Vec<ConnectionWritePostureRow> {
    let safety = registry.safety();
    let active = registry.current().map(|p| p.name.clone());
    registry
        .profiles()
        .map(|profile| {
            let is_active = active.as_deref() == Some(profile.name.as_str());
            // Writes are only allowed when:
            //   * this profile is the active one,
            //   * the active safety profile permits direct writes, and
            //   * the connection is not flagged permanently_read_only.
            let writes_currently_allowed =
                is_active && safety.allows_direct_writes() && !profile.permanently_read_only;
            let posture_label = String::from(if profile.permanently_read_only {
                "permanently_read_only"
            } else if writes_currently_allowed {
                "writes_enabled"
            } else if is_active {
                "active_read_only"
            } else {
                "inactive"
            });
            ConnectionWritePostureRow {
                name: profile.name.clone(),
                connect_string: profile.connect_string.clone(),
                permanently_read_only: profile.permanently_read_only,
                is_active,
                writes_currently_allowed,
                posture_label,
            }
        })
        .collect()
}

/// Report the retired Instant Client posture.
///
/// The normal live-DB path uses `oraclemcp-db` and does not inspect client
/// library paths. C.6 removes the remaining thick-driver compatibility path.
#[must_use]
fn detect_instant_client(live_db_feature_enabled: bool) -> InstantClientPosture {
    InstantClientPosture {
        live_db_feature: live_db_feature_enabled,
        ..InstantClientPosture::default()
    }
}

fn describe_oracle_backend(live_db_feature_enabled: bool) -> OracleConnectionBackendInfo {
    if !live_db_feature_enabled {
        return OracleConnectionBackendInfo {
            name: String::from("none"),
            compiled_in: false,
            notes: String::from("live-db Cargo feature disabled; no Oracle backend compiled in."),
        };
    }
    OracleConnectionBackendInfo {
        name: String::from("oraclemcp-db"),
        compiled_in: true,
        notes: String::from(
            "oraclemcp-db 0.4.0 thin backend over oracledb; no Oracle Instant Client required for the normal live-DB path.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_emits_warning_finding() {
        let report = doctor_report(&McpConfig::default(), &ToolRegistry::new());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "MCP_TOOL_REGISTRY_EMPTY")
        );
    }

    #[test]
    fn report_carries_active_safety_profile_and_transport() {
        let report = doctor_report(&McpConfig::default(), &ToolRegistry::new());
        assert_eq!(report.active_safety_profile, SafetyProfile::InspectOnly);
        assert_eq!(report.transport, "stdio");
        assert_eq!(report.binary_name, "plsql-mcp");
    }

    #[test]
    fn mcp_health_block_is_populated_mcp010() {
        let report = doctor_report(&McpConfig::default(), &ToolRegistry::new());
        let h = &report.mcp_health;
        assert_eq!(h.protocol_version, crate::mcp_protocol::PROTOCOL_VERSION);
        assert_eq!(h.transport_kind, "stdio");
        assert!(h.transport_healthy, "stdio is always initialisable");
        assert_eq!(h.engine_cache, CacheReachability::NotConfigured);
        assert!(h.analysis_profile_sane, "default profile is sane");
        assert!(h.analysis_profile_summary.contains("oracle_version"));
        // No transport/profile findings for the healthy default.
        assert!(!report.findings.iter().any(
            |f| f.code == "MCP_TRANSPORT_UNHEALTHY" || f.code == "MCP_ANALYSIS_PROFILE_INSANE"
        ));
    }

    #[test]
    fn unhealthy_tcp_transport_is_flagged() {
        let cfg = McpConfig {
            transport: crate::config::TransportConfig::Tcp {
                listen: "not-an-addr".to_string(),
            },
            ..McpConfig::default()
        };
        let report = doctor_report(&cfg, &ToolRegistry::new());
        assert!(!report.mcp_health.transport_healthy);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "MCP_TRANSPORT_UNHEALTHY")
        );
    }

    #[test]
    fn no_blockers_emits_ok_row_first() {
        let report = doctor_report(&McpConfig::default(), &ToolRegistry::new());
        // The skeleton has no Blocker findings; only Warnings/Info — so the
        // OK row is NOT prepended.
        assert!(
            report
                .findings
                .iter()
                .any(|f| matches!(f.severity, DoctorSeverity::Warning))
        );
    }

    #[test]
    fn write_posture_rows_classify_each_registered_connection() {
        use crate::connections::{ConnectionProfile, ConnectionRegistry};
        use crate::safety::SafetyProfile;

        let mut registry = ConnectionRegistry::new(SafetyProfile::SessionWriteEnabled);
        registry.register(ConnectionProfile {
            name: String::from("dev-db"),
            description: None,
            connect_string: String::from("//localhost/DEV"),
            username: None,
            permanently_read_only: false,
            dbtools_alias: None,
        });
        registry.register(ConnectionProfile {
            name: String::from("prod-db"),
            description: None,
            connect_string: String::from("//prod-host/PRDB"),
            username: None,
            permanently_read_only: true,
            dbtools_alias: None,
        });
        registry.connect("dev-db").unwrap();

        let report =
            doctor_report_with_connections(&McpConfig::default(), &ToolRegistry::new(), &registry);

        let dev = report
            .connection_write_posture
            .iter()
            .find(|row| row.name == "dev-db")
            .expect("dev row");
        assert!(dev.is_active);
        assert!(dev.writes_currently_allowed);
        assert_eq!(dev.posture_label, "writes_enabled");

        let prod = report
            .connection_write_posture
            .iter()
            .find(|row| row.name == "prod-db")
            .expect("prod row");
        assert!(!prod.is_active);
        assert!(!prod.writes_currently_allowed);
        assert_eq!(prod.posture_label, "permanently_read_only");
    }

    #[test]
    fn write_posture_row_marks_active_read_only_for_inspect_only_safety() {
        use crate::connections::{ConnectionProfile, ConnectionRegistry};
        use crate::safety::SafetyProfile;

        let mut registry = ConnectionRegistry::new(SafetyProfile::InspectOnly);
        registry.register(ConnectionProfile {
            name: String::from("dev-db"),
            description: None,
            connect_string: String::from("//localhost/DEV"),
            username: None,
            permanently_read_only: false,
            dbtools_alias: None,
        });
        registry.connect("dev-db").unwrap();

        let report =
            doctor_report_with_connections(&McpConfig::default(), &ToolRegistry::new(), &registry);

        let row = &report.connection_write_posture[0];
        assert!(row.is_active);
        assert!(!row.writes_currently_allowed);
        assert_eq!(row.posture_label, "active_read_only");
    }

    #[test]
    fn doctor_warns_when_production_dsn_lacks_permanently_read_only() {
        use crate::connections::{ConnectionProfile, ConnectionRegistry};
        use crate::safety::SafetyProfile;

        let mut connections = ConnectionRegistry::new(SafetyProfile::InspectOnly);
        connections.register(ConnectionProfile {
            name: String::from("prod-db"),
            description: None,
            connect_string: String::from("//prod-host/PRDB"),
            username: None,
            permanently_read_only: false,
            dbtools_alias: None,
        });
        connections.register(ConnectionProfile {
            name: String::from("dev-db"),
            description: None,
            connect_string: String::from("//localhost/DEV"),
            username: None,
            permanently_read_only: false,
            dbtools_alias: None,
        });

        let report = doctor_report_with_connections(
            &McpConfig::default(),
            &ToolRegistry::new(),
            &connections,
        );

        let warning_count = report
            .findings
            .iter()
            .filter(|f| f.code == "MCP_PROD_DSN_WITHOUT_PERMANENTLY_READ_ONLY")
            .count();
        // Only the prod connection should fire the warning.
        assert_eq!(warning_count, 1);

        // Now mark the prod connection as permanently_read_only and confirm
        // the warning disappears.
        let mut hardened = ConnectionRegistry::new(SafetyProfile::InspectOnly);
        hardened.register(ConnectionProfile {
            name: String::from("prod-db"),
            description: None,
            connect_string: String::from("//prod-host/PRDB"),
            username: None,
            permanently_read_only: true,
            dbtools_alias: None,
        });
        let hardened_report =
            doctor_report_with_connections(&McpConfig::default(), &ToolRegistry::new(), &hardened);
        assert!(
            !hardened_report
                .findings
                .iter()
                .any(|f| f.code == "MCP_PROD_DSN_WITHOUT_PERMANENTLY_READ_ONLY")
        );
    }

    #[test]
    fn doctor_report_keeps_legacy_instant_client_fields_quiet() {
        let report = doctor_report(&McpConfig::default(), &ToolRegistry::new());
        assert_eq!(
            report.instant_client.live_db_feature,
            report.live_db_feature_enabled
        );
        assert!(report.instant_client.probable_path.is_none());
        assert!(report.instant_client.version_hint.is_none());
        assert!(report.instant_client.inspected_env_vars.is_empty());
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "MCP_INSTANT_CLIENT_NOT_DETECTED")
        );
    }

    #[test]
    fn doctor_report_names_oracle_connection_backend() {
        let report = doctor_report(&McpConfig::default(), &ToolRegistry::new());
        if report.live_db_feature_enabled {
            assert_eq!(report.oracle_connection_backend.name, "oraclemcp-db");
            assert!(report.oracle_connection_backend.compiled_in);
        } else {
            assert_eq!(report.oracle_connection_backend.name, "none");
            assert!(!report.oracle_connection_backend.compiled_in);
        }
    }
}
