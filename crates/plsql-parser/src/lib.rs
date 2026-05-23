#![forbid(unsafe_code)]

//! PL/SQL parser frontend.
//!
//! This crate defines the backend-independent parsing API that all downstream
//! crates consume.  No ANTLR-generated types or grammar rule names escape this
//! boundary (R2 / R20).
//!
//! # Design
//!
//! A [`ParseBackend`] implementation converts raw source text into a
//! [`BackendParseResult`] containing the lossless **token tape**, a **CST**
//! (concrete syntax tree), and a typed **AST** (abstract syntax tree).
//!
//! The public [`parse_file`] / [`parse_with_backend`] functions wrap
//! [`BackendParseResult`] into a [`ParseResult`] that pairs the output with
//! the originating [`FileId`].
//!
//! # Lossless contract
//!
//! The token tape is the source of truth for round-tripping.  Every token and
//! trivia element carries a byte-offset span.  The AST is a *semantic*
//! projection — it is NOT required to preserve whitespace or comments.

pub mod ast;
pub mod dialect;
pub mod tokens;
pub mod visit;

use plsql_core::{Diagnostic, FileId};
use serde::{Deserialize, Serialize};
use tracing::instrument;

pub use dialect::{
    UNSUPPORTED_DIALECT_FEATURE_CODE, unsupported_dialect_feature_diagnostic,
    unsupported_dialect_feature_remediation,
};

pub use ast::{
    Ast, AstDecl, AstExpr, AstStatement, AstTypeDecl, ConcreteSyntaxTree, CstNodeId, SourceFile,
    SourceMap, Spanned,
};
pub use tokens::{Token, TokenKind, TokenTape, Trivia, TriviaTable};

// ---------------------------------------------------------------------------
// ParseOptions
// ---------------------------------------------------------------------------

/// Configuration knobs passed to every parse invocation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParseOptions {
    /// Which Oracle version to target (affects feature-gating in later passes).
    pub oracle_version: OracleTargetVersion,
    /// Whether the backend should attempt error recovery on syntax errors.
    pub recovery: RecoveryMode,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            oracle_version: OracleTargetVersion::Oracle19c,
            recovery: RecoveryMode::RecoverAtStatementBoundary,
        }
    }
}

/// Simplified Oracle version targeting for the parser.
///
/// This is intentionally *not* the same as `plsql_core::OracleVersion` — the
/// parser uses a smaller enum that only covers what the grammar supports.
/// Full version/feature policy lives in `AnalysisProfile`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum OracleTargetVersion {
    Oracle11g,
    Oracle12c,
    #[default]
    Oracle19c,
    Oracle21c,
    Oracle23ai,
    Oracle26ai,
}

/// Error-recovery strategy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum RecoveryMode {
    /// Stop at the first syntax error.
    FailFast,
    /// Skip to the next statement boundary (`;` or `/`) and continue.
    #[default]
    RecoverAtStatementBoundary,
    /// Aggressively recover at any plausible boundary (for corpus fuzzing).
    AggressiveRecovery,
}

// ---------------------------------------------------------------------------
// ParseMetrics
// ---------------------------------------------------------------------------

/// Observability counters emitted alongside every parse result.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParseMetrics {
    /// Total tokens produced by the lexer.
    pub total_tokens: u64,
    /// Number of trivia elements (whitespace, comments) captured.
    pub trivia_count: u64,
    /// Number of diagnostics emitted.
    pub diagnostic_count: u64,
    /// Number of recovery sites used (0 for a clean parse).
    pub recovery_count: u64,
    /// Number of bytes in the original source.
    pub source_bytes: u64,
}

// ---------------------------------------------------------------------------
// BackendParseResult
// ---------------------------------------------------------------------------

/// Raw output from a [`ParseBackend`] implementation.
///
/// This is the backend's *internal* result type.  The public API wraps it in
/// [`ParseResult`], which adds the originating `FileId`.
#[derive(Debug)]
pub struct BackendParseResult {
    /// The lossless concrete syntax tree.
    pub cst: ConcreteSyntaxTree,
    /// The typed abstract syntax tree (semantic projection).
    pub ast: Ast,
    /// Diagnostics emitted during lexing and parsing.
    pub diagnostics: Vec<Diagnostic>,
    /// Observability counters.
    pub metrics: ParseMetrics,
    /// `true` if error recovery was used at least once.
    pub recovered: bool,
}

// ---------------------------------------------------------------------------
// ParseResult
// ---------------------------------------------------------------------------

/// Public-facing parse result, paired with the file that produced it.
#[derive(Debug)]
pub struct ParseResult {
    /// Which file this result came from.
    pub file_id: FileId,
    /// The lossless concrete syntax tree.
    pub cst: ConcreteSyntaxTree,
    /// The typed abstract syntax tree.
    pub ast: Ast,
    /// Diagnostics emitted during lexing and parsing.
    pub diagnostics: Vec<Diagnostic>,
    /// Observability counters.
    pub metrics: ParseMetrics,
    /// `true` if error recovery was used at least once.
    pub recovered: bool,
}

impl ParseResult {
    /// Returns `true` if the parse completed without any diagnostics at
    /// [`Severity::Error`](plsql_core::Severity::Error) or above.
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_clean(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|d| d.severity >= plsql_core::Severity::Error)
    }

    /// Returns `true` if error recovery was used.
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn was_recovered(&self) -> bool {
        self.recovered
    }
}

// ---------------------------------------------------------------------------
// ParseBackend trait
// ---------------------------------------------------------------------------

/// Backend-independent parser interface (R2 / R20).
///
/// Every parser backend (antlr4rust, Java ANTLR subprocess, tree-sitter, etc.)
/// implements this trait.  Backend-internal types (ANTLR parse trees, grammar
/// rule names) are strictly private to the implementing crate.
///
/// The conformance test suite in `tests/conformance.rs` validates that all
/// backends behave identically on a canonical fixture set.
pub trait ParseBackend: Send + Sync {
    /// Human-readable backend name (e.g. `"antlr4rust"`, `"java-antlr"`).
    fn name(&self) -> &'static str;

    /// Parse the given source text and return a [`BackendParseResult`].
    ///
    /// # Contract
    ///
    /// - MUST NOT panic on any input (adversarial or otherwise).
    /// - MUST populate `cst.token_tape` such that `reconstruct(tape) == input`
    ///   byte-for-byte (the lossless round-trip property).
    /// - MUST emit at least one diagnostic per syntax error encountered.
    /// - MUST set `recovered = true` if recovery was used.
    fn parse(&self, input: &str, file_id: FileId, opts: &ParseOptions) -> BackendParseResult;
}

// ---------------------------------------------------------------------------
// Public convenience functions
// ---------------------------------------------------------------------------

/// Parse a single file with the given backend and options.
#[instrument(level = "debug", skip(backend, opts))]
pub fn parse_with_backend<B: ParseBackend>(
    input: &str,
    file_id: FileId,
    backend: &B,
    opts: &ParseOptions,
) -> ParseResult {
    let span = tracing::info_span!("parse_with_backend", backend = backend.name());
    let _enter = span.enter();

    let backend_result = backend.parse(input, file_id, opts);

    ParseResult {
        file_id,
        cst: backend_result.cst,
        ast: backend_result.ast,
        diagnostics: backend_result.diagnostics,
        metrics: backend_result.metrics,
        recovered: backend_result.recovered,
    }
}

/// Parse a single file with the given backend, using default [`ParseOptions`].
///
/// This is a thin convenience wrapper over [`parse_with_backend`] for the
/// common case where the caller does not need to customize parse options
/// (Oracle 19c target, statement-boundary recovery).
///
/// A backend is supplied explicitly: this crate is the *backend-independent*
/// parsing surface (R2 / R20) and intentionally has no knowledge of any
/// concrete backend. Callers that need a zero-configuration entry point
/// construct their chosen backend once and pass it here.
///
/// ```
/// # use plsql_parser::{parse_file, ParseBackend, BackendParseResult,
/// #     ParseOptions, ParseMetrics, Ast, ConcreteSyntaxTree};
/// # use plsql_core::FileId;
/// # struct MyBackend;
/// # impl ParseBackend for MyBackend {
/// #     fn name(&self) -> &'static str { "doc" }
/// #     fn parse(&self, _i: &str, _f: FileId, _o: &ParseOptions) -> BackendParseResult {
/// #         BackendParseResult {
/// #             cst: ConcreteSyntaxTree::new(), ast: Ast::new(),
/// #             diagnostics: Vec::new(), metrics: ParseMetrics::default(),
/// #             recovered: false,
/// #         }
/// #     }
/// # }
/// let result = parse_file("BEGIN NULL; END;", FileId::new(1), &MyBackend);
/// assert!(result.is_clean());
/// ```
#[instrument(level = "debug", skip(backend))]
pub fn parse_file<B: ParseBackend>(input: &str, file_id: FileId, backend: &B) -> ParseResult {
    parse_with_backend(input, file_id, backend, &ParseOptions::default())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_options_default_is_19c_with_recovery() {
        let opts = ParseOptions::default();
        assert_eq!(opts.oracle_version, OracleTargetVersion::Oracle19c);
        assert_eq!(opts.recovery, RecoveryMode::RecoverAtStatementBoundary);
    }

    #[test]
    fn parse_options_round_trips_through_json() {
        let opts = ParseOptions::default();
        let json = serde_json::to_string(&opts).unwrap();
        let back: ParseOptions = serde_json::from_str(&json).unwrap();
        assert_eq!(back.oracle_version, OracleTargetVersion::Oracle19c);
        assert_eq!(back.recovery, RecoveryMode::RecoverAtStatementBoundary);
    }

    #[test]
    fn parse_metrics_default_is_zero() {
        let m = ParseMetrics::default();
        assert_eq!(m.total_tokens, 0);
        assert_eq!(m.trivia_count, 0);
        assert_eq!(m.diagnostic_count, 0);
        assert_eq!(m.recovery_count, 0);
        assert_eq!(m.source_bytes, 0);
    }

    // -----------------------------------------------------------------
    // parse_file — convenience entry point over an explicit backend
    // -----------------------------------------------------------------

    /// A faithful in-test [`ParseBackend`] that records every [`ParseOptions`]
    /// value it is handed, so tests can prove `parse_file` forwards the
    /// expected defaults rather than fabricating them.
    struct RecordingBackend {
        seen_opts: std::sync::Mutex<Vec<ParseOptions>>,
    }

    impl RecordingBackend {
        fn new() -> Self {
            Self {
                seen_opts: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl ParseBackend for RecordingBackend {
        fn name(&self) -> &'static str {
            "recording"
        }

        fn parse(&self, input: &str, _file_id: FileId, opts: &ParseOptions) -> BackendParseResult {
            self.seen_opts
                .lock()
                .expect("opts mutex poisoned")
                .push(opts.clone());
            BackendParseResult {
                cst: ConcreteSyntaxTree::new(),
                ast: Ast::new(),
                diagnostics: Vec::new(),
                metrics: ParseMetrics {
                    source_bytes: input.len() as u64,
                    ..ParseMetrics::default()
                },
                recovered: false,
            }
        }
    }

    #[test]
    fn parse_file_forwards_default_parse_options() {
        let backend = RecordingBackend::new();
        let _ = parse_file("BEGIN NULL; END;", FileId::new(1), &backend);

        let seen = backend.seen_opts.lock().expect("opts mutex poisoned");
        assert_eq!(seen.len(), 1, "backend must be invoked exactly once");
        assert_eq!(seen[0].oracle_version, OracleTargetVersion::Oracle19c);
        assert_eq!(seen[0].recovery, RecoveryMode::RecoverAtStatementBoundary);
    }

    #[test]
    fn parse_file_pairs_result_with_its_file_id() {
        let backend = RecordingBackend::new();
        let result = parse_file("SELECT 1 FROM dual;", FileId::new(42), &backend);
        assert_eq!(result.file_id, FileId::new(42));
    }

    #[test]
    fn parse_file_propagates_backend_metrics() {
        let backend = RecordingBackend::new();
        let input = "CREATE PACKAGE p IS END;";
        let result = parse_file(input, FileId::new(7), &backend);
        assert_eq!(result.metrics.source_bytes, input.len() as u64);
        assert!(
            result.is_clean(),
            "a clean recording parse carries no error diagnostics"
        );
        assert!(!result.was_recovered());
    }

    #[test]
    fn parse_file_handles_empty_input_without_panicking() {
        let backend = RecordingBackend::new();
        let result = parse_file("", FileId::new(0), &backend);
        assert_eq!(result.metrics.source_bytes, 0);
        assert_eq!(result.file_id, FileId::new(0));
    }
}
