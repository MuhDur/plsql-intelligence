//! Overload identity + call resolution.
//!
//! Oracle allows several subprograms to share a name inside a
//! package (or as standalone routines that a synonym fans into).
//! A call site picks exactly one of them by *overload resolution*:
//! match the supplied actual arguments — positional and/or named,
//! with defaults filling the gaps — against each candidate's formal
//! parameter list, then accept the unique survivor (PLS-00307 if
//! more than one survives, no match if none).
//!
//! This module is input-agnostic: a [`RoutineSignature`] can be
//! built from the IR [`DeclTable`] ([`RoutineSignature::from_decl`])
//! *or* assembled from catalog metadata (`ALL_ARGUMENTS`) by a
//! caller — the resolver only sees the normalized signature shape,
//! so source-only and catalog-derived overloads resolve through the
//! same code path.
//!
//! ## Oracle rules implemented
//!
//! * Positional actuals bind left-to-right; named actuals bind by
//!   formal name. A named actual may not be followed by a
//!   positional one (PLS-00312) — enforced.
//! * A formal with no actual must have a default, else the
//!   candidate is rejected.
//! * Supplying the same formal both positionally and by name, or a
//!   named actual that names no formal, rejects the candidate.
//! * More actuals than formals rejects the candidate.
//! * Type compatibility is treated *conservatively*: a known actual
//!   type that differs (case-insensitively) from a known formal
//!   type rejects the candidate; an unknown type on either side is
//!   permissive (static analysis cannot prove Oracle's implicit
//!   conversions, so we neither invent a match nor a mismatch).
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — "Overloads
//!   of PL/SQL Subprograms" and "Subprogram Parameter Passing"
//!   chapters govern the binding order and ambiguity rule.
//! * `LOW-LEVEL-CATALOGS.md` — `ALL_ARGUMENTS` (`ARGUMENT_NAME`,
//!   `POSITION`, `IN_OUT`, `DATA_TYPE`, `DEFAULTED`) is the
//!   server-side mirror a catalog-derived [`RoutineSignature`]
//!   is assembled from.

use serde::{Deserialize, Serialize};

use plsql_core::SymbolInterner;
use plsql_ir::{DeclId, Declaration, ParamMode, TypeRef};

use crate::DeclTable;

/// One formal parameter in a candidate's signature.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParamSig {
    /// Formal name, upper-cased for case-insensitive named binding.
    pub name: String,
    pub mode: ParamMode,
    /// Declared type, upper-cased. `None` when the type could not
    /// be lifted (anchored `%TYPE` not yet resolved, etc.).
    pub type_name: Option<String>,
    /// Whether the formal has a default expression.
    pub has_default: bool,
}

/// A resolvable subprogram signature. Built from IR or catalog.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutineSignature {
    pub decl: DeclId,
    /// Subprogram name, upper-cased.
    pub name: String,
    pub params: Vec<ParamSig>,
    /// `true` for a function (has a return type), `false` for a
    /// procedure. Overload sets never mix the two for a given call
    /// context, but the flag lets callers filter.
    pub is_function: bool,
}

impl RoutineSignature {
    /// Lift a [`RoutineSignature`] from a procedure/function decl in
    /// `table`. Returns `None` if `decl` is not a subprogram.
    #[must_use]
    pub fn from_decl(table: &DeclTable, interner: &SymbolInterner, decl: DeclId) -> Option<Self> {
        let d = table.get(decl)?;
        let (param_ids, is_function) = match d {
            Declaration::Procedure(p) => (&p.params, false),
            Declaration::Function(f) => (&f.params, true),
            _ => return None,
        };
        let name = interner
            .resolve(d.common().name)
            .unwrap_or_default()
            .to_ascii_uppercase();
        let mut params = Vec::with_capacity(param_ids.len());
        for pid in param_ids {
            let Some(Declaration::Param(pd)) = table.get(*pid) else {
                // A non-param child of a routine's param list is a
                // malformed IR; skip it rather than mis-binding.
                continue;
            };
            params.push(ParamSig {
                name: interner
                    .resolve(pd.common.name)
                    .unwrap_or_default()
                    .to_ascii_uppercase(),
                mode: pd.mode,
                type_name: type_name_of(pd.ty.as_ref()),
                has_default: pd.default_text.is_some(),
            });
        }
        Some(Self {
            decl,
            name,
            params,
            is_function,
        })
    }
}

fn type_name_of(ty: Option<&TypeRef>) -> Option<String> {
    match ty {
        Some(TypeRef::Unresolved(s)) => Some(s.trim().to_ascii_uppercase()),
        Some(TypeRef::Anchored(a)) => Some(a.raw.trim().to_ascii_uppercase()),
        None => None,
    }
}

/// One actual argument at a call site.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CallArg {
    /// `Some(formal_name)` for `=> ` named notation, `None` for a
    /// positional actual. Names are matched case-insensitively.
    pub name: Option<String>,
    /// Best-effort static type of the actual, upper-cased. `None`
    /// when the analyser cannot infer it.
    pub type_name: Option<String>,
}

impl CallArg {
    #[must_use]
    pub fn positional(type_name: Option<&str>) -> Self {
        Self {
            name: None,
            type_name: type_name.map(|s| s.trim().to_ascii_uppercase()),
        }
    }

    #[must_use]
    pub fn named(name: &str, type_name: Option<&str>) -> Self {
        Self {
            name: Some(name.trim().to_ascii_uppercase()),
            type_name: type_name.map(|s| s.trim().to_ascii_uppercase()),
        }
    }
}

/// Why a single candidate failed to bind the actuals.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindFailure {
    /// More actuals than formals.
    TooManyArguments,
    /// A positional actual appeared after a named one.
    PositionalAfterNamed,
    /// A named actual referenced no formal of this candidate.
    UnknownParameterName,
    /// A formal received an actual both positionally and by name.
    DuplicateBinding,
    /// A formal received no actual and has no default.
    MissingRequiredArgument,
    /// A known actual type contradicts a known formal type.
    TypeMismatch,
}

/// Outcome of resolving a call against a candidate set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum OverloadResolution {
    /// Exactly one candidate bound. `binding[i]` is the actual index
    /// bound to formal `i`, or `None` when the formal took its
    /// default.
    Resolved {
        decl: DeclId,
        binding: Vec<Option<usize>>,
    },
    /// More than one candidate bound — PLS-00307 territory.
    Ambiguous { candidates: Vec<DeclId> },
    /// No candidate bound. Per-candidate failure reasons, in
    /// candidate order, for diagnostics.
    NoMatch { reasons: Vec<BindFailure> },
}

/// Try to bind `args` to one `sig`. `Ok(binding)` maps each formal
/// to the actual index that filled it (`None` = default used).
fn bind_candidate(
    sig: &RoutineSignature,
    args: &[CallArg],
) -> Result<Vec<Option<usize>>, BindFailure> {
    if args.len() > sig.params.len() {
        return Err(BindFailure::TooManyArguments);
    }
    let mut binding: Vec<Option<usize>> = vec![None; sig.params.len()];
    let mut seen_named = false;

    for (arg_idx, arg) in args.iter().enumerate() {
        let formal_idx = match &arg.name {
            None => {
                if seen_named {
                    return Err(BindFailure::PositionalAfterNamed);
                }
                arg_idx
            }
            Some(n) => {
                seen_named = true;
                match sig.params.iter().position(|p| &p.name == n) {
                    Some(i) => i,
                    None => return Err(BindFailure::UnknownParameterName),
                }
            }
        };
        if binding[formal_idx].is_some() {
            return Err(BindFailure::DuplicateBinding);
        }
        if !types_compatible(
            arg.type_name.as_deref(),
            sig.params[formal_idx].type_name.as_deref(),
        ) {
            return Err(BindFailure::TypeMismatch);
        }
        binding[formal_idx] = Some(arg_idx);
    }

    for (i, slot) in binding.iter().enumerate() {
        if slot.is_none() && !sig.params[i].has_default {
            return Err(BindFailure::MissingRequiredArgument);
        }
    }
    Ok(binding)
}

/// Conservative type check: unknown on either side is permissive;
/// two known types must match case-insensitively.
fn types_compatible(actual: Option<&str>, formal: Option<&str>) -> bool {
    match (actual, formal) {
        (Some(a), Some(f)) => a.eq_ignore_ascii_case(f),
        _ => true,
    }
}

/// Resolve a call described by `args` against `candidates` (all
/// sharing the call's name; the caller is responsible for gathering
/// the overload set). Deterministic and `O(candidates * args)`.
#[must_use]
pub fn resolve_overload(candidates: &[RoutineSignature], args: &[CallArg]) -> OverloadResolution {
    let mut bound: Vec<(DeclId, Vec<Option<usize>>)> = Vec::new();
    let mut reasons: Vec<BindFailure> = Vec::with_capacity(candidates.len());

    for c in candidates {
        match bind_candidate(c, args) {
            Ok(binding) => bound.push((c.decl, binding)),
            Err(reason) => reasons.push(reason),
        }
    }

    match bound.len() {
        0 => OverloadResolution::NoMatch { reasons },
        1 => {
            let (decl, binding) = bound.into_iter().next().unwrap();
            OverloadResolution::Resolved { decl, binding }
        }
        _ => OverloadResolution::Ambiguous {
            candidates: bound.into_iter().map(|(d, _)| d).collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(
        decl: u64,
        name: &str,
        params: &[(&str, ParamMode, Option<&str>, bool)],
    ) -> RoutineSignature {
        RoutineSignature {
            decl: DeclId::new(decl),
            name: name.to_ascii_uppercase(),
            params: params
                .iter()
                .map(|(n, m, t, d)| ParamSig {
                    name: n.to_ascii_uppercase(),
                    mode: *m,
                    type_name: t.map(|s| s.to_ascii_uppercase()),
                    has_default: *d,
                })
                .collect(),
            is_function: false,
        }
    }

    #[test]
    fn unique_positional_match_resolves() {
        let c = vec![sig(
            1,
            "POST",
            &[("AMOUNT", ParamMode::In, Some("NUMBER"), false)],
        )];
        let r = resolve_overload(&c, &[CallArg::positional(Some("NUMBER"))]);
        match r {
            OverloadResolution::Resolved { decl, binding } => {
                assert_eq!(decl, DeclId::new(1));
                assert_eq!(binding, vec![Some(0)]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn arity_distinguishes_overloads() {
        let c = vec![
            sig(1, "F", &[("A", ParamMode::In, None, false)]),
            sig(
                2,
                "F",
                &[
                    ("A", ParamMode::In, None, false),
                    ("B", ParamMode::In, None, false),
                ],
            ),
        ];
        let r = resolve_overload(&c, &[CallArg::positional(None), CallArg::positional(None)]);
        assert!(matches!(
            r,
            OverloadResolution::Resolved { decl, .. } if decl == DeclId::new(2)
        ));
    }

    #[test]
    fn named_notation_binds_by_name() {
        let c = vec![sig(
            7,
            "G",
            &[
                ("FIRST", ParamMode::In, None, false),
                ("SECOND", ParamMode::In, None, false),
            ],
        )];
        let r = resolve_overload(
            &c,
            &[
                CallArg::named("SECOND", None),
                CallArg::named("first", None),
            ],
        );
        match r {
            OverloadResolution::Resolved { binding, .. } => {
                // FIRST <- arg 1, SECOND <- arg 0.
                assert_eq!(binding, vec![Some(1), Some(0)]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn defaulted_formal_may_be_omitted() {
        let c = vec![sig(
            1,
            "H",
            &[
                ("A", ParamMode::In, None, false),
                ("B", ParamMode::In, Some("NUMBER"), true),
            ],
        )];
        let r = resolve_overload(&c, &[CallArg::positional(None)]);
        match r {
            OverloadResolution::Resolved { binding, .. } => {
                assert_eq!(binding, vec![Some(0), None]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn missing_required_arg_is_no_match() {
        let c = vec![sig(1, "H", &[("A", ParamMode::In, None, false)])];
        let r = resolve_overload(&c, &[]);
        match r {
            OverloadResolution::NoMatch { reasons } => {
                assert_eq!(reasons, vec![BindFailure::MissingRequiredArgument]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn positional_after_named_rejected() {
        let c = vec![sig(
            1,
            "H",
            &[
                ("A", ParamMode::In, None, true),
                ("B", ParamMode::In, None, true),
            ],
        )];
        let r = resolve_overload(&c, &[CallArg::named("A", None), CallArg::positional(None)]);
        assert!(matches!(
            r,
            OverloadResolution::NoMatch { ref reasons }
                if reasons == &vec![BindFailure::PositionalAfterNamed]
        ));
    }

    #[test]
    fn unknown_named_parameter_rejected() {
        let c = vec![sig(1, "H", &[("A", ParamMode::In, None, true)])];
        let r = resolve_overload(&c, &[CallArg::named("NOPE", None)]);
        assert!(matches!(
            r,
            OverloadResolution::NoMatch { ref reasons }
                if reasons == &vec![BindFailure::UnknownParameterName]
        ));
    }

    #[test]
    fn duplicate_binding_rejected() {
        let c = vec![sig(
            1,
            "H",
            &[
                ("A", ParamMode::In, None, false),
                ("B", ParamMode::In, None, true),
            ],
        )];
        // Positional fills A (idx 0), then named "A" targets it again.
        let r = resolve_overload(&c, &[CallArg::positional(None), CallArg::named("A", None)]);
        assert!(matches!(
            r,
            OverloadResolution::NoMatch { ref reasons }
                if reasons == &vec![BindFailure::DuplicateBinding]
        ));
    }

    #[test]
    fn too_many_arguments_rejected() {
        let c = vec![sig(1, "H", &[("A", ParamMode::In, None, false)])];
        let r = resolve_overload(&c, &[CallArg::positional(None), CallArg::positional(None)]);
        assert!(matches!(
            r,
            OverloadResolution::NoMatch { ref reasons }
                if reasons == &vec![BindFailure::TooManyArguments]
        ));
    }

    #[test]
    fn type_mismatch_disqualifies_but_unknown_is_permissive() {
        let c = vec![sig(1, "H", &[("A", ParamMode::In, Some("DATE"), false)])];
        // Known mismatch → rejected.
        assert!(matches!(
            resolve_overload(&c, &[CallArg::positional(Some("NUMBER"))]),
            OverloadResolution::NoMatch { .. }
        ));
        // Unknown actual type → permissive, resolves.
        assert!(matches!(
            resolve_overload(&c, &[CallArg::positional(None)]),
            OverloadResolution::Resolved { .. }
        ));
    }

    #[test]
    fn type_disambiguates_same_arity_overloads() {
        let c = vec![
            sig(10, "CONV", &[("V", ParamMode::In, Some("NUMBER"), false)]),
            sig(11, "CONV", &[("V", ParamMode::In, Some("VARCHAR2"), false)]),
        ];
        let r = resolve_overload(&c, &[CallArg::positional(Some("varchar2"))]);
        assert!(matches!(
            r,
            OverloadResolution::Resolved { decl, .. } if decl == DeclId::new(11)
        ));
    }

    #[test]
    fn genuinely_ambiguous_call_reports_all_survivors() {
        // Two identical signatures, untyped actual → both bind.
        let c = vec![
            sig(1, "AMB", &[("A", ParamMode::In, None, false)]),
            sig(2, "AMB", &[("A", ParamMode::In, None, false)]),
        ];
        let r = resolve_overload(&c, &[CallArg::positional(None)]);
        match r {
            OverloadResolution::Ambiguous { candidates } => {
                assert_eq!(candidates, vec![DeclId::new(1), DeclId::new(2)]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn serde_round_trip_resolution() {
        let r = OverloadResolution::Resolved {
            decl: DeclId::new(3),
            binding: vec![Some(0), None],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"outcome\":\"resolved\""));
        let back: OverloadResolution = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    // --- Property / fuzz (deterministic, no proptest dependency) ---

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn upto(&mut self, n: u64) -> u64 {
            if n == 0 { 0 } else { self.next() % n }
        }
    }

    #[test]
    fn resolve_overload_never_panics_and_binding_is_well_formed() {
        let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
        let names = ["A", "B", "C", "D"];
        let types = [None, Some("NUMBER"), Some("VARCHAR2"), Some("DATE")];

        for _ in 0..5000 {
            // 0-3 candidate signatures, each with 0-4 params.
            let cand_n = rng.upto(4);
            let mut cands = Vec::new();
            for c in 0..cand_n {
                let pn = rng.upto(5) as usize;
                let params: Vec<ParamSig> = (0..pn)
                    .map(|i| ParamSig {
                        name: names[i % names.len()].into(),
                        mode: ParamMode::In,
                        type_name: types[(rng.upto(4)) as usize].map(str::to_string),
                        has_default: rng.upto(2) == 0,
                    })
                    .collect();
                cands.push(RoutineSignature {
                    decl: DeclId::new(c),
                    name: "F".into(),
                    params,
                    is_function: false,
                });
            }
            // 0-5 actuals, mixed positional/named.
            let an = rng.upto(6) as usize;
            let args: Vec<CallArg> = (0..an)
                .map(|_| {
                    let ty = types[(rng.upto(4)) as usize];
                    if rng.upto(2) == 0 {
                        CallArg::positional(ty)
                    } else {
                        CallArg::named(names[(rng.upto(4)) as usize], ty)
                    }
                })
                .collect();

            // Must never panic.
            let r = resolve_overload(&cands, &args);

            // Invariant: a Resolved binding has exactly one entry per
            // formal, every Some(idx) is a valid actual index, and no
            // two formals bind the same actual.
            if let OverloadResolution::Resolved { decl, binding } = r {
                let sig = cands.iter().find(|c| c.decl == decl).expect("decl exists");
                assert_eq!(binding.len(), sig.params.len());
                let mut used = std::collections::BTreeSet::new();
                for slot in binding.iter().flatten() {
                    assert!(*slot < args.len(), "binding index out of range");
                    assert!(used.insert(*slot), "actual bound to two formals");
                }
            }
        }
    }
}
