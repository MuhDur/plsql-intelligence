//! Connection-management surface for the live-DB tools (`PLSQL-MCP-LIVE-002`).
//!
//! `plsql-mcp` exposes five connection management tools — `list_connections`,
//! `connect`, `disconnect`, `current_database`, and `switch_database` — that
//! wrap the same `OracleConnection` abstraction `plsql-catalog extract` uses
//! (D16). Credentials live in `~/.plsql-mcp/connections.toml` and optionally
//! mirror `~/.dbtools` entries via the [`DbToolsAlias`] resolver.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::safety::SafetyProfile;

/// A single named Oracle connection profile loaded from
/// `~/.plsql-mcp/connections.toml` (or, when the matching key is reused,
/// from a `~/.dbtools` saved profile via [`DbToolsAlias`]).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectionProfile {
    /// Stable identifier the agent calls — e.g. `"billing-dev"`.
    pub name: String,
    /// Friendly description shown in `list_connections` output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Connect identifier (TNS alias, EZConnect string, or wallet alias).
    pub connect_string: String,
    /// Optional Oracle username. `None` when the credential lives elsewhere
    /// (wallet, OS auth, OCI IAM).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Whether this connection is permanently read-only — `enable_writes`
    /// is rejected unconditionally for the lifetime of the process.
    #[serde(default)]
    pub permanently_read_only: bool,
    /// Optional `dbtools` alias the profile mirrors. The
    /// [`DbToolsAlias::resolve`] step copies fields from the matching
    /// `~/.dbtools` entry when the alias key matches a row there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dbtools_alias: Option<String>,
}

impl ConnectionProfile {
    /// A "production-looking" connect string per the §13A.3 heuristic
    /// (matches `prod` / `production` / configured production allowlist).
    /// The doctor uses this to flag any connection lacking
    /// `permanently_read_only` against a production DSN.
    #[must_use]
    pub fn is_production_looking(&self) -> bool {
        let lower = self.connect_string.to_ascii_lowercase();
        lower.contains("prod") || lower.contains("production")
    }
}

/// Result of resolving a `dbtools` alias against the live `~/.dbtools`
/// store. `available` is true when the alias was located; `connect_string`
/// is the resolved EZConnect / TNS alias if reading the store succeeded.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DbToolsAlias {
    pub alias: String,
    pub available: bool,
    pub connect_string: Option<String>,
    pub source: Option<PathBuf>,
}

impl DbToolsAlias {
    /// Look up `alias` in the user's `~/.dbtools` store. The bead skeleton
    /// performs only a structural check — does the file exist and contain a
    /// section named after the alias? — and defers full parsing to the
    /// concrete `connect` implementation in `PLSQL-MCP-LIVE-003`.
    #[must_use]
    pub fn probe(alias: &str, home: &Path) -> Self {
        let candidates = ["dbtools.json", ".dbtools", ".dbtools.conf"];
        for candidate in candidates {
            let path = home.join(candidate);
            if !path.is_file() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if text.contains(alias) {
                return Self {
                    alias: String::from(alias),
                    available: true,
                    connect_string: None,
                    source: Some(path),
                };
            }
        }
        Self {
            alias: String::from(alias),
            available: false,
            connect_string: None,
            source: None,
        }
    }
}

/// `~/.plsql-mcp/connections.toml` document layout
/// (`PLSQL-MCP-LIVE-009`). Each `[[connection]]` table becomes a
/// [`ConnectionProfile`].
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectionsToml {
    #[serde(default, rename = "connection")]
    pub connections: Vec<ConnectionProfile>,
}

impl ConnectionsToml {
    /// Parse a TOML document into a [`ConnectionsToml`].
    pub fn from_toml_str(text: &str) -> Result<Self, ConnectionError> {
        toml::from_str(text).map_err(|err| ConnectionError::TomlParse {
            message: err.to_string(),
        })
    }
}

/// In-process registry of named connection profiles. Tools borrow from this
/// instead of holding their own copies, which keeps `switch_database` /
/// `disconnect` operating on shared state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectionRegistry {
    profiles: BTreeMap<String, ConnectionProfile>,
    active: Option<String>,
    safety: SafetyProfile,
}

impl ConnectionRegistry {
    #[must_use]
    pub fn new(safety: SafetyProfile) -> Self {
        Self {
            profiles: BTreeMap::new(),
            active: None,
            safety,
        }
    }

    /// Register a profile. Returns the previous entry if `name` was reused.
    pub fn register(&mut self, profile: ConnectionProfile) -> Option<ConnectionProfile> {
        self.profiles.insert(profile.name.clone(), profile)
    }

    /// Iterate registered profiles in stable (name-sorted) order.
    pub fn profiles(&self) -> impl Iterator<Item = &ConnectionProfile> {
        self.profiles.values()
    }

    /// Snapshot of the registry exposed via the `list_connections` tool.
    #[must_use]
    pub fn list(&self) -> Vec<ConnectionListEntry> {
        self.profiles
            .values()
            .map(|profile| ConnectionListEntry {
                name: profile.name.clone(),
                description: profile.description.clone(),
                connect_string: profile.connect_string.clone(),
                username: profile.username.clone(),
                permanently_read_only: profile.permanently_read_only,
                is_active: self.active.as_deref() == Some(profile.name.as_str()),
            })
            .collect()
    }

    /// Mark `name` as the active profile. Returns the resolved profile, or
    /// a `ConnectionError::UnknownProfile` if the name is not registered.
    pub fn connect(&mut self, name: &str) -> Result<&ConnectionProfile, ConnectionError> {
        if !self.profiles.contains_key(name) {
            return Err(ConnectionError::UnknownProfile {
                name: String::from(name),
            });
        }
        self.active = Some(String::from(name));
        Ok(&self.profiles[name])
    }

    /// Clear the active profile. Returns the formerly-active profile name
    /// if there was one; `None` is a no-op.
    pub fn disconnect(&mut self) -> Option<String> {
        self.active.take()
    }

    /// Returns the currently-active profile.
    #[must_use]
    pub fn current(&self) -> Option<&ConnectionProfile> {
        self.active
            .as_deref()
            .and_then(|name| self.profiles.get(name))
    }

    /// Switch the active profile, returning the previously-active profile
    /// name and the newly-active profile.
    pub fn switch(
        &mut self,
        name: &str,
    ) -> Result<(Option<String>, &ConnectionProfile), ConnectionError> {
        if !self.profiles.contains_key(name) {
            return Err(ConnectionError::UnknownProfile {
                name: String::from(name),
            });
        }
        let previous = self.active.take();
        self.active = Some(String::from(name));
        Ok((previous, &self.profiles[name]))
    }

    /// Active safety profile (mirrors the binary-wide setting; tools
    /// override per-call when they need a stricter posture).
    #[must_use]
    pub fn safety(&self) -> SafetyProfile {
        self.safety
    }

    /// Update the safety profile. Returns an error if a `permanently_read_only`
    /// connection is active and `next` would allow writes.
    pub fn set_safety(&mut self, next: SafetyProfile) -> Result<(), ConnectionError> {
        if next.allows_direct_writes() {
            if let Some(profile) = self.current() {
                if profile.permanently_read_only {
                    return Err(ConnectionError::PermanentlyReadOnly {
                        name: profile.name.clone(),
                    });
                }
            }
        }
        self.safety = next;
        Ok(())
    }
}

/// One row in the `list_connections` tool's structured output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectionListEntry {
    pub name: String,
    pub description: Option<String>,
    pub connect_string: String,
    pub username: Option<String>,
    pub permanently_read_only: bool,
    pub is_active: bool,
}

/// Errors raised by the connection-management tools.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum ConnectionError {
    #[error(
        "no connection profile named `{name}`; call `list_connections` to see registered profiles"
    )]
    UnknownProfile { name: String },
    #[error("active connection `{name}` is permanently_read_only; cannot enable writes")]
    PermanentlyReadOnly { name: String },
    #[error("no active connection; call `connect <name>` first")]
    NoActiveConnection,
    #[error("connections.toml parse error: {message}")]
    TomlParse { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs;

    fn fixture(name: &str, prod: bool) -> ConnectionProfile {
        ConnectionProfile {
            name: String::from(name),
            description: Some(format!("{name} fixture")),
            connect_string: if prod {
                String::from("//prod-host/PROD_DB")
            } else {
                String::from("//localhost/DEV_DB")
            },
            username: Some(String::from("scott")),
            permanently_read_only: prod,
            dbtools_alias: None,
        }
    }

    #[test]
    fn registry_lists_profiles_in_name_sorted_order() {
        let mut registry = ConnectionRegistry::new(SafetyProfile::InspectOnly);
        registry.register(fixture("zeta", false));
        registry.register(fixture("alpha", false));
        let names: Vec<String> = registry.list().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
    }

    #[test]
    fn connect_then_current_returns_active_profile() {
        let mut registry = ConnectionRegistry::new(SafetyProfile::InspectOnly);
        registry.register(fixture("alpha", false));
        assert!(registry.current().is_none());
        let active = registry.connect("alpha").unwrap();
        assert_eq!(active.name, "alpha");
        assert!(registry.current().is_some());
    }

    #[test]
    fn connect_to_unknown_profile_errors() {
        let mut registry = ConnectionRegistry::new(SafetyProfile::InspectOnly);
        let err = registry.connect("missing").unwrap_err();
        assert!(matches!(err, ConnectionError::UnknownProfile { .. }));
    }

    #[test]
    fn disconnect_clears_active_and_is_idempotent() {
        let mut registry = ConnectionRegistry::new(SafetyProfile::InspectOnly);
        registry.register(fixture("alpha", false));
        registry.connect("alpha").unwrap();
        let prev = registry.disconnect();
        assert_eq!(prev.as_deref(), Some("alpha"));
        // Disconnect again — should be None.
        assert!(registry.disconnect().is_none());
    }

    #[test]
    fn switch_returns_previous_and_new_profiles() {
        let mut registry = ConnectionRegistry::new(SafetyProfile::InspectOnly);
        registry.register(fixture("alpha", false));
        registry.register(fixture("beta", false));
        registry.connect("alpha").unwrap();
        let (previous, new) = registry.switch("beta").unwrap();
        assert_eq!(previous.as_deref(), Some("alpha"));
        assert_eq!(new.name, "beta");
    }

    #[test]
    fn set_safety_refuses_writes_on_permanently_readonly_connection() {
        let mut registry = ConnectionRegistry::new(SafetyProfile::InspectOnly);
        registry.register(fixture("prod-db", true));
        registry.connect("prod-db").unwrap();
        let err = registry
            .set_safety(SafetyProfile::SessionWriteEnabled)
            .unwrap_err();
        assert!(matches!(err, ConnectionError::PermanentlyReadOnly { .. }));
    }

    #[test]
    fn is_production_looking_matches_prod_heuristic() {
        assert!(fixture("p", true).is_production_looking());
        assert!(!fixture("d", false).is_production_looking());
    }

    #[test]
    fn dbtools_alias_probe_finds_existing_section() {
        let tmp = temp_dir().join("plsql-mcp-test-dbtools");
        fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("dbtools.json");
        fs::write(&path, "{ \"connections\": [{\"name\": \"billing-dev\"}] }").unwrap();
        let probe = DbToolsAlias::probe("billing-dev", &tmp);
        assert!(probe.available);
        assert_eq!(probe.source.as_deref(), Some(path.as_path()));
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&tmp).unwrap();
    }

    #[test]
    fn connections_toml_round_trips_permanently_read_only_flag() {
        let toml_text = r#"
[[connection]]
name = "prod-db"
description = "Production billing"
connect_string = "//prod-host.example.com/PRDB"
username = "billing_ro"
permanently_read_only = true

[[connection]]
name = "dev-db"
connect_string = "//localhost/DEV"
"#;
        let parsed = ConnectionsToml::from_toml_str(toml_text).expect("parse");
        assert_eq!(parsed.connections.len(), 2);
        let prod = parsed
            .connections
            .iter()
            .find(|c| c.name == "prod-db")
            .expect("prod-db");
        assert!(prod.permanently_read_only);
        assert!(prod.is_production_looking());
        let dev = parsed
            .connections
            .iter()
            .find(|c| c.name == "dev-db")
            .expect("dev-db");
        // permanently_read_only defaults to false.
        assert!(!dev.permanently_read_only);
        assert!(!dev.is_production_looking());
    }

    #[test]
    fn connections_toml_surfaces_parse_errors() {
        let result = ConnectionsToml::from_toml_str("this is not valid toml [[");
        assert!(matches!(result, Err(ConnectionError::TomlParse { .. })));
    }

    #[test]
    fn dbtools_alias_probe_returns_unavailable_when_no_file() {
        let probe = DbToolsAlias::probe("anything", Path::new("/nonexistent/dir"));
        assert!(!probe.available);
        assert!(probe.connect_string.is_none());
    }
}
