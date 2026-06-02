//! `compile_with_warnings` tool.
//!
//! Wraps `ALTER ... COMPILE` with the session-level `PLSQL_WARNINGS =
//! ENABLE:ALL` setting so every category of warning surfaces during the
//! compile attempt. After the compile completes the tool reads
//! `USER_ERRORS` / `ALL_ERRORS` for the object, categorizes each row into
//! `severe` / `informational` / `performance`, and returns a structured
//! response.
//!
//! No PL/Scope dependency — the warning categorization is derived from the
//! Oracle error-code ranges documented in the database error reference
//! (PLW-05xxx = severe, PLW-06xxx = informational, PLW-07xxx = performance).

use plsql_catalog::{CatalogError, OracleConnection};
use plsql_core::UnknownReason;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::source::{ObjectError, run_get_errors};

/// Severity bucket the warning falls into per Oracle's PLW range docs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarningCategory {
    /// Compile-blocker error / non-warning row from `USER_ERRORS`.
    Severe,
    /// `PLW-07xxx` — performance hints (e.g. NOCOPY benefit).
    Performance,
    /// `PLW-06xxx` — informational hints / unused identifiers / implicit
    /// truncation / etc.
    Informational,
    /// Catch-all when the message number does not match any known range.
    Other,
}

/// Categorize an error row by Oracle message-number range. Defaults to
/// `Severe` when the row attribute is `ERROR` regardless of code, so
/// compile failures always bubble up as severe.
#[must_use]
pub fn categorize_error(error: &ObjectError) -> WarningCategory {
    if error.attribute.eq_ignore_ascii_case("ERROR") {
        return WarningCategory::Severe;
    }
    let code = error.message_number;
    // Oracle's documented PLW ranges: SEVERE = PLW-05xxx (5000-5999),
    // INFORMATIONAL = PLW-06xxx (6000-6249), PERFORMANCE = PLW-07xxx
    // (7000-7249). The 6xxx and 7xxx buckets must NOT be swapped.
    if (5000..6000).contains(&code) {
        WarningCategory::Severe
    } else if (6000..7000).contains(&code) {
        WarningCategory::Informational
    } else if (7000..8000).contains(&code) {
        WarningCategory::Performance
    } else {
        WarningCategory::Other
    }
}

/// Tool response.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompileWithWarningsResponse {
    pub owner: String,
    pub object_name: String,
    pub object_type: String,
    pub success: bool,
    pub severe: Vec<ObjectError>,
    pub performance: Vec<ObjectError>,
    pub informational: Vec<ObjectError>,
    pub other: Vec<ObjectError>,
    pub unknown_reasons: Vec<UnknownReason>,
}

#[derive(Debug, Error)]
pub enum CompileToolError {
    #[error("oracle backend error: {0}")]
    Backend(#[from] CatalogError),
    #[error("oracle backend error during compile: {0}")]
    BackendCompile(String),
    #[error("source-tool error while fetching post-compile errors: {0}")]
    Source(#[from] crate::source::SourceToolError),
}

/// Run `compile_with_warnings` for the given object. The function:
///
/// 1. Issues `ALTER SESSION SET PLSQL_WARNINGS = 'ENABLE:ALL'`.
/// 2. Issues the relevant `ALTER ... COMPILE` statement for the object.
/// 3. Re-reads `USER_ERRORS` / `ALL_ERRORS` for the object via
///    [`run_get_errors`].
/// 4. Buckets the rows by [`WarningCategory`].
///
/// The compile statement itself is bound positionally where possible. The
/// `ALTER ... COMPILE` form Oracle accepts does not support bind variables
/// for the identifier, so the function validates owner/name/object_type
/// against an allowlist before interpolating them into the DDL string.
pub fn run_compile_with_warnings<C: OracleConnection>(
    conn: &C,
    owner: &str,
    object_name: &str,
    object_type: &str,
) -> Result<CompileWithWarningsResponse, CompileToolError> {
    validate_identifier(owner)?;
    validate_identifier(object_name)?;
    let normalized_type = normalize_object_type(object_type)?;

    conn.execute("alter session set plsql_warnings = 'ENABLE:ALL'", &[])
        .map_err(|err| CompileToolError::BackendCompile(err.to_string()))?;

    let compile_sql = format!(
        "alter {kind} {owner}.{name} compile plsql_warnings = 'ENABLE:ALL'",
        kind = normalized_type.statement_kind,
        owner = owner,
        name = object_name,
    );
    let mut success = true;
    let mut unknown_reasons = Vec::new();
    if let Err(err) = conn.execute(&compile_sql, &[]) {
        success = false;
        unknown_reasons.push(UnknownReason::ParserRecoveryRegion);
        // Oracle's ALTER ... COMPILE raises when the object has errors,
        // but it also persists those errors into ALL_ERRORS — so we keep
        // going and let the structured fetch surface them.
        tracing::warn!(
            ?err,
            "alter compile failed; falling through to USER_ERRORS read"
        );
    }

    let errors_response = run_get_errors(conn, owner, object_name)?;
    // Propagate the K18 sanitization signal: if `run_get_errors` scrubbed
    // any free-text field on any row, the compile response must also flag
    // `ResponseSanitized` so the agent sees the same honesty marker. Avoid
    // emitting a duplicate reason if it is already present.
    for &reason in &errors_response.unknown_reasons {
        if !unknown_reasons.contains(&reason) {
            unknown_reasons.push(reason);
        }
    }
    let mut response = CompileWithWarningsResponse {
        owner: owner.to_string(),
        object_name: object_name.to_string(),
        object_type: normalized_type.statement_kind.to_string(),
        success,
        severe: Vec::new(),
        performance: Vec::new(),
        informational: Vec::new(),
        other: Vec::new(),
        unknown_reasons,
    };
    for error in errors_response.errors {
        let category = categorize_error(&error);
        // A severe row implies the compile did not actually succeed.
        if matches!(category, WarningCategory::Severe) {
            response.success = false;
        }
        match category {
            WarningCategory::Severe => response.severe.push(error),
            WarningCategory::Performance => response.performance.push(error),
            WarningCategory::Informational => response.informational.push(error),
            WarningCategory::Other => response.other.push(error),
        }
    }
    Ok(response)
}

/// Validate an Oracle identifier — letters / digits / underscore / dollar /
/// pound; cannot start with a digit; max 128 chars (23ai limit). The MCP
/// transport must reject anything that fails this check before reaching
/// the DDL string.
fn validate_identifier(identifier: &str) -> Result<(), CompileToolError> {
    if identifier.is_empty() || identifier.len() > 128 {
        return Err(CompileToolError::BackendCompile(format!(
            "rejected identifier (length): `{identifier}`"
        )));
    }
    let mut chars = identifier.chars();
    let Some(first) = chars.next() else {
        return Err(CompileToolError::BackendCompile(String::from(
            "identifier is empty",
        )));
    };
    if !first.is_ascii_alphabetic() {
        return Err(CompileToolError::BackendCompile(format!(
            "rejected identifier (must start with a letter): `{identifier}`"
        )));
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '#') {
            return Err(CompileToolError::BackendCompile(format!(
                "rejected identifier (illegal char `{c}`): `{identifier}`"
            )));
        }
    }
    Ok(())
}

struct NormalizedObjectType {
    statement_kind: &'static str,
}

fn normalize_object_type(text: &str) -> Result<NormalizedObjectType, CompileToolError> {
    Ok(NormalizedObjectType {
        statement_kind: match text.to_ascii_uppercase().as_str() {
            "PACKAGE" => "PACKAGE",
            "PACKAGE BODY" | "PACKAGE_BODY" => "PACKAGE BODY",
            "PROCEDURE" => "PROCEDURE",
            "FUNCTION" => "FUNCTION",
            "TRIGGER" => "TRIGGER",
            "TYPE" => "TYPE",
            "TYPE BODY" | "TYPE_BODY" => "TYPE BODY",
            "VIEW" => "VIEW",
            other => {
                return Err(CompileToolError::BackendCompile(format!(
                    "ALTER ... COMPILE not supported for object type `{other}`",
                )));
            }
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_catalog::{OracleBackend, OracleBind, OracleConnectionInfo, OracleRow};
    use std::sync::Mutex;

    struct CompileStubConn {
        error_rows: Vec<OracleRow>,
        executes: Mutex<Vec<String>>,
        fail_compile: bool,
    }

    impl OracleConnection for CompileStubConn {
        fn backend(&self) -> OracleBackend {
            OracleBackend::RustOracle
        }
        fn ping(&self) -> Result<(), CatalogError> {
            Ok(())
        }
        fn describe(&self) -> Result<OracleConnectionInfo, CatalogError> {
            Ok(OracleConnectionInfo {
                backend: OracleBackend::RustOracle,
                connect_string: String::from("//localhost/XE"),
                current_schema: Some(String::from("BILLING")),
                server_version: String::from("23.0.0.0.0"),
                db_name: String::from("XE"),
                db_domain: String::new(),
                service_name: String::from("XE"),
                instance_name: String::from("xe"),
                server_type: String::from("Dedicated"),
                max_identifier_length: 128,
                max_open_cursors: 500,
            })
        }
        fn query_rows(
            &self,
            _sql: &str,
            _params: &[OracleBind],
        ) -> Result<Vec<OracleRow>, CatalogError> {
            Ok(self.error_rows.clone())
        }
        fn execute(&self, sql: &str, _params: &[OracleBind]) -> Result<u64, CatalogError> {
            self.executes.lock().unwrap().push(sql.to_string());
            if self.fail_compile && sql.contains("compile") {
                Err(CatalogError::OracleBackendError {
                    backend: OracleBackend::RustOracle,
                    message: String::from("ORA-24344: success with compilation error"),
                })
            } else {
                Ok(0)
            }
        }
    }

    fn error_row(line: u32, attribute: &str, message_number: i64, text: &str) -> OracleRow {
        let mut row = OracleRow::default();
        row.insert("OWNER", "VARCHAR2(128)", Some(String::from("BILLING")));
        row.insert("NAME", "VARCHAR2(128)", Some(String::from("BILLING_PKG")));
        row.insert("TYPE", "VARCHAR2(30)", Some(String::from("PACKAGE BODY")));
        row.insert("LINE", "NUMBER", Some(line.to_string()));
        row.insert("POSITION", "NUMBER", Some(String::from("4")));
        row.insert("ATTRIBUTE", "VARCHAR2(9)", Some(attribute.to_string()));
        row.insert("MESSAGE_NUMBER", "NUMBER", Some(message_number.to_string()));
        row.insert("TEXT", "VARCHAR2(4000)", Some(text.to_string()));
        row
    }

    #[test]
    fn categorize_error_buckets_by_plw_range() {
        assert_eq!(
            categorize_error(&ObjectError {
                owner: String::new(),
                object_name: String::new(),
                object_type: String::new(),
                line: 0,
                position: 0,
                attribute: String::from("WARNING"),
                message_number: 5400,
                text: String::new(),
            }),
            WarningCategory::Severe
        );
        assert_eq!(
            categorize_error(&ObjectError {
                owner: String::new(),
                object_name: String::new(),
                object_type: String::new(),
                line: 0,
                position: 0,
                attribute: String::from("WARNING"),
                message_number: 6010,
                text: String::new(),
            }),
            WarningCategory::Informational,
            "PLW-06xxx is Oracle INFORMATIONAL, not performance"
        );
        assert_eq!(
            categorize_error(&ObjectError {
                owner: String::new(),
                object_name: String::new(),
                object_type: String::new(),
                line: 0,
                position: 0,
                attribute: String::from("WARNING"),
                message_number: 7203,
                text: String::new(),
            }),
            WarningCategory::Performance,
            "PLW-07203 (NOCOPY benefit) is Oracle PERFORMANCE, not informational"
        );
        assert_eq!(
            categorize_error(&ObjectError {
                owner: String::new(),
                object_name: String::new(),
                object_type: String::new(),
                line: 0,
                position: 0,
                attribute: String::from("ERROR"),
                message_number: 0,
                text: String::new(),
            }),
            WarningCategory::Severe,
            "ERROR attribute is always severe regardless of code"
        );
        assert_eq!(
            categorize_error(&ObjectError {
                owner: String::new(),
                object_name: String::new(),
                object_type: String::new(),
                line: 0,
                position: 0,
                attribute: String::from("WARNING"),
                message_number: 9999,
                text: String::new(),
            }),
            WarningCategory::Other
        );
    }

    #[test]
    fn validate_identifier_accepts_typical_names() {
        assert!(validate_identifier("BILLING_PKG").is_ok());
        assert!(validate_identifier("APP$NAME").is_ok());
        assert!(validate_identifier("PKG#1").is_ok());
        assert!(validate_identifier("PKG_2026").is_ok());
    }

    #[test]
    fn validate_identifier_rejects_dangerous_patterns() {
        assert!(validate_identifier("").is_err());
        assert!(validate_identifier("1STARTS_WITH_DIGIT").is_err());
        assert!(validate_identifier("HAS SPACE").is_err());
        assert!(validate_identifier("BAD;DROP_TABLE").is_err());
        assert!(validate_identifier(&"X".repeat(200)).is_err());
    }

    #[test]
    fn normalize_object_type_supports_compilable_kinds() {
        for kind in [
            "PACKAGE",
            "PACKAGE BODY",
            "PACKAGE_BODY",
            "PROCEDURE",
            "FUNCTION",
            "TRIGGER",
            "TYPE",
            "TYPE BODY",
            "VIEW",
        ] {
            assert!(normalize_object_type(kind).is_ok());
        }
        assert!(normalize_object_type("TABLE").is_err());
        assert!(normalize_object_type("SEQUENCE").is_err());
    }

    #[test]
    fn compile_returns_success_when_no_errors_present() {
        let stub = CompileStubConn {
            error_rows: vec![],
            executes: Mutex::new(Vec::new()),
            fail_compile: false,
        };
        let response =
            run_compile_with_warnings(&stub, "BILLING", "BILLING_PKG", "PACKAGE BODY").unwrap();
        assert!(response.success);
        assert!(response.severe.is_empty());
        assert!(response.performance.is_empty());
        assert!(response.informational.is_empty());
        assert!(response.other.is_empty());
        let executes = stub.executes.lock().unwrap();
        assert!(executes.iter().any(|s| s.contains("plsql_warnings")));
        assert!(
            executes
                .iter()
                .any(|s| s.contains("compile") && s.contains("BILLING.BILLING_PKG"))
        );
    }

    #[test]
    fn compile_returns_categorized_errors_when_present() {
        let stub = CompileStubConn {
            error_rows: vec![
                error_row(2, "ERROR", 201, "PLS-00201: identifier 'FOO'"),
                error_row(5, "WARNING", 6005, "PLW-06005: implicit truncation"),
                error_row(9, "WARNING", 7203, "PLW-07203: parameter 'p_id'"),
            ],
            executes: Mutex::new(Vec::new()),
            fail_compile: false,
        };
        let response =
            run_compile_with_warnings(&stub, "BILLING", "BILLING_PKG", "PACKAGE BODY").unwrap();
        assert!(!response.success); // severe row → success = false
        assert_eq!(response.severe.len(), 1);
        assert_eq!(response.performance.len(), 1);
        assert_eq!(response.informational.len(), 1);
        assert!(response.other.is_empty());
        // Pin each row to its correct Oracle-documented bucket so the
        // 6xxx/7xxx mapping cannot silently invert again:
        //   PLW-06005 (implicit truncation) is INFORMATIONAL,
        //   PLW-07203 (NOCOPY benefit)       is PERFORMANCE.
        assert_eq!(
            response.informational[0].message_number, 6005,
            "PLW-06005 must land in the informational bucket"
        );
        assert_eq!(
            response.performance[0].message_number, 7203,
            "PLW-07203 must land in the performance bucket"
        );
        assert_eq!(
            response.severe[0].message_number, 201,
            "PLS-00201 ERROR row must land in the severe bucket"
        );
    }

    #[test]
    fn compile_propagates_response_sanitized_from_get_errors() {
        // An error row whose TEXT carries attacker-influenceable tool-call
        // markup must be K18-scrubbed by run_get_errors, and the compile
        // wrapper must propagate the ResponseSanitized signal so the agent
        // sees the same honesty marker. Assemble the markup at runtime.
        let tainted_text = format!(
            "PLS-00201: identifier {lt}{slash}tool_call{gt} must be declared",
            lt = '<',
            gt = '>',
            slash = '/'
        );
        let stub = CompileStubConn {
            error_rows: vec![error_row(1, "ERROR", 201, &tainted_text)],
            executes: Mutex::new(Vec::new()),
            fail_compile: false,
        };
        let response =
            run_compile_with_warnings(&stub, "BILLING", "BILLING_PKG", "PACKAGE BODY").unwrap();
        assert_eq!(response.severe.len(), 1);
        assert!(
            !response.severe[0].text.contains('<') && !response.severe[0].text.contains('>'),
            "severe row text retained a raw angle bracket"
        );
        assert!(
            response
                .unknown_reasons
                .contains(&UnknownReason::ResponseSanitized),
            "compile response must propagate ResponseSanitized from get_errors"
        );
    }

    #[test]
    fn compile_handles_alter_failure_gracefully() {
        // Oracle raises on compile when the object has hard errors; the
        // tool must still read USER_ERRORS to surface them.
        let stub = CompileStubConn {
            error_rows: vec![error_row(1, "ERROR", 201, "PLS-00201")],
            executes: Mutex::new(Vec::new()),
            fail_compile: true,
        };
        let response =
            run_compile_with_warnings(&stub, "BILLING", "BILLING_PKG", "PACKAGE BODY").unwrap();
        assert!(!response.success);
        assert_eq!(response.severe.len(), 1);
        assert!(
            response
                .unknown_reasons
                .contains(&UnknownReason::ParserRecoveryRegion)
        );
    }
}
