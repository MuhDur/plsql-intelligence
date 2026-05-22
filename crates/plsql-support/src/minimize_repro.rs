//! `plsql support minimize-repro` skeleton (PLSQL-SUPPORT-010).
//!
//! When a customer ships a `SupportBundle` containing a failing
//! input, support needs to whittle it down to the smallest
//! reproducer before filing an upstream bug. This module is the
//! pure planning layer that the CLI wraps; the actual ddmin /
//! delta-debug loop lands in a follow-up bead.
//!
//! The skeleton's job is twofold:
//!
//! 1. **Refuse non-redacted input.** Every input blob we are about
//!    to minimise must carry a positive `redactions_applied` count
//!    OR an explicit `allow_unredacted` flag. The default is to
//!    refuse, preserving the SUPPORT-001 invariant that no
//!    pre-redaction content escapes the customer's machine.
//! 2. **Plan the minimisation.** Walk each input and emit a
//!    `MinimizationPlan` describing the strategy — line-removal,
//!    statement-removal, identifier-renaming — along with a
//!    suggested ordinal for each step. The actual byte-level
//!    minimisation isn't implemented here; the consumer drives the
//!    plan via a separate worker.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{NamedBlob, SupportBundle};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MinimizeError {
    #[error(
        "minimize-repro refused: input {name:?} has no redactions applied; pass --allow-unredacted to override"
    )]
    UnredactedInput { name: String },
    #[error("minimize-repro refused: bundle has no inputs")]
    NoInputs,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinimizationPlan {
    pub schema_id: String,
    pub schema_version: u32,
    /// One entry per input blob; preserves bundle ordering.
    pub inputs: Vec<MinimizationInput>,
    /// Configuration recap so the CLI consumer can echo it in the
    /// audit log without re-deriving anything.
    pub allow_unredacted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinimizationInput {
    pub name: String,
    pub sha256: String,
    pub redactions_applied: usize,
    pub steps: Vec<MinimizationStep>,
}

/// One step in the minimisation plan. Each step has a stable
/// `ordinal` so the worker can resume mid-run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinimizationStep {
    pub ordinal: u32,
    pub strategy: MinimizationStrategy,
    /// Human-readable description of what the step will do.
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinimizationStrategy {
    /// Delete contiguous runs of blank/comment lines.
    StripCommentsAndBlankLines,
    /// Drop top-level statements one at a time, checking that the
    /// failure still reproduces.
    DropStatement,
    /// Drop individual PL/SQL block declarations.
    DropDeclaration,
    /// Rename identifiers to short stable names (a, b, c…).
    RenameIdentifiers,
    /// Final pass: convert remaining literals to short forms.
    ShrinkLiterals,
}

const SCHEMA_ID: &str = "plsql.support.minimize_repro";
const SCHEMA_VERSION: u32 = 1;

/// Plan a minimisation pass over `bundle`. Refuses any input blob
/// whose `redactions_applied == 0` unless `allow_unredacted` is
/// `true`. Returns the plan or the first failing blob's name.
pub fn plan_minimize(
    bundle: &SupportBundle,
    allow_unredacted: bool,
) -> Result<MinimizationPlan, MinimizeError> {
    if bundle.inputs.is_empty() {
        return Err(MinimizeError::NoInputs);
    }
    if !allow_unredacted
        && let Some(blob) = bundle.inputs.iter().find(|b| b.redactions_applied == 0)
    {
        return Err(MinimizeError::UnredactedInput {
            name: blob.name.clone(),
        });
    }

    let inputs = bundle.inputs.iter().map(plan_for_blob).collect();

    Ok(MinimizationPlan {
        schema_id: SCHEMA_ID.into(),
        schema_version: SCHEMA_VERSION,
        inputs,
        allow_unredacted,
    })
}

fn plan_for_blob(blob: &NamedBlob) -> MinimizationInput {
    let steps = vec![
        MinimizationStep {
            ordinal: 1,
            strategy: MinimizationStrategy::StripCommentsAndBlankLines,
            description: "Remove comment-only and blank lines.".into(),
        },
        MinimizationStep {
            ordinal: 2,
            strategy: MinimizationStrategy::DropStatement,
            description: "Drop top-level statements one at a time.".into(),
        },
        MinimizationStep {
            ordinal: 3,
            strategy: MinimizationStrategy::DropDeclaration,
            description: "Drop PL/SQL block declarations one at a time.".into(),
        },
        MinimizationStep {
            ordinal: 4,
            strategy: MinimizationStrategy::RenameIdentifiers,
            description: "Rename remaining identifiers to short stable names.".into(),
        },
        MinimizationStep {
            ordinal: 5,
            strategy: MinimizationStrategy::ShrinkLiterals,
            description: "Convert remaining literals to short canonical forms.".into(),
        },
    ];
    MinimizationInput {
        name: blob.name.clone(),
        sha256: blob.sha256.clone(),
        redactions_applied: blob.redactions_applied,
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RedactionManifest, RedactionRule, SupportBundleBuilder};

    fn rule(pattern: &str) -> RedactionRule {
        RedactionRule {
            name: "schema".into(),
            pattern: pattern.into(),
            replacement: "<X>".into(),
        }
    }

    fn manifest(rules: Vec<RedactionRule>) -> RedactionManifest {
        RedactionManifest { version: 1, rules }
    }

    fn bundle_with(rules: Vec<RedactionRule>, content: &str) -> SupportBundle {
        let mut b = SupportBundleBuilder::new("1.0", "t", manifest(rules));
        b.operator_note("repro").unwrap();
        b.add_input("repro.sql", content);
        b.build()
    }

    #[test]
    fn empty_bundle_rejected() {
        let mut b = SupportBundleBuilder::new("1.0", "t", RedactionManifest::empty());
        b.operator_note("x").unwrap();
        let bundle = b.build();
        let err = plan_minimize(&bundle, false).unwrap_err();
        assert_eq!(err, MinimizeError::NoInputs);
    }

    #[test]
    fn unredacted_input_rejected_by_default() {
        let bundle = bundle_with(vec![], "SELECT * FROM HR.EMPLOYEES");
        let err = plan_minimize(&bundle, false).unwrap_err();
        assert!(matches!(err, MinimizeError::UnredactedInput { name } if name == "repro.sql"));
    }

    #[test]
    fn unredacted_input_allowed_with_override() {
        let bundle = bundle_with(vec![], "SELECT * FROM HR.EMPLOYEES");
        let plan = plan_minimize(&bundle, true).unwrap();
        assert_eq!(plan.inputs.len(), 1);
        assert!(plan.allow_unredacted);
    }

    #[test]
    fn redacted_input_accepted_without_override() {
        let bundle = bundle_with(vec![rule("HR.")], "SELECT * FROM HR.EMPLOYEES");
        let plan = plan_minimize(&bundle, false).unwrap();
        assert_eq!(plan.inputs.len(), 1);
        assert!(!plan.allow_unredacted);
        assert!(plan.inputs[0].redactions_applied > 0);
    }

    #[test]
    fn plan_emits_five_steps_in_canonical_order() {
        let bundle = bundle_with(vec![rule("HR.")], "SELECT * FROM HR.EMPLOYEES");
        let plan = plan_minimize(&bundle, false).unwrap();
        let strategies: Vec<MinimizationStrategy> =
            plan.inputs[0].steps.iter().map(|s| s.strategy).collect();
        assert_eq!(
            strategies,
            vec![
                MinimizationStrategy::StripCommentsAndBlankLines,
                MinimizationStrategy::DropStatement,
                MinimizationStrategy::DropDeclaration,
                MinimizationStrategy::RenameIdentifiers,
                MinimizationStrategy::ShrinkLiterals,
            ]
        );
        for (i, step) in plan.inputs[0].steps.iter().enumerate() {
            assert_eq!(step.ordinal, (i as u32) + 1);
        }
    }

    #[test]
    fn plan_carries_schema_id_and_version() {
        let bundle = bundle_with(vec![rule("HR.")], "HR.X");
        let plan = plan_minimize(&bundle, false).unwrap();
        assert_eq!(plan.schema_id, "plsql.support.minimize_repro");
        assert_eq!(plan.schema_version, 1);
    }

    #[test]
    fn first_unredacted_blob_short_circuits() {
        // Build a bundle with one redacted + one un-redacted
        // input. The plan must refuse on the un-redacted blob.
        let mut b = SupportBundleBuilder::new("1.0", "t", manifest(vec![rule("HR.")]));
        b.operator_note("repro").unwrap();
        b.add_input("a.sql", "SELECT * FROM HR.EMPLOYEES");
        // Second blob has no HR. so no redactions hit it.
        b.add_input("b.sql", "SELECT 1 FROM dual");
        let bundle = b.build();
        let err = plan_minimize(&bundle, false).unwrap_err();
        assert!(matches!(err, MinimizeError::UnredactedInput { name } if name == "b.sql"));
    }

    #[test]
    fn plan_serialises_round_trip() {
        let bundle = bundle_with(vec![rule("HR.")], "HR.X");
        let plan = plan_minimize(&bundle, false).unwrap();
        let json = serde_json::to_string(&plan).unwrap();
        let back: MinimizationPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back, plan);
        // snake_case strategy tag in wire form.
        assert!(json.contains("strip_comments_and_blank_lines"));
    }
}
