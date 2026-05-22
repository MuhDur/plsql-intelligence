//! The `usr-loop scan` integration test (spec §10 P1 exit
//! criterion) lives in `tools/usr-loop/tests/scan_integration.rs`,
//! not here.
//!
//! Rationale: it must spawn the real `usr-loop` binary via
//! `CARGO_BIN_EXE_usr-loop`, which Cargo only defines for the
//! binary's *own* crate. Placing it here would require
//! `plsql-accretion` to dev-depend on `usr-loop` (which depends on
//! `plsql-accretion`) — a reverse edge that violates the
//! one-directional layering rule (R20 / spec §6). The test is
//! therefore co-located with the binary it drives.
//!
//! This file is intentionally empty (a no-op test target) to keep
//! the relocation discoverable from the crate that owns the schema.

#[test]
fn scan_integration_lives_in_tools_usr_loop() {
    // Pointer test — see tools/usr-loop/tests/scan_integration.rs.
}
