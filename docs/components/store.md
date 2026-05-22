# plsql-store

Content-addressed SQLite cache for analysis artifacts. Layer 0.

## Purpose

Parsing and analysing a large Oracle estate is expensive enough that we
keep cached `AnalysisRun` artifacts addressable by content hash. The store
lets every consumer ask "do you already have a parse for SHA-256:abcd…?"
before re-doing the work, and lets the daemon mode service repeated
queries cheaply.

## Surface

| Type | Purpose |
|------|---------|
| `Store` | Handle on an open SQLite database |
| `BlobHash` | Content-addressed identifier (sha256 + size) |
| `cache_strategy` registry | Maps artifact kind → eviction policy |

## Modes

| Mode | Lifecycle |
|------|-----------|
| **Embedded** | Library users open a `Store` directly; no daemon |
| **Daemon** | `plsql-store-daemon` exposes the cache over IPC for concurrent CLIs |
| **Read-only / immutable** | CI uses a write-once store baked at build time |

## Invariants

- **WAL + busy_timeout** mandatory for concurrent reader safety.
- **`PRAGMA synchronous = NORMAL`** in daemon mode; `FULL` in CI.
- **Schema version pin** stored in `meta` table — startup refuses to open
  a store from an incompatible version, never silently migrates.

## Pointers

- Source: `crates/plsql-store/src/`
- Plan: `plan.md` §6.2 (Layer 0), §18.7 (cache strategy), §10A (analysis engine)
- Consumers: `plsql-engine`, every CLI in daemon mode
