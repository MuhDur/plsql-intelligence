//! Stateful live-DB runtime for connected MCP sessions.
//!
//! The profile registry in [`crate::connections`] describes possible Oracle
//! connections; this module owns the state that only exists after a session is
//! live: the opened `oraclemcp-db` connection, the per-connection safety state,
//! the active-session lease, and the preview/approval registry.

use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oraclemcp_db::{OracleBackend, OracleConnection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ConnectionProfile, EnableWritesToken, GuardedAudit, PreviewError, PreviewRegistry,
    PreviewedDdl, SafetyProfile, SafetyProfileError, SessionSafetyState,
};

/// Boxed upstream Oracle connection used by the live runtime.
pub type BoxedOracleConnection = Box<dyn OracleConnection>;

/// A generation-stamped lease for the currently active live session.
///
/// The lease is not a security token. It is a cheap stale-session guard for the
/// dispatch layer: if `switch_database` or reconnect replaces the active
/// session while a request is being prepared, the generation no longer matches.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LiveSessionLease {
    /// Connection profile name this lease was minted for.
    pub connection: String,
    /// Monotonic in-process activation generation.
    pub generation: u64,
    /// Unix timestamp used for operator-facing audit output.
    pub issued_at: u64,
}

/// One connected profile: profile metadata, the live upstream connection, and
/// the per-session safety state.
pub struct LiveDbSession {
    profile: ConnectionProfile,
    connection: BoxedOracleConnection,
    safety: SessionSafetyState,
}

impl LiveDbSession {
    fn new(
        profile: ConnectionProfile,
        connection: BoxedOracleConnection,
        safety_profile: SafetyProfile,
    ) -> Self {
        let permanently_read_only = profile.permanently_read_only;
        Self {
            profile,
            connection,
            safety: SessionSafetyState::new(safety_profile, permanently_read_only),
        }
    }

    /// Profile metadata associated with this live connection.
    #[must_use]
    pub fn profile(&self) -> &ConnectionProfile {
        &self.profile
    }

    /// Borrow the live upstream Oracle connection.
    #[must_use]
    pub fn connection(&self) -> &dyn OracleConnection {
        self.connection.as_ref()
    }

    /// Mutably borrow the live upstream Oracle connection.
    #[must_use]
    pub fn connection_mut(&mut self) -> &mut dyn OracleConnection {
        self.connection.as_mut()
    }

    /// The upstream backend that owns the round trips.
    #[must_use]
    pub fn backend(&self) -> OracleBackend {
        self.connection.backend()
    }

    /// Per-connection safety/session state.
    #[must_use]
    pub fn safety(&self) -> &SessionSafetyState {
        &self.safety
    }

    /// Mutable access to the per-connection safety/session state.
    #[must_use]
    pub fn safety_mut(&mut self) -> &mut SessionSafetyState {
        &mut self.safety
    }

    /// Mint the single-use operator confirmation token for this connection.
    pub fn mint_enable_writes_token(
        &mut self,
        operation_summary: impl Into<String>,
        token_value: impl Into<String>,
    ) -> Result<EnableWritesToken, SafetyProfileError> {
        self.safety.mint_token(
            self.profile.name.clone(),
            operation_summary.into(),
            token_value.into(),
        )
    }

    /// Consume an operator confirmation token for this connection.
    pub fn enable_writes(&mut self, token_value: &str, now: u64) -> Result<(), SafetyProfileError> {
        self.safety
            .enable_writes(token_value, &self.profile.name, now)
    }

    /// Disable write privilege for this connection.
    pub fn disable_writes(&mut self) -> Result<(), SafetyProfileError> {
        self.safety.disable_writes()
    }
}

impl fmt::Debug for LiveDbSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveDbSession")
            .field("profile", &self.profile)
            .field("backend", &self.backend())
            .field("safety", &self.safety)
            .finish_non_exhaustive()
    }
}

/// In-process runtime state needed by live-DB MCP tools.
pub struct LiveDbRuntime {
    default_safety_profile: SafetyProfile,
    sessions: BTreeMap<String, LiveDbSession>,
    active: Option<String>,
    active_lease: Option<LiveSessionLease>,
    next_generation: u64,
    previews: PreviewRegistry,
    guarded_audit: Option<GuardedAudit>,
}

impl LiveDbRuntime {
    /// Build an empty runtime. New sessions start in `inspect_only`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            default_safety_profile: SafetyProfile::InspectOnly,
            sessions: BTreeMap::new(),
            active: None,
            active_lease: None,
            next_generation: 0,
            previews: PreviewRegistry::new(),
            guarded_audit: None,
        }
    }

    /// Build an empty runtime with a non-write default safety profile.
    pub fn with_default_safety(
        default_safety_profile: SafetyProfile,
    ) -> Result<Self, LiveRuntimeError> {
        if default_safety_profile.allows_direct_writes() {
            return Err(LiveRuntimeError::DefaultSafetyAllowsWrites);
        }
        Ok(Self {
            default_safety_profile,
            ..Self::new()
        })
    }

    /// Build an empty runtime with a configured guarded-write auditor.
    #[must_use]
    pub fn with_guarded_audit(audit: GuardedAudit) -> Self {
        let mut runtime = Self::new();
        runtime.guarded_audit = Some(audit);
        runtime
    }

    /// Install or replace the guarded-write auditor for this runtime.
    pub fn install_guarded_audit(&mut self, audit: GuardedAudit) {
        self.guarded_audit = Some(audit);
    }

    /// Configured guarded-write auditor, if any.
    #[must_use]
    pub fn guarded_audit(&self) -> Option<&GuardedAudit> {
        self.guarded_audit.as_ref()
    }

    /// Default safety profile used for newly inserted sessions.
    #[must_use]
    pub fn default_safety_profile(&self) -> SafetyProfile {
        self.default_safety_profile
    }

    /// Insert a live upstream connection for `profile`.
    ///
    /// Reusing a profile name replaces the previous live session. If that name
    /// was active, the active lease is invalidated and the caller must activate
    /// the new session explicitly.
    pub fn insert_connected(
        &mut self,
        profile: ConnectionProfile,
        connection: BoxedOracleConnection,
    ) -> Result<Option<LiveDbSession>, LiveRuntimeError> {
        if profile.name.trim().is_empty() {
            return Err(LiveRuntimeError::EmptyProfileName);
        }
        let name = profile.name.clone();
        let session = LiveDbSession::new(profile, connection, self.default_safety_profile);
        let previous = self.sessions.insert(name.clone(), session);
        if self.active.as_deref() == Some(name.as_str()) {
            self.active = None;
            self.active_lease = None;
            self.next_generation = self.next_generation.saturating_add(1);
        }
        Ok(previous)
    }

    /// Insert and activate a live upstream connection in one step.
    pub fn insert_and_activate(
        &mut self,
        profile: ConnectionProfile,
        connection: BoxedOracleConnection,
    ) -> Result<LiveSessionLease, LiveRuntimeError> {
        let name = profile.name.clone();
        self.insert_connected(profile, connection)?;
        self.activate(&name)
    }

    /// Activate an already-connected profile and mint a fresh lease.
    pub fn activate(&mut self, name: &str) -> Result<LiveSessionLease, LiveRuntimeError> {
        if !self.sessions.contains_key(name) {
            return Err(LiveRuntimeError::UnknownConnection {
                name: String::from(name),
            });
        }
        self.next_generation = self.next_generation.saturating_add(1);
        let lease = LiveSessionLease {
            connection: String::from(name),
            generation: self.next_generation,
            issued_at: unix_now_seconds(),
        };
        self.active = Some(String::from(name));
        self.active_lease = Some(lease.clone());
        Ok(lease)
    }

    /// Remove a connected profile, returning its live session.
    pub fn remove_connection(&mut self, name: &str) -> Result<LiveDbSession, LiveRuntimeError> {
        let removed =
            self.sessions
                .remove(name)
                .ok_or_else(|| LiveRuntimeError::UnknownConnection {
                    name: String::from(name),
                })?;
        if self.active.as_deref() == Some(name) {
            self.active = None;
            self.active_lease = None;
            self.next_generation = self.next_generation.saturating_add(1);
            self.previews.consume(name);
        }
        Ok(removed)
    }

    /// Remove the active connection.
    pub fn remove_active(&mut self) -> Result<LiveDbSession, LiveRuntimeError> {
        let name = self
            .active
            .clone()
            .ok_or(LiveRuntimeError::NoActiveConnection)?;
        self.remove_connection(&name)
    }

    /// Clear the active pointer without closing any connected sessions.
    pub fn clear_active(&mut self) -> Option<LiveSessionLease> {
        self.active = None;
        self.active_lease.take()
    }

    /// Active connection name, if any.
    #[must_use]
    pub fn active_name(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// Current active-session lease, if any.
    #[must_use]
    pub fn active_lease(&self) -> Option<&LiveSessionLease> {
        self.active_lease.as_ref()
    }

    /// Borrow the active live session.
    pub fn active_session(&self) -> Result<&LiveDbSession, LiveRuntimeError> {
        let name = self
            .active
            .as_deref()
            .ok_or(LiveRuntimeError::NoActiveConnection)?;
        self.session(name)
    }

    /// Mutably borrow the active live session.
    pub fn active_session_mut(&mut self) -> Result<&mut LiveDbSession, LiveRuntimeError> {
        let name = self
            .active
            .clone()
            .ok_or(LiveRuntimeError::NoActiveConnection)?;
        self.session_mut(&name)
    }

    /// Borrow a connected session by profile name.
    pub fn session(&self, name: &str) -> Result<&LiveDbSession, LiveRuntimeError> {
        self.sessions
            .get(name)
            .ok_or_else(|| LiveRuntimeError::UnknownConnection {
                name: String::from(name),
            })
    }

    /// Mutably borrow a connected session by profile name.
    pub fn session_mut(&mut self, name: &str) -> Result<&mut LiveDbSession, LiveRuntimeError> {
        self.sessions
            .get_mut(name)
            .ok_or_else(|| LiveRuntimeError::UnknownConnection {
                name: String::from(name),
            })
    }

    /// Borrow the active session only if `lease` still matches the active
    /// activation generation.
    pub fn session_for_lease(
        &self,
        lease: &LiveSessionLease,
    ) -> Result<&LiveDbSession, LiveRuntimeError> {
        self.validate_lease(lease)?;
        self.session(&lease.connection)
    }

    /// Mutably borrow the active session only if `lease` is still current.
    pub fn session_for_lease_mut(
        &mut self,
        lease: &LiveSessionLease,
    ) -> Result<&mut LiveDbSession, LiveRuntimeError> {
        self.validate_lease(lease)?;
        self.session_mut(&lease.connection)
    }

    /// Set the active session's non-write safety profile.
    ///
    /// `session_write_enabled` can only be reached by consuming an
    /// `enable_writes` token; this helper refuses to bypass that flow.
    pub fn set_active_safety_profile(
        &mut self,
        profile: SafetyProfile,
    ) -> Result<(), LiveRuntimeError> {
        if profile.allows_direct_writes() {
            return Err(LiveRuntimeError::EnableWritesRequired);
        }
        let session = self.active_session_mut()?;
        session.safety.profile = profile;
        session.safety.session_writes_enabled = false;
        session.safety.active_token = None;
        Ok(())
    }

    /// Preview DDL against the active connection and store it in the session
    /// preview registry.
    pub fn preview_active_sql(
        &mut self,
        operation_summary: impl Into<String>,
        ddl_bytes: impl Into<String>,
        token_value: impl Into<String>,
    ) -> Result<PreviewedDdl, LiveRuntimeError> {
        let session = self.active_session()?;
        let profile = session.safety().profile;
        if !profile.allows_ddl_preview() {
            return Err(LiveRuntimeError::PreviewNotAllowed { profile });
        }
        let connection = session.profile().name.clone();
        Ok(self
            .previews
            .preview_sql(connection, operation_summary, ddl_bytes, token_value)?)
    }

    /// Borrow the preview/approval registry.
    #[must_use]
    pub fn preview_registry(&self) -> &PreviewRegistry {
        &self.previews
    }

    /// Mutably borrow the preview/approval registry.
    #[must_use]
    pub fn preview_registry_mut(&mut self) -> &mut PreviewRegistry {
        &mut self.previews
    }

    /// Iterate connected profile names in stable order.
    pub fn connected_names(&self) -> impl Iterator<Item = &str> {
        self.sessions.keys().map(String::as_str)
    }

    /// Number of connected live sessions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether no live sessions are connected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    fn validate_lease(&self, lease: &LiveSessionLease) -> Result<(), LiveRuntimeError> {
        if let Some(active) = self.active_lease.as_ref() {
            let same_connection = active
                .connection
                .as_str()
                .cmp(lease.connection.as_str())
                .is_eq();
            let same_generation = active.generation.cmp(&lease.generation).is_eq();
            if same_connection && same_generation {
                return Ok(());
            }
        }
        Err(LiveRuntimeError::StaleLease {
            connection: lease.connection.clone(),
            generation: lease.generation,
        })
    }
}

impl Default for LiveDbRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for LiveDbRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveDbRuntime")
            .field("default_safety_profile", &self.default_safety_profile)
            .field(
                "connected_names",
                &self.connected_names().collect::<Vec<_>>(),
            )
            .field("active", &self.active)
            .field("active_lease", &self.active_lease)
            .field("preview_count", &self.previews.len())
            .field("guarded_audit_configured", &self.guarded_audit.is_some())
            .finish_non_exhaustive()
    }
}

/// Runtime errors surfaced before a live-DB tool can safely execute.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum LiveRuntimeError {
    /// A connection profile had no usable stable name.
    #[error("live-db runtime refused a connection profile with an empty name")]
    EmptyProfileName,
    /// The runtime default would start a session in a write-enabled posture.
    #[error("live-db runtime default safety profile cannot be session_write_enabled")]
    DefaultSafetyAllowsWrites,
    /// No connected profile with this name exists.
    #[error("no live Oracle connection named `{name}`; call connect first")]
    UnknownConnection { name: String },
    /// No active live connection exists.
    #[error("no active live Oracle connection; call connect first")]
    NoActiveConnection,
    /// A previously-minted active-session lease no longer matches runtime
    /// state.
    #[error("stale live-session lease for `{connection}` generation {generation}")]
    StaleLease { connection: String, generation: u64 },
    /// A direct safety profile transition would bypass the operator token.
    #[error("session_write_enabled requires enable_writes; refusing direct safety transition")]
    EnableWritesRequired,
    /// The active safety profile does not expose DDL preview tools.
    #[error("active safety profile {profile:?} does not allow DDL preview")]
    PreviewNotAllowed { profile: SafetyProfile },
    /// Safety-state transition failed.
    #[error(transparent)]
    Safety(#[from] SafetyProfileError),
    /// Preview/approval registry operation failed.
    #[error(transparent)]
    Preview(#[from] PreviewError),
}

fn unix_now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::Cx;
    use async_trait::async_trait;
    use oraclemcp_db::{DbError, OracleBind, OracleConnectionInfo, OracleRow};

    struct StubOracleConnection {
        backend: OracleBackend,
    }

    impl StubOracleConnection {
        fn boxed() -> BoxedOracleConnection {
            Box::new(Self {
                backend: OracleBackend::RustOracle,
            })
        }
    }

    #[async_trait(?Send)]
    impl OracleConnection for StubOracleConnection {
        fn backend(&self) -> OracleBackend {
            self.backend
        }

        async fn ping(&self, _cx: &Cx) -> Result<(), DbError> {
            Ok(())
        }

        async fn describe(&self, _cx: &Cx) -> Result<OracleConnectionInfo, DbError> {
            Ok(OracleConnectionInfo {
                backend: Some(self.backend),
                ..OracleConnectionInfo::default()
            })
        }

        async fn query_rows(
            &self,
            _cx: &Cx,
            _sql: &str,
            _binds: &[OracleBind],
        ) -> Result<Vec<OracleRow>, DbError> {
            Ok(Vec::new())
        }

        async fn execute(
            &self,
            _cx: &Cx,
            _sql: &str,
            _binds: &[OracleBind],
        ) -> Result<u64, DbError> {
            Ok(0)
        }

        async fn commit(&self, _cx: &Cx) -> Result<(), DbError> {
            Ok(())
        }

        async fn rollback(&self, _cx: &Cx) -> Result<(), DbError> {
            Ok(())
        }
    }

    fn profile(name: &str, permanently_read_only: bool) -> ConnectionProfile {
        ConnectionProfile {
            name: String::from(name),
            description: Some(format!("{name} test profile")),
            connect_string: String::from("//localhost/FREEPDB1"),
            username: Some(String::from("scott")),
            permanently_read_only,
            dbtools_alias: None,
        }
    }

    #[test]
    fn runtime_holds_stub_connection_and_session_state() {
        let mut runtime = LiveDbRuntime::new();
        let lease = runtime
            .insert_and_activate(profile("billing-dev", false), StubOracleConnection::boxed())
            .unwrap();

        assert_eq!(runtime.len(), 1);
        assert_eq!(runtime.active_name(), Some("billing-dev"));
        assert_eq!(runtime.active_lease(), Some(&lease));

        let session = runtime.active_session().unwrap();
        assert_eq!(session.profile().name, "billing-dev");
        assert_eq!(session.backend(), OracleBackend::RustOracle);
        assert_eq!(session.safety().profile, SafetyProfile::InspectOnly);
        assert!(!session.safety().session_writes_enabled);
        assert!(!session.safety().writes_allowed());
    }

    #[test]
    fn permanently_read_only_profile_blocks_operator_token_mint() {
        let mut runtime = LiveDbRuntime::new();
        runtime
            .insert_and_activate(profile("prod-db", true), StubOracleConnection::boxed())
            .unwrap();

        let err = runtime
            .active_session_mut()
            .unwrap()
            .mint_enable_writes_token("ALTER TABLE T ADD C NUMBER", "tok-prod")
            .unwrap_err();
        assert!(matches!(
            err,
            SafetyProfileError::PermanentlyReadOnly { .. }
        ));
    }

    #[test]
    fn direct_transition_to_write_enabled_is_refused() {
        let mut runtime = LiveDbRuntime::new();
        runtime
            .insert_and_activate(profile("dev", false), StubOracleConnection::boxed())
            .unwrap();
        let err = runtime
            .set_active_safety_profile(SafetyProfile::SessionWriteEnabled)
            .unwrap_err();
        assert_eq!(err, LiveRuntimeError::EnableWritesRequired);
    }

    #[test]
    fn preview_registry_is_active_session_scoped_and_profile_gated() {
        let mut runtime = LiveDbRuntime::new();
        runtime
            .insert_and_activate(profile("dev", false), StubOracleConnection::boxed())
            .unwrap();

        let denied = runtime
            .preview_active_sql("op", "ALTER TABLE FOO ADD BAR NUMBER;", "tok-denied")
            .unwrap_err();
        assert_eq!(
            denied,
            LiveRuntimeError::PreviewNotAllowed {
                profile: SafetyProfile::InspectOnly
            }
        );

        runtime
            .set_active_safety_profile(SafetyProfile::DdlGuarded)
            .unwrap();
        let preview = runtime
            .preview_active_sql("op", "ALTER TABLE FOO ADD BAR NUMBER;", "tok-a")
            .unwrap();
        let now = preview.issued_at + 1;
        assert_eq!(preview.connection, "dev");
        assert_eq!(
            runtime
                .preview_registry()
                .read_patch_preview("tok-a", now)
                .unwrap()
                .ddl_bytes,
            "ALTER TABLE FOO ADD BAR NUMBER;"
        );
    }

    #[test]
    fn activation_lease_detects_stale_session_after_switch() {
        let mut runtime = LiveDbRuntime::new();
        let first = runtime
            .insert_and_activate(profile("alpha", false), StubOracleConnection::boxed())
            .unwrap();
        runtime
            .insert_connected(profile("beta", false), StubOracleConnection::boxed())
            .unwrap();
        let second = runtime.activate("beta").unwrap();

        assert_ne!(first, second);
        let err = runtime.session_for_lease(&first).unwrap_err();
        assert_eq!(
            err,
            LiveRuntimeError::StaleLease {
                connection: String::from("alpha"),
                generation: first.generation
            }
        );
        assert_eq!(
            runtime.session_for_lease(&second).unwrap().profile().name,
            "beta"
        );
    }

    #[test]
    fn replacing_active_session_invalidates_lease_until_reactivated() {
        let mut runtime = LiveDbRuntime::new();
        let first = runtime
            .insert_and_activate(profile("dev", false), StubOracleConnection::boxed())
            .unwrap();
        let replaced = runtime
            .insert_connected(profile("dev", false), StubOracleConnection::boxed())
            .unwrap();

        assert!(replaced.is_some());
        assert_eq!(runtime.active_name(), None);
        assert!(runtime.active_lease().is_none());
        assert_eq!(
            runtime.session_for_lease(&first).unwrap_err(),
            LiveRuntimeError::StaleLease {
                connection: String::from("dev"),
                generation: first.generation
            }
        );
    }

    #[test]
    fn removing_active_session_consumes_matching_preview() {
        let mut runtime = LiveDbRuntime::with_default_safety(SafetyProfile::DdlGuarded).unwrap();
        runtime
            .insert_and_activate(profile("dev", false), StubOracleConnection::boxed())
            .unwrap();
        runtime
            .preview_active_sql("op", "ALTER TABLE FOO ADD BAR NUMBER;", "tok")
            .unwrap();
        assert_eq!(runtime.preview_registry().len(), 1);

        let removed = runtime.remove_active().unwrap();
        assert_eq!(removed.profile().name, "dev");
        assert!(runtime.preview_registry().is_empty());
        assert_eq!(runtime.active_name(), None);
    }

    #[test]
    fn default_safety_never_starts_sessions_write_enabled() {
        let err =
            LiveDbRuntime::with_default_safety(SafetyProfile::SessionWriteEnabled).unwrap_err();
        assert_eq!(err, LiveRuntimeError::DefaultSafetyAllowsWrites);
    }
}
