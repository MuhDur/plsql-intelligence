//! Parser-level delta-debugging shrinker.
//!
//! Consumes a textual PL/SQL input plus a caller-supplied
//! oracle ("does the bug still reproduce?") and returns the
//! smallest input that still triggers the oracle. The algorithm
//! is the classic ddmin shape (Zeller / Hildebrandt): split the
//! input into chunks at the granularity supplied by the caller,
//! probe combinations greedily, halve the chunk size when no
//! reduction works, terminate at granularity 1.
//!
//! The shrinker is library-shaped: every probe is a call into
//! `ReproOracle::reproduces(&str) -> bool`. The caller wires that
//! to whatever they need — running the parser, running a CI gate,
//! running a CLI binary. Tests use a small in-crate oracle that
//! checks for a literal substring so we exercise the search
//! without dragging in the parser.
//!
//! Granularity comes from the SQL*Plus statement splitter for
//! statement-level shrinking; the line-based splitter ships here as
//! a fallback that doesn't require `plsql-project` at the support
//! layer.

use serde::{Deserialize, Serialize};

/// Caller-supplied oracle. Return `true` when the input still
/// reproduces the bug.
pub trait ReproOracle {
    fn reproduces(&mut self, candidate: &str) -> bool;
}

/// Granularity for the initial split. `Lines` matches the
/// fallback when the caller doesn't want to depend on the
/// SQL*Plus splitter; `Statements` is the preferred mode and
/// expects pre-split chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Granularity {
    Lines,
    /// Caller supplies the chunks directly via
    /// [`shrink_with_chunks`].
    Chunks,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShrinkResult {
    pub minimised: String,
    pub original_size: usize,
    pub minimised_size: usize,
    pub probes: u32,
}

/// Shrink `input` against `oracle` at line granularity. Useful
/// fallback when the caller does not have a statement splitter
/// handy.
pub fn shrink_lines<O: ReproOracle>(input: &str, oracle: &mut O) -> ShrinkResult {
    let chunks: Vec<String> = input.split_inclusive('\n').map(String::from).collect();
    shrink_with_chunks(&chunks, oracle)
}

/// Shrink with caller-supplied chunks (typically PL/SQL statements
/// from `plsql-project::split_script`).
pub fn shrink_with_chunks<O: ReproOracle>(chunks: &[String], oracle: &mut O) -> ShrinkResult {
    let original = chunks.concat();
    let original_size = original.len();

    let mut keep: Vec<bool> = vec![true; chunks.len()];
    let mut probes: u32 = 0;
    // Confirm the original still reproduces before we start
    // pruning; if it doesn't, return the original unchanged.
    probes += 1;
    if !oracle.reproduces(&assemble(chunks, &keep)) {
        return ShrinkResult {
            minimised: original,
            original_size,
            minimised_size: original_size,
            probes,
        };
    }

    // ddmin shape: try removing larger groups first, halving on
    // failure, terminating when the group size hits 1.
    let mut group_size = chunks.len().max(1);
    while group_size >= 1 {
        let mut progressed = false;
        let mut i = 0;
        while i < chunks.len() {
            let end = (i + group_size).min(chunks.len());
            // Snapshot the current `keep` decision in case the
            // probe fails.
            let mut probe = keep.clone();
            let mut touched_any_true = false;
            for slot in probe.iter_mut().take(end).skip(i) {
                if *slot {
                    *slot = false;
                    touched_any_true = true;
                }
            }
            if !touched_any_true {
                i = end;
                continue;
            }
            probes += 1;
            if oracle.reproduces(&assemble(chunks, &probe)) {
                keep = probe;
                progressed = true;
            }
            i = end;
        }
        if progressed {
            // After a successful pass try the SAME group size again
            // before halving so we exhaust local prunes.
            continue;
        }
        if group_size == 1 {
            break;
        }
        group_size = group_size.div_ceil(2);
    }

    let minimised = assemble(chunks, &keep);
    let minimised_size = minimised.len();
    ShrinkResult {
        minimised,
        original_size,
        minimised_size,
        probes,
    }
}

fn assemble(chunks: &[String], keep: &[bool]) -> String {
    let mut out = String::new();
    for (i, c) in chunks.iter().enumerate() {
        if keep[i] {
            out.push_str(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ContainsOracle {
        needle: String,
        probes: u32,
    }
    impl ContainsOracle {
        fn new(needle: &str) -> Self {
            Self {
                needle: needle.into(),
                probes: 0,
            }
        }
    }
    impl ReproOracle for ContainsOracle {
        fn reproduces(&mut self, candidate: &str) -> bool {
            self.probes += 1;
            candidate.contains(&self.needle)
        }
    }

    #[test]
    fn shrink_isolates_the_failing_line() {
        let input = "line a\nline b\nBAD\nline d\nline e\n";
        let mut oracle = ContainsOracle::new("BAD");
        let result = shrink_lines(input, &mut oracle);
        assert!(result.minimised.contains("BAD"));
        assert!(result.minimised.len() < input.len());
        // Every non-failing line dropped.
        assert!(!result.minimised.contains("line a"));
        assert!(!result.minimised.contains("line e"));
    }

    #[test]
    fn shrink_preserves_input_when_not_reproducing() {
        let input = "no bug here\n";
        let mut oracle = ContainsOracle::new("BAD");
        let result = shrink_lines(input, &mut oracle);
        // Oracle returns false on the first probe → bail with the
        // original intact.
        assert_eq!(result.minimised, input);
        assert_eq!(result.original_size, result.minimised_size);
    }

    #[test]
    fn shrink_counts_probes() {
        let input = "a\nb\nBAD\nc\n";
        let mut oracle = ContainsOracle::new("BAD");
        let result = shrink_lines(input, &mut oracle);
        assert!(result.probes >= 2);
    }

    #[test]
    fn shrink_with_chunks_accepts_statement_split() {
        // Statement-level shrinking: each chunk is one whole
        // statement. The shrinker should isolate the failing one.
        let chunks: Vec<String> = vec![
            "BEGIN good_1(); END;\n".into(),
            "BEGIN good_2(); END;\n".into(),
            "BEGIN bad_call(); END;\n".into(),
            "BEGIN good_3(); END;\n".into(),
        ];
        let mut oracle = ContainsOracle::new("bad_call");
        let result = shrink_with_chunks(&chunks, &mut oracle);
        assert!(result.minimised.contains("bad_call"));
        assert!(!result.minimised.contains("good_1"));
        assert!(!result.minimised.contains("good_3"));
    }

    #[test]
    fn shrink_empty_input_is_idempotent() {
        let mut oracle = ContainsOracle::new("BAD");
        let result = shrink_lines("", &mut oracle);
        assert_eq!(result.minimised, "");
        assert_eq!(result.original_size, 0);
    }

    #[test]
    fn shrink_single_line_with_bug_is_already_minimal() {
        let input = "BAD\n";
        let mut oracle = ContainsOracle::new("BAD");
        let result = shrink_lines(input, &mut oracle);
        assert_eq!(result.minimised, input);
        assert_eq!(result.minimised_size, result.original_size);
    }

    #[test]
    fn granularity_serialises_snake_case() {
        let lines = serde_json::to_string(&Granularity::Lines).unwrap();
        assert_eq!(lines, "\"lines\"");
        let chunks = serde_json::to_string(&Granularity::Chunks).unwrap();
        assert_eq!(chunks, "\"chunks\"");
    }
}
