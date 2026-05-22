//! Trust Block (PLSQL-MCP-007, plan §1.5).
//!
//! Every successful MCP response carries `result.meta.trust_block`
//! so an agent never consumes an answer without the provenance
//! and epistemic caveats attached to it. The block states, in
//! machine-readable form, what the foundation server *is* and
//! what it deliberately is *not*:
//!
//! * static analysis only — no live database was queried;
//! * findings/answers are evidence-bounded, not authoritative
//!   execution results (R13: uncertainty is disclosed, never
//!   silently dropped);
//! * the schema id/version so a consumer can gate on it.
//!
//! Injection happens once, centrally, in
//! [`JsonRpcResponse::ok`](crate::mcp_protocol) — individual
//! tools never hand-roll it, so it cannot be forgotten on a new
//! tool.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Stable schema id/version for the trust block itself.
pub const TRUST_BLOCK_SCHEMA_ID: &str = "plsql.mcp.trust_block";
pub const TRUST_BLOCK_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustBlock {
    pub schema_id: String,
    pub schema_version: String,
    /// This is the foundation (Apache/MIT) server.
    pub tier: String,
    /// No live DB was contacted to produce this response.
    pub live_database_used: bool,
    /// Answers are derived from static analysis of source +
    /// (optional) catalog snapshot, not from executing code.
    pub analysis_kind: String,
    /// R13 caveat surfaced verbatim to the agent.
    pub completeness_caveat: String,
    /// Tool tiering / authorization disclaimer.
    pub disclaimer: String,
}

impl Default for TrustBlock {
    fn default() -> Self {
        Self {
            schema_id: TRUST_BLOCK_SCHEMA_ID.to_string(),
            schema_version: TRUST_BLOCK_SCHEMA_VERSION.to_string(),
            tier: "foundation".to_string(),
            live_database_used: false,
            analysis_kind: "static".to_string(),
            completeness_caveat: "Results are evidence-bounded: opaque dynamic SQL, \
                missing catalog/PL-Scope, or parser-recovered regions are reported as \
                explicit gaps, never silently treated as clean."
                .to_string(),
            disclaimer: "Foundation static-analysis output. Not an authoritative execution \
                result; verify before acting on production systems."
                .to_string(),
        }
    }
}

/// The trust block as a JSON value.
#[must_use]
pub fn trust_block_value() -> Value {
    serde_json::to_value(TrustBlock::default()).expect("TrustBlock serializes")
}

/// Inject `meta.trust_block` into a tool result. If `result` is a
/// JSON object the block is merged under an existing-or-new
/// `meta`; a non-object result (rare) is wrapped as
/// `{ "value": <result>, "meta": { "trust_block": … } }` so the
/// caveat is *never* dropped (R13).
#[must_use]
pub fn attach_trust_block(result: Value) -> Value {
    match result {
        Value::Object(mut map) => {
            let meta = map
                .entry("meta".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(meta_map) = meta {
                meta_map.insert("trust_block".to_string(), trust_block_value());
            } else {
                // `meta` existed but was not an object — preserve
                // it and still attach the block alongside.
                let mut m = Map::new();
                m.insert("trust_block".to_string(), trust_block_value());
                m.insert("prior_meta".to_string(), meta.clone());
                *meta = Value::Object(m);
            }
            Value::Object(map)
        }
        other => {
            let mut m = Map::new();
            let mut meta = Map::new();
            meta.insert("trust_block".to_string(), trust_block_value());
            m.insert("value".to_string(), other);
            m.insert("meta".to_string(), Value::Object(meta));
            Value::Object(m)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_result_gets_meta_trust_block() {
        let r = attach_trust_block(json!({"tools": []}));
        assert!(r["tools"].is_array(), "original payload preserved");
        assert_eq!(r["meta"]["trust_block"]["schema_id"], TRUST_BLOCK_SCHEMA_ID);
        assert_eq!(r["meta"]["trust_block"]["live_database_used"], false);
        assert_eq!(r["meta"]["trust_block"]["analysis_kind"], "static");
        assert!(
            r["meta"]["trust_block"]["completeness_caveat"]
                .as_str()
                .unwrap()
                .contains("evidence-bounded")
        );
    }

    #[test]
    fn existing_object_meta_is_merged_not_clobbered() {
        let r = attach_trust_block(json!({"x": 1, "meta": {"existing": true}}));
        assert_eq!(r["meta"]["existing"], true, "pre-existing meta kept");
        assert_eq!(r["meta"]["trust_block"]["tier"], "foundation");
    }

    #[test]
    fn non_object_meta_is_preserved_under_prior_meta() {
        let r = attach_trust_block(json!({"meta": "stringy"}));
        assert_eq!(r["meta"]["prior_meta"], "stringy");
        assert!(r["meta"]["trust_block"].is_object());
    }

    #[test]
    fn non_object_result_is_wrapped_caveat_never_dropped() {
        let r = attach_trust_block(json!([1, 2, 3]));
        assert_eq!(r["value"], json!([1, 2, 3]));
        assert!(r["meta"]["trust_block"].is_object());
    }

    #[test]
    fn trust_block_round_trips() {
        let tb = TrustBlock::default();
        let j = serde_json::to_string(&tb).unwrap();
        let back: TrustBlock = serde_json::from_str(&j).unwrap();
        assert_eq!(back, tb);
    }
}
