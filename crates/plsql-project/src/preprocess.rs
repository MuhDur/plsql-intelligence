//! PL/SQL conditional-compilation preprocessor.
//!
//! Oracle's PL/SQL supports `$IF` / `$ELSIF` / `$ELSE` / `$END`
//! and `$ERROR` directives that select which lines of source are
//! visible to the compiler. The preprocessor in this module
//! evaluates those directives against an [`AnalysisProfile`]
//! (caller-supplied `PLSQL_CCFLAGS` map + dbms_db_version) and
//! produces:
//!
//! * A `selected_source` string — the original text with inactive
//!   regions replaced by blank lines so downstream parser line
//!   numbers stay aligned.
//! * An `inactive_regions` vector — each entry records the
//!   1-based line span that was suppressed and the directive that
//!   caused the suppression, so reports can explain why a chunk
//!   isn't analysed.
//! * An `evaluation_errors` vector for directives that referenced
//!   unknown flags or used unsupported operators — surfaced rather
//!   than silently treated as `FALSE`.
//!
//! The evaluator is intentionally minimal: it handles equality
//! comparison (`$IF foo = 1 $THEN`), boolean literals, and the
//! `dbms_db_version.version` / `.release` numeric comparisons.
//! Complex expressions (parentheses, AND/OR nesting beyond a
//! single conjunction) surface as evaluation errors so the
//! operator knows they were not analysed.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference routing —
//!   the conditional-compilation chapter spells out the `$IF` /
//!   `$THEN` / `$ELSIF` / `$ELSE` / `$END` / `$ERROR` directives
//!   and their evaluation rules.
//! * `LOW-LEVEL-CATALOGS.md` Supplied Package Buckets —
//!   `DBMS_PREPROCESSOR` would resolve these at runtime; the
//!   source-only path replicates the same selection for offline
//!   analysis.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Caller-supplied state that drives `$IF` evaluation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisProfile {
    /// Key → value pairs equivalent to Oracle's `PLSQL_CCFLAGS`
    /// parameter. Keys are case-insensitive on the way in; values
    /// are stored as strings and compared lexicographically (the
    /// directive carries any numeric coercion the user wants).
    pub plsql_ccflags: BTreeMap<String, String>,
    /// Major release number (e.g. 19 for 19c, 23 for 23ai). Drives
    /// `dbms_db_version.version` comparisons.
    pub dbms_db_version_major: Option<u32>,
    /// Release update number (e.g. 23 for 19.23). Drives
    /// `dbms_db_version.release` comparisons.
    pub dbms_db_version_release: Option<u32>,
}

impl AnalysisProfile {
    /// Build a profile from a raw `PLSQL_CCFLAGS` string of the
    /// `key:value,key:value` form Oracle accepts. Whitespace
    /// around each key / value is trimmed.
    #[must_use]
    pub fn from_ccflags(text: &str) -> Self {
        let mut map = BTreeMap::new();
        for pair in text.split(',') {
            if let Some((k, v)) = pair.split_once(':') {
                let key = k.trim().to_ascii_lowercase();
                let value = v.trim().to_string();
                if !key.is_empty() {
                    map.insert(key, value);
                }
            }
        }
        Self {
            plsql_ccflags: map,
            ..Self::default()
        }
    }

    fn lookup(&self, key: &str) -> Option<&str> {
        self.plsql_ccflags
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// Result of preprocessing a source file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreprocessedSource {
    pub selected_source: String,
    pub inactive_regions: Vec<InactiveRegion>,
    pub evaluation_errors: Vec<EvaluationError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InactiveRegion {
    /// 1-based inclusive start line of the suppressed region.
    pub line_start: u32,
    /// 1-based inclusive end line of the suppressed region.
    pub line_end: u32,
    /// The directive expression text that caused the suppression
    /// (verbatim from source, useful for telling the operator why
    /// this region isn't analysed).
    pub directive: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationError {
    pub line: u32,
    pub directive: String,
    pub reason: String,
}

/// Preprocess `source` against `profile`. Returns the selected
/// view + the inactive-region provenance. Lines outside `$IF`/
/// `$END` brackets are always emitted; lines inside an inactive
/// branch are replaced with empty lines.
pub fn preprocess(source: &str, profile: &AnalysisProfile) -> PreprocessedSource {
    let mut out = PreprocessedSource::default();
    let mut buffer = String::with_capacity(source.len());

    // Stack of (branch_active, any_branch_already_taken_in_this_if).
    // We need both because once a branch in an $IF/$ELSIF chain is
    // selected, all later $ELSIF / $ELSE bodies suppress regardless
    // of their own condition.
    let mut stack: Vec<Frame> = Vec::new();

    let mut current_line: u32 = 0;
    for raw in source.split_inclusive('\n') {
        current_line += 1;
        let trimmed = raw.trim_start();
        let upper = trimmed.to_ascii_uppercase();

        if upper.starts_with("$IF ") || upper == "$IF" {
            let expr_text = extract_directive_body(trimmed, "$IF");
            let parent_active = stack.iter().all(|f| f.active);
            let cond = if parent_active {
                eval_expression(&expr_text, profile, current_line, &mut out)
            } else {
                false
            };
            stack.push(Frame {
                active: parent_active && cond,
                taken: cond,
                directive_line: current_line,
                directive_text: expr_text,
                inactive_start: if parent_active && !cond {
                    Some(current_line + 1)
                } else {
                    None
                },
            });
            buffer.push('\n');
            continue;
        }

        if upper.starts_with("$ELSIF ") {
            let parent_active = stack[..stack.len().saturating_sub(1)]
                .iter()
                .all(|f| f.active);
            if let Some(top) = stack.last_mut() {
                close_inactive_region(top, current_line, &mut out);
                let expr_text = extract_directive_body(trimmed, "$ELSIF");
                let cond = if parent_active && !top.taken {
                    eval_expression(&expr_text, profile, current_line, &mut out)
                } else {
                    false
                };
                top.active = parent_active && !top.taken && cond;
                if cond {
                    top.taken = true;
                }
                top.directive_text = expr_text;
                top.directive_line = current_line;
                if parent_active && !top.active {
                    top.inactive_start = Some(current_line + 1);
                }
                buffer.push('\n');
                continue;
            }
        }

        if upper.starts_with("$ELSE") {
            let parent_active = stack[..stack.len().saturating_sub(1)]
                .iter()
                .all(|f| f.active);
            if let Some(top) = stack.last_mut() {
                close_inactive_region(top, current_line, &mut out);
                top.active = parent_active && !top.taken;
                if top.active {
                    top.taken = true;
                }
                top.directive_text = "$ELSE".into();
                top.directive_line = current_line;
                if parent_active && !top.active {
                    top.inactive_start = Some(current_line + 1);
                }
                buffer.push('\n');
                continue;
            }
        }

        if upper.starts_with("$END") {
            if let Some(mut top) = stack.pop() {
                close_inactive_region(&mut top, current_line, &mut out);
                buffer.push('\n');
                continue;
            }
        }

        // $ERROR directive — emit as an evaluation error but only
        // when the surrounding branch is active (matches Oracle's
        // behaviour).
        if upper.starts_with("$ERROR") && stack.iter().all(|f| f.active) {
            out.evaluation_errors.push(EvaluationError {
                line: current_line,
                directive: trimmed.trim_end().to_string(),
                reason: "PL/SQL $ERROR directive fired".into(),
            });
            buffer.push('\n');
            continue;
        }

        // Plain line — copy verbatim if every enclosing branch is
        // active, otherwise emit a blank line so downstream line
        // numbers stay aligned.
        let line_is_active = stack.iter().all(|f| f.active);
        if line_is_active {
            buffer.push_str(raw);
        } else {
            buffer.push('\n');
        }
    }

    // Unclosed $IF: leave the inactive region open through EOF so
    // the caller can spot the unbalanced bracket via
    // evaluation_errors.
    while let Some(top) = stack.pop() {
        if let Some(start) = top.inactive_start {
            out.inactive_regions.push(InactiveRegion {
                line_start: start,
                line_end: current_line,
                directive: top.directive_text.clone(),
            });
        }
        out.evaluation_errors.push(EvaluationError {
            line: top.directive_line,
            directive: top.directive_text,
            reason: "Unterminated $IF directive at end of file".into(),
        });
    }

    out.selected_source = buffer;
    out
}

fn close_inactive_region(frame: &mut Frame, current_line: u32, out: &mut PreprocessedSource) {
    if let Some(start) = frame.inactive_start.take()
        && current_line > start
    {
        out.inactive_regions.push(InactiveRegion {
            line_start: start,
            line_end: current_line - 1,
            directive: frame.directive_text.clone(),
        });
    }
}

#[derive(Debug)]
struct Frame {
    active: bool,
    taken: bool,
    directive_line: u32,
    directive_text: String,
    inactive_start: Option<u32>,
}

fn extract_directive_body(line: &str, keyword: &str) -> String {
    let trimmed = line.trim();
    let rest = trimmed.get(keyword.len()..).unwrap_or("");
    let rest = rest.trim();
    // Strip trailing $THEN if present — it's syntactic noise here.
    let upper = rest.to_ascii_uppercase();
    if let Some(idx) = upper.rfind("$THEN") {
        rest[..idx].trim().to_string()
    } else {
        rest.to_string()
    }
}

/// Evaluate an `$IF` expression. Supported shapes:
///
/// * `<flag> = <literal>` — string compare against `PLSQL_CCFLAGS`.
/// * `dbms_db_version.version >= N`
/// * `dbms_db_version.release >= N`
/// * `TRUE` / `FALSE` literals
fn eval_expression(
    expr: &str,
    profile: &AnalysisProfile,
    line: u32,
    out: &mut PreprocessedSource,
) -> bool {
    let trimmed = expr.trim();
    let upper = trimmed.to_ascii_uppercase();
    if upper == "TRUE" {
        return true;
    }
    if upper == "FALSE" {
        return false;
    }

    // dbms_db_version comparisons.
    if let Some(rest) = upper.strip_prefix("DBMS_DB_VERSION.VERSION") {
        return numeric_compare(rest, profile.dbms_db_version_major, trimmed, line, out);
    }
    if let Some(rest) = upper.strip_prefix("DBMS_DB_VERSION.RELEASE") {
        return numeric_compare(rest, profile.dbms_db_version_release, trimmed, line, out);
    }

    // Equality comparison on a CCFLAG.
    if let Some((lhs, rhs)) = trimmed.split_once('=') {
        let key = lhs.trim();
        let want = rhs.trim().trim_matches('\'').trim();
        if let Some(value) = profile.lookup(key) {
            return value.eq_ignore_ascii_case(want);
        }
        out.evaluation_errors.push(EvaluationError {
            line,
            directive: trimmed.to_string(),
            reason: format!("PLSQL_CCFLAGS key {key:?} not bound — treated as FALSE"),
        });
        return false;
    }

    out.evaluation_errors.push(EvaluationError {
        line,
        directive: trimmed.to_string(),
        reason: "expression shape not supported by source-only preprocessor".into(),
    });
    false
}

fn numeric_compare(
    rest: &str,
    actual: Option<u32>,
    directive: &str,
    line: u32,
    out: &mut PreprocessedSource,
) -> bool {
    let trimmed = rest.trim();
    let Some(actual) = actual else {
        out.evaluation_errors.push(EvaluationError {
            line,
            directive: directive.to_string(),
            reason: "dbms_db_version unset in AnalysisProfile — treated as FALSE".into(),
        });
        return false;
    };
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() != 2 {
        out.evaluation_errors.push(EvaluationError {
            line,
            directive: directive.to_string(),
            reason: "dbms_db_version compare expected `<op> <n>`".into(),
        });
        return false;
    }
    let op = parts[0];
    let want: u32 = match parts[1].parse() {
        Ok(n) => n,
        Err(_) => {
            out.evaluation_errors.push(EvaluationError {
                line,
                directive: directive.to_string(),
                reason: format!("unable to parse comparison rhs {:?}", parts[1]),
            });
            return false;
        }
    };
    match op {
        "=" | "==" => actual == want,
        "<" => actual < want,
        "<=" => actual <= want,
        ">" => actual > want,
        ">=" => actual >= want,
        "!=" | "<>" => actual != want,
        _ => {
            out.evaluation_errors.push(EvaluationError {
                line,
                directive: directive.to_string(),
                reason: format!("unsupported dbms_db_version comparison operator {op:?}"),
            });
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(pairs: &[(&str, &str)]) -> AnalysisProfile {
        let mut ccflags = BTreeMap::new();
        for (k, v) in pairs {
            ccflags.insert((*k).to_ascii_lowercase(), (*v).to_string());
        }
        AnalysisProfile {
            plsql_ccflags: ccflags,
            ..AnalysisProfile::default()
        }
    }

    #[test]
    fn plain_source_passes_through_unchanged() {
        let src = "SELECT 1 FROM dual;\n";
        let r = preprocess(src, &AnalysisProfile::default());
        assert_eq!(r.selected_source, src);
        assert!(r.inactive_regions.is_empty());
        assert!(r.evaluation_errors.is_empty());
    }

    #[test]
    fn if_true_branch_kept_inactive_replaced_with_blanks() {
        let src = "$IF debug = 1 $THEN\n  DBMS_OUTPUT.PUT_LINE('on');\n$ELSE\n  DBMS_OUTPUT.PUT_LINE('off');\n$END\n";
        let r = preprocess(src, &profile(&[("debug", "1")]));
        assert!(r.selected_source.contains("DBMS_OUTPUT.PUT_LINE('on')"));
        assert!(!r.selected_source.contains("DBMS_OUTPUT.PUT_LINE('off')"));
        assert_eq!(r.inactive_regions.len(), 1);
        assert_eq!(r.inactive_regions[0].line_start, 4);
        assert_eq!(r.inactive_regions[0].line_end, 4);
    }

    #[test]
    fn if_false_branch_inactivates_then_else_active() {
        let src = "$IF debug = 1 $THEN\n  -- on\n$ELSE\n  SELECT 1 FROM dual;\n$END\n";
        let r = preprocess(src, &profile(&[("debug", "0")]));
        assert!(r.selected_source.contains("SELECT 1 FROM dual;"));
        assert!(!r.selected_source.contains("-- on"));
    }

    #[test]
    fn elsif_chain_picks_first_match() {
        let src = "$IF mode = 'A' $THEN\n  A_BODY\n$ELSIF mode = 'B' $THEN\n  B_BODY\n$ELSE\n  C_BODY\n$END\n";
        let r = preprocess(src, &profile(&[("mode", "B")]));
        assert!(!r.selected_source.contains("A_BODY"));
        assert!(r.selected_source.contains("B_BODY"));
        assert!(!r.selected_source.contains("C_BODY"));
    }

    #[test]
    fn dbms_db_version_compare_works() {
        let src = "$IF dbms_db_version.version >= 19 $THEN\n  MODERN\n$ELSE\n  LEGACY\n$END\n";
        let p = AnalysisProfile {
            dbms_db_version_major: Some(23),
            ..AnalysisProfile::default()
        };
        let r = preprocess(src, &p);
        assert!(r.selected_source.contains("MODERN"));
        assert!(!r.selected_source.contains("LEGACY"));
    }

    #[test]
    fn unknown_flag_records_evaluation_error_and_falls_to_false() {
        let src = "$IF nonexistent = 1 $THEN\n  X\n$END\n";
        let r = preprocess(src, &AnalysisProfile::default());
        assert!(!r.selected_source.contains("X"));
        assert_eq!(r.evaluation_errors.len(), 1);
        assert!(r.evaluation_errors[0].reason.contains("not bound"));
    }

    #[test]
    fn error_directive_fires_when_branch_is_active() {
        let src = "$IF prod = 1 $THEN\n$ERROR 'no prod build allowed'\n$END\n";
        let r = preprocess(src, &profile(&[("prod", "1")]));
        assert_eq!(r.evaluation_errors.len(), 1);
        assert!(r.evaluation_errors[0].reason.contains("$ERROR"));
    }

    #[test]
    fn error_directive_skipped_when_branch_is_inactive() {
        let src = "$IF prod = 1 $THEN\n$ERROR 'no prod build allowed'\n$END\n";
        let r = preprocess(src, &profile(&[("prod", "0")]));
        assert!(r.evaluation_errors.is_empty());
    }

    #[test]
    fn unterminated_if_records_evaluation_error() {
        let src = "$IF debug = 1 $THEN\n-- forgot $END\n";
        let r = preprocess(src, &profile(&[("debug", "1")]));
        assert!(
            r.evaluation_errors
                .iter()
                .any(|e| e.reason.contains("Unterminated"))
        );
    }

    #[test]
    fn from_ccflags_parses_comma_separated_pairs() {
        let p = AnalysisProfile::from_ccflags("DEBUG:1, MODE:'A' ,STRICT: TRUE");
        assert_eq!(p.lookup("debug"), Some("1"));
        assert_eq!(p.lookup("mode"), Some("'A'"));
        assert_eq!(p.lookup("strict"), Some("TRUE"));
    }

    #[test]
    fn nested_if_inactive_outer_suppresses_inner_evaluation() {
        let src = "$IF outer = 1 $THEN\n  $IF inner = 1 $THEN\n    BODY\n  $END\n$END\n";
        let r = preprocess(src, &profile(&[("outer", "0"), ("inner", "1")]));
        assert!(!r.selected_source.contains("BODY"));
    }
}
