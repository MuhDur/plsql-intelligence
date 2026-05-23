//! Feed dynamic-SQL `StringShape` into `DynamicSqlEvidence`.
//!
//! FLOW-001/FLOW-002 compute a [`plsql_ir::StringShape`] for the
//! variable that feeds an `EXECUTE IMMEDIATE` / `OPEN FOR`
//! statement. This module joins that shape with the
//! [`DynamicSqlEvidence`] recorded by and produces
//! an [`EnrichedDynamicSql`] carrying a *refined* confidence.
//!
//! The string shape sharpens the opacity verdict the recogniser
//! alone could only guess at:
//!
//! * `StringShape::Literal` — the whole statement is a constant
//!   the flow pass already proved; even if the recogniser saw an
//!   `<expr>` fragment, the resolved value is literal → High.
//! * `StringShape::InterpolatedWithFix` — there is a fixed
//!   prefix/suffix; the object set is bounded by that skeleton →
//!   at least Medium (lifted from Low when no DBMS_ASSERT).
//! * `StringShape::FullyOpaque` — no usable fixed substring →
//!   keep the recogniser's (Low/Opaque) verdict.
//! * `StringShape::Empty` — degenerate; treat as Literal.
//!
//! The original [`DynamicSqlEvidence`] is left untouched (cc_1's
//! consumers depend on its shape); the refinement is an additive
//! wrapper.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — native
//!   dynamic SQL whose text is a compile-time-constant string is
//!   analysable exactly; interpolation bounds the object set.
//! * `LOW-LEVEL-CATALOGS.md` Supplied Package Buckets —
//!   `DBMS_ASSERT` still applies on top of the shape signal.

use plsql_core::{Confidence, ConfidenceLevel};
use plsql_ir::StringShape;

use crate::dynamic_sql::DynamicSqlEvidence;
use crate::dynamic_sql_confidence::score_dynamic_sql_edge;

/// A `DynamicSqlEvidence` joined with the flow-derived string
/// shape of the variable feeding it, plus the refined confidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnrichedDynamicSql {
    pub evidence: DynamicSqlEvidence,
    pub string_shape: Option<StringShape>,
    pub refined_confidence: Confidence,
}

/// Join `evidence` with the optional flow-derived `shape`. When
/// `shape` is `None` (flow couldn't determine it) the refined
/// confidence is just the recogniser's score (SYM-005 / DEP-007).
#[must_use]
pub fn enrich_dynamic_sql(
    evidence: DynamicSqlEvidence,
    shape: Option<StringShape>,
) -> EnrichedDynamicSql {
    let base = score_dynamic_sql_edge(&evidence);
    let refined = match &shape {
        None => base,
        Some(StringShape::Literal { .. }) | Some(StringShape::Empty) => Confidence {
            level: ConfidenceLevel::High,
            explanation: Some(
                "flow analysis proved the dynamic-SQL string is a compile-time-constant literal"
                    .into(),
            ),
        },
        Some(StringShape::InterpolatedWithFix {
            literal_prefix,
            literal_suffix,
        }) => {
            // Lift toward Medium — we know the fixed skeleton, so
            // the object set is bounded. Never downgrade below the
            // base (e.g. if base was already High from DBMS_ASSERT).
            let lifted = max_level(base.level, ConfidenceLevel::Medium);
            Confidence {
                level: lifted,
                explanation: Some(format!(
                    "flow analysis bounded the dynamic SQL with fixed prefix {:?} / suffix {:?}",
                    literal_prefix, literal_suffix
                )),
            }
        }
        Some(StringShape::FullyOpaque) => base,
    };
    EnrichedDynamicSql {
        evidence,
        string_shape: shape,
        refined_confidence: refined,
    }
}

/// Return the *higher-trust* of two confidence levels using the
/// engine's rank order (Opaque < Low < Medium < High).
fn max_level(a: ConfidenceLevel, b: ConfidenceLevel) -> ConfidenceLevel {
    if rank(a) >= rank(b) { a } else { b }
}

fn rank(c: ConfidenceLevel) -> u8 {
    match c {
        ConfidenceLevel::Opaque => 0,
        ConfidenceLevel::Low => 1,
        ConfidenceLevel::Medium => 2,
        ConfidenceLevel::High => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamic_sql::recognise_dynamic_sql;

    fn ev(text: &str) -> DynamicSqlEvidence {
        recognise_dynamic_sql(text, "p:1").unwrap()
    }

    #[test]
    fn literal_shape_forces_high() {
        // Even though the recogniser saw an interpolated `||`,
        // flow proved the resolved value is a literal.
        let e = ev("EXECUTE IMMEDIATE 'SELECT * FROM ' || tbl;");
        let enriched = enrich_dynamic_sql(
            e,
            Some(StringShape::Literal {
                value: "SELECT * FROM employees".into(),
            }),
        );
        assert_eq!(enriched.refined_confidence.level, ConfidenceLevel::High);
        assert!(
            enriched
                .refined_confidence
                .explanation
                .unwrap()
                .contains("compile-time-constant")
        );
    }

    #[test]
    fn empty_shape_treated_as_literal() {
        let e = ev("EXECUTE IMMEDIATE v_sql;");
        let enriched = enrich_dynamic_sql(e, Some(StringShape::Empty));
        // OPEN-style / EXECUTE IMMEDIATE of a bare var is normally
        // RefCursorBind/Opaque, but a proven-empty shape is
        // degenerate-literal.
        assert_eq!(enriched.refined_confidence.level, ConfidenceLevel::High);
    }

    #[test]
    fn interpolated_with_fix_lifts_to_at_least_medium() {
        let e = ev("EXECUTE IMMEDIATE 'SELECT * FROM ' || tbl || ' WHERE 1=1';");
        // Base (no DBMS_ASSERT) would be Low.
        let enriched = enrich_dynamic_sql(
            e,
            Some(StringShape::InterpolatedWithFix {
                literal_prefix: "SELECT * FROM ".into(),
                literal_suffix: " WHERE 1=1".into(),
            }),
        );
        assert_eq!(enriched.refined_confidence.level, ConfidenceLevel::Medium);
        assert!(
            enriched
                .refined_confidence
                .explanation
                .unwrap()
                .contains("fixed prefix")
        );
    }

    #[test]
    fn interpolated_with_fix_never_downgrades_high_base() {
        // A DBMS_ASSERT-wrapped site scores Medium at base; an
        // InterpolatedWithFix shape must not drop it.
        let e =
            ev("EXECUTE IMMEDIATE 'DROP TABLE ' || DBMS_ASSERT.SIMPLE_SQL_NAME(p) || ' CASCADE';");
        let base = score_dynamic_sql_edge(&e).level;
        let enriched = enrich_dynamic_sql(
            e,
            Some(StringShape::InterpolatedWithFix {
                literal_prefix: "DROP TABLE ".into(),
                literal_suffix: " CASCADE".into(),
            }),
        );
        assert!(rank(enriched.refined_confidence.level) >= rank(base));
        assert!(rank(enriched.refined_confidence.level) >= rank(ConfidenceLevel::Medium));
    }

    #[test]
    fn fully_opaque_keeps_base_verdict() {
        let e = ev("EXECUTE IMMEDIATE 'SELECT * FROM ' || tbl;");
        let base = score_dynamic_sql_edge(&e);
        let enriched = enrich_dynamic_sql(e, Some(StringShape::FullyOpaque));
        assert_eq!(enriched.refined_confidence.level, base.level);
    }

    #[test]
    fn no_shape_falls_back_to_base_score() {
        let e = ev("EXECUTE IMMEDIATE 'SELECT 1 FROM dual';");
        let base = score_dynamic_sql_edge(&e);
        let enriched = enrich_dynamic_sql(e, None);
        assert_eq!(enriched.refined_confidence.level, base.level);
        assert!(enriched.string_shape.is_none());
    }

    #[test]
    fn original_evidence_preserved() {
        let e = ev("EXECUTE IMMEDIATE 'SELECT 1 FROM dual';");
        let clone = e.clone();
        let enriched = enrich_dynamic_sql(e, Some(StringShape::FullyOpaque));
        assert_eq!(enriched.evidence, clone);
    }

    #[test]
    fn max_level_picks_higher_trust() {
        assert_eq!(
            max_level(ConfidenceLevel::Low, ConfidenceLevel::Medium),
            ConfidenceLevel::Medium
        );
        assert_eq!(
            max_level(ConfidenceLevel::High, ConfidenceLevel::Medium),
            ConfidenceLevel::High
        );
        assert_eq!(
            max_level(ConfidenceLevel::Opaque, ConfidenceLevel::Opaque),
            ConfidenceLevel::Opaque
        );
    }
}
