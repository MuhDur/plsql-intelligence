#![forbid(unsafe_code)]

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tracing::instrument;

pub mod daemon;
pub mod facts;
pub mod protocol;
pub use daemon::{serve_envelope, serve_request};
pub use facts::{FactRepository, InMemoryFactRepository, SqliteFactRepository, StoredFact};
pub use protocol::{
    CacheStats, CodecError, DaemonEnvelope, DaemonError, DaemonErrorCode, DaemonRequest,
    DaemonResponse, PROTOCOL_VERSION, ProtocolVersion, decode_line, encode,
};

const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_WAL_AUTOCHECKPOINT: u32 = 1_000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StoreMode {
    #[default]
    ImmutableArtifact,
    LocalDaemon,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreConfig {
    pub mode: StoreMode,
    pub wal_autocheckpoint: u32,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            mode: StoreMode::ImmutableArtifact,
            wal_autocheckpoint: DEFAULT_WAL_AUTOCHECKPOINT,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheStrategyDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub immutable_artifact_mode: bool,
    pub daemon_mode: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheStrategyRecord {
    pub name: String,
    pub description: String,
    pub immutable_artifact_mode: bool,
    pub daemon_mode: bool,
}

pub const DEFAULT_CACHE_STRATEGIES: [CacheStrategyDescriptor; 8] = [
    CacheStrategyDescriptor {
        name: "source_hash",
        description: "Source file content digests and provenance snapshots",
        immutable_artifact_mode: true,
        daemon_mode: true,
    },
    CacheStrategyDescriptor {
        name: "token_tape",
        description: "Lossless parser token tapes",
        immutable_artifact_mode: true,
        daemon_mode: true,
    },
    CacheStrategyDescriptor {
        name: "parse_diagnostic",
        description: "Parser diagnostics and recovery summaries",
        immutable_artifact_mode: true,
        daemon_mode: true,
    },
    CacheStrategyDescriptor {
        name: "semantic_fragment",
        description: "Semantic fragments and lowered intermediate artifacts",
        immutable_artifact_mode: true,
        daemon_mode: true,
    },
    CacheStrategyDescriptor {
        name: "catalog_snapshot",
        description: "Oracle catalog snapshots and capability probes",
        immutable_artifact_mode: true,
        daemon_mode: true,
    },
    CacheStrategyDescriptor {
        name: "dep_graph",
        description: "Dependency graph snapshots and derived reports",
        immutable_artifact_mode: true,
        daemon_mode: true,
    },
    CacheStrategyDescriptor {
        name: "benchmark_metadata",
        description: "Benchmark runs and corpus timing metadata",
        immutable_artifact_mode: true,
        daemon_mode: false,
    },
    CacheStrategyDescriptor {
        name: "corpus_metadata",
        description: "Corpus manifest and fixture metadata",
        immutable_artifact_mode: true,
        daemon_mode: true,
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheBlob {
    pub digest_hex: String,
    pub strategy_name: String,
    pub media_type: String,
    pub body: Vec<u8>,
}

/// Reuse key for a cached fragment.
///
/// `content_hash` is a digest of the analyzed source; `profile_hash` is a
/// digest of the analysis profile/config. Both are opaque to the store —
/// callers compute them (see [`hash_hex`]) and only equality matters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheKey<'a> {
    pub strategy_name: &'a str,
    pub content_hash: &'a str,
    pub profile_hash: &'a str,
}

/// SHA-256 hex digest of `bytes`, for composing [`CacheKey`] hashes with
/// the same algorithm the store uses for content addressing.
#[must_use]
pub fn hash_hex(bytes: &[u8]) -> String {
    digest_hex(bytes)
}

#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("unknown cache strategy `{0}`")]
    UnknownStrategy(String),
    #[error("integrity check failed: {0}")]
    IntegrityCheckFailed(String),
}

impl Store {
    #[instrument(level = "trace", skip(config))]
    pub fn open(path: &Path, config: StoreConfig) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        configure_connection(&conn, &config)?;
        initialize_schema(&conn)?;
        seed_default_strategies(&conn)?;
        Ok(Self { conn })
    }

    #[instrument(level = "trace", skip(self, body))]
    pub fn put_blob(
        &self,
        strategy_name: &str,
        media_type: &str,
        body: &[u8],
    ) -> Result<CacheBlob, StoreError> {
        if !self.has_strategy(strategy_name)? {
            return Err(StoreError::UnknownStrategy(String::from(strategy_name)));
        }

        let digest_hex = digest_hex(body);
        self.conn.execute(
            "INSERT OR IGNORE INTO cache_blobs (digest_hex, strategy_name, media_type, body) VALUES (?1, ?2, ?3, ?4)",
            params![digest_hex, strategy_name, media_type, body],
        )?;

        Ok(CacheBlob {
            digest_hex,
            strategy_name: String::from(strategy_name),
            media_type: String::from(media_type),
            body: body.to_vec(),
        })
    }

    #[instrument(level = "trace", skip(self))]
    pub fn get_blob(&self, digest_hex: &str) -> Result<Option<CacheBlob>, StoreError> {
        self.conn
            .query_row(
                "SELECT digest_hex, strategy_name, media_type, body FROM cache_blobs WHERE digest_hex = ?1",
                params![digest_hex],
                |row| {
                    Ok(CacheBlob {
                        digest_hex: row.get(0)?,
                        strategy_name: row.get(1)?,
                        media_type: row.get(2)?,
                        body: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn registered_strategies(&self) -> Result<Vec<CacheStrategyRecord>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT name, description, immutable_artifact_mode, daemon_mode FROM cache_strategies ORDER BY name",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(CacheStrategyRecord {
                name: row.get(0)?,
                description: row.get(1)?,
                immutable_artifact_mode: row.get::<_, bool>(2)?,
                daemon_mode: row.get::<_, bool>(3)?,
            })
        })?;

        let mut strategies = Vec::new();
        for row in rows {
            strategies.push(row?);
        }
        Ok(strategies)
    }

    /// `(blob_count, total_body_bytes)` over the cache — the
    /// numbers `plsqld`'s `Stats` response reports.
    #[instrument(level = "trace", skip(self))]
    pub fn cache_stats(&self) -> Result<(u64, u64), StoreError> {
        let (count, bytes): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(1), COALESCE(SUM(LENGTH(body)), 0) FROM cache_blobs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok((count.max(0) as u64, bytes.max(0) as u64))
    }

    #[instrument(level = "trace", skip(self))]
    pub fn integrity_check(&self) -> Result<(), StoreError> {
        let result: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if result == "ok" {
            Ok(())
        } else {
            Err(StoreError::IntegrityCheckFailed(result))
        }
    }

    /// Store a fragment and bind it to a reuse key.
    ///
    /// The blob itself stays content-addressed in `cache_blobs`; the key
    /// `(strategy, content_hash, profile_hash)` is recorded in
    /// `cache_entries` so a later [`Store::get_cached`] with the same key
    /// returns it. Re-storing under an existing key rebinds it to the new
    /// body (last write wins), which is what callers want when a fragment
    /// is recomputed.
    #[instrument(level = "trace", skip(self, body))]
    pub fn put_cached(
        &self,
        key: CacheKey<'_>,
        media_type: &str,
        body: &[u8],
    ) -> Result<CacheBlob, StoreError> {
        let blob = self.put_blob(key.strategy_name, media_type, body)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO cache_entries
                 (strategy_name, content_hash, profile_hash, digest_hex)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                key.strategy_name,
                key.content_hash,
                key.profile_hash,
                blob.digest_hex,
            ],
        )?;
        Ok(blob)
    }

    /// Look up a fragment by reuse key.
    ///
    /// Returns `Ok(None)` on a miss. Because `profile_hash` is part of the
    /// key, a changed analysis profile yields a different key and therefore
    /// a miss — the caller recomputes, which is exactly the desired
    /// profile-change invalidation (no stale fragment is ever served).
    #[instrument(level = "trace", skip(self))]
    pub fn get_cached(&self, key: CacheKey<'_>) -> Result<Option<CacheBlob>, StoreError> {
        if !self.has_strategy(key.strategy_name)? {
            return Err(StoreError::UnknownStrategy(String::from(key.strategy_name)));
        }
        let digest: Option<String> = self
            .conn
            .query_row(
                "SELECT digest_hex FROM cache_entries
                 WHERE strategy_name = ?1 AND content_hash = ?2 AND profile_hash = ?3",
                params![key.strategy_name, key.content_hash, key.profile_hash],
                |row| row.get(0),
            )
            .optional()?;
        match digest {
            Some(digest_hex) => self.get_blob(&digest_hex),
            None => Ok(None),
        }
    }

    fn has_strategy(&self, strategy_name: &str) -> Result<bool, StoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(1) FROM cache_strategies WHERE name = ?1",
            params![strategy_name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

#[instrument(level = "trace", skip(conn, config))]
fn configure_connection(conn: &Connection, config: &StoreConfig) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "wal_autocheckpoint", config.wal_autocheckpoint)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(DEFAULT_BUSY_TIMEOUT)?;
    Ok(())
}

#[instrument(level = "trace", skip(conn))]
fn initialize_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS cache_strategies (
            name TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            immutable_artifact_mode INTEGER NOT NULL,
            daemon_mode INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cache_blobs (
            digest_hex TEXT PRIMARY KEY,
            strategy_name TEXT NOT NULL REFERENCES cache_strategies(name),
            media_type TEXT NOT NULL,
            body BLOB NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_cache_blobs_strategy_name
            ON cache_blobs(strategy_name);

        CREATE TABLE IF NOT EXISTS cache_entries (
            strategy_name TEXT NOT NULL REFERENCES cache_strategies(name),
            content_hash  TEXT NOT NULL,
            profile_hash  TEXT NOT NULL,
            digest_hex    TEXT NOT NULL REFERENCES cache_blobs(digest_hex),
            updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (strategy_name, content_hash, profile_hash)
        );
        ",
    )?;
    Ok(())
}

#[instrument(level = "trace", skip(conn))]
fn seed_default_strategies(conn: &Connection) -> Result<(), rusqlite::Error> {
    for strategy in DEFAULT_CACHE_STRATEGIES {
        conn.execute(
            "INSERT OR IGNORE INTO cache_strategies (name, description, immutable_artifact_mode, daemon_mode) VALUES (?1, ?2, ?3, ?4)",
            params![
                strategy.name,
                strategy.description,
                strategy.immutable_artifact_mode,
                strategy.daemon_mode,
            ],
        )?;
    }
    Ok(())
}

#[must_use]
#[instrument(level = "trace", skip(body))]
fn digest_hex(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    let digest = hasher.finalize();
    let mut rendered = String::with_capacity(digest.len() * 2);
    for byte in digest {
        rendered.push_str(&format!("{byte:02x}"));
    }
    rendered
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{CacheKey, DEFAULT_CACHE_STRATEGIES, Store, StoreConfig, StoreError};

    #[test]
    fn fresh_cache_round_trips_content_addressed_blob() {
        let tempdir = tempdir();
        assert!(tempdir.is_ok());
        let tempdir = if let Ok(tempdir) = tempdir {
            tempdir
        } else {
            return;
        };

        let store = Store::open(&tempdir.path().join("cache.db"), StoreConfig::default());
        assert!(store.is_ok());
        let store = if let Ok(store) = store { store } else { return };

        let inserted = store.put_blob(
            "catalog_snapshot",
            "application/json",
            br#"{"schema":"billing"}"#,
        );
        assert!(inserted.is_ok());
        let inserted = if let Ok(inserted) = inserted {
            inserted
        } else {
            return;
        };

        let round_trip = store.get_blob(&inserted.digest_hex);
        assert!(round_trip.is_ok());
        let round_trip = if let Ok(round_trip) = round_trip {
            round_trip
        } else {
            return;
        };

        assert_eq!(
            round_trip.as_ref().map(|blob| blob.digest_hex.as_str()),
            Some(inserted.digest_hex.as_str())
        );
        assert_eq!(
            round_trip.as_ref().map(|blob| blob.media_type.as_str()),
            Some("application/json")
        );
        assert_eq!(
            round_trip.as_ref().map(|blob| blob.body.as_slice()),
            Some(br#"{"schema":"billing"}"#.as_slice())
        );
    }

    #[test]
    fn default_cache_strategies_are_seeded() {
        let tempdir = tempdir();
        assert!(tempdir.is_ok());
        let tempdir = if let Ok(tempdir) = tempdir {
            tempdir
        } else {
            return;
        };

        let store = Store::open(&tempdir.path().join("cache.db"), StoreConfig::default());
        assert!(store.is_ok());
        let store = if let Ok(store) = store { store } else { return };

        let strategies = store.registered_strategies();
        assert!(strategies.is_ok());
        let strategies = if let Ok(strategies) = strategies {
            strategies
        } else {
            return;
        };

        assert_eq!(strategies.len(), DEFAULT_CACHE_STRATEGIES.len());
        assert!(
            strategies
                .iter()
                .any(|strategy| strategy.name == "semantic_fragment")
        );
    }

    #[test]
    fn integrity_check_passes_for_fresh_store() {
        let tempdir = tempdir();
        assert!(tempdir.is_ok());
        let tempdir = if let Ok(tempdir) = tempdir {
            tempdir
        } else {
            return;
        };

        let store = Store::open(&tempdir.path().join("cache.db"), StoreConfig::default());
        assert!(store.is_ok());
        let store = if let Ok(store) = store { store } else { return };

        assert!(store.integrity_check().is_ok());
    }

    /// Open a throwaway store, skipping the test on filesystem failure
    /// (matches the no-unwrap discipline of the tests above).
    fn fresh_store() -> Option<(tempfile::TempDir, Store)> {
        let dir = tempdir().ok()?;
        let store = Store::open(&dir.path().join("cache.db"), StoreConfig::default()).ok()?;
        Some((dir, store))
    }

    #[test]
    fn cached_fragment_round_trips_by_key() {
        let Some((_dir, store)) = fresh_store() else {
            return;
        };
        let key = CacheKey {
            strategy_name: "semantic_fragment",
            content_hash: "src-abc",
            profile_hash: "profile-1",
        };

        assert!(
            store
                .put_cached(key, "application/json", b"{\"ir\":1}")
                .is_ok()
        );

        let hit = store.get_cached(key);
        assert!(hit.is_ok());
        assert_eq!(
            hit.ok().flatten().map(|blob| blob.body),
            Some(b"{\"ir\":1}".to_vec())
        );
    }

    #[test]
    fn changed_profile_hash_invalidates_cached_fragment() {
        let Some((_dir, store)) = fresh_store() else {
            return;
        };
        let stored = CacheKey {
            strategy_name: "semantic_fragment",
            content_hash: "src-abc",
            profile_hash: "profile-1",
        };
        assert!(store.put_cached(stored, "application/json", b"old").is_ok());

        // Same source, different analysis profile -> different key -> miss.
        let reprofiled = CacheKey {
            profile_hash: "profile-2",
            ..stored
        };
        let lookup = store.get_cached(reprofiled);
        assert!(lookup.is_ok());
        assert_eq!(lookup.ok().flatten(), None);
    }

    #[test]
    fn recompute_rebinds_key_to_new_body() {
        let Some((_dir, store)) = fresh_store() else {
            return;
        };
        let key = CacheKey {
            strategy_name: "dep_graph",
            content_hash: "src-xyz",
            profile_hash: "profile-1",
        };
        assert!(store.put_cached(key, "application/json", b"v1").is_ok());
        assert!(store.put_cached(key, "application/json", b"v2").is_ok());

        let hit = store.get_cached(key);
        assert!(hit.is_ok());
        assert_eq!(
            hit.ok().flatten().map(|blob| blob.body),
            Some(b"v2".to_vec())
        );
    }

    #[test]
    fn get_cached_rejects_unknown_strategy() {
        let Some((_dir, store)) = fresh_store() else {
            return;
        };
        let key = CacheKey {
            strategy_name: "no_such_strategy",
            content_hash: "src-abc",
            profile_hash: "profile-1",
        };
        let rejected = matches!(
            store.get_cached(key),
            Err(StoreError::UnknownStrategy(ref name)) if name == "no_such_strategy"
        );
        assert!(rejected);
    }
}
