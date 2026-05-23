//! Confidence scoring for dynamic-SQL edges.
//!
//! A dynamic-SQL call site (recognised by 's
//! [`DynamicSqlEvidence`]) yields candidate dependency edges, but
//! how much the engine trusts those edges depends on *how*
//! dynamic the statement is. This module maps a
//! `DynamicSqlEvidence` to a [`plsql_core::Confidence`] so the
//! dependency-graph layer can stamp each derived edge correctly
//! and the Trust Block (plan §1.5) reports honest numbers.
//!
//! Scoring policy (most → least trustworthy):
//!
//! * `LiteralOnly` with no expression fragments → **High**.
//!   The assembled statement is a constant; the engine knows
//!   exactly what runs.
//! * `ContainsExpression` but every interpolated identifier is
//!   wrapped in a `DBMS_ASSERT` sanitiser → **Medium**. The
//!   shape is parameterised; the object set is bounded by the
//!   asserted names.
//! * `ContainsExpression` with binds but no DBMS_ASSERT →
//!   **Low**. We see the literal skeleton but the substituted
//!   object names are unknown.
//! * `DbmsSqlChain` / `RefCursorBind` → **Opaque**. The
//!   statement text is assembled through an API the source-only
//!   view cannot follow.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — Dynamic
//!   SQL chapter distinguishes native dynamic SQL (analysable
//!   skeleton) from `DBMS_SQL` (opaque).
//! * `LOW-LEVEL-CATALOGS.md` Supplied Package Buckets —
//!   `DBMS_ASSERT` is the sanctioned cleanser that lifts a
//!   dynamic edge out of Low into Medium.

use plsql_core::{Confidence, ConfidenceLevel};

use crate::dynamic_sql::{DynamicSqlEvidence, OpacityReason};

/// Score the confidence of edges derived from `evidence`.
#[must_use]
pub fn score_dynamic_sql_edge(evidence: &DynamicSqlEvidence) -> Confidence {
    match evidence.opacity_reason {
        OpacityReason::DbmsSqlChain => Confidence {
            level: ConfidenceLevel::Opaque,
            explanation: Some(
                "DBMS_SQL call chain — assembled statement text is not visible to source-only analysis".into(),
            ),
        },
        OpacityReason::RefCursorBind => Confidence {
            level: ConfidenceLevel::Opaque,
            explanation: Some(
                "OPEN cursor FOR <expression> — the cursor query is whatever the bind value provides".into(),
            ),
        },
        OpacityReason::LiteralOnly => {
            // Literal-only is High unless the recogniser still
            // flagged an <expr> fragment (defensive — opacity
            // says literal but a placeholder leaked in).
            if evidence.fragments.iter().any(|f| f.contains("<expr>")) {
                medium_or_low(evidence)
            } else {
                Confidence {
                    level: ConfidenceLevel::High,
                    explanation: Some(
                        "literal-only dynamic SQL — assembled statement is a compile-time constant".into(),
                    ),
                }
            }
        }
        OpacityReason::ContainsExpression => medium_or_low(evidence),
    }
}

fn medium_or_low(evidence: &DynamicSqlEvidence) -> Confidence {
    if !evidence.dbms_assert_calls.is_empty() {
        Confidence {
            level: ConfidenceLevel::Medium,
            explanation: Some(format!(
                "interpolated dynamic SQL, {} identifier(s) routed through DBMS_ASSERT — object set is bounded by the asserted names",
                evidence.dbms_assert_calls.len()
            )),
        }
    } else {
        Confidence {
            level: ConfidenceLevel::Low,
            explanation: Some(
                "interpolated dynamic SQL with no DBMS_ASSERT sanitisation — substituted object names are unknown".into(),
            ),
        }
    }
}

/// Convenience: just the [`ConfidenceLevel`] without the
/// explanation string. Used by callers that only need the tier
/// for a histogram bucket.
#[must_use]
pub fn dynamic_sql_confidence_level(evidence: &DynamicSqlEvidence) -> ConfidenceLevel {
    score_dynamic_sql_edge(evidence).level
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamic_sql::recognise_dynamic_sql;

    #[test]
    fn literal_only_is_high() {
        let ev = recognise_dynamic_sql("EXECUTE IMMEDIATE 'SELECT 1 FROM dual';", "p:1").unwrap();
        assert_eq!(dynamic_sql_confidence_level(&ev), ConfidenceLevel::High);
    }

    #[test]
    fn interpolated_without_assert_is_low() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'SELECT * FROM ' || tbl || ' WHERE 1=1';",
            "p:2",
        )
        .unwrap();
        assert_eq!(dynamic_sql_confidence_level(&ev), ConfidenceLevel::Low);
    }

    #[test]
    fn interpolated_with_dbms_assert_is_medium() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'DROP TABLE ' || DBMS_ASSERT.SIMPLE_SQL_NAME(p_tab);",
            "p:3",
        )
        .unwrap();
        assert_eq!(dynamic_sql_confidence_level(&ev), ConfidenceLevel::Medium);
    }

    #[test]
    fn dbms_sql_chain_is_opaque() {
        let ev = recognise_dynamic_sql("v := DBMS_SQL.OPEN_CURSOR;", "p:4").unwrap();
        assert_eq!(dynamic_sql_confidence_level(&ev), ConfidenceLevel::Opaque);
    }

    #[test]
    fn ref_cursor_bind_is_opaque() {
        let ev = recognise_dynamic_sql("OPEN c FOR v_dynamic USING v_x;", "p:5").unwrap();
        assert_eq!(dynamic_sql_confidence_level(&ev), ConfidenceLevel::Opaque);
    }

    #[test]
    fn explanation_string_is_populated() {
        let ev = recognise_dynamic_sql("EXECUTE IMMEDIATE 'SELECT 1 FROM dual';", "p:6").unwrap();
        let c = score_dynamic_sql_edge(&ev);
        assert!(c.explanation.is_some());
        assert!(c.explanation.unwrap().contains("literal-only"));
    }

    #[test]
    fn medium_explanation_counts_assert_calls() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'GRANT ' || DBMS_ASSERT.SIMPLE_SQL_NAME(p) || ' TO ' || DBMS_ASSERT.ENQUOTE_NAME(g);",
            "p:7",
        )
        .unwrap();
        let c = score_dynamic_sql_edge(&ev);
        assert_eq!(c.level, ConfidenceLevel::Medium);
        assert!(c.explanation.unwrap().contains("DBMS_ASSERT"));
    }
}
