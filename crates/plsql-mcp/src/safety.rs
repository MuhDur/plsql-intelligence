//! Named safety profiles for the live-DB tool surface (§13A.3).
//!
//! introduced the [`SafetyProfile`] enum.
//! adds the session-state surface that wraps it:
//! [`SessionSafetyState`] tracks the active profile, the read-only-by-default
//! posture, the active `enable_writes` token (single-use, time-limited per
//! plan §13A.3), and the `permanently_read_only` hard guard.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oraclemcp_guard::MonotonicDeadline;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Named safety profile governing which live-DB tools are reachable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyProfile {
    /// No live-DB tools available; static-analysis surface only.
    StaticOnly,
    /// Default when `live-db` is enabled; read-only tools only.
    #[default]
    InspectOnly,
    /// Preview + approval flows are available; direct writes are blocked.
    DdlGuarded,
    /// Temporary post-operator-confirmation state. Reverts to
    /// [`InspectOnly`](Self::InspectOnly) on session end.
    SessionWriteEnabled,
}

impl SafetyProfile {
    /// Returns the stable kebab-case name used in CLI flags + config files.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StaticOnly => "static_only",
            Self::InspectOnly => "inspect_only",
            Self::DdlGuarded => "ddl_guarded",
            Self::SessionWriteEnabled => "session_write_enabled",
        }
    }

    /// Whether read-only inspection tools (e.g. `describe_table`) are
    /// allowed under this profile.
    #[must_use]
    pub fn allows_read_only_live_tools(self) -> bool {
        !matches!(self, Self::StaticOnly)
    }

    /// Whether DDL preview tools (`preview_sql`, `read_patch_preview`) are
    /// allowed under this profile.
    #[must_use]
    pub fn allows_ddl_preview(self) -> bool {
        matches!(self, Self::DdlGuarded | Self::SessionWriteEnabled)
    }

    /// Whether `execute_approved` and direct DML/DDL writes are allowed.
    /// `SessionWriteEnabled` is the only profile that returns `true`; the
    /// `permanently_read_only` per-connection flag overrides this regardless
    /// of profile.
    #[must_use]
    pub fn allows_direct_writes(self) -> bool {
        matches!(self, Self::SessionWriteEnabled)
    }
}

/// Errors raised when parsing or transitioning safety profiles.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum SafetyProfileError {
    #[error(
        "unknown safety profile `{name}`; expected one of: static_only, inspect_only, ddl_guarded, session_write_enabled"
    )]
    Unknown { name: String },
    #[error("connection is permanently_read_only; cannot transition to {requested}")]
    PermanentlyReadOnly { requested: &'static str },
    #[error(
        "enable_writes refused: operator confirmation token missing or expired (single-use, {ttl_seconds}s TTL)"
    )]
    EnableWritesTokenMissing { ttl_seconds: u64 },
    #[error("enable_writes refused: operator confirmation token mismatch")]
    EnableWritesTokenMismatch,
    #[error("disable_writes called but session was already read-only")]
    AlreadyReadOnly,
}

/// Default time-to-live for the `enable_writes` operator-confirmation token
/// (§13A.3 — destructive operations never accidental, never invisible).
pub const ENABLE_WRITES_TOKEN_TTL_SECONDS: u64 = 60;

/// Session-level safety state. Wraps the active [`SafetyProfile`] with the
/// read-only-by-default session toggle and the single-use, time-limited
/// `enable_writes` confirmation token described in plan §13A.3.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionSafetyState {
    pub profile: SafetyProfile,
    pub session_writes_enabled: bool,
    pub permanently_read_only: bool,
    pub active_token: Option<EnableWritesToken>,
}

/// Single-use, time-limited token issued by `preview_writes` and consumed by
/// `enable_writes`. Plan §13A.3 keeps the token tied to a specific connection
/// and a specific operation summary — re-issuing invalidates prior tokens.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnableWritesToken {
    /// Opaque token string the operator inspects. Format is implementation-
    /// defined; the reference implementation expects a URL-safe,
    /// random-looking string supplied by the caller.
    pub token: String,
    /// Connection profile the token authorizes writes against.
    pub connection: String,
    /// Operation summary the operator approved (mirrors the audit trail).
    pub operation_summary: String,
    /// Unix timestamp (seconds) when the token was issued. **Display / audit
    /// only** — never the authoritative expiry (a backward clock would make it
    /// lie). See [`EnableWritesToken::is_expired_at`].
    pub issued_at: u64,
    /// Token TTL in seconds (display / audit).
    pub ttl_seconds: u64,
    /// The authoritative, monotonic expiry deadline (plan §5.10, P0-CLK).
    /// Anchored at mint on [`MonotonicDeadline`] so a backward wall-clock jump
    /// cannot extend the window. Deliberately **not serialized**: a token
    /// reconstructed from a serialized snapshot (or a prior process
    /// generation) has `None` here and is therefore treated as expired
    /// (fail-closed) — the authoritative state lives only in the live
    /// in-process session.
    #[serde(skip)]
    pub deadline: Option<MonotonicDeadline>,
}

impl EnableWritesToken {
    /// Whether the token has expired. **Fail-closed and monotonic:** the
    /// monotonic deadline is authoritative (a backward clock cannot revive an
    /// expired token, and a deserialized token with no live deadline is
    /// expired); the wall-clock `now` check is retained as forward-only
    /// defense-in-depth so the two only ever agree to *expire*, never to
    /// extend.
    #[must_use]
    pub fn is_expired_at(&self, now: u64) -> bool {
        self.is_expired_monotonic() || now.saturating_sub(self.issued_at) >= self.ttl_seconds
    }

    /// The authoritative monotonic expiry: expired if the live deadline has
    /// passed, or if there is no live deadline (deserialized / prior-generation
    /// token — fail-closed).
    #[must_use]
    pub fn is_expired_monotonic(&self) -> bool {
        self.deadline.is_none_or(|d| d.is_expired())
    }

    /// Whether the token is expired against the (monotonic) clock.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        self.is_expired_at(now)
    }
}

impl SessionSafetyState {
    /// Build a fresh session state with an explicit profile and the
    /// `permanently_read_only` flag carried from the connection.
    #[must_use]
    pub fn new(profile: SafetyProfile, permanently_read_only: bool) -> Self {
        Self {
            profile,
            session_writes_enabled: false,
            permanently_read_only,
            active_token: None,
        }
    }

    /// Whether the current state authorizes a write tool. The
    /// `permanently_read_only` flag is the hardest of the guards — it
    /// overrides every other state.
    #[must_use]
    pub fn writes_allowed(&self) -> bool {
        if self.permanently_read_only {
            return false;
        }
        self.session_writes_enabled && self.profile.allows_direct_writes()
    }

    /// Mint a `EnableWritesToken` for `operation_summary`. Re-issuing
    /// invalidates any prior token. Plan §13A.3 keeps token issuance
    /// behind operator review (the agent never approves itself).
    pub fn mint_token(
        &mut self,
        connection: impl Into<String>,
        operation_summary: impl Into<String>,
        token_value: impl Into<String>,
    ) -> Result<EnableWritesToken, SafetyProfileError> {
        if self.permanently_read_only {
            return Err(SafetyProfileError::PermanentlyReadOnly {
                requested: "enable_writes",
            });
        }
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        let token = EnableWritesToken {
            token: token_value.into(),
            connection: connection.into(),
            operation_summary: operation_summary.into(),
            issued_at,
            ttl_seconds: ENABLE_WRITES_TOKEN_TTL_SECONDS,
            deadline: Some(MonotonicDeadline::after(Duration::from_secs(
                ENABLE_WRITES_TOKEN_TTL_SECONDS,
            ))),
        };
        self.active_token = Some(token.clone());
        Ok(token)
    }

    /// Consume a token by exact match to flip the session into the
    /// `SessionWriteEnabled` profile. Per §13A.3 the token is single-use:
    /// success clears `active_token`; failure leaves it intact so a
    /// retry with the right token is still possible.
    pub fn enable_writes(
        &mut self,
        token_value: &str,
        connection: &str,
        now: u64,
    ) -> Result<(), SafetyProfileError> {
        if self.permanently_read_only {
            return Err(SafetyProfileError::PermanentlyReadOnly {
                requested: "enable_writes",
            });
        }
        let Some(token) = self.active_token.as_ref() else {
            return Err(SafetyProfileError::EnableWritesTokenMissing {
                ttl_seconds: ENABLE_WRITES_TOKEN_TTL_SECONDS,
            });
        };
        if token.is_expired_at(now) {
            self.active_token = None;
            return Err(SafetyProfileError::EnableWritesTokenMissing {
                ttl_seconds: ENABLE_WRITES_TOKEN_TTL_SECONDS,
            });
        }
        if token.token != token_value || token.connection != connection {
            return Err(SafetyProfileError::EnableWritesTokenMismatch);
        }
        self.active_token = None;
        self.session_writes_enabled = true;
        self.profile = SafetyProfile::SessionWriteEnabled;
        Ok(())
    }

    /// Drop the write privilege. Returns `AlreadyReadOnly` when the session
    /// was already read-only so the caller can render an idempotent ack.
    pub fn disable_writes(&mut self) -> Result<(), SafetyProfileError> {
        if !self.session_writes_enabled {
            return Err(SafetyProfileError::AlreadyReadOnly);
        }
        self.session_writes_enabled = false;
        self.active_token = None;
        // Revert to the inspect_only default; concrete profile selection on
        // the next preview_writes round can promote it back to ddl_guarded.
        self.profile = SafetyProfile::InspectOnly;
        Ok(())
    }
}

impl std::str::FromStr for SafetyProfile {
    type Err = SafetyProfileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "static_only" => Ok(Self::StaticOnly),
            "inspect_only" => Ok(Self::InspectOnly),
            "ddl_guarded" => Ok(Self::DdlGuarded),
            "session_write_enabled" => Ok(Self::SessionWriteEnabled),
            other => Err(SafetyProfileError::Unknown {
                name: String::from(other),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_inspect_only() {
        assert_eq!(SafetyProfile::default(), SafetyProfile::InspectOnly);
    }

    #[test]
    fn parse_round_trips_through_as_str() {
        for profile in [
            SafetyProfile::StaticOnly,
            SafetyProfile::InspectOnly,
            SafetyProfile::DdlGuarded,
            SafetyProfile::SessionWriteEnabled,
        ] {
            let name = profile.as_str();
            let parsed: SafetyProfile = name.parse().expect("parse");
            assert_eq!(parsed, profile);
        }
    }

    #[test]
    fn unknown_profile_errors() {
        let err: Result<SafetyProfile, _> = "wide_open".parse();
        assert!(matches!(err, Err(SafetyProfileError::Unknown { .. })));
    }

    #[test]
    fn write_capability_table_matches_spec() {
        assert!(!SafetyProfile::StaticOnly.allows_read_only_live_tools());
        assert!(SafetyProfile::InspectOnly.allows_read_only_live_tools());
        assert!(!SafetyProfile::InspectOnly.allows_ddl_preview());
        assert!(SafetyProfile::DdlGuarded.allows_ddl_preview());
        assert!(!SafetyProfile::DdlGuarded.allows_direct_writes());
        assert!(SafetyProfile::SessionWriteEnabled.allows_direct_writes());
    }

    #[test]
    fn session_state_defaults_to_read_only_inspect_only() {
        let state = SessionSafetyState::default();
        assert_eq!(state.profile, SafetyProfile::InspectOnly);
        assert!(!state.session_writes_enabled);
        assert!(!state.permanently_read_only);
        assert!(state.active_token.is_none());
        assert!(!state.writes_allowed());
    }

    #[test]
    fn enable_writes_consumes_valid_token_and_flips_profile() {
        let mut state = SessionSafetyState::new(SafetyProfile::DdlGuarded, false);
        let token = state
            .mint_token(
                "billing-dev",
                "ALTER TABLE INVOICES ADD STATUS VARCHAR2(20)",
                "tok-abc",
            )
            .unwrap();
        let now = token.issued_at + 1;
        state.enable_writes("tok-abc", "billing-dev", now).unwrap();
        assert!(state.session_writes_enabled);
        assert_eq!(state.profile, SafetyProfile::SessionWriteEnabled);
        assert!(state.writes_allowed());
        // Token is single-use — re-call should fail.
        let retry = state.enable_writes("tok-abc", "billing-dev", now);
        assert!(matches!(
            retry,
            Err(SafetyProfileError::EnableWritesTokenMissing { .. })
        ));
    }

    #[test]
    fn enable_writes_rejects_expired_token() {
        let mut state = SessionSafetyState::new(SafetyProfile::DdlGuarded, false);
        let token = state
            .mint_token("billing-dev", "destructive op", "tok-expired")
            .unwrap();
        let now = token.issued_at + ENABLE_WRITES_TOKEN_TTL_SECONDS + 1;
        let result = state.enable_writes("tok-expired", "billing-dev", now);
        assert!(matches!(
            result,
            Err(SafetyProfileError::EnableWritesTokenMissing { .. })
        ));
        // Expired token is cleared so the agent must mint a fresh one.
        assert!(state.active_token.is_none());
        assert!(!state.session_writes_enabled);
    }

    #[test]
    fn enable_writes_refused_for_permanently_read_only() {
        let mut state = SessionSafetyState::new(SafetyProfile::DdlGuarded, true);
        let mint = state.mint_token("prod-db", "anything", "tok");
        assert!(matches!(
            mint,
            Err(SafetyProfileError::PermanentlyReadOnly { .. })
        ));
        assert!(state.active_token.is_none());

        // Even if a token had been pre-injected (shouldn't be possible via
        // the API, but defense-in-depth), enable_writes itself rejects.
        state.active_token = Some(EnableWritesToken {
            token: String::from("sneaky"),
            connection: String::from("prod-db"),
            operation_summary: String::from("any"),
            issued_at: 0,
            ttl_seconds: ENABLE_WRITES_TOKEN_TTL_SECONDS,
            deadline: Some(MonotonicDeadline::after(Duration::from_secs(
                ENABLE_WRITES_TOKEN_TTL_SECONDS,
            ))),
        });
        let result = state.enable_writes("sneaky", "prod-db", 1);
        assert!(matches!(
            result,
            Err(SafetyProfileError::PermanentlyReadOnly { .. })
        ));
    }

    #[test]
    fn enable_writes_rejects_token_or_connection_mismatch() {
        let mut state = SessionSafetyState::new(SafetyProfile::DdlGuarded, false);
        let token = state.mint_token("billing-dev", "op", "tok-a").unwrap();
        let now = token.issued_at + 1;
        // Wrong token text.
        let result = state.enable_writes("tok-b", "billing-dev", now);
        assert!(matches!(
            result,
            Err(SafetyProfileError::EnableWritesTokenMismatch)
        ));
        // Wrong connection.
        let result = state.enable_writes("tok-a", "other-db", now);
        assert!(matches!(
            result,
            Err(SafetyProfileError::EnableWritesTokenMismatch)
        ));
        // Token is still active for the right caller (the mismatch path
        // does not consume the token).
        state.enable_writes("tok-a", "billing-dev", now).unwrap();
        assert!(state.writes_allowed());
    }

    #[test]
    fn backward_clock_jump_cannot_revive_an_expired_token() {
        // P0-CLK (§5.10): the live fail-open. A monotonically-expired token
        // whose wall clock reads *fresh* (a backward NTP/VM jump:
        // now < issued_at, so the old saturating_sub clamps to 0) must still
        // be expired. The monotonic deadline is authoritative.
        let mut state = SessionSafetyState::new(SafetyProfile::DdlGuarded, false);
        state
            .mint_token("billing-dev", "op", "tok-clk")
            .expect("mint");
        // Simulate the monotonic window having elapsed (deadline in the past)
        // while the wall clock jumped backward to before issue time.
        let issued_at = state.active_token.as_ref().unwrap().issued_at;
        state.active_token.as_mut().unwrap().deadline =
            Some(MonotonicDeadline::after(Duration::from_secs(0)));
        let wall_now_after_backward_jump = issued_at.saturating_sub(30);
        // Wall clock alone would read this as fresh (saturating_sub -> 0):
        assert_eq!(wall_now_after_backward_jump.saturating_sub(issued_at), 0);
        // …but enable_writes must refuse it (monotonic deadline expired).
        let result = state.enable_writes("tok-clk", "billing-dev", wall_now_after_backward_jump);
        assert!(matches!(
            result,
            Err(SafetyProfileError::EnableWritesTokenMissing { .. })
        ));
        assert!(state.active_token.is_none());
        assert!(!state.session_writes_enabled);
    }

    #[test]
    fn deserialized_token_without_live_deadline_is_expired() {
        // A token round-tripped through serde has no live monotonic deadline
        // (the field is #[serde(skip)]), so it is fail-closed expired — a prior
        // process generation's window can never be replayed.
        let token = EnableWritesToken {
            token: String::from("tok"),
            connection: String::from("dev"),
            operation_summary: String::from("op"),
            issued_at: 0,
            ttl_seconds: ENABLE_WRITES_TOKEN_TTL_SECONDS,
            deadline: None,
        };
        let json = serde_json::to_string(&token).expect("serialize");
        let restored: EnableWritesToken = serde_json::from_str(&json).expect("deserialize");
        assert!(restored.deadline.is_none());
        assert!(restored.is_expired_monotonic());
        // Even with a wall `now` that looks fresh (now == issued_at), expired.
        assert!(restored.is_expired_at(0));
    }

    #[test]
    fn disable_writes_is_idempotent_for_read_only_sessions() {
        let mut state = SessionSafetyState::default();
        let err = state.disable_writes().unwrap_err();
        assert!(matches!(err, SafetyProfileError::AlreadyReadOnly));
    }

    #[test]
    fn disable_writes_reverts_profile_to_inspect_only() {
        let mut state = SessionSafetyState::new(SafetyProfile::DdlGuarded, false);
        let token = state.mint_token("billing-dev", "op", "tok").unwrap();
        state
            .enable_writes("tok", "billing-dev", token.issued_at + 1)
            .unwrap();
        state.disable_writes().unwrap();
        assert!(!state.session_writes_enabled);
        assert_eq!(state.profile, SafetyProfile::InspectOnly);
        assert!(state.active_token.is_none());
    }
}
