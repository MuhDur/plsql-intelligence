#![forbid(unsafe_code)]

use plsql_engine::InvocationClosureV1;

/// Builds the package wrapper used by the package-lowering fuzz target.
#[must_use]
pub fn package_source(body: &str) -> String {
    format!(
        "CREATE OR REPLACE PACKAGE fuzz_pkg AS
           PROCEDURE run(p_value NUMBER DEFAULT 7);
         END fuzz_pkg;
         /
         CREATE OR REPLACE PACKAGE BODY fuzz_pkg AS
           g_value NUMBER := 3;
           PROCEDURE run(p_value NUMBER DEFAULT 7) IS
           BEGIN
             NULL;
             {body}
           END run;
         BEGIN
           NULL;
         END fuzz_pkg;
         /"
    )
}

/// Checks properties that must hold for an effects-extraction fuzz input.
///
/// A parser error makes the source incomplete, so no invocation closure from
/// that source may claim to be clean. The harness also includes a known
/// executable statement and therefore must not produce an empty effect set.
pub fn check_effects_invariants(
    closure: &InvocationClosureV1,
    has_parse_diagnostics: bool,
) -> Result<(), &'static str> {
    if has_parse_diagnostics && closure.clean {
        return Err("parse diagnostics produced a clean invocation closure");
    }
    if closure.effects.effects.is_empty() {
        return Err("routine body with an executable statement has no effects");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_effects_invariants;
    use plsql_engine::InvocationClosureV1;

    #[test]
    fn fuzz_regression_diagnostics_never_allow_a_clean_closure() {
        let known_bad: InvocationClosureV1 = serde_json::from_value(serde_json::json!({
            "contract": "InvocationClosureV1",
            "root": "FUZZ_ROOT",
            "units": ["FUZZ_ROOT"],
            "effects": {
                "contract": "RoutineEffectsV1",
                "effects": ["Ddl"]
            },
            "clean": true,
            "unknown_reasons": []
        }))
        .expect("known-bad closure fixture must deserialize");

        assert_eq!(
            check_effects_invariants(&known_bad, true),
            Err("parse diagnostics produced a clean invocation closure")
        );
    }
}
