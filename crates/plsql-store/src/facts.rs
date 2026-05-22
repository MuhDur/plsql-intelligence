//! Fact persistence + query (PLSQL-FACT-005).
//!
//! The normalized fact stream (PLSQL-FACT-001/002/004, defined in
//! `plsql-ir`) needs a durable home so a later engine run, a CLI
//! query, or a consumer process can read facts back without
//! re-analysing source. This module provides a backend-agnostic
//! [`FactRepository`] with two implementations:
//!
//! * [`InMemoryFactRepository`] — process-lifetime, ideal for tests
//!   and one-shot CLI runs that never touch disk.
//! * [`SqliteFactRepository`] — durable, file- or memory-backed
//!   SQLite, ideal for the local daemon and incremental runs.
//!
//! Layer hygiene: this module does **not** depend on `plsql-ir`'s
//! `Fact` type. A caller serialises a fact into a flat
//! [`StoredFact`] (`id` + `kind` + canonical `payload_json`) before
//! handing it over — the same decoupling the IR layer uses for
//! `DeclLike`. Persisting JSON keeps the store schema stable as the
//! fact payload enum grows.
//!
//! De-duplication is by `id`: the IR layer mints `fact:<sha256>`
//! ids over `(kind, provenance, payload)`, so `put` returning
//! `false` means "this exact fact was already stored", matching the
//! emitter's post-dedup count contract.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::StoreError;

/// A fact flattened for storage. `payload_json` is the canonical
/// JSON the IR layer produced for the fact's typed payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredFact {
    /// Stable `fact:<hex>` id minted by the IR layer.
    pub id: String,
    /// Family discriminator (`declaration`, `privilege`,
    /// `dynamic_sql_evidence`, `opacity`, …) for cheap filtering
    /// without parsing the payload.
    pub kind: String,
    /// Canonical JSON of the fact (id + kind + provenance + payload).
    pub payload_json: String,
}

/// Backend-agnostic persistence + query surface for facts.
pub trait FactRepository {
    /// Insert a fact. Returns `Ok(true)` if it was newly stored,
    /// `Ok(false)` if a fact with the same `id` was already present
    /// (dedup, not an error).
    fn put(&mut self, fact: &StoredFact) -> Result<bool, StoreError>;

    /// Fetch a single fact by id.
    fn get(&self, id: &str) -> Result<Option<StoredFact>, StoreError>;

    /// All facts of a family, ordered by id for deterministic output.
    fn by_kind(&self, kind: &str) -> Result<Vec<StoredFact>, StoreError>;

    /// Total fact count.
    fn len(&self) -> Result<usize, StoreError>;

    /// Whether the repository holds no facts.
    fn is_empty(&self) -> Result<bool, StoreError> {
        Ok(self.len()? == 0)
    }
}

/// Process-lifetime fact store. `BTreeMap` keeps iteration ordered
/// by id so `by_kind` is deterministic without an explicit sort.
#[derive(Clone, Debug, Default)]
pub struct InMemoryFactRepository {
    facts: BTreeMap<String, StoredFact>,
}

impl InMemoryFactRepository {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl FactRepository for InMemoryFactRepository {
    fn put(&mut self, fact: &StoredFact) -> Result<bool, StoreError> {
        if self.facts.contains_key(&fact.id) {
            return Ok(false);
        }
        self.facts.insert(fact.id.clone(), fact.clone());
        Ok(true)
    }

    fn get(&self, id: &str) -> Result<Option<StoredFact>, StoreError> {
        Ok(self.facts.get(id).cloned())
    }

    fn by_kind(&self, kind: &str) -> Result<Vec<StoredFact>, StoreError> {
        Ok(self
            .facts
            .values()
            .filter(|f| f.kind == kind)
            .cloned()
            .collect())
    }

    fn len(&self) -> Result<usize, StoreError> {
        Ok(self.facts.len())
    }
}

/// SQLite-backed fact store. Owns its own connection + `facts`
/// table, independent of the artifact-cache [`crate::Store`] so the
/// two schemas evolve separately.
#[derive(Debug)]
pub struct SqliteFactRepository {
    conn: Connection,
}

impl SqliteFactRepository {
    /// Open a file-backed repository, creating the schema if needed.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// Open an ephemeral in-memory SQLite repository — durable for
    /// the connection's lifetime, useful for tests and transient
    /// daemon scratch state.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self, StoreError> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS facts (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_facts_kind ON facts(kind);
            ",
        )?;
        Ok(Self { conn })
    }
}

impl FactRepository for SqliteFactRepository {
    fn put(&mut self, fact: &StoredFact) -> Result<bool, StoreError> {
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO facts (id, kind, payload_json) VALUES (?1, ?2, ?3)",
            params![fact.id, fact.kind, fact.payload_json],
        )?;
        Ok(changed == 1)
    }

    fn get(&self, id: &str) -> Result<Option<StoredFact>, StoreError> {
        self.conn
            .query_row(
                "SELECT id, kind, payload_json FROM facts WHERE id = ?1",
                params![id],
                |row| {
                    Ok(StoredFact {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        payload_json: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn by_kind(&self, kind: &str) -> Result<Vec<StoredFact>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, kind, payload_json FROM facts WHERE kind = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![kind], |row| {
            Ok(StoredFact {
                id: row.get(0)?,
                kind: row.get(1)?,
                payload_json: row.get(2)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn len(&self) -> Result<usize, StoreError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(1) FROM facts", [], |row| row.get(0))?;
        Ok(usize::try_from(n).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(id: &str, kind: &str) -> StoredFact {
        StoredFact {
            id: id.into(),
            kind: kind.into(),
            payload_json: format!("{{\"id\":\"{id}\",\"kind\":\"{kind}\"}}"),
        }
    }

    /// One conformance body run against both backends so the two
    /// implementations cannot drift in observable behaviour.
    fn round_trip_contract<R: FactRepository>(repo: &mut R) {
        assert!(repo.is_empty().unwrap());

        assert!(repo.put(&fact("fact:a", "privilege")).unwrap());
        assert!(repo.put(&fact("fact:b", "privilege")).unwrap());
        assert!(repo.put(&fact("fact:c", "opacity")).unwrap());

        // Re-putting the same id is a dedup no-op, not an error.
        assert!(!repo.put(&fact("fact:a", "privilege")).unwrap());

        assert_eq!(repo.len().unwrap(), 3);
        assert!(!repo.is_empty().unwrap());

        let got = repo.get("fact:b").unwrap().expect("present");
        assert_eq!(got.kind, "privilege");
        assert!(repo.get("fact:missing").unwrap().is_none());

        let priv_facts = repo.by_kind("privilege").unwrap();
        assert_eq!(priv_facts.len(), 2);
        // Deterministic id ordering.
        assert_eq!(priv_facts[0].id, "fact:a");
        assert_eq!(priv_facts[1].id, "fact:b");

        assert_eq!(repo.by_kind("opacity").unwrap().len(), 1);
        assert!(repo.by_kind("declaration").unwrap().is_empty());
    }

    #[test]
    fn in_memory_backend_satisfies_contract() {
        round_trip_contract(&mut InMemoryFactRepository::new());
    }

    #[test]
    fn sqlite_memory_backend_satisfies_contract() {
        round_trip_contract(&mut SqliteFactRepository::open_in_memory().unwrap());
    }

    #[test]
    fn sqlite_file_backend_persists_across_reopen() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("plsql_facts_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let mut repo = SqliteFactRepository::open(&path).unwrap();
            assert!(repo.put(&fact("fact:persist", "reference")).unwrap());
        }
        {
            let repo = SqliteFactRepository::open(&path).unwrap();
            assert_eq!(repo.len().unwrap(), 1);
            assert_eq!(repo.get("fact:persist").unwrap().unwrap().kind, "reference");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn backends_agree_on_dedup_semantics() {
        let mut mem = InMemoryFactRepository::new();
        let mut sql = SqliteFactRepository::open_in_memory().unwrap();
        let f = fact("fact:dup", "privilege");
        assert_eq!(mem.put(&f).unwrap(), sql.put(&f).unwrap());
        assert_eq!(mem.put(&f).unwrap(), sql.put(&f).unwrap());
        assert_eq!(mem.len().unwrap(), sql.len().unwrap());
    }
}
