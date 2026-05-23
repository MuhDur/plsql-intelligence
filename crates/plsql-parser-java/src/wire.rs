//! Parser-backend wire protocol.
//!
//! The stable, neutral contract the Rust side
//! ([`JavaAntlrBackend`](crate::JavaAntlrBackend)) speaks to the
//! Java ANTLR worker subprocess. **Definition only** — no
//! transport, no worker; just the versioned message types,
//! framing, and the isolation invariant, with round-trip tests.
//!
//! ## R20 — backend isolation (the load-bearing rule)
//!
//! Not one field here may name a Java or ANTLR type, rule, or
//! class. The worker is a black box: it receives source text +
//! neutral options and returns a neutral lossless token tape +
//! neutral diagnostics. The Rust side reconstructs the CST/AST
//! from the token tape itself, never from ANTLR parse-tree
//! shapes. This lets the Java worker ship in production without
//! leaking grammar internals into the Rust API (and lets a
//! third backend reuse the same protocol).
//!
//! ## Framing & versioning
//!
//! Newline-delimited JSON: exactly one [`WireRequest`] line in,
//! one [`WireResponse`] line out. [`PROTOCOL_VERSION`] is carried
//! on both; a worker MUST reject a major mismatch and MAY accept
//! `request.minor <= worker.minor` (additive evolution) — the
//! same policy used by the store daemon protocol.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// `major.minor.patch` of the parser-backend wire protocol.
pub const PROTOCOL_VERSION: WireVersion = WireVersion {
    major: 1,
    minor: 0,
    patch: 0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl WireVersion {
    /// Can a worker speaking `self` serve a request tagged
    /// `other`? Same-major; request minor `<=` worker minor.
    #[must_use]
    pub fn accepts(self, other: WireVersion) -> bool {
        self.major == other.major && other.minor <= self.minor
    }
}

/// Neutral target-version selector (mirrors the parser's own
/// version enum *by value* so no parser type crosses the wire).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireOracleVersion {
    Oracle11g,
    Oracle12c,
    #[default]
    Oracle19c,
    Oracle21c,
    Oracle23ai,
    Oracle26ai,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireRecovery {
    FailFast,
    #[default]
    RecoverAtStatementBoundary,
    AggressiveRecovery,
}

/// What the Rust side sends the worker (one framed line).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRequest {
    pub protocol_version: WireVersion,
    /// The raw PL/SQL source to parse. The *only* input — the
    /// worker is stateless.
    pub source: String,
    pub oracle_version: WireOracleVersion,
    pub recovery: WireRecovery,
}

impl WireRequest {
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            source: source.into(),
            oracle_version: WireOracleVersion::default(),
            recovery: WireRecovery::default(),
        }
    }
}

/// Severity, neutral (not Java's `RecognitionException` etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireSeverity {
    Info,
    Warn,
    Error,
    Fatal,
}

/// One neutral diagnostic — byte offsets + 1-based line/col, no
/// grammar rule names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireDiagnostic {
    pub code: String,
    pub severity: WireSeverity,
    pub message: String,
    pub start_byte: u32,
    pub end_byte: u32,
    pub line: u32,
    pub column: u32,
}

/// One lossless token-tape element. `kind` is a neutral token
/// category string the protocol owns (e.g. `"identifier"`,
/// `"keyword"`, `"string"`, `"trivia.comment"`) — never an ANTLR
/// token-type number or symbolic name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireToken {
    pub kind: String,
    pub start_byte: u32,
    pub end_byte: u32,
    pub line: u32,
}

/// What the worker returns (one framed line). The token tape is
/// the source of truth for round-tripping; the Rust side rebuilds
/// the CST from it. No AST shape crosses the wire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireResponse {
    pub protocol_version: WireVersion,
    /// `false` ⇒ the worker could not produce a usable tape
    /// (still well-formed; `diagnostics` says why — R13).
    pub ok: bool,
    pub tokens: Vec<WireToken>,
    pub diagnostics: Vec<WireDiagnostic>,
    /// `true` if the worker used error recovery.
    pub recovered: bool,
}

#[derive(Debug)]
pub enum WireCodecError {
    Serialize(String),
    EmbeddedNewline,
    Deserialize(String),
}

impl std::fmt::Display for WireCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(e) => write!(f, "serialize failed: {e}"),
            Self::EmbeddedNewline => {
                write!(f, "framed line must not contain an interior newline")
            }
            Self::Deserialize(e) => write!(f, "deserialize failed: {e}"),
        }
    }
}
impl std::error::Error for WireCodecError {}

/// Encode a value as one framed line (JSON + trailing `\n`).
///
/// # Errors
/// [`WireCodecError`] on serialize failure or an interior newline.
pub fn encode<T: Serialize>(v: &T) -> Result<String, WireCodecError> {
    let json = serde_json::to_string(v).map_err(|e| WireCodecError::Serialize(e.to_string()))?;
    if json.contains('\n') {
        return Err(WireCodecError::EmbeddedNewline);
    }
    let mut s = String::with_capacity(json.len() + 1);
    let _ = write!(s, "{json}");
    s.push('\n');
    Ok(s)
}

/// Decode one framed line. Tolerates a single trailing
/// `\n`/`\r\n`/`\r`; an interior newline/CR is a framing
/// violation (typed, never a confusing serde error).
///
/// # Errors
/// [`WireCodecError`] on framing violation or deserialize failure.
pub fn decode_line<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T, WireCodecError> {
    let trimmed = line
        .strip_suffix('\n')
        .map(|s| s.strip_suffix('\r').unwrap_or(s))
        .or_else(|| line.strip_suffix('\r'))
        .unwrap_or(line);
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(WireCodecError::EmbeddedNewline);
    }
    serde_json::from_str(trimmed).map_err(|e| WireCodecError::Deserialize(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compat_policy() {
        let worker = WireVersion {
            major: 1,
            minor: 4,
            patch: 2,
        };
        assert!(worker.accepts(WireVersion {
            major: 1,
            minor: 0,
            patch: 9
        }));
        assert!(worker.accepts(worker));
        assert!(!worker.accepts(WireVersion {
            major: 1,
            minor: 5,
            patch: 0
        }));
        assert!(!worker.accepts(WireVersion {
            major: 2,
            minor: 0,
            patch: 0
        }));
    }

    #[test]
    fn request_round_trips_single_line() {
        let req = WireRequest::new("BEGIN NULL; END;");
        let line = encode(&req).unwrap();
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
        let back: WireRequest = decode_line(&line).unwrap();
        assert_eq!(back, req);
        assert_eq!(back.protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn response_round_trips_with_tape_and_diagnostics() {
        let resp = WireResponse {
            protocol_version: PROTOCOL_VERSION,
            ok: true,
            tokens: vec![WireToken {
                kind: "keyword".into(),
                start_byte: 0,
                end_byte: 5,
                line: 1,
            }],
            diagnostics: vec![WireDiagnostic {
                code: "PLS-00103".into(),
                severity: WireSeverity::Error,
                message: "unexpected symbol".into(),
                start_byte: 6,
                end_byte: 7,
                line: 1,
                column: 7,
            }],
            recovered: true,
        };
        let line = encode(&resp).unwrap();
        let back: WireResponse = decode_line(&line).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn not_ok_response_is_well_formed_with_reason() {
        let resp = WireResponse {
            protocol_version: PROTOCOL_VERSION,
            ok: false,
            tokens: vec![],
            diagnostics: vec![WireDiagnostic {
                code: "WORKER-INIT".into(),
                severity: WireSeverity::Fatal,
                message: "grammar load failed".into(),
                start_byte: 0,
                end_byte: 0,
                line: 0,
                column: 0,
            }],
            recovered: false,
        };
        let j = serde_json::to_string(&resp).unwrap();
        assert!(j.contains("\"ok\":false"));
        assert!(j.contains("grammar load failed"));
    }

    #[test]
    fn decode_tolerates_crlf_rejects_interior_newline() {
        let req = WireRequest::new("x");
        let json = serde_json::to_string(&req).unwrap();
        assert!(decode_line::<WireRequest>(&format!("{json}\r\n")).is_ok());
        assert!(decode_line::<WireRequest>(&format!("{json}\r")).is_ok());
        let two = format!("{json}\n{json}\n");
        assert!(matches!(
            decode_line::<WireRequest>(&two),
            Err(WireCodecError::EmbeddedNewline)
        ));
    }

    #[test]
    fn malformed_line_is_typed_codec_error() {
        let e = decode_line::<WireResponse>("{not json").unwrap_err();
        assert!(matches!(e, WireCodecError::Deserialize(_)));
    }

    #[test]
    fn r20_isolation_no_java_or_antlr_identifier_in_serialized_shape() {
        // The serialized contract must never carry a Java/ANTLR
        // type, rule, or class name. Probe a fully-populated
        // round-trip's JSON for forbidden tokens.
        let resp = WireResponse {
            protocol_version: PROTOCOL_VERSION,
            ok: true,
            tokens: vec![WireToken {
                kind: "identifier".into(),
                start_byte: 0,
                end_byte: 1,
                line: 1,
            }],
            diagnostics: vec![],
            recovered: false,
        };
        let req_json = serde_json::to_string(&WireRequest::new("SELECT 1 FROM dual")).unwrap();
        let resp_json = serde_json::to_string(&resp).unwrap();
        for forbidden in [
            "antlr",
            "Antlr",
            "ANTLR",
            "RuleContext",
            "ParserRuleContext",
            "org.antlr",
            "PlSqlParser",
            "java.lang",
            "RecognitionException",
        ] {
            assert!(
                !req_json.contains(forbidden) && !resp_json.contains(forbidden),
                "R20 violation: wire shape leaks `{forbidden}`"
            );
        }
    }

    #[test]
    fn neutral_severity_is_snake_case_tagged() {
        let j = serde_json::to_string(&WireSeverity::Error).unwrap();
        assert_eq!(j, "\"error\"");
    }
}
