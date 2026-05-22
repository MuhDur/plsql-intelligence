//! Overload disambiguation for emitted wrappers (PLSQL-BG-007).
//!
//! PL/SQL packages let two subprograms share a name as long as
//! their parameter signatures differ ("overloading"). Rust does
//! not — each `pub fn` must be unique within its module. This
//! module assigns a deterministic, parameter-name-based suffix to
//! every overload so the emitted bindings keep all callers
//! reachable from Rust.
//!
//! ## Strategy
//!
//! 1. Group `RoutineBinding`s by their PL/SQL name.
//! 2. For singletons, leave the Rust name alone.
//! 3. For groups of two or more:
//!    * compute the longest common parameter-name prefix among the
//!      group, then drop it (it is the same across overloads, so it
//!      carries no disambiguating signal).
//!    * for each overload, build a suffix from the remaining
//!      parameter names joined by `_`.
//!    * append `_<suffix>` to the routine name.
//!
//! Parameter names are case-folded and snake-cased before suffix
//! assembly so the resulting Rust name is stable across PL/SQL
//! identifier-quoting variations.
//!
//! ## Why parameter names, not types
//!
//! Type-based overloading (the more common Rust convention) breaks
//! down on PL/SQL: many overload pairs differ only by mode
//! (`IN VARCHAR2` vs `IN OUT VARCHAR2`) or by an Oracle-specific
//! attribute (`NUMBER(5)` vs `NUMBER(10)`) that Rust collapses to
//! the same type. Parameter names are guaranteed unique within a
//! single routine and meaningful to the operator reading the
//! generated code.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference routing —
//!   PL/SQL §9.5 "Overloading Subprograms" governs the rules for
//!   when two routines share a name.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families — the
//!   overload set comes from `ALL_PROCEDURES.OVERLOAD` (a single
//!   string per overload slot, 1-based).

use std::collections::BTreeMap;

use crate::RoutineBinding;

/// Apply overload-suffixing to `routines` in place. Returns the
/// per-routine assigned Rust identifier in the same order as the
/// input slice; callers (the emitter) use the indices to drive
/// `RoutineBinding.name` substitution.
#[must_use]
pub fn disambiguate_overloads(routines: &[RoutineBinding]) -> Vec<String> {
    // Bucket indices by lowercase PL/SQL name.
    let mut buckets: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, r) in routines.iter().enumerate() {
        buckets
            .entry(r.name.to_ascii_lowercase())
            .or_default()
            .push(idx);
    }

    let mut out: Vec<String> = vec![String::new(); routines.len()];
    for (base_name, indices) in buckets {
        if indices.len() == 1 {
            let only = indices[0];
            out[only] = routines[only].name.clone();
            continue;
        }
        let suffixes = assemble_suffixes(routines, &indices);
        for (i, idx) in indices.iter().enumerate() {
            let suffix = &suffixes[i];
            if suffix.is_empty() {
                // Two zero-parameter overloads collide and we have
                // nothing to disambiguate on — fall back to an
                // ordinal suffix so the emitter never produces two
                // identical Rust names.
                out[*idx] = format!("{base_name}_{}", i + 1);
            } else {
                out[*idx] = format!("{base_name}_{suffix}");
            }
        }
    }
    out
}

/// Compute the disambiguating suffix for each overload in the
/// supplied group of routines (indexed into `routines`). The suffix
/// is the snake-case join of parameter names *not* shared with
/// every other overload, so two routines differing in just one
/// argument name produce a tight suffix.
fn assemble_suffixes(routines: &[RoutineBinding], indices: &[usize]) -> Vec<String> {
    // Each overload's snake_case parameter-name list.
    let groups: Vec<Vec<String>> = indices
        .iter()
        .map(|i| {
            routines[*i]
                .parameters
                .iter()
                .map(|p| snake_case(&p.name))
                .collect()
        })
        .collect();

    // The shared prefix of names that appear at the same index in
    // every overload — these carry no signal and get dropped.
    let shared_prefix_len = shared_prefix_length(&groups);

    groups
        .iter()
        .map(|names| {
            let tail: Vec<&str> = names
                .iter()
                .skip(shared_prefix_len)
                .map(String::as_str)
                .collect();
            tail.join("_")
        })
        .collect()
}

/// Longest index prefix where every group agrees on the parameter
/// name. Empty groups contribute 0.
fn shared_prefix_length(groups: &[Vec<String>]) -> usize {
    let Some(min_len) = groups.iter().map(Vec::len).min() else {
        return 0;
    };
    let mut prefix = 0_usize;
    'outer: for i in 0..min_len {
        let first = &groups[0][i];
        for g in &groups[1..] {
            if g[i] != *first {
                break 'outer;
            }
        }
        prefix += 1;
    }
    prefix
}

fn snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_under = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_under = false;
        } else if !prev_under {
            out.push('_');
            prev_under = true;
        }
    }
    // Trim leading / trailing underscores produced by stray
    // non-alnum characters at the edges.
    out.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ParameterBinding, ParameterMode, RoutineKind, RustTypeRef};

    fn rb(name: &str, params: Vec<&str>) -> RoutineBinding {
        RoutineBinding {
            name: name.into(),
            kind: RoutineKind::Procedure,
            parameters: params
                .into_iter()
                .map(|p| ParameterBinding {
                    name: p.into(),
                    mode: ParameterMode::In,
                    rust_type: RustTypeRef {
                        path: "i64".into(),
                        nullable: false,
                    },
                    has_default: false,
                })
                .collect(),
            return_type: None,
            autonomous_transaction: false,
        }
    }

    #[test]
    fn singletons_are_left_alone() {
        let routines = vec![rb("foo", vec!["p_id"]), rb("bar", vec!["p_name"])];
        let names = disambiguate_overloads(&routines);
        assert_eq!(names, vec!["foo", "bar"]);
    }

    #[test]
    fn two_overloads_get_param_name_suffixes() {
        let routines = vec![rb("hire", vec!["p_emp_id"]), rb("hire", vec!["p_emp_name"])];
        let names = disambiguate_overloads(&routines);
        // Common prefix length is 0 (first param name differs).
        assert_eq!(names, vec!["hire_p_emp_id", "hire_p_emp_name"]);
    }

    #[test]
    fn shared_prefix_dropped_from_suffix() {
        let routines = vec![
            rb("update_emp", vec!["p_emp_id", "p_dept_id"]),
            rb("update_emp", vec!["p_emp_id", "p_salary"]),
        ];
        let names = disambiguate_overloads(&routines);
        // p_emp_id is shared at index 0 → dropped. Suffix is the
        // tail name only.
        assert_eq!(names, vec!["update_emp_p_dept_id", "update_emp_p_salary"]);
    }

    #[test]
    fn three_way_overload_keeps_all_suffixes_unique() {
        let routines = vec![
            rb("compute", vec!["p_in_1"]),
            rb("compute", vec!["p_in_2"]),
            rb("compute", vec!["p_in_3"]),
        ];
        let names = disambiguate_overloads(&routines);
        let unique: std::collections::BTreeSet<_> = names.iter().collect();
        assert_eq!(unique.len(), 3, "{names:?}");
    }

    #[test]
    fn zero_param_collision_falls_back_to_ordinal() {
        let routines = vec![rb("touch", vec![]), rb("touch", vec![])];
        let names = disambiguate_overloads(&routines);
        assert_eq!(names, vec!["touch_1", "touch_2"]);
    }

    #[test]
    fn snake_case_normalises_mixed_case_and_quotes() {
        assert_eq!(snake_case("P_EMP_ID"), "p_emp_id");
        assert_eq!(snake_case("\"P_QUOTED\""), "p_quoted");
        assert_eq!(snake_case("p Emp Id"), "p_emp_id");
    }

    #[test]
    fn case_insensitive_grouping_collapses_capitalisation() {
        let routines = vec![rb("HIRE", vec!["p_emp_id"]), rb("hire", vec!["p_emp_name"])];
        let names = disambiguate_overloads(&routines);
        // Both end up grouped under "hire" → both get suffixes.
        assert!(names[0].starts_with("hire_") && names[1].starts_with("hire_"));
        assert_ne!(names[0], names[1]);
    }

    #[test]
    fn assigned_names_are_stable_under_input_order_within_a_group() {
        // Same routines fed in different orders must produce the
        // same name-set (positions match the input order).
        let a = vec![rb("hire", vec!["p_a"]), rb("hire", vec!["p_b"])];
        let b = vec![rb("hire", vec!["p_b"]), rb("hire", vec!["p_a"])];
        let na = disambiguate_overloads(&a);
        let nb = disambiguate_overloads(&b);
        assert_eq!(na[0], "hire_p_a");
        assert_eq!(na[1], "hire_p_b");
        assert_eq!(nb[0], "hire_p_b");
        assert_eq!(nb[1], "hire_p_a");
    }
}
