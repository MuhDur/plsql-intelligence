//! DB-link reference recording (PLSQL-SYM-004).
//!
//! Oracle's database-link feature lets a PL/SQL routine reference
//! an object in a remote database by appending `@<link_name>` to
//! the object reference (`SELECT * FROM remote_table@HR_LINK`).
//! The remote object's metadata isn't reachable from offline
//! analysis — its tables, columns, types, and privileges live in
//! the remote dictionary, behind the network.
//!
//! Rather than silently dropping these references, the resolver
//! records each one as a [`DbLinkReference`] and surfaces them as
//! `UnknownReason::DbLinkRemoteObject` (R13). Downstream
//! consumers (lineage, doc, SAST) see the references and can
//! report them honestly.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference —
//!   Database Links chapter governs the `name@dblink` syntax and
//!   the rule that the remote object's metadata is opaque from
//!   the local PL/SQL compiler.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_DB_LINKS` is the local-side view that lists known
//!   db-link names; a future bead can cross-check our recorded
//!   refs against it.

use serde::{Deserialize, Serialize};

/// One observed `<name>@<dblink>` reference. Fields are stored
/// case-folded so the registry can dedupe regardless of how the
/// operator typed the reference.
#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct DbLinkReference {
    /// Optional schema prefix (`HR.EMPLOYEES@…`).
    pub schema: Option<String>,
    /// Object name on the remote side.
    pub object: String,
    /// DB-link name.
    pub db_link: String,
}

/// Append-only collector — the resolver pushes references as it
/// walks; the report layer reads them out in declaration order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbLinkRegistry {
    pub references: Vec<DbLinkReference>,
}

impl DbLinkRegistry {
    /// Record a `<name>@<dblink>` reference. The function is
    /// idempotent — the same reference inserted twice produces
    /// only one entry. Insertion is case-insensitive on each
    /// part.
    pub fn record(&mut self, reference: DbLinkReference) {
        let normalised = DbLinkReference {
            schema: reference.schema.map(|s| s.to_ascii_uppercase()),
            object: reference.object.to_ascii_uppercase(),
            db_link: reference.db_link.to_ascii_uppercase(),
        };
        if !self.references.contains(&normalised) {
            self.references.push(normalised);
        }
    }

    /// Parse a single textual reference (`schema.object@dblink` /
    /// `object@dblink`) and record it. Returns true if a db-link
    /// reference was recognised, false otherwise (no `@` token).
    pub fn record_from_text(&mut self, raw: &str) -> bool {
        let Some((lhs, link)) = raw.rsplit_once('@') else {
            return false;
        };
        let link = link.trim().trim_end_matches(';').trim();
        if link.is_empty() {
            return false;
        }
        let (schema, object) = match lhs.rsplit_once('.') {
            Some((s, o)) => (Some(s.trim().to_string()), o.trim().to_string()),
            None => (None, lhs.trim().to_string()),
        };
        if object.is_empty() {
            return false;
        }
        self.record(DbLinkReference {
            schema,
            object,
            db_link: link.to_string(),
        });
        true
    }

    /// Distinct db-link names referenced, case-folded.
    #[must_use]
    pub fn distinct_links(&self) -> Vec<String> {
        let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for r in &self.references {
            out.insert(r.db_link.clone());
        }
        out.into_iter().collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.references.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.references.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_dedupes_case_insensitively() {
        let mut reg = DbLinkRegistry::default();
        reg.record(DbLinkReference {
            schema: Some("hr".into()),
            object: "employees".into(),
            db_link: "remote_link".into(),
        });
        reg.record(DbLinkReference {
            schema: Some("HR".into()),
            object: "EMPLOYEES".into(),
            db_link: "REMOTE_LINK".into(),
        });
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.references[0].schema.as_deref(), Some("HR"));
        assert_eq!(reg.references[0].object, "EMPLOYEES");
        assert_eq!(reg.references[0].db_link, "REMOTE_LINK");
    }

    #[test]
    fn record_from_text_handles_schema_prefix() {
        let mut reg = DbLinkRegistry::default();
        assert!(reg.record_from_text("hr.employees@remote_link"));
        assert_eq!(reg.references[0].schema.as_deref(), Some("HR"));
        assert_eq!(reg.references[0].object, "EMPLOYEES");
        assert_eq!(reg.references[0].db_link, "REMOTE_LINK");
    }

    #[test]
    fn record_from_text_handles_bare_object() {
        let mut reg = DbLinkRegistry::default();
        assert!(reg.record_from_text("employees@remote_link"));
        assert!(reg.references[0].schema.is_none());
        assert_eq!(reg.references[0].object, "EMPLOYEES");
    }

    #[test]
    fn record_from_text_rejects_input_without_at_token() {
        let mut reg = DbLinkRegistry::default();
        assert!(!reg.record_from_text("hr.employees"));
        assert!(reg.is_empty());
    }

    #[test]
    fn record_from_text_rejects_empty_object_or_link() {
        let mut reg = DbLinkRegistry::default();
        assert!(!reg.record_from_text("@link"));
        assert!(!reg.record_from_text("object@"));
        assert!(reg.is_empty());
    }

    #[test]
    fn distinct_links_dedupes_across_references() {
        let mut reg = DbLinkRegistry::default();
        reg.record_from_text("hr.employees@LINK_A");
        reg.record_from_text("hr.departments@LINK_A");
        reg.record_from_text("hr.salaries@LINK_B");
        let links = reg.distinct_links();
        assert_eq!(links, vec!["LINK_A".to_string(), "LINK_B".to_string()]);
    }

    #[test]
    fn references_serde_round_trip() {
        let mut reg = DbLinkRegistry::default();
        reg.record_from_text("hr.employees@remote_link");
        let json = serde_json::to_string(&reg).unwrap();
        let back: DbLinkRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, reg);
        assert!(json.contains("REMOTE_LINK"));
    }

    #[test]
    fn ordering_preserves_insertion() {
        let mut reg = DbLinkRegistry::default();
        reg.record_from_text("z@first");
        reg.record_from_text("a@second");
        assert_eq!(reg.references[0].db_link, "FIRST");
        assert_eq!(reg.references[1].db_link, "SECOND");
    }

    #[test]
    fn trailing_semicolon_on_link_stripped() {
        let mut reg = DbLinkRegistry::default();
        reg.record_from_text("hr.employees@remote_link;");
        assert_eq!(reg.references[0].db_link, "REMOTE_LINK");
    }
}
