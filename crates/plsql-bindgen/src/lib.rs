//! `plsql-bindgen` — `BindingPlan` IR for the bindings generator.
//!
//! Input: a semantic model of a PL/SQL package (resolved signatures, parameter
//! modes, return types). Output: a per-package `BindingPlan` describing the
//! exact set of Rust wrappers to emit. Concrete code emission lives in a
//! downstream module that consumes this IR.
//!
//! This crate defines the IR shape; it does not emit Rust source itself.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

pub mod defaulted;
pub mod doctor;
pub mod emit;
pub mod executor;
pub mod oracle_types;
pub mod overload;

// Re-export the bindings-coverage doctor (PLSQL-BG-013 / oracle-vyho).
pub use doctor::{
    BindingsCoverageReport, BindingsPosture, CodeCountRow, SKIPPED_SAMPLE_LIMIT, coverage_report,
};
pub mod structs;
pub mod type_mapping;
pub use defaulted::{Defaulted, should_use_defaulted};
pub use emit::emit_wrappers;
pub use executor::{BindValue, ExecutionError, OracleExecutor, Row};
pub use oracle_types::{
    DateTimeBackend, IntervalYM, OracleDateTime, OracleTimestamp, OracleTimestampLtz,
    OracleTimestampTz,
};
pub use overload::disambiguate_overloads;
pub use structs::{
    ConstructorBinding, StructBinding, StructFieldBinding, StructOrigin, struct_for_object_type,
    struct_for_record, struct_for_rowtype,
};
pub use type_mapping::{
    DriverCapability, OracleType, TypeMapping, map_oracle_type, map_oracle_type_with_capability,
    with_defaulted, with_nullable,
};

/// Per-package plan for binding emission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingPlan {
    /// Schema-qualified package identifier (e.g. `hr.emp_pkg`).
    pub package_id: String,
    /// Original-case display name.
    pub package_name: String,
    /// One `RoutineBinding` per public procedure/function the generator will wrap.
    pub routines: Vec<RoutineBinding>,
    /// Per-routine diagnostics (e.g. unsupported `BOOLEAN` parameter, REF cursor
    /// without inferable projection, pipelined function); preserved in the
    /// plan so the emitter and report renderer can surface them consistently.
    pub diagnostics: Vec<BindingDiagnostic>,
}

/// Plan for a single subprogram wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineBinding {
    /// Subprogram name as it appears in the package.
    pub name: String,
    /// `procedure` or `function`.
    pub kind: RoutineKind,
    /// Ordered parameters.
    pub parameters: Vec<ParameterBinding>,
    /// Function return type (None for procedures).
    pub return_type: Option<RustTypeRef>,
    /// True if the routine is marked `PRAGMA AUTONOMOUS_TRANSACTION` — bindings
    /// generator must surface this in the wrapper's docs.
    pub autonomous_transaction: bool,
}

/// Whether this subprogram is a procedure or a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutineKind {
    Procedure,
    Function,
}

/// A single parameter and how it maps into the Rust wrapper signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterBinding {
    pub name: String,
    pub mode: ParameterMode,
    pub rust_type: RustTypeRef,
    /// Whether the parameter has a default (`DEFAULT` clause). When `true`
    /// for an IN / IN OUT parameter, the emitted wrapper exposes it as
    /// `Defaulted<T>` (or `Defaulted<Option<T>>` when also nullable) so the
    /// caller can choose `Omit` (server evaluates the declared default),
    /// `Null` (explicit NULL bind), or `Value(T)`. See `defaulted.rs`.
    pub has_default: bool,
}

/// PL/SQL parameter passing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ParameterMode {
    In,
    Out,
    InOut,
}

/// Reference to a Rust type the wrapper will use. Resolution against the
/// concrete driver type lives in the emitter; this IR keeps it as a string ref.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RustTypeRef {
    /// Fully-qualified Rust type path, e.g. `i64`, `String`, `chrono::NaiveDate`.
    pub path: String,
    /// True when this parameter is wrapped in `Option<…>`.
    pub nullable: bool,
}

/// A per-routine emission diagnostic. Mirrors the structure used by
/// `plsql-output` so the bindings generator can surface unsupported edges
/// without losing detail.
///
/// Implements. The `code` field is set from a
/// [`BindingDiagnosticCode`] so callers get a stable, exhaustive vocabulary
/// of unsupported-construct identifiers; `BindingDiagnostic::new_unsupported`
/// uses the enum's per-code message and suggested manual workaround so the
/// generator never has to invent text. `BG-008` (REF cursor) and `BG-009`
/// (pipelined functions) both build on this surface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BindingDiagnostic {
    /// The routine the diagnostic attaches to, if any.
    pub routine: Option<String>,
    /// Machine code; stable identifier consumed by tests and CI gates.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Optional suggested manual workaround the user can paste into their
    /// `.plsql-bindgen.toml` overrides, or a manual wrapper they can write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_workaround: Option<String>,
    /// Source span the diagnostic attaches to (file + 1-based line range).
    /// `None` is used for plan-level diagnostics that don't trace back to a
    /// single token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<DiagnosticSpan>,
    /// Severity tier.
    pub severity: BindingSeverity,
}

/// 1-based file/line span attached to a [`BindingDiagnostic`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticSpan {
    /// Project-relative file path of the source carrying the construct.
    pub file: String,
    /// Inclusive starting line, 1-based.
    pub line_start: u32,
    /// Inclusive ending line, 1-based. Equal to `line_start` for single-line spans.
    pub line_end: u32,
}

impl DiagnosticSpan {
    /// Convenience constructor for single-line spans.
    #[must_use]
    pub fn single_line(file: impl Into<String>, line: u32) -> Self {
        Self {
            file: file.into(),
            line_start: line,
            line_end: line,
        }
    }

    /// Multi-line constructor.
    #[must_use]
    pub fn line_range(file: impl Into<String>, line_start: u32, line_end: u32) -> Self {
        Self {
            file: file.into(),
            line_start,
            line_end,
        }
    }
}

/// Severity for a `BindingDiagnostic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingSeverity {
    /// Generator skipped this routine; user must implement manually.
    Skip,
    /// Generator emitted a wrapper but flagged a behavior worth reviewing.
    Warn,
    /// Informational; emitter behavior was nominal.
    Info,
}

/// Stable enumeration of unsupported-construct diagnostic codes the
/// generator can emit. Each variant maps to a `BG_…` string identifier
/// consumed by tests / CI gates; the enum carries severity, default message,
/// and suggested manual workaround so the generator never has to invent
/// text per call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingDiagnosticCode {
    /// `BG_UNSUPPORTED_REF_CURSOR` — PL/SQL `REF CURSOR` return without an
    /// explicit row-shape override (plan §13.5).
    RefCursor,
    /// `BG_UNSUPPORTED_PIPELINED` — pipelined function (plan §13.5).
    PipelinedFunction,
    /// `BG_UNSUPPORTED_BOOLEAN` — PL/SQL `BOOLEAN` parameter not bindable by
    /// the current driver.
    PlSqlBoolean,
    /// `BG_UNSUPPORTED_ASSOC_ARRAY` — associative array (`PLS_TABLE`,
    /// `TYPE … IS TABLE OF … INDEX BY …`).
    AssociativeArray,
    /// `BG_UNSUPPORTED_RECORD` — package-scoped `RECORD` type used in a
    /// public signature without a corresponding SQL object type.
    PlSqlRecord,
    /// `BG_UNSUPPORTED_NESTED_TABLE` — collection `TABLE OF …` parameter
    /// without a corresponding SQL collection type.
    NestedTableInParameter,
    /// `BG_UNSUPPORTED_VARRAY` — `VARRAY` parameter without a corresponding
    /// SQL type.
    VarrayInParameter,
    /// `BG_UNSUPPORTED_DEFAULT_EXPR` — non-literal `DEFAULT` expression that
    /// requires Oracle-side evaluation; generator cannot inline.
    NonLiteralDefault,
    /// `BG_UNSUPPORTED_LONG` — `LONG` / `LONG RAW` legacy type.
    LongOrLongRawColumn,
    /// `BG_UNSUPPORTED_AUTONOMOUS_TX` — `PRAGMA AUTONOMOUS_TRANSACTION`
    /// surface that the wrapper docs must call out explicitly.
    AutonomousTransaction,
    /// `BG_UNSUPPORTED_INVOKER_RIGHTS` — `AUTHID CURRENT_USER` package
    /// without a corresponding `current_user` runtime hint.
    InvokerRightsWithoutHint,
    /// `BG_UNSUPPORTED_OPAQUE_TYPE` — Oracle opaque type (e.g. `XMLTYPE`,
    /// `SYS.ANYDATA`) — `RustTypeRef` falls back to opaque buffer.
    OpaqueType,
    /// `BG_UNSUPPORTED_OVERLOAD_AMBIGUOUS` — multiple overloads collapse to
    /// the same Rust signature; generator cannot disambiguate.
    OverloadAmbiguity,
    /// `BG_UNSUPPORTED_WRAPPED_BODY` — package body is `WRAPPED`; generator
    /// cannot inspect body but emits the spec wrappers.
    WrappedPackageBody,
}

impl BindingDiagnosticCode {
    /// Returns the stable `BG_…` string code consumed by tests/CI gates.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RefCursor => "BG_UNSUPPORTED_REF_CURSOR",
            Self::PipelinedFunction => "BG_UNSUPPORTED_PIPELINED",
            Self::PlSqlBoolean => "BG_UNSUPPORTED_BOOLEAN",
            Self::AssociativeArray => "BG_UNSUPPORTED_ASSOC_ARRAY",
            Self::PlSqlRecord => "BG_UNSUPPORTED_RECORD",
            Self::NestedTableInParameter => "BG_UNSUPPORTED_NESTED_TABLE",
            Self::VarrayInParameter => "BG_UNSUPPORTED_VARRAY",
            Self::NonLiteralDefault => "BG_UNSUPPORTED_DEFAULT_EXPR",
            Self::LongOrLongRawColumn => "BG_UNSUPPORTED_LONG",
            Self::AutonomousTransaction => "BG_UNSUPPORTED_AUTONOMOUS_TX",
            Self::InvokerRightsWithoutHint => "BG_UNSUPPORTED_INVOKER_RIGHTS",
            Self::OpaqueType => "BG_UNSUPPORTED_OPAQUE_TYPE",
            Self::OverloadAmbiguity => "BG_UNSUPPORTED_OVERLOAD_AMBIGUOUS",
            Self::WrappedPackageBody => "BG_UNSUPPORTED_WRAPPED_BODY",
        }
    }

    /// Default severity. Skipping (no wrapper emitted) is the default for
    /// constructs the generator cannot bind safely. Warnings are emitted
    /// when the wrapper still ships but the user must read the docs.
    #[must_use]
    pub fn severity(self) -> BindingSeverity {
        match self {
            // The generator skips (no partial wrappers) when the construct
            // fundamentally has no driver-shaped binding.
            Self::RefCursor
            | Self::PipelinedFunction
            | Self::PlSqlBoolean
            | Self::AssociativeArray
            | Self::PlSqlRecord
            | Self::NestedTableInParameter
            | Self::VarrayInParameter
            | Self::OpaqueType
            | Self::OverloadAmbiguity => BindingSeverity::Skip,
            // The generator emits the wrapper but flags it for human review.
            Self::NonLiteralDefault
            | Self::LongOrLongRawColumn
            | Self::AutonomousTransaction
            | Self::InvokerRightsWithoutHint
            | Self::WrappedPackageBody => BindingSeverity::Warn,
        }
    }

    /// Default human-readable message — terse, single-line. The generator
    /// can override this when it has additional per-call-site context.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::RefCursor => {
                "REF CURSOR return without explicit row-shape override; wrapper skipped."
            }
            Self::PipelinedFunction => {
                "Pipelined function not supported by current driver surface; wrapper skipped."
            }
            Self::PlSqlBoolean => {
                "PL/SQL BOOLEAN parameter not bindable by current Oracle driver; wrapper skipped."
            }
            Self::AssociativeArray => {
                "Associative array (PLS_TABLE) parameter not bindable; wrapper skipped."
            }
            Self::PlSqlRecord => {
                "Package-scoped RECORD type without an SQL object analogue; wrapper skipped."
            }
            Self::NestedTableInParameter => {
                "Nested table parameter without a matching SQL collection type; wrapper skipped."
            }
            Self::VarrayInParameter => {
                "VARRAY parameter without a matching SQL type; wrapper skipped."
            }
            Self::NonLiteralDefault => {
                "Non-literal DEFAULT expression requires Oracle-side evaluation; wrapper emitted but caller must opt in to the default."
            }
            Self::LongOrLongRawColumn => {
                "LONG / LONG RAW legacy column; wrapper emitted but mapped to opaque bytes."
            }
            Self::AutonomousTransaction => {
                "PRAGMA AUTONOMOUS_TRANSACTION present; wrapper emitted, behavior documented."
            }
            Self::InvokerRightsWithoutHint => {
                "AUTHID CURRENT_USER package without a current_user runtime hint; wrapper emitted with caveat."
            }
            Self::OpaqueType => {
                "Oracle opaque type (e.g. XMLTYPE, SYS.ANYDATA); wrapper skipped, manual interop required."
            }
            Self::OverloadAmbiguity => {
                "Multiple overloads collapse to the same Rust signature; generator cannot disambiguate, wrapper skipped."
            }
            Self::WrappedPackageBody => {
                "Package body is WRAPPED; specs are wrapped from declaration only, body invariants cannot be inspected."
            }
        }
    }

    /// Default suggested workaround for the diagnostic. Generator may
    /// override per call site if a more specific suggestion is available.
    #[must_use]
    pub fn suggested_workaround(self) -> Option<&'static str> {
        match self {
            Self::RefCursor => Some(
                "Add a `[row_shape]` override in `.plsql-bindgen.toml` mapping the routine to an explicit row type.",
            ),
            Self::PipelinedFunction => Some(
                "Write the wrapper manually: open a cursor over the function and consume rows; see manual wrapper template in docs/bindings.md.",
            ),
            Self::PlSqlBoolean => Some(
                "Add a thin SQL wrapper that converts BOOLEAN to NUMBER(1) at the procedure boundary, then bind the wrapper.",
            ),
            Self::AssociativeArray => Some(
                "Convert the associative array to a SQL nested table type at the call site, then bind the nested table.",
            ),
            Self::PlSqlRecord => Some(
                "Define an SQL object type matching the RECORD shape; the generator will then bind the SQL type.",
            ),
            Self::NestedTableInParameter | Self::VarrayInParameter => Some(
                "Move the collection type into the SQL namespace (`CREATE TYPE … AS …`) and reference it in the signature.",
            ),
            Self::NonLiteralDefault => Some(
                "Either accept the parameter as required in the wrapper, or pre-evaluate the default on the Rust side.",
            ),
            Self::LongOrLongRawColumn => {
                Some("Migrate the column to CLOB / BLOB to get a driver-supported binding shape.")
            }
            Self::AutonomousTransaction => Some(
                "No change required; review the wrapper docs to confirm transaction-isolation behavior matches expectations.",
            ),
            Self::InvokerRightsWithoutHint => Some(
                "Set `current_user` in the runtime hint table, or document the expected proxy user explicitly.",
            ),
            Self::OpaqueType => Some(
                "Use the driver's opaque-type API directly in a manual wrapper; do not rely on generated bindings.",
            ),
            Self::OverloadAmbiguity => Some(
                "Rename one of the overloads in PL/SQL, or add an explicit Rust suffix in `.plsql-bindgen.toml` overrides.",
            ),
            Self::WrappedPackageBody => Some(
                "Provide an unwrapped reference build for analysis; the generated spec wrappers are correct on their own.",
            ),
        }
    }
}

impl BindingDiagnostic {
    /// Build a diagnostic from a [`BindingDiagnosticCode`] for `routine`. The
    /// code-supplied default message and suggested workaround are used; the
    /// emitter may swap them later via `BindingDiagnostic::with_message`.
    #[must_use]
    pub fn new_unsupported(
        code: BindingDiagnosticCode,
        routine: impl Into<Option<String>>,
        span: impl Into<Option<DiagnosticSpan>>,
    ) -> Self {
        Self {
            routine: routine.into(),
            code: String::from(code.as_str()),
            message: String::from(code.message()),
            suggested_workaround: code.suggested_workaround().map(String::from),
            span: span.into(),
            severity: code.severity(),
        }
    }

    /// Replace the default message with a call-site-specific one (e.g.
    /// adding the parameter name that triggered the diagnostic).
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Replace the default suggested workaround.
    #[must_use]
    pub fn with_suggested_workaround(
        mut self,
        suggested_workaround: impl Into<Option<String>>,
    ) -> Self {
        self.suggested_workaround = suggested_workaround.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_roundtrip_json() {
        let plan = BindingPlan {
            package_id: "hr.emp_pkg".into(),
            package_name: "EMP_PKG".into(),
            routines: vec![RoutineBinding {
                name: "FIND_BY_ID".into(),
                kind: RoutineKind::Function,
                parameters: vec![ParameterBinding {
                    name: "p_id".into(),
                    mode: ParameterMode::In,
                    rust_type: RustTypeRef {
                        path: "i64".into(),
                        nullable: false,
                    },
                    has_default: false,
                }],
                return_type: Some(RustTypeRef {
                    path: "String".into(),
                    nullable: true,
                }),
                autonomous_transaction: false,
            }],
            diagnostics: vec![BindingDiagnostic::new_unsupported(
                BindingDiagnosticCode::PlSqlBoolean,
                Some("LEGACY_PROC".to_string()),
                Some(DiagnosticSpan::single_line("hr/emp_pkg.sql", 42)),
            )],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let back: BindingPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.package_id, plan.package_id);
        assert_eq!(back.routines.len(), 1);
        assert_eq!(back.diagnostics.len(), 1);
    }

    #[test]
    fn diagnostic_codes_have_distinct_stable_strings() {
        use BindingDiagnosticCode::*;
        let codes = [
            RefCursor,
            PipelinedFunction,
            PlSqlBoolean,
            AssociativeArray,
            PlSqlRecord,
            NestedTableInParameter,
            VarrayInParameter,
            NonLiteralDefault,
            LongOrLongRawColumn,
            AutonomousTransaction,
            InvokerRightsWithoutHint,
            OpaqueType,
            OverloadAmbiguity,
            WrappedPackageBody,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for code in codes {
            let label = code.as_str();
            assert!(
                label.starts_with("BG_UNSUPPORTED_"),
                "diagnostic codes must use the BG_UNSUPPORTED_ prefix: {label}"
            );
            assert!(
                seen.insert(label),
                "diagnostic code {label} must be unique across the catalog"
            );
            // Every code carries a non-empty default message and a workaround.
            assert!(
                !code.message().is_empty(),
                "{label} has empty default message"
            );
            assert!(
                code.suggested_workaround().is_some(),
                "{label} must declare a suggested manual workaround"
            );
        }
        assert_eq!(seen.len(), 14, "expected 14 diagnostic codes");
    }

    #[test]
    fn skip_codes_default_to_skip_severity() {
        use BindingDiagnosticCode::*;
        for code in [
            RefCursor,
            PipelinedFunction,
            PlSqlBoolean,
            AssociativeArray,
            PlSqlRecord,
            NestedTableInParameter,
            VarrayInParameter,
            OpaqueType,
            OverloadAmbiguity,
        ] {
            assert_eq!(
                code.severity(),
                BindingSeverity::Skip,
                "code {} should default to Skip severity",
                code.as_str()
            );
        }
    }

    #[test]
    fn warn_codes_default_to_warn_severity() {
        use BindingDiagnosticCode::*;
        for code in [
            NonLiteralDefault,
            LongOrLongRawColumn,
            AutonomousTransaction,
            InvokerRightsWithoutHint,
            WrappedPackageBody,
        ] {
            assert_eq!(
                code.severity(),
                BindingSeverity::Warn,
                "code {} should default to Warn severity",
                code.as_str()
            );
        }
    }

    #[test]
    fn new_unsupported_copies_code_defaults_and_carries_span() {
        let span = DiagnosticSpan::line_range("hr/emp_pkg.sql", 12, 14);
        let diagnostic = BindingDiagnostic::new_unsupported(
            BindingDiagnosticCode::RefCursor,
            Some("FIND_BY_DEPT".to_string()),
            Some(span.clone()),
        );
        assert_eq!(diagnostic.code, "BG_UNSUPPORTED_REF_CURSOR");
        assert_eq!(diagnostic.severity, BindingSeverity::Skip);
        assert!(diagnostic.message.starts_with("REF CURSOR"));
        assert!(diagnostic.suggested_workaround.is_some());
        assert_eq!(diagnostic.span.as_ref(), Some(&span));
        assert_eq!(diagnostic.routine.as_deref(), Some("FIND_BY_DEPT"));
    }

    #[test]
    fn with_message_and_workaround_override_defaults() {
        let diagnostic = BindingDiagnostic::new_unsupported(
            BindingDiagnosticCode::OverloadAmbiguity,
            None::<String>,
            None,
        )
        .with_message("ambiguous overload: rename one variant to disambiguate")
        .with_suggested_workaround(Some(String::from(
            "add explicit `routine_rust_name = \"new_name\"` to .plsql-bindgen.toml",
        )));
        assert_eq!(
            diagnostic.message,
            "ambiguous overload: rename one variant to disambiguate"
        );
        assert!(
            diagnostic
                .suggested_workaround
                .as_deref()
                .map(|s| s.contains("plsql-bindgen.toml"))
                .unwrap_or(false)
        );
        // Code and severity still come from the original variant.
        assert_eq!(diagnostic.code, "BG_UNSUPPORTED_OVERLOAD_AMBIGUOUS");
        assert_eq!(diagnostic.severity, BindingSeverity::Skip);
    }

    #[test]
    fn diagnostic_skips_empty_span_and_workaround_in_json() {
        let diagnostic = BindingDiagnostic {
            routine: None,
            code: String::from("BG_UNSUPPORTED_OPAQUE_TYPE"),
            message: String::from("custom message"),
            suggested_workaround: None,
            span: None,
            severity: BindingSeverity::Skip,
        };
        let json = serde_json::to_string(&diagnostic).unwrap();
        assert!(!json.contains("suggested_workaround"));
        assert!(!json.contains("\"span\""));
        let back: BindingDiagnostic = serde_json::from_str(&json).unwrap();
        assert_eq!(back, diagnostic);
    }

    #[test]
    fn diagnostic_span_constructors_are_consistent() {
        let single = DiagnosticSpan::single_line("a.sql", 10);
        let range = DiagnosticSpan::line_range("a.sql", 10, 10);
        assert_eq!(single, range);
        let multi = DiagnosticSpan::line_range("a.sql", 5, 8);
        assert_eq!(multi.line_start, 5);
        assert_eq!(multi.line_end, 8);
    }
}
