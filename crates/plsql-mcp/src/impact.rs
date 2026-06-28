//! Guarded-write blast-radius payloads.
//!
//! The live DDL tools attach this typed wrapper to dry-run/apply responses
//! before Oracle execution. It reuses the existing `plsql-cicd` change-impact
//! envelope and adds the live-MCP context that envelope intentionally does not
//! carry: the parsed target name, the source of the evidence, and explicit
//! lineage/SAST blind spots when no project `AnalysisRun` is bound to the live
//! session.

use plsql_cicd::{
    ChangeImpactEnvelope, ChangeSet, ChangedObject, ChangedObjectKind, PredictMode,
    change_impact_envelope, predict,
};
use plsql_core::{ObjectName, SymbolInterner, UnknownReason};
use serde::{Deserialize, Serialize};

/// Stable schema id for the MCP guarded-write impact wrapper.
pub const GUARDED_WRITE_IMPACT_SCHEMA_ID: &str = "plsql.mcp.guarded_write_impact";

/// MCP response payload attached to guarded write tools.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GuardedWriteImpact {
    pub schema_id: String,
    pub schema_version: String,
    pub computed_before_execute: bool,
    pub tool_name: String,
    pub connection: String,
    pub target: ImpactTarget,
    pub change_impact: ChangeImpactEnvelope,
    pub lineage: EvidenceStatus,
    pub sast: EvidenceStatus,
}

/// Parsed target object for the change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImpactTarget {
    pub owner: String,
    pub name: String,
    pub object_type: String,
    pub changed_kind: String,
    pub classification_source: String,
}

/// Whether a richer analysis family was computed for this response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceStatus {
    pub status: String,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClassifiedDdl {
    target: ImpactTarget,
    kind: ChangedObjectKind,
    uncertainties: Vec<UnknownReason>,
}

/// Build the best available impact summary for one guarded write.
#[must_use]
pub fn guarded_write_impact(tool_name: &str, connection: &str, sql: &str) -> GuardedWriteImpact {
    let classified = classify_guarded_sql(sql);
    let prediction = predict(&changeset_for(&classified), PredictMode::SourceOnly);
    GuardedWriteImpact {
        schema_id: GUARDED_WRITE_IMPACT_SCHEMA_ID.to_string(),
        schema_version: String::from("1.0.0"),
        computed_before_execute: true,
        tool_name: tool_name.to_string(),
        connection: connection.to_string(),
        target: classified.target,
        change_impact: change_impact_envelope(&prediction, Vec::new()),
        lineage: EvidenceStatus {
            status: String::from("not_bound"),
            notes: vec![String::from(
                "No project AnalysisRun/FactStore is bound to this live write session; \
                 response includes direct source-only invalidation rules, not transitive \
                 dependency-graph lineage.",
            )],
        },
        sast: EvidenceStatus {
            status: String::from("not_bound"),
            notes: vec![String::from(
                "No project source tree or flow facts are attached to this guarded write; \
                 run plsql_analyze/analyze_project for SAST findings over the full project.",
            )],
        },
    }
}

fn changeset_for(classified: &ClassifiedDdl) -> ChangeSet {
    let mut interner = SymbolInterner::new();
    let owner = interner
        .intern_schema_name(classified.target.owner.clone())
        .unwrap_or_default();
    let name = interner
        .intern(classified.target.name.clone())
        .map(ObjectName::from)
        .unwrap_or_default();
    ChangeSet {
        objects: vec![ChangedObject {
            owner,
            name,
            kind: classified.kind.clone(),
            new_hash: None,
            previous_hash: None,
            file_paths: Vec::new(),
            uncertainties: classified.uncertainties.clone(),
        }],
        ..ChangeSet::empty()
    }
}

fn classify_guarded_sql(sql: &str) -> ClassifiedDdl {
    classify_create_or_replace(sql)
        .or_else(|| classify_table_ddl(sql))
        .or_else(|| classify_grant_or_revoke(sql))
        .unwrap_or_else(|| ClassifiedDdl {
            target: ImpactTarget {
                owner: String::from("UNKNOWN_SCHEMA"),
                name: String::from("UNCLASSIFIED_DDL"),
                object_type: String::from("UNKNOWN"),
                changed_kind: String::from("Unclassified"),
                classification_source: String::from("unclassified_sql"),
            },
            kind: ChangedObjectKind::Unclassified,
            uncertainties: vec![UnknownReason::UnsupportedDialectFeature],
        })
}

fn classify_create_or_replace(sql: &str) -> Option<ClassifiedDdl> {
    let kind = crate::create_or_replace::classify_kind(sql).ok()?;
    let target = crate::create_or_replace::parse_target_object(sql).ok()?;
    let owner = target
        .owner
        .unwrap_or_else(|| String::from("CURRENT_SCHEMA"));
    let (changed_kind, object_type) = match kind.as_str() {
        "PACKAGE" => (ChangedObjectKind::PackageSpec, "PACKAGE"),
        "PACKAGE BODY" => (ChangedObjectKind::PackageBody, "PACKAGE BODY"),
        "PROCEDURE" | "FUNCTION" => (ChangedObjectKind::StandaloneRoutineSignature, "ROUTINE"),
        "TRIGGER" => (ChangedObjectKind::TriggerChange, "TRIGGER"),
        "VIEW" => (ChangedObjectKind::ViewDefinitionChange, "VIEW"),
        "TYPE" | "TYPE BODY" => (ChangedObjectKind::TypeEvolution, "TYPE"),
        "SYNONYM" => (ChangedObjectKind::SynonymRetargeting, "SYNONYM"),
        other => (
            ChangedObjectKind::OtherKnownKind {
                object_type: other.to_string(),
            },
            other,
        ),
    };
    Some(ClassifiedDdl {
        target: ImpactTarget {
            owner,
            name: target.object,
            object_type: object_type.to_string(),
            changed_kind: format!("{changed_kind:?}"),
            classification_source: String::from("create_or_replace_header"),
        },
        kind: changed_kind,
        uncertainties: vec![UnknownReason::MissingCatalogObject],
    })
}

fn classify_table_ddl(sql: &str) -> Option<ClassifiedDdl> {
    let tokens = sql_tokens(sql);
    let head = tokens.first()?.as_str();
    let table_idx = match head {
        "ALTER" | "DROP" | "TRUNCATE" => {
            tokens.iter().position(|word| word.as_str().eq("TABLE"))?
        }
        _ => return None,
    };
    let name = tokens.get(table_idx + 1)?;
    let (owner, object) = split_qualified_token(name);
    let changed_kind = if head.eq("ALTER") && tokens.iter().any(|word| word.as_str().eq("ADD")) {
        ChangedObjectKind::TableAdditiveDdl
    } else {
        ChangedObjectKind::TableDestructiveDdl
    };
    Some(ClassifiedDdl {
        target: ImpactTarget {
            owner,
            name: object,
            object_type: String::from("TABLE"),
            changed_kind: format!("{changed_kind:?}"),
            classification_source: String::from("table_ddl_head"),
        },
        kind: changed_kind,
        uncertainties: vec![UnknownReason::MissingCatalogObject],
    })
}

fn classify_grant_or_revoke(sql: &str) -> Option<ClassifiedDdl> {
    let tokens = sql_tokens(sql);
    let head = tokens.first()?.as_str();
    if !matches!(head, "GRANT" | "REVOKE") {
        return None;
    }
    let object = tokens
        .iter()
        .position(|word| word.as_str().eq("ON"))
        .and_then(|idx| tokens.get(idx + 1))
        .map(|name| split_qualified_token(name))
        .unwrap_or_else(|| {
            (
                String::from("UNKNOWN_SCHEMA"),
                String::from("PRIVILEGE_CHANGE"),
            )
        });
    Some(ClassifiedDdl {
        target: ImpactTarget {
            owner: object.0,
            name: object.1,
            object_type: String::from("PRIVILEGE"),
            changed_kind: String::from("GrantOrRevoke"),
            classification_source: String::from("grant_or_revoke_head"),
        },
        kind: ChangedObjectKind::GrantOrRevoke,
        uncertainties: vec![UnknownReason::MissingCatalogObject],
    })
}

fn sql_tokens(sql: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = sql.trim_start().chars().peekable();
    let mut in_quote = false;

    while let Some(ch) = chars.next() {
        if in_quote {
            if ch == '"' {
                if matches!(chars.peek(), Some('"')) {
                    current.push('"');
                    chars.next();
                } else {
                    in_quote = false;
                }
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '"' => in_quote = true,
            c if c.is_whitespace() || matches!(c, '(' | ')' | ';' | ',') => {
                push_token(&mut tokens, &mut current);
            }
            _ => current.push(ch),
        }
    }
    push_token(&mut tokens, &mut current);
    tokens
}

fn push_token(tokens: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        tokens.push(trimmed.to_ascii_uppercase());
    }
    current.clear();
}

fn split_qualified_token(token: &str) -> (String, String) {
    let mut parts = token.split('.').filter(|part| !part.trim().is_empty());
    let first = parts.next().unwrap_or("UNKNOWN_SCHEMA");
    match (first, parts.next()) {
        (owner, Some(object)) => (owner.to_string(), object.to_string()),
        (object, None) => (String::from("CURRENT_SCHEMA"), object.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_view_impact_payload_matches_golden() {
        let impact = guarded_write_impact(
            "create_or_replace",
            "billing-dev",
            "CREATE OR REPLACE VIEW BILLING.REPORT_VIEW AS SELECT 1 AS X FROM DUAL",
        );
        let actual = serde_json::to_string_pretty(&impact).expect("serialize impact payload");
        let expected =
            include_str!("../tests/golden/guarded_write_impact_create_view.json").trim_end();

        assert_eq!(actual, expected);
    }

    #[test]
    fn table_ddl_classifies_additive_and_destructive_shapes() {
        let additive = guarded_write_impact(
            "execute_approved",
            "billing-dev",
            "alter table billing.customers add (vip_flag number)",
        );
        assert_eq!(additive.target.changed_kind, "TableAdditiveDdl");

        let destructive = guarded_write_impact(
            "execute_approved",
            "billing-dev",
            "drop table billing.customers",
        );
        assert_eq!(destructive.target.changed_kind, "TableDestructiveDdl");
    }
}
