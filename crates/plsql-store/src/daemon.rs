//! `plsqld` request handler (PLSQL-STORE-DAEMON-002).
//!
//! The pure, fully-tested core of the optional local daemon:
//! one framed [`DaemonRequest`] line in, one framed
//! [`DaemonResponse`] line out, served from a [`Store`]. The
//! socket accept-loop lives in `src/bin/plsqld.rs` and is a thin
//! shell around [`serve_envelope`] — all dispatch logic and every
//! failure mode is unit-tested here, no socket required.
//!
//! ## No network telemetry, explicit cache directory
//!
//! `plsqld` only ever binds a **Unix-domain socket** under an
//! operator-supplied cache directory (never a TCP port, never an
//! outbound connection — R17). This module is transport-agnostic;
//! the binary enforces the UDS-only / explicit-dir policy.
//!
//! ## R13 — no silent gaps
//!
//! An absent artifact is `Artifact{found: None}` (a first-class
//! answer). A request this daemon version does not serve
//! (`QueryFacts` — fact querying is the engine fact-store's job,
//! not the blob cache's) returns a typed
//! [`DaemonErrorCode::Unavailable`] with a directing message —
//! never a silently-empty success.

use crate::Store;
use crate::protocol::{
    ArtifactPayload, CacheStats, DaemonEnvelope, DaemonError, DaemonErrorCode, DaemonRequest,
    DaemonResponse, PROTOCOL_VERSION, decode_line, encode,
};

fn err(code: DaemonErrorCode, message: impl Into<String>) -> DaemonResponse {
    DaemonResponse::Error(DaemonError {
        code,
        message: message.into(),
    })
}

/// Dispatch one decoded request against `store`. Pure; never
/// panics — every store error degrades to a typed
/// [`DaemonResponse::Error`].
#[must_use]
pub fn serve_request(store: &Store, req: &DaemonRequest) -> DaemonResponse {
    match req {
        DaemonRequest::Ping => DaemonResponse::Pong,

        DaemonRequest::GetArtifact { digest_hex } => match store.get_blob(digest_hex) {
            Ok(None) => DaemonResponse::Artifact { found: None },
            Ok(Some(blob)) => match String::from_utf8(blob.body) {
                Ok(body) => DaemonResponse::Artifact {
                    found: Some(ArtifactPayload {
                        digest_hex: blob.digest_hex,
                        media_type: blob.media_type,
                        body,
                    }),
                },
                Err(_) => err(
                    DaemonErrorCode::Unavailable,
                    "artifact body is not UTF-8 (binary cache strategies are out of scope for \
                     protocol v1)",
                ),
            },
            Err(e) => err(DaemonErrorCode::Unavailable, format!("store error: {e}")),
        },

        DaemonRequest::Stats => {
            let (blob_count, total_bytes) = match store.cache_stats() {
                Ok(v) => v,
                Err(e) => {
                    return err(DaemonErrorCode::Unavailable, format!("store error: {e}"));
                }
            };
            let strategies = match store.registered_strategies() {
                Ok(s) => s.into_iter().map(|r| r.name).collect(),
                Err(e) => {
                    return err(DaemonErrorCode::Unavailable, format!("store error: {e}"));
                }
            };
            DaemonResponse::Stats(CacheStats {
                blob_count,
                total_bytes,
                strategies,
            })
        }

        DaemonRequest::QueryFacts { .. } => err(
            DaemonErrorCode::Unavailable,
            "fact querying is not served by plsqld v1 (the blob cache holds artifacts, not \
             facts); query the engine fact-store API directly",
        ),
    }
}

/// Decode one framed request line, enforce protocol
/// compatibility, dispatch, and encode the framed response line.
/// Always returns a single valid framed line — a decode failure
/// or protocol mismatch becomes a typed error response, never a
/// dropped connection.
#[must_use]
pub fn serve_envelope(store: &Store, line: &str) -> String {
    let response: DaemonResponse = match decode_line::<DaemonRequest>(line) {
        Err(e) => err(
            DaemonErrorCode::MalformedRequest,
            format!("could not decode request frame: {e}"),
        ),
        Ok(env) => {
            if !PROTOCOL_VERSION.accepts(env.protocol_version) {
                err(
                    DaemonErrorCode::IncompatibleProtocol,
                    format!(
                        "client protocol {}.{}.{} is incompatible with server {}.{}.{}",
                        env.protocol_version.major,
                        env.protocol_version.minor,
                        env.protocol_version.patch,
                        PROTOCOL_VERSION.major,
                        PROTOCOL_VERSION.minor,
                        PROTOCOL_VERSION.patch,
                    ),
                )
            } else {
                serve_request(store, &env.payload)
            }
        }
    };
    let out = DaemonEnvelope::new(response);
    encode(&out).unwrap_or_else(|_| {
        // The response types never contain a newline, so encode
        // cannot actually fail here; keep a valid framed fallback
        // rather than ever emitting an unframed line.
        "{\"protocol_version\":{\"major\":1,\"minor\":0,\"patch\":0},\
         \"payload\":{\"result\":\"error\",\"code\":\"unavailable\",\
         \"message\":\"response encode failed\"}}\n"
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreConfig;
    use crate::protocol::{DaemonEnvelope, ProtocolVersion};

    fn temp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("cache.db"), StoreConfig::default()).unwrap();
        (dir, store)
    }

    fn req_line(req: DaemonRequest) -> String {
        encode(&DaemonEnvelope::new(req)).unwrap()
    }

    #[test]
    fn ping_returns_pong() {
        let (_d, s) = temp_store();
        let line = serve_envelope(&s, &req_line(DaemonRequest::Ping));
        let env: DaemonEnvelope<DaemonResponse> = decode_line(&line).unwrap();
        assert_eq!(env.payload, DaemonResponse::Pong);
    }

    #[test]
    fn get_artifact_hit_round_trips_the_body() {
        let (_d, s) = temp_store();
        let blob = s
            .put_blob("dep_graph", "application/json", b"{\"k\":1}")
            .unwrap();
        let line = serve_envelope(
            &s,
            &req_line(DaemonRequest::GetArtifact {
                digest_hex: blob.digest_hex.clone(),
            }),
        );
        let env: DaemonEnvelope<DaemonResponse> = decode_line(&line).unwrap();
        match env.payload {
            DaemonResponse::Artifact { found: Some(p) } => {
                assert_eq!(p.body, "{\"k\":1}");
                assert_eq!(p.media_type, "application/json");
                assert_eq!(p.digest_hex, blob.digest_hex);
            }
            other => panic!("expected artifact hit, got {other:?}"),
        }
    }

    #[test]
    fn get_artifact_miss_is_found_none_not_error() {
        let (_d, s) = temp_store();
        let line = serve_envelope(
            &s,
            &req_line(DaemonRequest::GetArtifact {
                digest_hex: "deadbeef-not-present".to_string(),
            }),
        );
        let env: DaemonEnvelope<DaemonResponse> = decode_line(&line).unwrap();
        assert_eq!(env.payload, DaemonResponse::Artifact { found: None });
    }

    #[test]
    fn stats_reports_blob_count_bytes_and_strategies() {
        let (_d, s) = temp_store();
        s.put_blob("dep_graph", "application/json", b"abcde")
            .unwrap();
        let line = serve_envelope(&s, &req_line(DaemonRequest::Stats));
        let env: DaemonEnvelope<DaemonResponse> = decode_line(&line).unwrap();
        match env.payload {
            DaemonResponse::Stats(st) => {
                assert_eq!(st.blob_count, 1);
                assert_eq!(st.total_bytes, 5);
                assert!(st.strategies.iter().any(|n| n == "dep_graph"));
            }
            other => panic!("expected stats, got {other:?}"),
        }
    }

    #[test]
    fn query_facts_is_typed_unavailable_not_silent_empty() {
        let (_d, s) = temp_store();
        let line = serve_envelope(
            &s,
            &req_line(DaemonRequest::QueryFacts {
                kind: None,
                limit: 0,
            }),
        );
        let env: DaemonEnvelope<DaemonResponse> = decode_line(&line).unwrap();
        match env.payload {
            DaemonResponse::Error(e) => {
                assert_eq!(e.code, DaemonErrorCode::Unavailable);
                assert!(e.message.contains("fact querying"));
            }
            other => panic!("QueryFacts must be a typed Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn incompatible_protocol_major_is_rejected() {
        let (_d, s) = temp_store();
        // Hand-craft an envelope tagged with a future major.
        let bad = DaemonEnvelope {
            protocol_version: ProtocolVersion {
                major: PROTOCOL_VERSION.major + 1,
                minor: 0,
                patch: 0,
            },
            payload: DaemonRequest::Ping,
        };
        let line = serve_envelope(&s, &encode(&bad).unwrap());
        let env: DaemonEnvelope<DaemonResponse> = decode_line(&line).unwrap();
        match env.payload {
            DaemonResponse::Error(e) => {
                assert_eq!(e.code, DaemonErrorCode::IncompatibleProtocol)
            }
            other => panic!("expected protocol rejection, got {other:?}"),
        }
    }

    #[test]
    fn malformed_line_is_typed_error_not_a_panic() {
        let (_d, s) = temp_store();
        let line = serve_envelope(&s, "{not a valid frame");
        let env: DaemonEnvelope<DaemonResponse> = decode_line(&line).unwrap();
        match env.payload {
            DaemonResponse::Error(e) => {
                assert_eq!(e.code, DaemonErrorCode::MalformedRequest)
            }
            other => panic!("expected malformed-request error, got {other:?}"),
        }
    }

    #[test]
    fn every_response_is_exactly_one_frame() {
        let (_d, s) = temp_store();
        for r in [
            DaemonRequest::Ping,
            DaemonRequest::Stats,
            DaemonRequest::GetArtifact {
                digest_hex: "x".into(),
            },
        ] {
            let line = serve_envelope(&s, &req_line(r));
            assert!(line.ends_with('\n'));
            assert_eq!(line.matches('\n').count(), 1, "exactly one framed line");
        }
    }
}
