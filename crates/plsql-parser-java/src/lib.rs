#![forbid(unsafe_code)]

//! Subprocess-based Java ANTLR [`ParseBackend`] — **inert
//! historical spike**.
//!
//! ## Status: not a usable crate
//!
//! This crate is a **dead-end prototype**, kept in the tree for
//! its history and CI coverage only. It is **not published**
//! (`publish = false`) and **nothing in the workspace depends on
//! it** — the live PL/SQL parser backend is `plsql-parser-antlr`,
//! the parser-backend *tournament* winner. `plsql-parser-java`
//! is the tournament **loser**.
//!
//! By design it **never produces a real parse**: even on a
//! fully successful Java-worker run, [`JavaAntlrBackend::parse`]
//! discards the worker's output and returns a degraded,
//! empty-but-diagnosed [`BackendParseResult`]. Decoding the
//! worker's output into a real `Ast`/CST was the job of a
//! follow-on effort (the parser-backend wire protocol in
//! [`wire`]) that was never wired into `parse`. So
//! regardless of input or environment, this backend yields no
//! AST. Treat the crate as a frozen design artifact, not a
//! component to build on or revive without re-doing that work.
//!
//! It is retained — rather than deleted — so the panic-free
//! degradation path it pioneered, and the neutral wire-protocol
//! sketch in [`wire`], stay on record and under test.
//!
//! ## What it *does* still demonstrate (R13 / `ParseBackend`)
//!
//! Even as a spike it honours the [`ParseBackend`] contract:
//! `parse` MUST NOT panic on any input and MUST return a
//! well-formed [`BackendParseResult`]. Whatever happens (no jar
//! configured, jar missing, `java` missing, spawn/exit failure,
//! or — always — a "successful" run whose output is never
//! decoded), it returns an empty AST/CST plus a typed
//! [`Diagnostic`] stating exactly why no parse was produced. It
//! never fabricates an AST and never silently returns an
//! empty-but-clean result — the gap is always diagnosed.

pub mod wire;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use plsql_core::{Diagnostic, FileId, Severity};
use plsql_parser::{Ast, BackendParseResult, ConcreteSyntaxTree, ParseBackend, ParseMetrics};

/// Diagnostic code stamped on every degraded result so callers
/// (and the backend tournament, PARSE-000C) can filter
/// java-backend availability deterministically.
pub const JAVA_BACKEND_DIAG_CODE: &str = "PARSE-JAVA-UNAVAILABLE";

/// Env var holding the absolute path to the Java ANTLR worker
/// jar. Unset ⇒ the backend is unavailable (degraded result).
pub const WORKER_JAR_ENV: &str = "PLSQL_JAVA_ANTLR_JAR";
/// Env var overriding the `java` launcher (default: `java` on
/// `PATH`).
pub const JAVA_BIN_ENV: &str = "PLSQL_JAVA_BIN";

/// Resolved worker configuration. Built from the environment by
/// [`JavaWorkerConfig::from_env`]; injectable for tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaWorkerConfig {
    pub java_bin: String,
    pub worker_jar: Option<PathBuf>,
}

impl JavaWorkerConfig {
    #[must_use]
    pub fn from_env() -> Self {
        let java_bin = std::env::var(JAVA_BIN_ENV)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "java".to_string());
        let worker_jar = std::env::var(WORKER_JAR_ENV)
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        Self {
            java_bin,
            worker_jar,
        }
    }

    /// `Some(reason)` if this config cannot run a parse.
    fn unavailable_reason(&self) -> Option<String> {
        match &self.worker_jar {
            None => Some(format!("no worker jar configured ({WORKER_JAR_ENV} unset)")),
            Some(p) if !p.is_file() => Some(format!("worker jar not found at {}", p.display())),
            Some(_) => None,
        }
    }
}

/// The subprocess Java ANTLR backend — **inert spike**.
///
/// See the crate-level docs: this type's [`parse`](JavaAntlrBackend::parse)
/// never produces a real parse, by design. It is not a usable
/// parser backend; `plsql-parser-antlr` is.
#[derive(Clone, Debug)]
pub struct JavaAntlrBackend {
    config: JavaWorkerConfig,
}

impl JavaAntlrBackend {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            config: JavaWorkerConfig::from_env(),
        }
    }

    #[must_use]
    pub fn with_config(config: JavaWorkerConfig) -> Self {
        Self { config }
    }
}

impl Default for JavaAntlrBackend {
    fn default() -> Self {
        Self::from_env()
    }
}

/// Build a well-formed but empty result carrying a single typed
/// diagnostic — the canonical degraded outcome.
fn degraded(severity: Severity, reason: &str) -> BackendParseResult {
    BackendParseResult {
        cst: ConcreteSyntaxTree::default(),
        ast: Ast::default(),
        diagnostics: vec![Diagnostic::new(
            JAVA_BACKEND_DIAG_CODE,
            severity,
            format!("java-antlr backend produced no parse: {reason}"),
        )],
        metrics: ParseMetrics::default(),
        recovered: false,
    }
}

impl ParseBackend for JavaAntlrBackend {
    fn name(&self) -> &'static str {
        "java-antlr"
    }

    fn parse(
        &self,
        input: &str,
        _file_id: FileId,
        _opts: &plsql_parser::ParseOptions,
    ) -> BackendParseResult {
        // 1. Availability gate — no jar / missing jar ⇒ degraded.
        if let Some(reason) = self.config.unavailable_reason() {
            return degraded(Severity::Error, &reason);
        }
        let jar = self
            .config
            .worker_jar
            .as_ref()
            .expect("unavailable_reason() returned None ⇒ jar is Some");

        // 2. Spawn the worker, feeding source on stdin. Any OS
        //    error (java missing, exec failure) degrades — never
        //    panics.
        let child = Command::new(&self.config.java_bin)
            .arg("-jar")
            .arg(jar)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                return degraded(
                    Severity::Error,
                    &format!("could not launch `{}`: {e}", self.config.java_bin),
                );
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            // A broken pipe (worker exited early) is just another
            // degradation, not a panic.
            let _ = stdin.write_all(input.as_bytes());
        }
        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                return degraded(Severity::Error, &format!("worker wait failed: {e}"));
            }
        };
        if !output.status.success() {
            return degraded(
                Severity::Error,
                &format!(
                    "worker exited with {} (stderr: {})",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            );
        }

        // 3. The worker ran. Decoding its structured output into a
        //    real Ast/CST is the stable wire protocol owned by
        //    PLSQL-PARSE-000D — until that lands we do NOT
        //    fabricate an AST; we report, honestly, that the run
        //    succeeded but structured decoding is not yet wired.
        degraded(
            Severity::Warn,
            "worker ran successfully but Ast/CST decoding is gated on the \
             parser-backend wire protocol (PLSQL-PARSE-000D)",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_parser::{ParseOptions, parse_with_backend};

    fn opts() -> ParseOptions {
        ParseOptions::default()
    }

    #[test]
    fn name_is_stable() {
        assert_eq!(JavaAntlrBackend::from_env().name(), "java-antlr");
    }

    #[test]
    fn unconfigured_backend_degrades_without_panic() {
        let be = JavaAntlrBackend::with_config(JavaWorkerConfig {
            java_bin: "java".into(),
            worker_jar: None,
        });
        let r = be.parse(
            "CREATE PROCEDURE p IS BEGIN NULL; END;",
            FileId::new(1),
            &opts(),
        );
        assert!(!r.recovered);
        assert_eq!(r.diagnostics.len(), 1);
        assert_eq!(r.diagnostics[0].code, JAVA_BACKEND_DIAG_CODE);
        assert_eq!(r.diagnostics[0].severity, Severity::Error);
        assert!(
            r.diagnostics[0]
                .message
                .contains("no worker jar configured")
        );
        // Empty but well-formed — never a fabricated parse.
        assert!(r.ast.root.declarations.is_empty());
    }

    #[test]
    fn missing_jar_path_is_diagnosed_not_panicked() {
        let be = JavaAntlrBackend::with_config(JavaWorkerConfig {
            java_bin: "java".into(),
            worker_jar: Some(PathBuf::from("/no/such/plsql-antlr-worker.jar")),
        });
        let r = be.parse("SELECT 1 FROM dual;", FileId::new(1), &opts());
        assert_eq!(r.diagnostics[0].code, JAVA_BACKEND_DIAG_CODE);
        assert!(r.diagnostics[0].message.contains("worker jar not found"));
    }

    #[test]
    fn adversarial_inputs_never_panic() {
        let be = JavaAntlrBackend::with_config(JavaWorkerConfig {
            java_bin: "definitely-not-a-real-java-binary-xyzzy".into(),
            worker_jar: Some(PathBuf::from("/no/such.jar")),
        });
        for input in ["", "\0\0\0", &"x".repeat(100_000), "'unterminated", "/*"] {
            let r = be.parse(input, FileId::new(7), &opts());
            // Contract: always a well-formed result, never a panic.
            assert!(!r.diagnostics.is_empty());
        }
    }

    #[test]
    fn integrates_through_parse_with_backend() {
        let be = JavaAntlrBackend::with_config(JavaWorkerConfig {
            java_bin: "java".into(),
            worker_jar: None,
        });
        let pr = parse_with_backend("BEGIN NULL; END;", FileId::new(3), &be, &opts());
        assert_eq!(pr.file_id, FileId::new(3));
        assert!(
            !pr.is_clean(),
            "an unavailable backend is not a clean parse"
        );
    }

    #[test]
    fn from_env_defaults_java_bin_and_no_jar() {
        // With no overrides set in this process, from_env yields
        // the documented defaults (java on PATH, no jar ⇒
        // backend unavailable). Injected-config paths above cover
        // the override behaviour without mutating process env
        // (this crate is #![forbid(unsafe_code)], so no set_var).
        let c = JavaWorkerConfig::from_env();
        assert!(!c.java_bin.is_empty());
        if c.worker_jar.is_none() {
            assert!(c.unavailable_reason().is_some());
        }
    }
}
