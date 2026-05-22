//! Normalized fact schema (PLSQL-FACT-001).
//!
//! Every analysis pass emits its results as a stream of
//! [`Fact`] records sharing one canonical shape. A fact is:
//!
//! 1. A stable [`FactId`] — the SHA-256 of the canonical
//!    serialisation of every other field, so re-emitting the
//!    same fact under the same inputs produces the same id.
//! 2. A [`FactKind`] discriminator naming the family it
//!    belongs to (declaration, reference, edge, opacity, …).
//! 3. A typed payload — the per-family struct carrying the
//!    actual evidence.
//! 4. A [`FactProvenance`] record naming the analysis pass that
//!    emitted the fact (component name, version, run id).
//!
//! Downstream consumers (lineage, doc, SAST, bindings) walk a
//! `FactStore` and filter by kind. This keeps the engine's
//! internal wiring loose — passes don't need to know about each
//! other, only that they emit compatible Facts.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   fact families (declarations, references, dependency
//!   edges, dynamic-SQL evidence) trace 1:1 to the PL/SQL
//!   declaration / reference / call grammar.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   each fact family has a server-side mirror
//!   (`ALL_OBJECTS` for declarations, `ALL_DEPENDENCIES` for
//!   edges, `ALL_SOURCE.WRAPPED` for the wrapped-source
//!   opacity fact, …).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::DeclId;

/// Stable identity for a fact — `fact:<hex>` form.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct FactId(pub String);

/// What family a fact belongs to. Drives consumer dispatch
/// without having to match the payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Declaration,
    Reference,
    DependencyEdge,
    DynamicSqlEvidence,
    DbLinkReference,
    Opacity,
    ResolutionReport,
    Privilege,
    ExceptionHandler,
    CursorForLoop,
    MissingInstrumentation,
    HardcodedCredential,
    InvokerRights,
    RefCursorReturn,
    DmlInFunction,
    UnboundedBulkCollect,
    DeprecatedFeature,
    DeterministicMisuse,
    MutatingTableTrigger,
    LogWithoutReraise,
    CrossSchemaWrite,
    SensitivePublicSynonym,
    IsNullOnIndexedColumn,
}

/// A fact record. Wraps a typed payload with stable id + family
/// + provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub id: FactId,
    pub kind: FactKind,
    pub provenance: FactProvenance,
    pub payload: FactPayload,
}

/// Provenance — which analysis pass produced the fact, when, at
/// what engine version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactProvenance {
    pub component: String,
    pub component_version: String,
    /// Stable run id from the engine's session — empty when the
    /// fact was minted by a one-shot CLI.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub run_id: String,
}

/// Discriminated payload — one variant per `FactKind`. The
/// per-family types are intentionally lightweight; consumers
/// that need richer detail re-fetch from the originating crate's
/// model (e.g. lineage's `LineageResult`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum FactPayload {
    Declaration {
        decl: DeclId,
        logical_id: String,
    },
    Reference {
        from_decl: DeclId,
        to_logical_id: String,
    },
    DependencyEdge {
        from_logical_id: String,
        to_logical_id: String,
        edge_kind: String,
    },
    DynamicSqlEvidence {
        site: String,
    },
    DbLinkReference {
        object: String,
        db_link: String,
    },
    Opacity {
        target_logical_id: String,
        reason: String,
    },
    ResolutionReport {
        reference: String,
        strategy: String,
    },
    Privilege {
        grantee: String,
        privilege: String,
        on: String,
    },
    /// An `EXCEPTION WHEN ... THEN ...` handler. `scope` is the
    /// caught condition (`others` or a named exception); `body_class`
    /// classifies the handler body so syntactic rules can decide
    /// without re-parsing: `noop` (only `NULL;` — QUAL001 swallowed
    /// exception), `commit` / `rollback` (QUAL004 transaction control
    /// in a handler), or `other`.
    ExceptionHandler {
        unit_logical_id: String,
        scope: String,
        body_class: String,
    },
    /// A cursor `FOR` loop (`FOR <var> IN (<query>|<cursor>) LOOP …
    /// END LOOP;`). `has_body_dml` is `true` when the loop body
    /// contains a row-level `INSERT`/`UPDATE`/`DELETE`/`MERGE` —
    /// PERF001 flags any cursor-FOR-loop as a bulk-collect
    /// candidate; PERF002 flags the `has_body_dml` subset as a
    /// `FORALL` candidate. Conservative (R13): an ambiguous shape
    /// yields no fact rather than a wrong one.
    CursorForLoop {
        unit_logical_id: String,
        loop_var: String,
        has_body_dml: bool,
    },
    /// A routine body in which no recognized instrumentation call
    /// (logging / tracing / audit) was found. STYLE001 (opt-in,
    /// per house policy) decides whether that is a finding; the
    /// fact only reports the *absence*, never asserts a violation.
    MissingInstrumentation {
        unit_logical_id: String,
    },
    /// A string literal that is, by strong syntactic context, a
    /// hardcoded secret (`IDENTIFIED BY '…'`, an assignment to a
    /// password/secret/token-named target, or a `password => '…'`
    /// named argument). `marker` records the matched context so
    /// SEC003 can explain the finding. Conservative (R13): only
    /// emitted when a literal directly follows a credential marker.
    HardcodedCredential {
        unit_logical_id: String,
        marker: String,
    },
    /// The unit declares `AUTHID CURRENT_USER` (invoker's rights).
    /// Resolution of privileges is deferred to call time, which
    /// widens the trust surface — SEC004 flags it for review (it is
    /// frequently intentional, so the rule is advisory, not a hard
    /// defect).
    InvokerRights {
        unit_logical_id: String,
    },
    /// A function whose `RETURN` type is a REF CURSOR
    /// (`SYS_REFCURSOR` / `REF CURSOR`). Hands an open cursor to the
    /// caller — a resource-ownership and (when the cursor wrapped
    /// dynamic SQL) injection-amplification concern. SEC007.
    RefCursorReturn {
        unit_logical_id: String,
    },
    /// A `FUNCTION` whose body performs row-level DML
    /// (`INSERT`/`UPDATE`/`DELETE`/`MERGE`). Side-effecting
    /// functions break purity, are unsafe in SQL/parallel/replication
    /// contexts, and surprise callers. QUAL007.
    DmlInFunction {
        unit_logical_id: String,
    },
    /// A `BULK COLLECT INTO` with no `LIMIT` in the same statement —
    /// the entire result set is materialized into PGA memory
    /// unbounded. QUAL003.
    UnboundedBulkCollect {
        unit_logical_id: String,
    },
    /// A well-known deprecated / legacy construct (`feature` names
    /// the match: `dbms_job`, legacy `(+)` outer join, `… work`
    /// transaction-control keyword). QUAL005.
    DeprecatedFeature {
        unit_logical_id: String,
        feature: String,
    },
    /// A function marked `DETERMINISTIC` whose body contains a
    /// non-deterministic construct (DML, query, SYSDATE/
    /// SYSTIMESTAMP, DBMS_RANDOM, sequence `.NEXTVAL`). QUAL008.
    DeterministicMisuse {
        unit_logical_id: String,
        construct: String,
    },
    /// A row-level (`FOR EACH ROW`) trigger whose body references
    /// its own base `table` in a query/DML — ORA-04091 mutating-
    /// table hazard. QUAL006.
    MutatingTableTrigger {
        unit_logical_id: String,
        table: String,
    },
    /// An exception handler that logs (or otherwise instruments)
    /// but neither re-raises nor signals, silently continuing —
    /// the failure is recorded yet swallowed. QUAL002.
    LogWithoutReraise {
        unit_logical_id: String,
    },
    /// A DML statement whose target object is schema-qualified to a
    /// schema other than the unit's own — a cross-schema write
    /// surface. `target` is `schema.object`. DEP001.
    CrossSchemaWrite {
        unit_logical_id: String,
        target: String,
    },
    /// A `CREATE PUBLIC SYNONYM` whose synonym or target name
    /// matches a sensitivity heuristic (credential/PII/finance).
    /// Public synonyms are visible to every account, so exposing a
    /// sensitive object through one widens its reach. SEC005.
    SensitivePublicSynonym {
        unit_logical_id: String,
        synonym: String,
        target: String,
    },
    /// A `<col> IS NULL` predicate on a `column` that the *same
    /// analyzed source* declares an index on (`CREATE INDEX … (col
    /// …)`). A B-tree index does not store all-NULL keys, so the
    /// predicate cannot use that index — a silent full-scan. PERF003.
    /// (Catalog-only indexes are out of this source-level scope.)
    IsNullOnIndexedColumn {
        unit_logical_id: String,
        column: String,
    },
}

impl FactPayload {
    /// Discriminate the family without matching the full enum.
    #[must_use]
    pub fn kind(&self) -> FactKind {
        match self {
            FactPayload::Declaration { .. } => FactKind::Declaration,
            FactPayload::Reference { .. } => FactKind::Reference,
            FactPayload::DependencyEdge { .. } => FactKind::DependencyEdge,
            FactPayload::DynamicSqlEvidence { .. } => FactKind::DynamicSqlEvidence,
            FactPayload::DbLinkReference { .. } => FactKind::DbLinkReference,
            FactPayload::Opacity { .. } => FactKind::Opacity,
            FactPayload::ResolutionReport { .. } => FactKind::ResolutionReport,
            FactPayload::Privilege { .. } => FactKind::Privilege,
            FactPayload::ExceptionHandler { .. } => FactKind::ExceptionHandler,
            FactPayload::CursorForLoop { .. } => FactKind::CursorForLoop,
            FactPayload::MissingInstrumentation { .. } => FactKind::MissingInstrumentation,
            FactPayload::HardcodedCredential { .. } => FactKind::HardcodedCredential,
            FactPayload::InvokerRights { .. } => FactKind::InvokerRights,
            FactPayload::RefCursorReturn { .. } => FactKind::RefCursorReturn,
            FactPayload::DmlInFunction { .. } => FactKind::DmlInFunction,
            FactPayload::UnboundedBulkCollect { .. } => FactKind::UnboundedBulkCollect,
            FactPayload::DeprecatedFeature { .. } => FactKind::DeprecatedFeature,
            FactPayload::DeterministicMisuse { .. } => FactKind::DeterministicMisuse,
            FactPayload::MutatingTableTrigger { .. } => FactKind::MutatingTableTrigger,
            FactPayload::LogWithoutReraise { .. } => FactKind::LogWithoutReraise,
            FactPayload::CrossSchemaWrite { .. } => FactKind::CrossSchemaWrite,
            FactPayload::SensitivePublicSynonym { .. } => FactKind::SensitivePublicSynonym,
            FactPayload::IsNullOnIndexedColumn { .. } => FactKind::IsNullOnIndexedColumn,
        }
    }
}

/// Build a `Fact` with the canonical id derived from `(kind,
/// provenance, payload)`. The id is `fact:<hex>` so it doesn't
/// collide with the `sha256:` namespace other engine bytes use.
#[must_use]
pub fn mint_fact(provenance: FactProvenance, payload: FactPayload) -> Fact {
    let kind = payload.kind();
    let id = compute_fact_id(kind, &provenance, &payload);
    Fact {
        id,
        kind,
        provenance,
        payload,
    }
}

fn compute_fact_id(kind: FactKind, provenance: &FactProvenance, payload: &FactPayload) -> FactId {
    // Canonical serialisation — JSON with sorted keys via
    // serde_json::to_string (BTreeMap-like determinism is
    // guaranteed by serde for tagged enums + struct-form
    // variants; sufficient for fact dedup).
    let kind_json = serde_json::to_string(&kind).unwrap_or_default();
    let prov_json = serde_json::to_string(provenance).unwrap_or_default();
    let payload_json = serde_json::to_string(payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(kind_json.as_bytes());
    hasher.update(b"|");
    hasher.update(prov_json.as_bytes());
    hasher.update(b"|");
    hasher.update(payload_json.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(5 + digest.len() * 2);
    hex.push_str("fact:");
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    FactId(hex)
}

/// Append-only collector — analysis passes push facts in;
/// consumers walk them out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactStore {
    pub facts: Vec<Fact>,
}

impl FactStore {
    pub fn push(&mut self, fact: Fact) -> FactId {
        let id = fact.id.clone();
        if !self.facts.iter().any(|f| f.id == id) {
            self.facts.push(fact);
        }
        id
    }

    /// Filter by family.
    pub fn by_kind(&self, kind: FactKind) -> impl Iterator<Item = &Fact> {
        self.facts.iter().filter(move |f| f.kind == kind)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prov() -> FactProvenance {
        FactProvenance {
            component: "plsql-lineage".into(),
            component_version: "0.1.0".into(),
            run_id: String::new(),
        }
    }

    fn payload() -> FactPayload {
        FactPayload::DependencyEdge {
            from_logical_id: "hr.foo".into(),
            to_logical_id: "hr.bar".into(),
            edge_kind: "Calls".into(),
        }
    }

    #[test]
    fn mint_fact_produces_fact_prefixed_id() {
        let f = mint_fact(prov(), payload());
        assert!(f.id.0.starts_with("fact:"));
    }

    #[test]
    fn mint_fact_is_deterministic_for_same_inputs() {
        let a = mint_fact(prov(), payload());
        let b = mint_fact(prov(), payload());
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn mint_fact_changes_id_when_payload_changes() {
        let a = mint_fact(prov(), payload());
        let mut diff = payload();
        if let FactPayload::DependencyEdge { edge_kind, .. } = &mut diff {
            *edge_kind = "Reads".into();
        }
        let b = mint_fact(prov(), diff);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn mint_fact_changes_id_when_provenance_changes() {
        let a = mint_fact(prov(), payload());
        let mut other_prov = prov();
        other_prov.component_version = "9.9.9".into();
        let b = mint_fact(other_prov, payload());
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn payload_kind_method_returns_matching_family() {
        let f = mint_fact(prov(), payload());
        assert_eq!(f.kind, FactKind::DependencyEdge);
        assert_eq!(f.payload.kind(), FactKind::DependencyEdge);
    }

    #[test]
    fn store_pushes_and_dedupes_by_id() {
        let mut store = FactStore::default();
        let f = mint_fact(prov(), payload());
        store.push(f.clone());
        store.push(f);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn store_filters_by_kind() {
        let mut store = FactStore::default();
        let decl = mint_fact(
            prov(),
            FactPayload::Declaration {
                decl: DeclId::new(1),
                logical_id: "hr.foo".into(),
            },
        );
        let edge = mint_fact(prov(), payload());
        store.push(decl);
        store.push(edge);
        assert_eq!(store.by_kind(FactKind::Declaration).count(), 1);
        assert_eq!(store.by_kind(FactKind::DependencyEdge).count(), 1);
        assert_eq!(store.by_kind(FactKind::Privilege).count(), 0);
    }

    #[test]
    fn fact_serialises_with_family_tag() {
        let f = mint_fact(prov(), payload());
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"kind\":\"dependency_edge\""));
        assert!(json.contains("\"family\":\"dependency_edge\""));
        assert!(json.contains("fact:"));
    }

    #[test]
    fn fact_round_trips_through_serde() {
        let f = mint_fact(prov(), payload());
        let json = serde_json::to_string(&f).unwrap();
        let back: Fact = serde_json::from_str(&json).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn run_id_omitted_when_empty() {
        let f = mint_fact(prov(), payload());
        let json = serde_json::to_string(&f).unwrap();
        assert!(!json.contains("\"run_id\""));
    }

    #[test]
    fn exception_handler_fact_kind_and_serde() {
        let f = mint_fact(
            prov(),
            FactPayload::ExceptionHandler {
                unit_logical_id: "hr.pay_pkg.run".into(),
                scope: "others".into(),
                body_class: "noop".into(),
            },
        );
        assert_eq!(f.kind, FactKind::ExceptionHandler);
        assert_eq!(f.payload.kind(), FactKind::ExceptionHandler);

        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"kind\":\"exception_handler\""));
        assert!(json.contains("\"family\":\"exception_handler\""));

        let back: Fact = serde_json::from_str(&json).unwrap();
        assert_eq!(back, f);

        let mut store = FactStore::default();
        store.push(f);
        assert_eq!(store.by_kind(FactKind::ExceptionHandler).count(), 1);
        assert_eq!(store.by_kind(FactKind::Privilege).count(), 0);
    }
}
