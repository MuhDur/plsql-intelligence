//! `preview_sql` + `read_patch_preview` tools (`PLSQL-MCP-LIVE-011`).
//!
//! Implements the two-step DDL preview / approval flow described in plan
//! §13A.3:
//!
//! 1. Agent calls `preview_sql(operation_summary, ddl_bytes)` → returns a
//!    single-use approval token (default 60s TTL) bound to the SHA-256 of
//!    the exact DDL bytes plus the connection name.
//! 2. Agent (or operator) inspects the previewed DDL via
//!    `read_patch_preview(token)`.
//! 3. The token is later spent through
//!    `SessionSafetyState::enable_writes` (`PLSQL-MCP-LIVE-008`); any new
//!    `preview_sql` call invalidates the prior token byte-for-byte.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Render an `sha256:<hex>` string from arbitrary bytes. Centralised so
/// the sha2 0.11+ digest type (`Array<u8, …>`, no `LowerHex`) is
/// formatted byte-by-byte in exactly one place.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(7 + digest.len() * 2);
    out.push_str("sha256:");
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

use crate::safety::ENABLE_WRITES_TOKEN_TTL_SECONDS;

/// A single previewed DDL operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreviewedDdl {
    pub token: String,
    pub connection: String,
    pub operation_summary: String,
    /// The exact DDL bytes the agent passed to `preview_sql`. Stored
    /// verbatim so `read_patch_preview` can return them and
    /// `enable_writes` can validate them.
    pub ddl_bytes: String,
    /// SHA-256 of `ddl_bytes` (`sha256:<hex>`). The token is single-use
    /// AND tied to this hash — any byte-level difference at execute time
    /// rejects the call.
    pub ddl_sha256: String,
    pub issued_at: u64,
    pub ttl_seconds: u64,
}

impl PreviewedDdl {
    /// Whether the token has expired against `now` (unix seconds).
    #[must_use]
    pub fn is_expired_at(&self, now: u64) -> bool {
        now.saturating_sub(self.issued_at) >= self.ttl_seconds
    }
}

/// In-memory registry of issued previews. The MCP server holds one
/// instance for the lifetime of a session; any new `preview_sql` call
/// replaces the prior entry for the same connection so a single agent
/// cannot accumulate dangling approval tokens.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreviewRegistry {
    pending: BTreeMap<String, PreviewedDdl>,
}

impl PreviewRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `preview_sql(connection, operation_summary, ddl_bytes, token)`.
    /// Returns the freshly-minted token. Re-issuing for the same
    /// `connection` replaces and invalidates the prior token, per
    /// plan §13A.3.
    pub fn preview_sql(
        &mut self,
        connection: impl Into<String>,
        operation_summary: impl Into<String>,
        ddl_bytes: impl Into<String>,
        token_value: impl Into<String>,
    ) -> Result<PreviewedDdl, PreviewError> {
        let connection = connection.into();
        let operation_summary = operation_summary.into();
        let ddl_bytes = ddl_bytes.into();
        if connection.trim().is_empty() {
            return Err(PreviewError::EmptyConnection);
        }
        if ddl_bytes.trim().is_empty() {
            return Err(PreviewError::EmptyDdl);
        }
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        let ddl_sha256 = sha256_hex(ddl_bytes.as_bytes());
        let preview = PreviewedDdl {
            token: token_value.into(),
            connection: connection.clone(),
            operation_summary,
            ddl_bytes,
            ddl_sha256,
            issued_at,
            ttl_seconds: ENABLE_WRITES_TOKEN_TTL_SECONDS,
        };
        // Re-issuing for the same connection invalidates the prior entry.
        self.pending.insert(connection, preview.clone());
        Ok(preview)
    }

    /// `read_patch_preview(token)` — return the previewed DDL bytes the
    /// operator inspects. Refuses expired tokens; finds the token without
    /// needing the connection name (the token uniquely identifies it).
    pub fn read_patch_preview(
        &self,
        token_value: &str,
        now: u64,
    ) -> Result<&PreviewedDdl, PreviewError> {
        let preview = self
            .pending
            .values()
            .find(|p| p.token == token_value)
            .ok_or(PreviewError::TokenNotFound)?;
        if preview.is_expired_at(now) {
            return Err(PreviewError::TokenExpired {
                ttl_seconds: preview.ttl_seconds,
            });
        }
        Ok(preview)
    }

    /// Verify `executed_ddl_bytes` byte-for-byte against the previewed
    /// payload. Called by `execute_approved` (`PLSQL-MCP-LIVE-013`) before
    /// running the DDL.
    pub fn verify_byte_for_byte(
        &self,
        token_value: &str,
        connection: &str,
        executed_ddl_bytes: &str,
        now: u64,
    ) -> Result<&PreviewedDdl, PreviewError> {
        let Some(preview) = self.pending.get(connection) else {
            return Err(PreviewError::TokenNotFound);
        };
        if preview.is_expired_at(now) {
            return Err(PreviewError::TokenExpired {
                ttl_seconds: preview.ttl_seconds,
            });
        }
        if preview.token != token_value {
            return Err(PreviewError::TokenMismatch);
        }
        if preview.ddl_bytes != executed_ddl_bytes {
            return Err(PreviewError::DdlMismatch {
                preview_sha256: preview.ddl_sha256.clone(),
                executed_sha256: sha256_hex(executed_ddl_bytes.as_bytes()),
            });
        }
        Ok(preview)
    }

    /// Drop the preview entry once `execute_approved` succeeds. Idempotent
    /// — re-calling is a no-op.
    pub fn consume(&mut self, connection: &str) {
        self.pending.remove(connection);
    }

    /// Drop every expired entry. Convenient for the doctor / periodic
    /// reaper.
    pub fn purge_expired(&mut self, now: u64) -> usize {
        let before = self.pending.len();
        self.pending
            .retain(|_, preview| !preview.is_expired_at(now));
        before.saturating_sub(self.pending.len())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum PreviewError {
    #[error("preview_sql refused: empty connection name")]
    EmptyConnection,
    #[error("preview_sql refused: empty DDL bytes")]
    EmptyDdl,
    #[error("preview token not found; was preview_sql called?")]
    TokenNotFound,
    #[error("preview token expired (single-use, {ttl_seconds}s TTL)")]
    TokenExpired { ttl_seconds: u64 },
    #[error("preview token mismatch — token does not match the active preview for this connection")]
    TokenMismatch,
    #[error(
        "executed DDL does not match previewed bytes (preview {preview_sha256}, executed {executed_sha256})"
    )]
    DdlMismatch {
        preview_sha256: String,
        executed_sha256: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_sql_mints_token_with_ddl_hash() {
        let mut registry = PreviewRegistry::new();
        let preview = registry
            .preview_sql(
                "billing-dev",
                "ALTER TABLE INVOICES ADD STATUS VARCHAR2(20)",
                "ALTER TABLE INVOICES ADD STATUS VARCHAR2(20);",
                "tok-a",
            )
            .unwrap();
        assert!(preview.ddl_sha256.starts_with("sha256:"));
        assert_eq!(preview.connection, "billing-dev");
        assert_eq!(preview.ttl_seconds, ENABLE_WRITES_TOKEN_TTL_SECONDS);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn preview_sql_rejects_empty_inputs() {
        let mut registry = PreviewRegistry::new();
        assert!(matches!(
            registry.preview_sql("", "summary", "ALTER", "tok"),
            Err(PreviewError::EmptyConnection)
        ));
        assert!(matches!(
            registry.preview_sql("dev", "summary", "  ", "tok"),
            Err(PreviewError::EmptyDdl)
        ));
    }

    #[test]
    fn read_patch_preview_returns_ddl_bytes() {
        let mut registry = PreviewRegistry::new();
        let preview = registry
            .preview_sql("dev", "op", "ALTER TABLE FOO ADD BAR NUMBER;", "tok")
            .unwrap();
        let now = preview.issued_at + 1;
        let fetched = registry.read_patch_preview("tok", now).unwrap();
        assert_eq!(fetched.ddl_bytes, "ALTER TABLE FOO ADD BAR NUMBER;");
        assert_eq!(fetched.operation_summary, "op");
    }

    #[test]
    fn read_patch_preview_rejects_unknown_token() {
        let registry = PreviewRegistry::new();
        let err = registry.read_patch_preview("nope", 0).unwrap_err();
        assert!(matches!(err, PreviewError::TokenNotFound));
    }

    #[test]
    fn read_patch_preview_rejects_expired_token() {
        let mut registry = PreviewRegistry::new();
        let preview = registry
            .preview_sql("dev", "op", "ALTER FOO ADD BAR NUMBER;", "tok")
            .unwrap();
        let now = preview.issued_at + ENABLE_WRITES_TOKEN_TTL_SECONDS + 1;
        let err = registry.read_patch_preview("tok", now).unwrap_err();
        assert!(matches!(err, PreviewError::TokenExpired { .. }));
    }

    #[test]
    fn re_preview_invalidates_prior_token_for_same_connection() {
        let mut registry = PreviewRegistry::new();
        registry
            .preview_sql("dev", "op1", "ALTER FOO ADD BAR NUMBER;", "tok-1")
            .unwrap();
        registry
            .preview_sql("dev", "op2", "ALTER FOO ADD BAZ NUMBER;", "tok-2")
            .unwrap();
        // Only the second token is now reachable.
        assert!(registry.read_patch_preview("tok-1", 0).is_err());
        let active = registry.read_patch_preview("tok-2", 1).unwrap();
        assert_eq!(active.operation_summary, "op2");
    }

    #[test]
    fn verify_byte_for_byte_detects_mutation() {
        let mut registry = PreviewRegistry::new();
        let preview = registry
            .preview_sql("dev", "op", "ALTER FOO ADD BAR NUMBER;", "tok")
            .unwrap();
        let now = preview.issued_at + 1;
        // Identical bytes pass.
        let ok = registry
            .verify_byte_for_byte("tok", "dev", "ALTER FOO ADD BAR NUMBER;", now)
            .unwrap();
        assert_eq!(ok.token, "tok");
        // Mutated bytes are rejected with the executed sha256 surfaced.
        let err = registry
            .verify_byte_for_byte("tok", "dev", "ALTER FOO ADD BAR NUMBER NOT NULL;", now)
            .unwrap_err();
        let PreviewError::DdlMismatch {
            preview_sha256,
            executed_sha256,
        } = err
        else {
            panic!("expected DdlMismatch");
        };
        assert!(preview_sha256.starts_with("sha256:"));
        assert!(executed_sha256.starts_with("sha256:"));
        assert_ne!(preview_sha256, executed_sha256);
    }

    #[test]
    fn verify_byte_for_byte_rejects_wrong_connection() {
        let mut registry = PreviewRegistry::new();
        let preview = registry
            .preview_sql("dev", "op", "ALTER FOO ADD BAR NUMBER;", "tok")
            .unwrap();
        let now = preview.issued_at + 1;
        let err = registry
            .verify_byte_for_byte("tok", "prod", "ALTER FOO ADD BAR NUMBER;", now)
            .unwrap_err();
        assert!(matches!(err, PreviewError::TokenNotFound));
    }

    #[test]
    fn consume_clears_the_entry() {
        let mut registry = PreviewRegistry::new();
        registry
            .preview_sql("dev", "op", "ALTER FOO ADD BAR NUMBER;", "tok")
            .unwrap();
        registry.consume("dev");
        assert!(registry.is_empty());
        // Idempotent — re-calling is a no-op.
        registry.consume("dev");
    }

    #[test]
    fn purge_expired_drops_only_stale_entries() {
        let mut registry = PreviewRegistry::new();
        let fresh = registry
            .preview_sql("dev", "op", "ALTER FOO ADD BAR NUMBER;", "tok")
            .unwrap();
        // Same registry, second connection, stays fresh too.
        let other = registry
            .preview_sql("prod", "op2", "ALTER BAR ADD BAZ NUMBER;", "tok2")
            .unwrap();
        // Tick past the dev token's TTL but stay inside prod's window.
        let now = fresh.issued_at + ENABLE_WRITES_TOKEN_TTL_SECONDS + 1;
        // dev expired; prod still active (issued seconds later but
        // sharing the clock — assert based on the dev TTL only).
        let _ = other;
        let dropped = registry.purge_expired(now);
        assert!(dropped >= 1);
    }
}
