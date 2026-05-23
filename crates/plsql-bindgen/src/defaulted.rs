//! `Defaulted<T>` semantics for IN parameters carrying DEFAULT
//! expressions.
//!
//! PL/SQL distinguishes three states for an IN parameter whose
//! declaration ships a `DEFAULT` clause:
//!
//! 1. The caller **omits** the argument entirely — Oracle uses the
//!    declared default expression at call time.
//! 2. The caller passes **`NULL`** — the parameter is bound to NULL,
//!    NOT the default. (This is a frequent source of surprise; the
//!    PL/SQL Language Reference §9.3 spells it out.)
//! 3. The caller passes a concrete **`Value(T)`** — bound directly.
//!
//! `Option<T>` cannot model this three-way distinction: `None` would
//! conflate "omit" and "NULL". We expose a dedicated `Defaulted<T>`
//! enum so the emitted wrappers can mirror PL/SQL semantics exactly.
//!
//! ## Mapping to BG-004 wrapper signatures
//!
//! Any IN / IN OUT parameter whose `ParameterBinding.has_default`
//! is `true` gets `Defaulted<T>` rather than `T` or `Option<T>` in
//! the emitted wrapper. OUT-only parameters never carry defaults
//! (PL/SQL forbids it), so OUT-mode arguments stay as `T`.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference §9.3 —
//!   "Default Values for Subprogram Parameters" governs the
//!   omit-vs-null distinction.
//! * `LOW-LEVEL-CATALOGS.md` Supplied Package Buckets — DBMS_SQL
//!   binding semantics for default vs explicit null pass-through.

use serde::{Deserialize, Serialize};

use crate::{ParameterBinding, ParameterMode};

/// Three-way state for a defaulted IN / IN OUT parameter.
///
/// `Omit` corresponds to Oracle's "use the declared default
/// expression". The wrapper emits no bind variable in this case
/// so the server evaluates the default. `Null` and `Value(T)` are
/// both explicit binds — the wrapper just chooses NULL vs the
/// supplied payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Defaulted<T> {
    Omit,
    Null,
    Value(T),
}

impl<T> Defaulted<T> {
    #[must_use]
    pub fn is_omit(&self) -> bool {
        matches!(self, Self::Omit)
    }

    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    #[must_use]
    pub fn as_value(&self) -> Option<&T> {
        match self {
            Self::Value(t) => Some(t),
            _ => None,
        }
    }

    /// Map the contained value while preserving the omit/null state.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Defaulted<U> {
        match self {
            Self::Omit => Defaulted::Omit,
            Self::Null => Defaulted::Null,
            Self::Value(t) => Defaulted::Value(f(t)),
        }
    }
}

impl<T> From<T> for Defaulted<T> {
    fn from(value: T) -> Self {
        Defaulted::Value(value)
    }
}

impl<T> From<Option<T>> for Defaulted<T> {
    /// Convenience: `Some(t)` → `Value(t)`, `None` → `Null`. Callers
    /// who want `Omit` must construct it explicitly — `Option::None`
    /// is ambiguous and we refuse to guess.
    fn from(value: Option<T>) -> Self {
        match value {
            Some(t) => Defaulted::Value(t),
            None => Defaulted::Null,
        }
    }
}

/// Decide whether a parameter's emitted Rust type should be
/// `Defaulted<T>`. Returns `true` for IN / IN OUT parameters whose
/// `has_default` flag is set. OUT-only parameters always return
/// `false` (PL/SQL forbids DEFAULT on OUT).
#[must_use]
pub fn should_use_defaulted(p: &ParameterBinding) -> bool {
    match p.mode {
        ParameterMode::In | ParameterMode::InOut => p.has_default,
        ParameterMode::Out => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RustTypeRef;

    fn param(mode: ParameterMode, has_default: bool) -> ParameterBinding {
        ParameterBinding {
            name: "p".into(),
            mode,
            rust_type: RustTypeRef {
                path: "i64".into(),
                nullable: false,
            },
            has_default,
        }
    }

    #[test]
    fn defaulted_value_round_trips() {
        let d: Defaulted<i64> = Defaulted::Value(42);
        assert_eq!(d.as_value(), Some(&42));
        assert!(!d.is_omit());
        assert!(!d.is_null());
    }

    #[test]
    fn defaulted_omit_distinguishes_from_null() {
        let omit: Defaulted<i64> = Defaulted::Omit;
        let null: Defaulted<i64> = Defaulted::Null;
        assert!(omit.is_omit());
        assert!(null.is_null());
        assert_ne!(omit, null);
    }

    #[test]
    fn from_t_lifts_to_value() {
        let d: Defaulted<i64> = 7_i64.into();
        assert_eq!(d, Defaulted::Value(7));
    }

    #[test]
    fn from_option_some_maps_to_value_none_maps_to_null() {
        let d1: Defaulted<i64> = Some(5_i64).into();
        let d2: Defaulted<i64> = Option::<i64>::None.into();
        assert_eq!(d1, Defaulted::Value(5));
        // From<Option<T>> deliberately picks Null over Omit — caller
        // who wants Omit must say so explicitly.
        assert_eq!(d2, Defaulted::Null);
    }

    #[test]
    fn map_preserves_state() {
        assert_eq!(Defaulted::Omit::<i64>.map(|t| t + 1), Defaulted::Omit);
        assert_eq!(Defaulted::Null::<i64>.map(|t| t + 1), Defaulted::Null);
        assert_eq!(Defaulted::Value(5_i64).map(|t| t + 1), Defaulted::Value(6));
    }

    #[test]
    fn should_use_defaulted_only_for_in_modes_with_default() {
        assert!(should_use_defaulted(&param(ParameterMode::In, true)));
        assert!(should_use_defaulted(&param(ParameterMode::InOut, true)));
        assert!(!should_use_defaulted(&param(ParameterMode::In, false)));
        assert!(!should_use_defaulted(&param(ParameterMode::InOut, false)));
        // OUT never gets Defaulted even if has_default is true (legal
        // representation but PL/SQL forbids DEFAULT on OUT — the
        // binding plan should not set the flag in practice).
        assert!(!should_use_defaulted(&param(ParameterMode::Out, true)));
    }

    #[test]
    fn serialises_in_three_variants() {
        let v: Defaulted<i64> = Defaulted::Value(5);
        let o: Defaulted<i64> = Defaulted::Omit;
        let n: Defaulted<i64> = Defaulted::Null;
        let jv = serde_json::to_string(&v).unwrap();
        let jo = serde_json::to_string(&o).unwrap();
        let jn = serde_json::to_string(&n).unwrap();
        assert!(jv.contains("Value"), "{jv}");
        assert_eq!(jo, "\"Omit\"");
        assert_eq!(jn, "\"Null\"");
    }
}
