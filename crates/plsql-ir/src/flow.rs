//! Value-flow, taint, constant, value-set, and string-shape
//! models.
//!
//! Downstream SAST and lineage layers reason about *how* values
//! propagate, not just *whether* a name binds. This module
//! defines the shapes those passes share so they all speak the
//! same vocabulary:
//!
//! * [`TaintKind`] — the family of taint a value carries
//!   (user-supplied, dynamic-SQL, db-link, file-system, …).
//! * [`ConstantValue`] — when a value is provably constant, its
//!   wire form (number / string / bool / null).
//! * [`ValueSet`] — abstract domain summarising the set of values
//!   a name might hold (Top / `OneOf` / `Range` / `Bottom`).
//! * [`StringShape`] — abstract domain for string values
//!   (literal / interpolated-with-prefix / fully-opaque).
//! * [`ValueFlow`] — the per-name aggregate the passes return.
//!
//! Population happens in the intra- / inter-procedural flow passes.
//! This module ships the types + serde + small helpers so the
//! consumers (SAST, bindings, doc) program against a stable surface
//! today.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   bind-variable + parameter-mode chapters drive how taint
//!   enters a routine. `DBMS_ASSERT` (see
//!   `LOW-LEVEL-CATALOGS.md` supplied-packages) is the
//!   sanctioned cleanser.

use serde::{Deserialize, Serialize};

/// Per-name aggregate flow report.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueFlow {
    pub taint: Taint,
    pub constant: Option<ConstantValue>,
    pub value_set: ValueSet,
    pub string_shape: Option<StringShape>,
}

/// Taint state. `kinds` lists the *live* (uncleansed) taint sources that
/// flow into the value — a bound sanitiser (e.g. a `DBMS_ASSERT.*` call)
/// removes the kinds it cleanses, so a sanitized value carries no live kind.
/// `cleansed_by` records which sanitisers fired anywhere in the value's
/// derivation (kept for reporting, not for the alarm). SAST emits a finding
/// iff `kinds` is non-empty. Tracking *live* kinds (rather than all-seen
/// kinds gated on an empty `cleansed_by`) binds cleansing to the sanitized
/// sub-expression, so taint concatenated alongside a sanitized operand still
/// alarms (e.g. `DBMS_ASSERT.ENQUOTE_LITERAL('x') || p_user`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Taint {
    pub kinds: Vec<TaintKind>,
    pub cleansed_by: Vec<TaintCleanser>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaintKind {
    /// Value came from an IN parameter of a public routine.
    UserInput,
    /// Value came from a bind variable.
    BindVariable,
    /// Value came from `EXECUTE IMMEDIATE` / `OPEN FOR <expr>`
    /// dynamic SQL substitution.
    DynamicSql,
    /// Value came from a remote `name@dblink` reference.
    DbLink,
    /// Value came from a file-system read (`UTL_FILE`).
    FileSystem,
    /// Value came from `UTL_HTTP` / `UTL_TCP` / `UTL_SMTP`.
    Network,
    /// Value came from the OS environment (`DBMS_SYSTEM`,
    /// `SYS_CONTEXT('USERENV', …)`).
    Environment,
    /// Value came from an Oracle scheduler argument
    /// (`DBMS_SCHEDULER.SET_JOB_ARGUMENT_VALUE`).
    SchedulerArgument,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaintCleanser {
    /// One of the `DBMS_ASSERT.*` sanitisers (per SYM-005).
    DbmsAssert,
    /// `SYS.UTL_RAW.CAST_TO_RAW` / equivalent hex-encode.
    HexEncode,
    /// Operator wrote a literal-only string — no taint flow.
    LiteralOnly,
    /// `DBMS_OUTPUT.PUT_LINE` consumer — taint does not flow
    /// back into the database (terminal sink).
    OutputSink,
    /// Caller explicitly annotated the value as cleansed via a
    /// project-local convention (e.g. comment marker).
    OperatorAttested,
}

/// When the value is provably constant, its wire form. Variants
/// use struct-form fields so the serde `tag = "kind"` adjacent-
/// encoding doesn't trip on newtypes carrying `String` /
/// primitive payloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConstantValue {
    /// Integer literal preserved verbatim.
    Int { value: String },
    /// Floating-point or fixed-point literal preserved verbatim.
    Float { value: String },
    /// String literal body, doubled-`''` already de-escaped.
    Str { value: String },
    /// Boolean literal.
    Bool { value: bool },
    /// `NULL` literal.
    Null,
}

/// Abstract domain summarising the set of values a name might
/// hold. The lattice is `Bottom < Range / OneOf < Top` —
/// passes refine `Top` toward the more specific variants as
/// they accumulate evidence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueSet {
    /// No information yet — could be anything.
    #[default]
    Top,
    /// Value is one of a finite set of constants.
    OneOf { values: Vec<ConstantValue> },
    /// Numeric range `[lo, hi]` inclusive — `lo` / `hi` carry the
    /// constant's wire form so `Range` covers integers, floats,
    /// and bounded enums.
    Range {
        lo: ConstantValue,
        hi: ConstantValue,
    },
    /// Empty set — the value is provably unreachable.
    Bottom,
}

/// Abstract domain for string values. Powers SAST rules around
/// dynamic-SQL composition + URL / file-path opening.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StringShape {
    /// String is a single literal.
    Literal { value: String },
    /// String is built from `literal_prefix` + a runtime
    /// expression + `literal_suffix`. Either prefix / suffix may
    /// be empty.
    InterpolatedWithFix {
        literal_prefix: String,
        literal_suffix: String,
    },
    /// String is a concat of constants and runtime expressions
    /// with no usable fixed substring on either end.
    FullyOpaque,
    /// String is empty.
    Empty,
}

impl Taint {
    /// True iff the value carries any *live* (uncleansed) taint kind.
    /// `kinds` already excludes anything a bound sanitiser consumed (see the
    /// struct doc), so the alarm is a simple non-emptiness check — no longer
    /// gated on `cleansed_by`, which a sibling cleanse used to satisfy and
    /// thereby mask a concatenated tainted operand (the SEC001 fail-open).
    #[must_use]
    pub fn flags_alarm(&self) -> bool {
        !self.kinds.is_empty()
    }
}

impl ValueSet {
    /// Merge two `ValueSet`s with the lattice join. Top
    /// dominates; Bottom yields the other side; two `OneOf`s
    /// union their value lists.
    #[must_use]
    pub fn join(self, other: ValueSet) -> ValueSet {
        match (self, other) {
            (ValueSet::Top, _) | (_, ValueSet::Top) => ValueSet::Top,
            (ValueSet::Bottom, x) | (x, ValueSet::Bottom) => x,
            (ValueSet::OneOf { mut values }, ValueSet::OneOf { values: other }) => {
                for v in other {
                    if !values.contains(&v) {
                        values.push(v);
                    }
                }
                ValueSet::OneOf { values }
            }
            // Range + OneOf / Range + Range → Top (over-approx).
            // Callers needing tighter joins can specialise.
            _ => ValueSet::Top,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taint_flags_alarm_when_no_cleanser() {
        let t = Taint {
            kinds: vec![TaintKind::UserInput],
            cleansed_by: vec![],
        };
        assert!(t.flags_alarm());
    }

    #[test]
    fn taint_does_not_flag_when_cleansed() {
        // A value sanitised by a bound cleanser carries NO live kind: the
        // cleanser drained the kinds it consumed. `cleansed_by` is retained only
        // for reporting and does not by itself suppress the alarm.
        let t = Taint {
            kinds: vec![],
            cleansed_by: vec![TaintCleanser::DbmsAssert],
        };
        assert!(!t.flags_alarm());
    }

    #[test]
    fn taint_flags_when_live_kind_present_despite_a_recorded_cleanser() {
        // Regression for the SEC001 fail-open: a cleanser recorded somewhere in
        // the derivation must NOT mask a live (uncleansed) kind from a sibling.
        let t = Taint {
            kinds: vec![TaintKind::UserInput],
            cleansed_by: vec![TaintCleanser::DbmsAssert],
        };
        assert!(t.flags_alarm());
    }

    #[test]
    fn taint_default_no_alarm() {
        assert!(!Taint::default().flags_alarm());
    }

    #[test]
    fn value_set_top_dominates_join() {
        let a = ValueSet::Top;
        let b = ValueSet::OneOf {
            values: vec![ConstantValue::Int { value: "1".into() }],
        };
        assert!(matches!(a.join(b), ValueSet::Top));
    }

    #[test]
    fn value_set_bottom_yields_other_side() {
        let a = ValueSet::Bottom;
        let b = ValueSet::OneOf {
            values: vec![ConstantValue::Int { value: "7".into() }],
        };
        match a.join(b) {
            ValueSet::OneOf { values } => {
                assert_eq!(values.len(), 1);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn one_of_join_unions_values_dedup() {
        let a = ValueSet::OneOf {
            values: vec![
                ConstantValue::Int { value: "1".into() },
                ConstantValue::Int { value: "2".into() },
            ],
        };
        let b = ValueSet::OneOf {
            values: vec![
                ConstantValue::Int { value: "2".into() },
                ConstantValue::Int { value: "3".into() },
            ],
        };
        match a.join(b) {
            ValueSet::OneOf { values } => {
                assert_eq!(values.len(), 3);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn range_plus_one_of_widens_to_top() {
        let a = ValueSet::Range {
            lo: ConstantValue::Int { value: "0".into() },
            hi: ConstantValue::Int { value: "10".into() },
        };
        let b = ValueSet::OneOf {
            values: vec![ConstantValue::Int { value: "5".into() }],
        };
        assert!(matches!(a.join(b), ValueSet::Top));
    }

    #[test]
    fn string_shape_variants_serialise_snake_case() {
        let lit = StringShape::Literal {
            value: "hello".into(),
        };
        let json = serde_json::to_string(&lit).unwrap();
        assert!(json.contains("\"kind\":\"literal\""));
        let opaque = StringShape::FullyOpaque;
        assert!(
            serde_json::to_string(&opaque)
                .unwrap()
                .contains("\"fully_opaque\"")
        );
    }

    #[test]
    fn value_flow_default_is_top_no_taint_no_constant() {
        let v = ValueFlow::default();
        assert!(matches!(v.value_set, ValueSet::Top));
        assert!(v.constant.is_none());
        assert!(v.string_shape.is_none());
        assert!(v.taint.kinds.is_empty());
    }

    #[test]
    fn value_flow_serde_round_trip() {
        let v = ValueFlow {
            taint: Taint {
                kinds: vec![TaintKind::UserInput, TaintKind::DynamicSql],
                cleansed_by: vec![TaintCleanser::DbmsAssert],
            },
            constant: Some(ConstantValue::Str {
                value: "hello".into(),
            }),
            value_set: ValueSet::OneOf {
                values: vec![ConstantValue::Int { value: "1".into() }],
            },
            string_shape: Some(StringShape::InterpolatedWithFix {
                literal_prefix: "SELECT * FROM ".into(),
                literal_suffix: " WHERE id = 1".into(),
            }),
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: ValueFlow = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
        assert!(json.contains("\"user_input\""));
        assert!(json.contains("\"dbms_assert\""));
    }
}
