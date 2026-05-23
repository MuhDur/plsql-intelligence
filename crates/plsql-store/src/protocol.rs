//! Local-daemon query protocol.
//!
//! This module is the **protocol definition only** — the wire
//! contract a future `plsqld` serves and
//! that CLIs/MCP use to query a warm cache instead of re-running
//! analysis. No socket, no server loop, no client runtime lives
//! here: just the versioned message types, their semantics, the
//! framing, and the error model, with round-trip tests.
//!
//! ## Transport & framing
//!
//! The daemon is **strictly local** (R17 — no network
//! telemetry): a Unix-domain socket (or Windows named pipe)
//! under an *explicitly configured* cache directory, never a TCP
//! port. The framing is **newline-delimited JSON**: exactly one
//! [`DaemonEnvelope`] JSON object per line, UTF-8, `\n`
//! terminated. [`encode`] / [`decode_line`] are the canonical
//! codec.
//!
//! ## Semantics
//!
//! Every request is a **pure query** — the daemon never mutates
//! the cache in response to one, so requests are idempotent and
//! safe to retry. A request that names a missing artifact is
//! **not** an error: the response carries `found: None` (R13 —
//! "absent" is a first-class answer, distinct from a failure).
//! Protocol/parse failures use [`DaemonError`] with a typed
//! [`DaemonErrorCode`].
//!
//! ## Versioning
//!
//! [`DaemonEnvelope::protocol_version`] carries
//! [`PROTOCOL_VERSION`]. A server MUST reject an envelope whose
//! major differs (incompatible wire shape) and MAY accept a
//! lower/equal minor (additive evolution) — same policy as the
//! engine's `schema_compatibility`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// `major.minor.patch` of the daemon wire protocol. Bump major
/// on any breaking message-shape change.
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion {
    major: 1,
    minor: 0,
    patch: 0,
};

/// A `major.minor.patch` triple identifying a daemon
/// wire-protocol revision (see [`PROTOCOL_VERSION`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    /// Major version. Bumped on any breaking message-shape
    /// change; a mismatch makes two peers incompatible.
    pub major: u16,
    /// Minor version. Bumped on additive, backward-compatible
    /// evolution; a server accepts any client minor `<=` its own.
    pub minor: u16,
    /// Patch version. Reserved for non-wire-affecting fixes;
    /// never consulted by the compatibility check.
    pub patch: u16,
}

impl ProtocolVersion {
    /// Can a server speaking `self` serve a client envelope
    /// tagged `other`? Same-major required; client minor may be
    /// `<=` server minor (additive evolution).
    #[must_use]
    pub fn accepts(self, other: ProtocolVersion) -> bool {
        self.major == other.major && other.minor <= self.minor
    }
}

/// A request the daemon can answer. Every variant is a pure,
/// side-effect-free query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum DaemonRequest {
    /// Liveness probe.
    Ping,
    /// Fetch a cached artifact (e.g. a serialized `AnalysisRun`)
    /// by its content digest. Absent ⇒ `found: None`.
    GetArtifact {
        /// Lower-case hex content digest of the wanted artifact.
        digest_hex: String,
    },
    /// Query persisted facts, optionally filtered by `FactKind`
    /// name (snake_case, e.g. `dynamic_sql_evidence`) and capped
    /// at `limit` rows (`0` ⇒ server default).
    QueryFacts {
        /// Optional `FactKind` name filter (snake_case). `None`
        /// ⇒ no kind filter; all kinds are eligible.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        /// Maximum rows to return; `0` ⇒ the server's default
        /// cap. The response's `truncated` flag reports whether
        /// more rows existed than were returned.
        #[serde(default)]
        limit: u32,
    },
    /// Cache health: blob count, total bytes, registered
    /// strategies.
    Stats,
}

/// A response frame. `Error` carries a typed code so a client
/// can branch without string-matching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "result")]
pub enum DaemonResponse {
    /// Reply to [`DaemonRequest::Ping`] — the daemon is alive.
    Pong,
    /// Reply to [`DaemonRequest::GetArtifact`].
    Artifact {
        /// The cached artifact, or `None` ⇒ the digest is not
        /// cached (a normal answer, not an error — R13).
        found: Option<ArtifactPayload>,
    },
    /// Reply to [`DaemonRequest::QueryFacts`].
    Facts {
        /// The matching fact rows, in the daemon's iteration
        /// order, at most `limit` of them.
        rows: Vec<FactRow>,
        /// `true` ⇒ more rows matched than `limit` returned;
        /// the result is a prefix, not the full set.
        truncated: bool,
    },
    /// Reply to [`DaemonRequest::Stats`] — cache health counters.
    Stats(CacheStats),
    /// A typed protocol/availability failure. Carries a
    /// [`DaemonErrorCode`] so clients can branch without string
    /// matching.
    Error(DaemonError),
}

/// One cached artifact, returned inside
/// [`DaemonResponse::Artifact`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPayload {
    /// Lower-case hex content digest the artifact is keyed by;
    /// echoes the [`DaemonRequest::GetArtifact`] `digest_hex`.
    pub digest_hex: String,
    /// MIME-style media type of `body`, e.g. `application/json`.
    pub media_type: String,
    /// Raw artifact bytes, base64-free: transported as a UTF-8
    /// string because cached artifacts are JSON (the engine's
    /// robot-JSON envelope). Binary strategies are out of scope
    /// for v1 and rejected at put time, not here.
    pub body: String,
}

/// One persisted fact, returned inside [`DaemonResponse::Facts`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactRow {
    /// Stable unique identifier of the fact within the store.
    pub fact_id: String,
    /// `FactKind` name (snake_case), e.g. `dynamic_sql_evidence`;
    /// the same value a [`DaemonRequest::QueryFacts`] `kind`
    /// filter matches against.
    pub kind: String,
    /// The fact's payload as a JSON string, kind-specific in
    /// shape and opaque to the protocol layer.
    pub payload_json: String,
}

/// Cache health counters, returned inside
/// [`DaemonResponse::Stats`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of distinct blobs (artifacts) currently cached.
    pub blob_count: u64,
    /// Total on-disk size of all cached blobs, in bytes.
    pub total_bytes: u64,
    /// Names of the registered cache strategies, e.g.
    /// `analysis_run`.
    pub strategies: Vec<String>,
}

/// Machine-readable failure class carried by [`DaemonError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonErrorCode {
    /// Envelope `protocol_version` major mismatch.
    IncompatibleProtocol,
    /// The line was not a valid `DaemonEnvelope`.
    MalformedRequest,
    /// A well-formed request the server cannot serve (e.g.
    /// daemon mode disabled, cache directory unreadable).
    Unavailable,
}

/// A typed protocol/availability failure, carried by
/// [`DaemonResponse::Error`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("daemon error [{code:?}]: {message}")]
pub struct DaemonError {
    /// Machine-readable failure class; lets a client branch
    /// without parsing `message`.
    pub code: DaemonErrorCode,
    /// Human-readable detail for logs and diagnostics. Not
    /// stable — do not match on its text.
    pub message: String,
}

/// Versioned wire envelope. One per newline-delimited line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonEnvelope<T> {
    /// Wire-protocol revision the sender speaks; the receiver
    /// applies [`ProtocolVersion::accepts`] before trusting
    /// `payload`.
    pub protocol_version: ProtocolVersion,
    /// The framed message — a [`DaemonRequest`] or
    /// [`DaemonResponse`] depending on direction.
    pub payload: T,
}

impl<T> DaemonEnvelope<T> {
    /// Wrap `payload` at the current [`PROTOCOL_VERSION`].
    pub fn new(payload: T) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload,
        }
    }
}

/// Codec error for the line framing ([`encode`] / [`decode_line`]).
#[derive(Debug, Error)]
pub enum CodecError {
    /// `serde_json` failed to serialize the envelope; the
    /// payload string is the underlying error.
    #[error("serialize failed: {0}")]
    Serialize(String),
    /// A framing violation: the JSON for a single frame contained
    /// an interior `\n` / `\r` (only a trailing terminator is
    /// allowed).
    #[error("the framed line must not contain a newline")]
    EmbeddedNewline,
    /// `serde_json` failed to deserialize the line into an
    /// envelope; the payload string is the underlying error.
    #[error("deserialize failed: {0}")]
    Deserialize(String),
}

/// Encode an envelope as one framed line (JSON + trailing `\n`).
/// Errors if the JSON would contain a newline (it never does for
/// these types, but the invariant is enforced, not assumed).
pub fn encode<T: Serialize>(env: &DaemonEnvelope<T>) -> Result<String, CodecError> {
    let json = serde_json::to_string(env).map_err(|e| CodecError::Serialize(e.to_string()))?;
    if json.contains('\n') {
        return Err(CodecError::EmbeddedNewline);
    }
    Ok(format!("{json}\n"))
}

/// Decode one framed line into an envelope. A single trailing
/// `\n` or `\r\n` (CRLF, from a Windows-authored client) is
/// tolerated; any *interior* `\n` or `\r` is a framing violation
/// (`EmbeddedNewline`) — caught here rather than leaking a
/// confusing `Deserialize` error from a stray carriage return.
pub fn decode_line<T: for<'de> Deserialize<'de>>(
    line: &str,
) -> Result<DaemonEnvelope<T>, CodecError> {
    // Strip one trailing line terminator: `\n`, `\r\n`, or a bare
    // `\r`. Exactly one frame per call, so only the suffix is a
    // legal terminator.
    let trimmed = line
        .strip_suffix('\n')
        .map(|s| s.strip_suffix('\r').unwrap_or(s))
        .or_else(|| line.strip_suffix('\r'))
        .unwrap_or(line);
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(CodecError::EmbeddedNewline);
    }
    serde_json::from_str(trimmed).map_err(|e| CodecError::Deserialize(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_compat_policy() {
        let server = ProtocolVersion {
            major: 1,
            minor: 3,
            patch: 0,
        };
        assert!(server.accepts(ProtocolVersion {
            major: 1,
            minor: 0,
            patch: 9
        }));
        assert!(server.accepts(server));
        // client minor ahead of server -> reject
        assert!(!server.accepts(ProtocolVersion {
            major: 1,
            minor: 4,
            patch: 0
        }));
        // major mismatch -> reject
        assert!(!server.accepts(ProtocolVersion {
            major: 2,
            minor: 0,
            patch: 0
        }));
    }

    #[test]
    fn request_envelope_round_trips_and_is_single_line() {
        for req in [
            DaemonRequest::Ping,
            DaemonRequest::GetArtifact {
                digest_hex: "abc123".into(),
            },
            DaemonRequest::QueryFacts {
                kind: Some("dynamic_sql_evidence".into()),
                limit: 50,
            },
            DaemonRequest::QueryFacts {
                kind: None,
                limit: 0,
            },
            DaemonRequest::Stats,
        ] {
            let env = DaemonEnvelope::new(req.clone());
            let line = encode(&env).unwrap();
            assert!(line.ends_with('\n'));
            assert_eq!(line.matches('\n').count(), 1, "exactly one frame");
            let back: DaemonEnvelope<DaemonRequest> = decode_line(&line).unwrap();
            assert_eq!(back.payload, req);
            assert_eq!(back.protocol_version, PROTOCOL_VERSION);
        }
    }

    #[test]
    fn response_variants_round_trip() {
        for resp in [
            DaemonResponse::Pong,
            DaemonResponse::Artifact { found: None },
            DaemonResponse::Artifact {
                found: Some(ArtifactPayload {
                    digest_hex: "d".into(),
                    media_type: "application/json".into(),
                    body: "{\"k\":1}".into(),
                }),
            },
            DaemonResponse::Facts {
                rows: vec![FactRow {
                    fact_id: "fact:abc".into(),
                    kind: "privilege".into(),
                    payload_json: "{}".into(),
                }],
                truncated: true,
            },
            DaemonResponse::Stats(CacheStats {
                blob_count: 3,
                total_bytes: 999,
                strategies: vec!["analysis_run".into()],
            }),
            DaemonResponse::Error(DaemonError {
                code: DaemonErrorCode::IncompatibleProtocol,
                message: "major mismatch".into(),
            }),
        ] {
            let env = DaemonEnvelope::new(resp.clone());
            let line = encode(&env).unwrap();
            let back: DaemonEnvelope<DaemonResponse> = decode_line(&line).unwrap();
            assert_eq!(back.payload, resp);
        }
    }

    #[test]
    fn missing_artifact_is_found_none_not_an_error() {
        // The protocol's R13 contract: "absent" is a normal
        // answer, structurally distinct from `Error`.
        let r = DaemonResponse::Artifact { found: None };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"result\":\"artifact\""));
        assert!(!j.contains("error"));
    }

    #[test]
    fn decode_tolerates_trailing_newline_rejects_interior() {
        let env = DaemonEnvelope::new(DaemonRequest::Ping);
        let line = encode(&env).unwrap();
        assert!(decode_line::<DaemonRequest>(&line).is_ok());
        assert!(decode_line::<DaemonRequest>(line.trim_end()).is_ok());
        let two = format!("{}{}", line, line);
        assert!(
            matches!(
                decode_line::<DaemonRequest>(&two),
                Err(CodecError::EmbeddedNewline)
            ),
            "two frames in one decode call is a framing violation"
        );
    }

    #[test]
    fn decode_tolerates_crlf_and_rejects_interior_carriage_return() {
        let env = DaemonEnvelope::new(DaemonRequest::Ping);
        let json = serde_json::to_string(&env).unwrap();
        // CRLF-terminated frame (Windows-authored client) decodes.
        let crlf = format!("{json}\r\n");
        assert!(
            decode_line::<DaemonRequest>(&crlf).is_ok(),
            "a single trailing CRLF must be tolerated"
        );
        // Bare trailing CR also tolerated.
        assert!(decode_line::<DaemonRequest>(&format!("{json}\r")).is_ok());
        // An interior CR (not a frame terminator) is a framing
        // violation, caught here rather than as a JSON error.
        let interior_cr = format!("{}\r{}", &json[..5], &json[5..]);
        assert!(
            matches!(
                decode_line::<DaemonRequest>(&interior_cr),
                Err(CodecError::EmbeddedNewline)
            ),
            "interior carriage return is a typed framing violation"
        );
    }

    #[test]
    fn malformed_line_is_a_typed_codec_error() {
        let e = decode_line::<DaemonRequest>("{not json").unwrap_err();
        assert!(matches!(e, CodecError::Deserialize(_)));
    }

    #[test]
    fn request_tagging_is_stable_snake_case() {
        let j = serde_json::to_string(&DaemonRequest::GetArtifact {
            digest_hex: "x".into(),
        })
        .unwrap();
        assert!(j.contains("\"op\":\"get_artifact\""));
    }
}
