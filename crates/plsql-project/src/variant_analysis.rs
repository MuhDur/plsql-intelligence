//! Variant-analysis mode for conditional-compilation regions.
//!
//! The base "selected source" view runs a single profile through a
//! single preprocessor pass. The variant view complements it —
//! every feasible combination of conditional branches gets its own
//! selected source, so downstream lineage / impact analysis can
//! report branch-specific edges.
//!
//! ## Strategy
//!
//! 1. Walk the source once to discover the directive frames (one
//!    per `$IF` block) and the branch labels they contain
//!    (`$THEN` body, each `$ELSIF` arm, `$ELSE`). The labels are
//!    deterministic — a frame with `$IF` + `$ELSIF` + `$ELSE` has
//!    three arms numbered `branch=0..=2`.
//! 2. Enumerate the Cartesian product across all frames. For each
//!    combination, run `preprocess` with a synthetic
//!    [`AnalysisProfile`] that exactly selects that combination.
//! 3. Return a [`VariantReport`] carrying one [`VariantSelection`]
//!    per combination: the synthetic profile description, the
//!    selected source, and the chosen-branch index per frame.
//!
//! ## Limits
//!
//! Variant analysis blows up exponentially. We cap the total number
//! of variants at `MAX_VARIANTS` (default 32). If the source has
//! more frames than the cap allows, the report flags the truncation
//! via `truncated` so consumers know they're seeing a sample, not
//! the complete enumeration.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference routing —
//!   the Conditional Compilation chapter is the authority for
//!   directive semantics; variant analysis is the source-only
//!   complement to `DBMS_PREPROCESSOR.GET_POST_PROCESSED_SOURCE`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::preprocess::{AnalysisProfile, PreprocessedSource, preprocess};

/// Maximum number of variants `analyse_variants` will enumerate.
/// 32 covers up to 5 binary frames or 3 ternary frames — plenty for
/// the corpus shapes the engine targets while still bounding the
/// analysis time.
pub const MAX_VARIANTS: usize = 32;

/// One concrete variant: a single preprocessor run with an explicit
/// branch choice per discovered conditional frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantSelection {
    /// Branch indices, one per frame in source order. `0` is the
    /// `$THEN` arm; `1..n-1` are `$ELSIF` arms; the final index
    /// (if present) is `$ELSE`.
    pub branch_choices: Vec<u32>,
    /// Synthetic AnalysisProfile that selects this combination.
    pub profile: AnalysisProfile,
    /// Post-preprocess output for this combination.
    pub source: PreprocessedSource,
}

/// Discovered frame: directive line + arm labels we'll iterate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameInfo {
    /// 1-based source line of the `$IF`.
    pub if_line: u32,
    /// Number of branches in the chain (1 `$IF` + N-1 `$ELSIF`,
    /// plus 1 if the chain ends with `$ELSE`).
    pub branch_count: u32,
    /// Verbatim expression text for each branch in declaration
    /// order. The last entry is the literal string `"$ELSE"` if
    /// the chain ends with one.
    pub branch_expressions: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantReport {
    pub frames: Vec<FrameInfo>,
    pub variants: Vec<VariantSelection>,
    /// `true` when the Cartesian product was capped at
    /// [`MAX_VARIANTS`] — only a prefix of variants was emitted.
    pub truncated: bool,
}

/// Enumerate every feasible variant of `source` and run the
/// preprocessor against each. The synthetic profile sets a unique
/// flag (`__variant_frame_<n> = <branch>`) per frame; the frame's
/// directive text gets rewritten via a small variant-rewriting pass
/// before each variant's preprocess call.
#[must_use]
pub fn analyse_variants(source: &str) -> VariantReport {
    let frames = discover_frames(source);
    let mut report = VariantReport {
        frames: frames.clone(),
        ..VariantReport::default()
    };

    if frames.is_empty() {
        // No directives → one variant: the original source.
        report.variants.push(VariantSelection {
            branch_choices: vec![],
            profile: AnalysisProfile::default(),
            source: preprocess(source, &AnalysisProfile::default()),
        });
        return report;
    }

    // Cap the variant explosion. We compute the product carefully so
    // we don't overflow.
    let mut total: usize = 1;
    for frame in &frames {
        total = total.saturating_mul(frame.branch_count as usize);
        if total > MAX_VARIANTS {
            report.truncated = true;
            total = MAX_VARIANTS;
            break;
        }
    }

    let mut combo = vec![0_u32; frames.len()];
    for _ in 0..total {
        let rewritten = rewrite_for_combo(source, &frames, &combo);
        let profile = synthetic_profile(&combo);
        let variant = preprocess(&rewritten, &profile);
        report.variants.push(VariantSelection {
            branch_choices: combo.clone(),
            profile,
            source: variant,
        });
        if !increment_combo(&mut combo, &frames) {
            break;
        }
    }

    report
}

/// Walk `source` once to discover every `$IF` block plus the arms
/// it spans. The discovery pass is independent of the preprocessor
/// so the variant analyser does not have to re-parse the directive
/// shape per variant.
fn discover_frames(source: &str) -> Vec<FrameInfo> {
    let mut frames: Vec<FrameInfo> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    for (idx, raw) in source.split_inclusive('\n').enumerate() {
        let line_no = (idx as u32) + 1;
        let trimmed = raw.trim_start();
        let upper = trimmed.to_ascii_uppercase();

        if upper.starts_with("$IF ") || upper == "$IF" {
            let expr = strip_directive(trimmed, "$IF");
            frames.push(FrameInfo {
                if_line: line_no,
                branch_count: 1,
                branch_expressions: vec![expr],
            });
            stack.push(frames.len() - 1);
        } else if upper.starts_with("$ELSIF ") {
            if let Some(&idx) = stack.last() {
                let expr = strip_directive(trimmed, "$ELSIF");
                frames[idx].branch_count += 1;
                frames[idx].branch_expressions.push(expr);
            }
        } else if upper.starts_with("$ELSE") {
            if let Some(&idx) = stack.last() {
                frames[idx].branch_count += 1;
                frames[idx].branch_expressions.push("$ELSE".into());
            }
        } else if upper.starts_with("$END") {
            stack.pop();
        }
    }
    frames
}

fn strip_directive(line: &str, kw: &str) -> String {
    let trimmed = line.trim();
    let rest = trimmed.get(kw.len()..).unwrap_or("").trim();
    let upper = rest.to_ascii_uppercase();
    if let Some(idx) = upper.rfind("$THEN") {
        rest[..idx].trim().to_string()
    } else {
        rest.to_string()
    }
}

/// Build the synthetic AnalysisProfile for the given combination.
/// Each frame contributes a key `__variant_frame_<idx>` whose value
/// is the selected branch index — the rewriter below rewrites
/// every `$IF` chain to compare against these synthetic keys, so
/// the standard preprocessor can drive variant selection.
fn synthetic_profile(combo: &[u32]) -> AnalysisProfile {
    let mut flags = BTreeMap::new();
    for (i, b) in combo.iter().enumerate() {
        flags.insert(format!("__variant_frame_{i}"), b.to_string());
    }
    AnalysisProfile {
        plsql_ccflags: flags,
        ..AnalysisProfile::default()
    }
}

/// Rewrite the source so each frame's `$IF`/`$ELSIF`/`$ELSE` chain
/// tests the synthetic frame key against the chosen branch index.
/// The rewriter is intentionally line-scoped so it preserves line
/// numbers — every directive line is replaced by another directive
/// of the same shape.
fn rewrite_for_combo(source: &str, frames: &[FrameInfo], _combo: &[u32]) -> String {
    let mut out = String::with_capacity(source.len() + 256);
    let mut stack: Vec<usize> = Vec::new();
    let mut next_frame_idx: usize = 0;
    let mut arm_in_frame: Vec<u32> = vec![0; frames.len()];

    for raw in source.split_inclusive('\n') {
        let trimmed = raw.trim_start();
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("$IF ") || upper == "$IF" {
            let idx = next_frame_idx;
            next_frame_idx += 1;
            stack.push(idx);
            // The `$IF` is always the 0th arm; the synthetic profile
            // chooses it iff combo[idx]==0.
            out.push_str(&format!("$IF __variant_frame_{idx} = 0 $THEN\n"));
            continue;
        }
        if upper.starts_with("$ELSIF ")
            && let Some(&idx) = stack.last()
        {
            arm_in_frame[idx] += 1;
            let arm = arm_in_frame[idx];
            // Nth `$ELSIF` is arm N; activates iff combo[idx]==N.
            out.push_str(&format!("$ELSIF __variant_frame_{idx} = {arm} $THEN\n"));
            continue;
        }
        if upper.starts_with("$ELSE")
            && let Some(&idx) = stack.last()
        {
            // Translate $ELSE into an $ELSIF that catches the
            // highest arm index for this frame. Counter arm is
            // (branch_count - 1).
            let last_arm = frames[idx].branch_count - 1;
            out.push_str(&format!(
                "$ELSIF __variant_frame_{idx} = {last_arm} $THEN\n"
            ));
            continue;
        }
        if upper.starts_with("$END") {
            stack.pop();
            out.push_str(raw);
            continue;
        }
        out.push_str(raw);
    }
    out
}

fn increment_combo(combo: &mut [u32], frames: &[FrameInfo]) -> bool {
    for i in (0..combo.len()).rev() {
        combo[i] += 1;
        if combo[i] < frames[i].branch_count {
            return true;
        }
        combo[i] = 0;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_directives_yields_single_variant() {
        let r = analyse_variants("SELECT 1 FROM dual;\n");
        assert_eq!(r.frames.len(), 0);
        assert_eq!(r.variants.len(), 1);
        assert!(!r.truncated);
    }

    #[test]
    fn single_if_then_end_yields_one_variant() {
        // `$IF flag = 1 $THEN BODY $END` with no $ELSE is one
        // branch (the `$IF` arm). The engine still enumerates the
        // single feasible combination.
        let src = "$IF debug = 1 $THEN\n  X\n$END\n";
        let r = analyse_variants(src);
        assert_eq!(r.frames.len(), 1);
        assert_eq!(r.frames[0].branch_count, 1);
        assert_eq!(r.variants.len(), 1);
    }

    #[test]
    fn if_else_enumerates_two_variants() {
        let src = "$IF debug = 1 $THEN\n  ON\n$ELSE\n  OFF\n$END\n";
        let r = analyse_variants(src);
        assert_eq!(r.frames.len(), 1);
        assert_eq!(r.frames[0].branch_count, 2);
        assert_eq!(r.variants.len(), 2);
        // Variant 0: $IF arm — body contains ON.
        assert!(
            r.variants[0].source.selected_source.contains("ON"),
            "v0 source:\n{}\nerrors: {:?}\nprofile: {:?}",
            r.variants[0].source.selected_source,
            r.variants[0].source.evaluation_errors,
            r.variants[0].profile,
        );
        assert!(!r.variants[0].source.selected_source.contains("OFF"));
        // Variant 1: $ELSE arm — body contains OFF.
        assert!(r.variants[1].source.selected_source.contains("OFF"));
        assert!(!r.variants[1].source.selected_source.contains("ON"));
    }

    #[test]
    fn elsif_chain_enumerates_three_variants() {
        let src = "$IF mode = 'A' $THEN\n  A\n$ELSIF mode = 'B' $THEN\n  B\n$ELSE\n  C\n$END\n";
        let r = analyse_variants(src);
        assert_eq!(r.frames[0].branch_count, 3);
        assert_eq!(r.variants.len(), 3);
        assert!(r.variants[0].source.selected_source.contains("A"));
        assert!(r.variants[1].source.selected_source.contains("B"));
        assert!(r.variants[2].source.selected_source.contains("C"));
    }

    #[test]
    fn two_frames_produce_cartesian_product() {
        let src = "\
$IF a = 1 $THEN\n A1\n$ELSE\n A2\n$END\n\
$IF b = 1 $THEN\n B1\n$ELSE\n B2\n$END\n";
        let r = analyse_variants(src);
        assert_eq!(r.frames.len(), 2);
        // 2 × 2 = 4 variants.
        assert_eq!(r.variants.len(), 4);
    }

    #[test]
    fn truncation_engages_when_product_exceeds_cap() {
        // 6 binary frames → 64 > MAX_VARIANTS (32).
        let mut src = String::new();
        for i in 0..6 {
            src.push_str(&format!("$IF f{i} = 1 $THEN\n A{i}\n$ELSE\n B{i}\n$END\n"));
        }
        let r = analyse_variants(&src);
        assert!(r.truncated);
        assert_eq!(r.variants.len(), MAX_VARIANTS);
    }

    #[test]
    fn frame_expressions_recorded_in_declaration_order() {
        let src = "$IF mode = 'A' $THEN\nA\n$ELSIF mode = 'B' $THEN\nB\n$ELSE\nC\n$END\n";
        let r = analyse_variants(src);
        let exprs = &r.frames[0].branch_expressions;
        assert_eq!(exprs.len(), 3);
        assert_eq!(exprs[0], "mode = 'A'");
        assert_eq!(exprs[1], "mode = 'B'");
        assert_eq!(exprs[2], "$ELSE");
    }
}
