#![no_main]
//! Coverage-guided fuzz target for live-DB runtime session state.
//!
//! Boundary: `LiveDbRuntime` owns connected-session state, active leases,
//! safety profiles, and preview approval state for live MCP tools. A protocol
//! request can drive these operations in many orders.
//!
//! Oracle: invalid names, stale leases, missing active sessions, and refused
//! safety transitions may return typed errors, but state operations and boxed
//! `oraclemcp-db` connections must never panic.

use std::{sync::Mutex, time::Duration};

use asupersync::{runtime::RuntimeBuilder, Cx};
use async_trait::async_trait;
use libfuzzer_sys::fuzz_target;
use oraclemcp_db::{
    DbError, OracleBackend, OracleBind, OracleConnection, OracleConnectionInfo, OracleRow,
};
use plsql_mcp::{
    BoxedOracleConnection, ConnectionProfile, LiveDbRuntime, LiveSessionLease, SafetyProfile,
};

const MAX_LEN: usize = 64 * 1024;
const MAX_OPS: usize = 256;
const NAME_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_-";

#[derive(Debug, Default)]
struct StubOracleConnection {
    call_timeout: Mutex<Option<Duration>>,
}

impl StubOracleConnection {
    fn boxed() -> BoxedOracleConnection {
        Box::new(Self::default())
    }
}

#[async_trait(?Send)]
impl OracleConnection for StubOracleConnection {
    fn backend(&self) -> OracleBackend {
        OracleBackend::RustOracle
    }

    async fn ping(&self, _cx: &Cx) -> Result<(), DbError> {
        Ok(())
    }

    async fn describe(&self, _cx: &Cx) -> Result<OracleConnectionInfo, DbError> {
        Ok(OracleConnectionInfo {
            backend: Some(OracleBackend::RustOracle),
            current_schema: Some("FUZZ".to_owned()),
            ..OracleConnectionInfo::default()
        })
    }

    async fn query_rows(
        &self,
        _cx: &Cx,
        _sql: &str,
        _binds: &[OracleBind],
    ) -> Result<Vec<OracleRow>, DbError> {
        Ok(Vec::new())
    }

    async fn execute(&self, _cx: &Cx, _sql: &str, _binds: &[OracleBind]) -> Result<u64, DbError> {
        Ok(0)
    }

    fn call_timeout(&self) -> Result<Option<Duration>, DbError> {
        self.call_timeout
            .lock()
            .map(|timeout| *timeout)
            .map_err(|err| DbError::Internal(format!("fuzz timeout lock poisoned: {err}")))
    }

    fn set_call_timeout(&self, timeout: Option<Duration>) -> Result<(), DbError> {
        let mut guard = self
            .call_timeout
            .lock()
            .map_err(|err| DbError::Internal(format!("fuzz timeout lock poisoned: {err}")))?;
        *guard = timeout;
        Ok(())
    }

    async fn commit(&self, _cx: &Cx) -> Result<(), DbError> {
        Ok(())
    }

    async fn rollback(&self, _cx: &Cx) -> Result<(), DbError> {
        Ok(())
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_LEN {
        return;
    }

    let mut runtime = match safety_profile(byte_at(data, 0)) {
        SafetyProfile::SessionWriteEnabled => LiveDbRuntime::new(),
        profile => match LiveDbRuntime::with_default_safety(profile) {
            Ok(runtime) => runtime,
            Err(_) => LiveDbRuntime::new(),
        },
    };
    let mut last_lease: Option<LiveSessionLease> = None;

    for chunk in data.chunks(8).take(MAX_OPS) {
        let op = byte_at(chunk, 0) % 13;
        let name = profile_name(chunk);
        match op {
            0 => {
                let result = runtime.insert_connected(
                    profile(&name, byte_at(chunk, 1)),
                    StubOracleConnection::boxed(),
                );
                drop(result);
            }
            1 => {
                let result = runtime.insert_and_activate(
                    profile(&name, byte_at(chunk, 1)),
                    StubOracleConnection::boxed(),
                );
                if let Ok(lease) = result {
                    last_lease = Some(lease);
                }
            }
            2 => {
                let result = runtime.activate(&name);
                if let Ok(lease) = result {
                    last_lease = Some(lease);
                }
            }
            3 => {
                let result = runtime.remove_connection(&name);
                drop(result);
            }
            4 => {
                let result = runtime.remove_active();
                drop(result);
            }
            5 => {
                let lease = runtime.clear_active();
                drop(lease);
            }
            6 => {
                let result = runtime.session(&name);
                drop(result);
                let result = runtime.active_session();
                drop(result);
            }
            7 => {
                if let Some(lease) = last_lease.as_ref() {
                    let result = runtime.session_for_lease(lease);
                    drop(result);
                }
            }
            8 => {
                let result = runtime.set_active_safety_profile(safety_profile(byte_at(chunk, 2)));
                drop(result);
            }
            9 => {
                let result = runtime.preview_active_sql(
                    operation_text(chunk, "fuzz operation"),
                    operation_text(chunk, "create table fuzz_t(x number)"),
                    operation_text(chunk, "fuzz-token"),
                );
                drop(result);
            }
            10 => {
                if let Ok(session) = runtime.active_session_mut() {
                    let result = session.mint_enable_writes_token(
                        operation_text(chunk, "fuzz write"),
                        operation_text(chunk, "fuzz-token"),
                    );
                    drop(result);
                    let result = session.enable_writes("fuzz-token", 0);
                    drop(result);
                    let result = session.disable_writes();
                    drop(result);
                }
            }
            11 => {
                if let Ok(session) = runtime.active_session() {
                    let result = session.connection().call_timeout();
                    drop(result);
                }
                if let Ok(session) = runtime.active_session_mut() {
                    let timeout = if byte_at(chunk, 3) & 1 == 0 {
                        None
                    } else {
                        Some(Duration::from_millis(u64::from(byte_at(chunk, 4))))
                    };
                    let result = session.connection_mut().set_call_timeout(timeout);
                    drop(result);
                }
            }
            _ => {
                let names: Vec<&str> = runtime.connected_names().collect();
                drop(names);
                let len = runtime.len();
                let empty = runtime.is_empty();
                let active = runtime.active_name();
                let lease = runtime.active_lease();
                let default_profile = runtime.default_safety_profile();
                let debug_text =
                    format!("{runtime:?}:{len}:{empty}:{active:?}:{lease:?}:{default_profile:?}");
                drop(debug_text);
            }
        }
    }

    if byte_at(data, 1) & 1 != 0 {
        let Ok(asuper_runtime) = RuntimeBuilder::current_thread().build() else {
            return;
        };
        asuper_runtime.block_on(async {
            let Some(cx) = Cx::current() else {
                return;
            };
            if let Ok(session) = runtime.active_session() {
                let ping = session.connection().ping(&cx).await;
                drop(ping);
                let describe = session.connection().describe(&cx).await;
                drop(describe);
            }
        });
    }
});

fn byte_at(data: &[u8], index: usize) -> u8 {
    data.get(index).copied().unwrap_or_default()
}

fn profile(name: &str, discriminator: u8) -> ConnectionProfile {
    ConnectionProfile {
        name: name.to_owned(),
        description: Some(format!("fuzz profile {name}")),
        connect_string: format!("//localhost/{name}"),
        username: Some("fuzz".to_owned()),
        permanently_read_only: discriminator & 0b0000_0001 != 0,
        dbtools_alias: (discriminator & 0b0000_0010 != 0).then(|| name.to_owned()),
    }
}

fn profile_name(data: &[u8]) -> String {
    if byte_at(data, 1) == 0 {
        return String::new();
    }
    let mut name = String::with_capacity(8);
    for byte in data.iter().copied().skip(1).take(7) {
        let index = usize::from(byte) % NAME_ALPHABET.len();
        let Some(ch) = NAME_ALPHABET.get(index).copied() else {
            continue;
        };
        name.push(char::from(ch));
    }
    name
}

fn safety_profile(byte: u8) -> SafetyProfile {
    match byte % 4 {
        0 => SafetyProfile::StaticOnly,
        1 => SafetyProfile::InspectOnly,
        2 => SafetyProfile::DdlGuarded,
        _ => SafetyProfile::SessionWriteEnabled,
    }
}

fn operation_text(data: &[u8], fallback: &str) -> String {
    let text: String = data
        .iter()
        .copied()
        .skip(1)
        .take(6)
        .map(|byte| char::from(32 + (byte % 95)))
        .collect();
    if text.trim().is_empty() {
        fallback.to_owned()
    } else {
        text
    }
}
