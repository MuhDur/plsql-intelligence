//! Stable finding fingerprints.
//!
//! Two SHA-256 digests per [`Finding`](crate::Finding), mirroring
//! the `sha256:`/`fact:` discipline the fact stream uses:
//!
//! * **primary** — `rule_id` + severity + message, *excluding*
//!   any source coordinates. Stable when code moves: the same
//!   logical issue keeps its primary fingerprint across runs even
//!   if lines shift, so a suppression baseline does not churn on
//!   reformatting. This is the SARIF `partialFingerprints`
//!   / baseline-match key.
//! * **location** — primary inputs **plus** file + line + byte
//!   span. Distinguishes two instances of the same rule at
//!   different sites within one run (exact dedupe).
//!
//! Field separators are `\u{1f}` (ASCII Unit Separator) so a
//! value containing the delimiter text cannot forge a collision.

use sha2::{Digest, Sha256};

use crate::Finding;

/// Unit-separator — cannot appear in rule ids / file paths and
/// is vanishingly unlikely in a message, so concatenated inputs
/// cannot be crafted to collide across field boundaries.
const SEP: char = '\u{1f}';

/// The two stable digests of a [`Finding`](crate::Finding).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FindingFingerprint {
    /// Location-insensitive logical identity (baseline key).
    pub primary: String,
    /// Exact-site identity (intra-run dedupe key).
    pub location: String,
}

fn hex(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            let mut buf = [0u8; 4];
            h.update(SEP.encode_utf8(&mut buf).as_bytes());
        }
        h.update(p.as_bytes());
    }
    let digest = h.finalize();
    let mut s = String::with_capacity(2 * digest.len());
    for b in digest {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Compute the [`FindingFingerprint`] for `f`.
#[must_use]
pub fn fingerprint(f: &Finding) -> FindingFingerprint {
    let severity = format!("{:?}", f.severity);
    // Primary: identity that should survive line/format drift.
    let primary = format!("sast-fp:{}", hex(&[&f.rule_id, &severity, &f.message]));
    // Location: primary inputs + exact source coordinates.
    let line = f.location.line.to_string();
    let span0 = f.location.byte_span.0.to_string();
    let span1 = f.location.byte_span.1.to_string();
    let location = format!(
        "sast-loc:{}",
        hex(&[
            &f.rule_id,
            &severity,
            &f.message,
            &f.location.file,
            &line,
            &span0,
            &span1,
        ])
    );
    FindingFingerprint { primary, location }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Severity, finding};

    #[test]
    fn fingerprint_is_deterministic() {
        let f = finding(
            "SEC001",
            Severity::Critical,
            "tainted at X",
            "a.sql",
            10,
            (3, 9),
        );
        assert_eq!(fingerprint(&f), fingerprint(&f.clone()));
    }

    #[test]
    fn primary_is_location_insensitive_but_location_is_not() {
        let a = finding(
            "SEC001",
            Severity::Critical,
            "tainted at X",
            "a.sql",
            10,
            (3, 9),
        );
        let moved = finding(
            "SEC001",
            Severity::Critical,
            "tainted at X",
            "a.sql",
            42,
            (80, 90),
        );
        let fa = fingerprint(&a);
        let fb = fingerprint(&moved);
        assert_eq!(fa.primary, fb.primary, "baseline must survive line drift");
        assert_ne!(fa.location, fb.location, "exact site must differ");
    }

    #[test]
    fn different_rule_or_message_changes_primary() {
        let base = finding("SEC001", Severity::High, "m", "f", 1, (0, 1));
        let other_rule = finding("SEC002", Severity::High, "m", "f", 1, (0, 1));
        let other_msg = finding("SEC001", Severity::High, "n", "f", 1, (0, 1));
        assert_ne!(fingerprint(&base).primary, fingerprint(&other_rule).primary);
        assert_ne!(fingerprint(&base).primary, fingerprint(&other_msg).primary);
    }

    #[test]
    fn severity_participates_in_identity() {
        let hi = finding("R", Severity::High, "m", "f", 1, (0, 1));
        let lo = finding("R", Severity::Low, "m", "f", 1, (0, 1));
        assert_ne!(fingerprint(&hi).primary, fingerprint(&lo).primary);
    }

    #[test]
    fn separator_prevents_field_boundary_collision() {
        // ("AB","C") vs ("A","BC") must not collide.
        let x = finding("AB", Severity::Info, "C", "f", 1, (0, 1));
        let y = finding("A", Severity::Info, "BC", "f", 1, (0, 1));
        assert_ne!(fingerprint(&x).primary, fingerprint(&y).primary);
    }

    #[test]
    fn fingerprint_round_trips_through_json() {
        let f = finding("R", Severity::Medium, "m", "f", 2, (1, 4));
        let fp = fingerprint(&f);
        let json = serde_json::to_string(&fp).unwrap();
        let back: FindingFingerprint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, fp);
    }

    #[test]
    fn digests_are_namespaced_hex() {
        let fp = fingerprint(&finding("R", Severity::Info, "m", "f", 1, (0, 1)));
        assert!(fp.primary.starts_with("sast-fp:"));
        assert!(fp.location.starts_with("sast-loc:"));
        // 64 hex chars after the namespace prefix.
        assert_eq!(fp.primary.trim_start_matches("sast-fp:").len(), 64);
    }
}
