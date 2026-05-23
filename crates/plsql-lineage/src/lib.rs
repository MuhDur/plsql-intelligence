#![forbid(unsafe_code)]
//! `plsql-lineage` — lineage query/result types.
//!
//! Defines the shape of lineage queries the engine accepts and the result
//! shape it returns. Concrete graph-walking lives in the engine module; this
//! crate is the public IR consumed by the CLI, the MCP adapter, and report
//! renderers.
//!
//! `--robot-json` envelope helpers for every lineage operation live near
//! the bottom of this file; they wrap the result types in versioned
//! `plsql-output::RobotJsonEnvelope` so the CLI, MCP, and CI gate
//! consumers see stable schema IDs (R5, R10).

use plsql_output::{RobotJsonEnvelope, SchemaDescriptor, SchemaVersion};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// Schema descriptor for the `impact(node)` operation output.
pub const IMPACT_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.impact",
    version: SchemaVersion::new(1, 0, 0),
    description: "Downstream impact traversal result with confidence aggregation",
};

/// Schema descriptor for the `dependencies(node)` upstream traversal.
pub const DEPENDENCIES_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.dependencies",
    version: SchemaVersion::new(1, 0, 0),
    description: "Upstream dependency traversal result",
};

/// Schema descriptor for `classify-change`.
pub const CLASSIFY_CHANGE_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.classify_change",
    version: SchemaVersion::new(1, 0, 0),
    description: "Semantic change-set classification across two analysis runs",
};

/// Schema descriptor for the lineage doctor report.
pub const DOCTOR_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.doctor",
    version: SchemaVersion::new(1, 0, 0),
    description: "Lineage graph completeness + low-confidence inventory",
};

/// Schema descriptor for customer-facing explain output.
pub const EXPLAIN_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.explain",
    version: SchemaVersion::new(1, 0, 0),
    description: "Customer-facing explanation of a lineage edge, node, or path",
};

/// Schema descriptor for `recompile_order` plans.
pub const RECOMPILE_ORDER_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.recompile_order",
    version: SchemaVersion::new(1, 0, 0),
    description: "Topological recompile order for a set of changed PL/SQL objects",
};

/// Schema descriptor for `callers(proc)` results.
pub const CALLERS_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.callers",
    version: SchemaVersion::new(1, 0, 0),
    description: "Direct callers of a PL/SQL routine, filtered to Calls edges",
};

/// Schema descriptor for `column_readers` / `column_writers` results.
pub const COLUMN_ACCESS_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.column_access",
    version: SchemaVersion::new(1, 0, 0),
    description: "Objects that read or write a specific column, including unknown-column-of-table heuristics",
};

/// All lineage-side schema descriptors, registered for callers that
/// want to pin compatible versions (mirrors `plsql_output::OUTPUT_SCHEMAS`).
pub const LINEAGE_SCHEMAS: [SchemaDescriptor; 14] = [
    IMPACT_SCHEMA,
    DEPENDENCIES_SCHEMA,
    CLASSIFY_CHANGE_SCHEMA,
    DOCTOR_SCHEMA,
    EXPLAIN_SCHEMA,
    RECOMPILE_ORDER_SCHEMA,
    CALLERS_SCHEMA,
    COLUMN_ACCESS_SCHEMA,
    UNSAFE_PATHS_SCHEMA,
    LINEAGE_GRAPHML_SCHEMA,
    LINEAGE_HTML_SCHEMA,
    CLASSIFY_RENAME_SCHEMA,
    COMPARE_ORACLE_DEPS_SCHEMA,
    ORPHAN_CANDIDATES_SCHEMA,
];

/// Schema descriptor for the customer-facing `compare-oracle-deps` report.
pub const COMPARE_ORACLE_DEPS_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.compare_oracle_deps",
    version: SchemaVersion::new(1, 0, 0),
    description: "Customer-facing comparison of depgraph edges vs Oracle ALL_DEPENDENCIES",
};

/// Confidence tier for a lineage edge or path. Mirrors the `plsql-core`
/// confidence concept while remaining serializable for wire transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// All inputs known; deterministic resolution.
    Exact,
    /// Resolution required heuristic (e.g. catalog inference, dynamic SQL
    /// shape analysis).
    Heuristic,
    /// Resolution unknown; an `UnknownReason` should accompany the edge.
    Unknown,
}

/// A request submitted to the lineage engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageQuery {
    /// Schema-qualified node identifier the user is asking about
    /// (e.g. `hr.customers.legacy_segment`).
    pub anchor: String,
    /// Direction of traversal.
    pub direction: LineageDirection,
    /// Maximum hops to follow. `None` means unbounded (engine clamps).
    pub max_depth: Option<u32>,
    /// Restrict to edges at or above this confidence tier.
    pub min_confidence: Option<Confidence>,
}

/// Direction of lineage traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineageDirection {
    /// What feeds this node (data sources).
    Upstream,
    /// What this node feeds (data sinks; "what breaks if I change this").
    Downstream,
    /// Both directions.
    Bidirectional,
}

/// Result of a `LineageQuery`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LineageResult {
    /// Echo of the input query for self-describing transport.
    pub query: Option<LineageQuery>,
    /// Discovered edges grouped by confidence tier so reports can partition
    /// findings without re-walking.
    pub edges: Vec<LineageEdge>,
    /// Edges whose target could not be resolved; each carries a stable
    /// `unknown_reason` discriminator the report renderer maps to a tier.
    pub unknown_edges: Vec<UnknownEdge>,
    /// Nodes reached during traversal, each with the *best* (most certain)
    /// confidence the engine could prove across any path from the anchor.
    /// Populated by impact/dependency walks that aggregate confidence
    /// along paths; upstream `dependencies` and other per-edge walks
    /// leave this empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_nodes: Vec<AffectedNode>,
}

/// A node reached during impact / dependency traversal, paired with the
/// strongest confidence the engine could prove along any path from the
/// anchor.
///
/// `hops` is the depth at which this node was first reached (0 means the
/// anchor itself). `path_confidence` is the *max* over all paths of the
/// *min* edge confidence along the path — that is, the most-confident
/// claim the graph supports for "the anchor's change reaches this node".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AffectedNode {
    pub logical_id: String,
    pub hops: u32,
    pub path_confidence: Confidence,
}

/// A resolved lineage edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub confidence: Confidence,
}

/// An edge whose target could not be resolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownEdge {
    pub source: String,
    /// Stable unknown-reason discriminator (e.g. `DynamicSqlOpaque`,
    /// `MissingPackageBody`, `DbLinkRemoteObject`, `WrappedSource`).
    pub unknown_reason: String,
    pub detail: Option<String>,
}

// ---------------------------------------------------------------------------
// SemanticChangeSet — diff model for classify-change (LIN-000)
// ---------------------------------------------------------------------------

/// Identifies an object in the dependency graph by its logical id
/// (schema.object[.member]).
pub type ObjectId = String;

/// A content hash (e.g. SHA-256 of source text).
pub type ContentHash = String;

/// The kind of change detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Object was created.
    Created,
    /// Object was dropped.
    Dropped,
    /// Object signature changed (parameters, return type, AUTHID, etc.).
    Signature,
    /// Object body changed (implementation, not interface).
    Body,
    /// Privilege grant or revoke.
    Privilege,
    /// Synonym target changed or synonym created/dropped.
    Synonym,
    /// Column added, removed, or type changed.
    Column,
    /// Type attribute or inheritance changed.
    Type,
    /// Grant added or removed.
    Grant,
    /// DDL structural change (index, constraint, trigger, sequence).
    Ddl,
}

/// What happened to a grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantAction {
    /// Grant was added.
    Granted,
    /// Grant was revoked.
    Revoked,
}

/// A change to an object's signature (parameters, return type, AUTHID).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignatureChange {
    pub object_id: ObjectId,
    pub old_signature: Option<String>,
    pub new_signature: Option<String>,
}

/// A change to an object's body (implementation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyChange {
    pub object_id: ObjectId,
    pub hash_before: Option<ContentHash>,
    pub hash_after: Option<ContentHash>,
}

/// A change to a privilege (grant/revoke).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrivilegeChange {
    pub object_id: ObjectId,
    pub grantee: String,
    pub privilege: String,
    pub action: GrantAction,
}

/// A change to a synonym (target changed, created, or dropped).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynonymChange {
    pub synonym_id: ObjectId,
    pub target_before: Option<String>,
    pub target_after: Option<String>,
}

/// A change to a column (added, removed, or type changed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnChange {
    pub object_id: ObjectId,
    pub column_name: String,
    pub change: ColumnChangeDetail,
}

/// Detail about what changed for a column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnChangeDetail {
    /// Column was added.
    Added,
    /// Column was dropped.
    Dropped,
    /// Column data type changed.
    TypeChanged {
        old_type: Option<String>,
        new_type: Option<String>,
    },
    /// Column nullability changed.
    NullabilityChanged {
        old_nullable: bool,
        new_nullable: bool,
    },
}

/// A change to a type (attribute added/removed, inheritance changed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeChange {
    pub type_id: ObjectId,
    pub detail: TypeChangeDetail,
}

/// Detail about what changed for a type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeChangeDetail {
    /// Attribute added.
    AttributeAdded { name: String },
    /// Attribute removed.
    AttributeRemoved { name: String },
    /// Finality changed.
    FinalityChanged,
    /// Instantiability changed.
    InstantiabilityChanged,
}

/// A grant change (wraps PrivilegeChange for the Grant variant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrantChange {
    pub object_id: ObjectId,
    pub grantee: String,
    pub privilege: String,
    pub action: GrantAction,
}

/// A DDL structural change (index, constraint, trigger, sequence, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdlChange {
    pub object_id: ObjectId,
    pub object_type: String,
    pub detail: String,
}

/// A single change record — one of the per-kind variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeRecord {
    /// Object created.
    Created { object_id: ObjectId },
    /// Object dropped.
    Dropped { object_id: ObjectId },
    /// Signature changed.
    Signature(SignatureChange),
    /// Body changed.
    Body(BodyChange),
    /// Privilege changed.
    Privilege(PrivilegeChange),
    /// Synonym changed.
    Synonym(SynonymChange),
    /// Column changed.
    Column(ColumnChange),
    /// Type changed.
    Type(TypeChange),
    /// Grant changed.
    Grant(GrantChange),
    /// DDL structural change.
    Ddl(DdlChange),
}

/// A complete set of semantic changes between two analysis runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SemanticChangeSet {
    /// Echo of the old analysis run id (if available).
    pub old_run_id: Option<String>,
    /// Echo of the new analysis run id (if available).
    pub new_run_id: Option<String>,
    /// All detected changes, ordered by kind then object id.
    pub changes: Vec<ChangeRecord>,
}

impl SemanticChangeSet {
    /// Create an empty change set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a change record.
    pub fn push(&mut self, record: ChangeRecord) {
        self.changes.push(record);
    }

    /// Returns true if there are no changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Count changes by kind.
    #[must_use]
    pub fn count_by_kind(&self, kind: ChangeKind) -> usize {
        self.changes
            .iter()
            .filter(|r| r.change_kind() == Some(kind))
            .count()
    }
}

impl ChangeRecord {
    /// Returns the `ChangeKind` for this record, if applicable.
    #[must_use]
    pub fn change_kind(&self) -> Option<ChangeKind> {
        match self {
            Self::Created { .. } => Some(ChangeKind::Created),
            Self::Dropped { .. } => Some(ChangeKind::Dropped),
            Self::Signature(_) => Some(ChangeKind::Signature),
            Self::Body(_) => Some(ChangeKind::Body),
            Self::Privilege(_) => Some(ChangeKind::Privilege),
            Self::Synonym(_) => Some(ChangeKind::Synonym),
            Self::Column(_) => Some(ChangeKind::Column),
            Self::Type(_) => Some(ChangeKind::Type),
            Self::Grant(_) => Some(ChangeKind::Grant),
            Self::Ddl(_) => Some(ChangeKind::Ddl),
        }
    }
}

// ---------------------------------------------------------------------------
// dependencies() — reverse traversal (who depends on this node?)
// ---------------------------------------------------------------------------

use plsql_depgraph::{DepGraph, NodeId, NodeSelector};

/// Walk INCOMING edges from `node` up to `max_depth` hops, collecting all
/// upstream (dependency) nodes. Returns a `LineageResult` whose edges
/// represent "source depends on target" relationships.
///
/// This is the reverse of impact analysis: instead of asking "what breaks if
/// I change X?" it answers "what does X depend on?".
// ---------------------------------------------------------------------------
// Change classifiers — emit SemanticChangeSet from various diff sources
// ---------------------------------------------------------------------------
use std::collections::BTreeMap;
use std::path::Path;

/// Classify changes between two git refs in a repository.
///
/// Shells out to `git diff --name-status <from> <to>` rooted at
/// `repo`, walks the output, and emits a [`SemanticChangeSet`] of
/// per-file [`ChangeRecord`] variants:
///
/// * `A` (added)    → [`ChangeRecord::Created`]
/// * `D` (deleted)  → [`ChangeRecord::Dropped`]
/// * `M` (modified) → [`ChangeRecord::Body`] with `git:<ref>` hashes
///   on either side; body bytes are NOT diffed by this function.
/// * `R*` / `C*` (rename / copy with similarity score) → a paired
///   `Dropped(old)` + `Created(new)` so downstream consumers do not
///   accidentally treat a rename as a body change.
///
/// Files that don't look like PL/SQL sources (recognised by
/// extension: `.sql`, `.pks`, `.pkb`, `.plsql`, etc.) are filtered
/// out. The returned changeset stamps `old_run_id` and
/// `new_run_id` with the supplied refs so downstream lineage
/// consumers can correlate it with run artifacts.
///
/// This is a structural classifier: it summarises the diff envelope
/// (which objects appeared / disappeared / changed) without parsing
/// the changed bodies. Body-level semantic classification (which
/// columns / signatures / SQL statements changed inside an `M`
/// record) is the parser's job and is intentionally out of scope
/// here; see [`classify_dir_diff`] for the directory-comparison
/// sibling with the same structural contract.
pub fn classify_git_diff(
    repo: &Path,
    from: &str,
    to: &str,
) -> Result<SemanticChangeSet, ClassifyError> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-status", from, to])
        .current_dir(repo)
        .output()
        .map_err(|e| ClassifyError::GitInvocation(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ClassifyError::GitInvocation(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut changeset = SemanticChangeSet::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            continue;
        }

        let status = parts[0];
        let path = parts[1];

        if !is_plsql_file(path) {
            continue;
        }

        let object_id = path_to_object_id(path);

        // Git reports rename and copy statuses with an attached similarity
        // score, so `parts[0]` is `R100`, `R087`, `C100`, etc. — bare `R`
        // and `C` never appear. Match on the leading byte instead.
        let leading = status.as_bytes().first().copied();
        match leading {
            Some(b'A') => changeset.push(ChangeRecord::Created { object_id }),
            Some(b'D') => changeset.push(ChangeRecord::Dropped { object_id }),
            Some(b'M') => changeset.push(ChangeRecord::Body(BodyChange {
                object_id,
                hash_before: Some(format!("git:{from}")),
                hash_after: Some(format!("git:{to}")),
            })),
            Some(b'R') | Some(b'C') if parts.len() >= 3 => {
                let old_id = path_to_object_id(parts[1]);
                let new_id = path_to_object_id(parts[2]);
                changeset.push(ChangeRecord::Dropped { object_id: old_id });
                changeset.push(ChangeRecord::Created { object_id: new_id });
            }
            _ => changeset.push(ChangeRecord::Body(BodyChange {
                object_id,
                hash_before: None,
                hash_after: None,
            })),
        }
    }

    changeset.old_run_id = Some(from.to_string());
    changeset.new_run_id = Some(to.to_string());
    Ok(changeset)
}

/// Classify changes between two directories by comparing file sets.
///
/// Walks both directories, compares PL/SQL files by relative path:
/// - Only in `after` -> Created
/// - Only in `before` -> Dropped
/// - In both with different content -> Body
/// - In both with same content -> skipped
pub fn classify_dir_diff(before: &Path, after: &Path) -> Result<SemanticChangeSet, ClassifyError> {
    let before_files = collect_plsql_files(before)?;
    let after_files = collect_plsql_files(after)?;

    let mut changeset = SemanticChangeSet::new();

    for rel_path in after_files.keys() {
        if !before_files.contains_key(rel_path) {
            changeset.push(ChangeRecord::Created {
                object_id: path_to_object_id(rel_path),
            });
        }
    }

    for rel_path in before_files.keys() {
        if !after_files.contains_key(rel_path) {
            changeset.push(ChangeRecord::Dropped {
                object_id: path_to_object_id(rel_path),
            });
        }
    }

    for (rel_path, before_hash) in &before_files {
        if let Some(after_hash) = after_files.get(rel_path) {
            if before_hash != after_hash {
                changeset.push(ChangeRecord::Body(BodyChange {
                    object_id: path_to_object_id(rel_path),
                    hash_before: Some(before_hash.clone()),
                    hash_after: Some(after_hash.clone()),
                }));
            }
        }
    }

    Ok(changeset)
}

/// Error type for change classification operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClassifyError {
    GitInvocation(String),
    Io(String),
    /// The unified-diff input could not be parsed (missing headers,
    /// malformed `+++` / `---` line, etc.). Carries a line number plus
    /// a one-line description.
    MalformedDiff {
        line: usize,
        detail: String,
    },
}

impl std::fmt::Display for ClassifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitInvocation(msg) => write!(f, "git invocation failed: {msg}"),
            Self::Io(msg) => write!(f, "i/o error: {msg}"),
            Self::MalformedDiff { line, detail } => {
                write!(f, "malformed unified diff at line {line}: {detail}")
            }
        }
    }
}

/// Parse a unified-diff file (as produced by `git diff -U…` or
/// `diff -u`) and classify each PL/SQL file pair into a
/// `SemanticChangeSet`. This is the parser behind the
/// `what-breaks --change <file>` workflow (LIN-007): the operator
/// hands the engine a static diff captured from a CI artifact or
/// an offline `git diff` invocation, and the engine answers
/// "which logical objects changed?" without having to re-run git.
///
/// Recognized status patterns:
///
/// * `--- /dev/null` → Created (file is new in the working tree)
/// * `+++ /dev/null` → Dropped (file removed)
/// * `--- a/<p>` paired with `+++ b/<p>` → Body change on `<p>`
/// * Different `<p>` on either side → Dropped(old) + Created(new)
///
/// Hunk lines (`@@ …@@`), context lines, additions, and deletions
/// are intentionally ignored — body-level semantic diffing requires
/// a parser (Layer 1) and is tracked separately. Non-PL/SQL files
/// (anything outside `.sql/.pls/.plsql/.pks/.pkb`) are skipped.
///
/// File paths feed through `path_to_object_id` so the report uses
/// logical object IDs, not raw filesystem paths. Captured body
/// changes carry `hash_before` / `hash_after` set to short tags
/// derived from each side's `+`/`-` line counts so downstream
/// consumers can tell two body changes apart without re-reading
/// the diff.
pub fn parse_change_file(path: &Path) -> Result<SemanticChangeSet, ClassifyError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| ClassifyError::Io(format!("{}: {}", path.display(), e)))?;
    parse_unified_diff(&raw)
}

/// In-memory variant of [`parse_change_file`] used by tests and by
/// callers that already hold the diff text (e.g. piped through
/// stdin). The string-form input keeps test fixtures self-contained.
pub fn parse_unified_diff(diff: &str) -> Result<SemanticChangeSet, ClassifyError> {
    let mut changeset = SemanticChangeSet::new();

    let mut pending_minus: Option<String> = None;
    let mut current_pair: Option<(String, String)> = None;
    let mut additions: usize = 0;
    let mut deletions: usize = 0;

    let flush =
        |pair: &Option<(String, String)>, add: usize, del: usize, set: &mut SemanticChangeSet| {
            let Some((minus, plus)) = pair else { return };
            let minus_dev_null = minus == "/dev/null";
            let plus_dev_null = plus == "/dev/null";

            let minus_path = strip_diff_prefix(minus);
            let plus_path = strip_diff_prefix(plus);

            let minus_is_plsql = !minus_dev_null && is_plsql_file(minus_path);
            let plus_is_plsql = !plus_dev_null && is_plsql_file(plus_path);

            // Drop pairs that don't involve PL/SQL on either side.
            if !minus_is_plsql && !plus_is_plsql {
                return;
            }

            match (minus_dev_null, plus_dev_null) {
                (true, false) if plus_is_plsql => set.push(ChangeRecord::Created {
                    object_id: path_to_object_id(plus_path),
                }),
                (false, true) if minus_is_plsql => set.push(ChangeRecord::Dropped {
                    object_id: path_to_object_id(minus_path),
                }),
                (false, false) => {
                    let same_path = minus_path == plus_path;
                    if same_path && minus_is_plsql {
                        set.push(ChangeRecord::Body(BodyChange {
                            object_id: path_to_object_id(minus_path),
                            hash_before: Some(format!("diff:-{del}")),
                            hash_after: Some(format!("diff:+{add}")),
                        }));
                    } else {
                        // Rename / move across paths is modelled as
                        // drop+create so the downstream graph treats it
                        // as the destruction of one object and the birth
                        // of another (R13 does not yet model renames).
                        if minus_is_plsql {
                            set.push(ChangeRecord::Dropped {
                                object_id: path_to_object_id(minus_path),
                            });
                        }
                        if plus_is_plsql {
                            set.push(ChangeRecord::Created {
                                object_id: path_to_object_id(plus_path),
                            });
                        }
                    }
                }
                _ => {}
            }
        };

    for (idx, line) in diff.lines().enumerate() {
        let lineno = idx + 1;
        if let Some(rest) = line.strip_prefix("--- ") {
            flush(&current_pair, additions, deletions, &mut changeset);
            current_pair = None;
            additions = 0;
            deletions = 0;
            pending_minus = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            let minus = pending_minus
                .take()
                .ok_or_else(|| ClassifyError::MalformedDiff {
                    line: lineno,
                    detail: "+++ header without preceding --- header".to_string(),
                })?;
            current_pair = Some((minus, rest.trim().to_string()));
        } else if line.starts_with('+') && !line.starts_with("+++") {
            additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }

    flush(&current_pair, additions, deletions, &mut changeset);

    if pending_minus.is_some() {
        return Err(ClassifyError::MalformedDiff {
            line: diff.lines().count(),
            detail: "trailing --- header without matching +++".to_string(),
        });
    }

    Ok(changeset)
}

fn strip_diff_prefix(path: &str) -> &str {
    // `diff -u` appends a tab + timestamp after the filename; git
    // diff omits it but accepts it. Trim everything after the first
    // tab so the timestamp doesn't leak into the path comparison.
    let path = path.split('\t').next().unwrap_or(path).trim_end();
    // git diff prefixes a/ and b/ by default; the standard diff(1)
    // does not. Either is acceptable here.
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
}

impl std::error::Error for ClassifyError {}

fn is_plsql_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".sql")
        || lower.ends_with(".pls")
        || lower.ends_with(".plsql")
        || lower.ends_with(".pkb")
        || lower.ends_with(".pks")
}

fn path_to_object_id(path: &str) -> String {
    let stripped = path.rsplit_once('.').map(|(base, _)| base).unwrap_or(path);
    stripped.replace("/", ".")
}

fn simple_content_hash(content: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("h:{:016x}", hasher.finish())
}

fn collect_plsql_files(dir: &Path) -> Result<BTreeMap<String, String>, ClassifyError> {
    let mut files = BTreeMap::new();
    collect_recursive(dir, dir, &mut files)?;
    Ok(files)
}

fn collect_recursive(
    root: &Path,
    current: &Path,
    out: &mut BTreeMap<String, String>,
) -> Result<(), ClassifyError> {
    let entries = std::fs::read_dir(current)
        .map_err(|e| ClassifyError::Io(format!("{}: {}", current.display(), e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| ClassifyError::Io(e.to_string()))?;
        let path = entry.path();

        if path.is_dir() {
            collect_recursive(root, &path, out)?;
            continue;
        }

        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        if !is_plsql_file(&rel) {
            continue;
        }

        let content = std::fs::read(&path)
            .map_err(|e| ClassifyError::Io(format!("{}: {}", path.display(), e)))?;
        out.insert(rel, simple_content_hash(&content));
    }

    Ok(())
}

pub fn dependencies(graph: &DepGraph, node: &NodeId, max_depth: Option<u32>) -> LineageResult {
    let selector = NodeSelector::NodeId(*node);
    let anchor_node = match graph.resolve_node(&selector) {
        Ok(n) => n,
        Err(_) => return LineageResult::default(),
    };

    let anchor_logical = anchor_node.logical_id.to_string();
    let depth_limit = max_depth.unwrap_or(u32::MAX);

    let mut result = LineageResult::default();
    let mut visited = std::collections::HashSet::new();
    visited.insert(*node);

    // BFS queue: (node_id, depth)
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((*node, 0u32));

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= depth_limit {
            continue;
        }

        // Find all incoming edges (edge.to == current)
        let nr = graph.query_reverse_neighbors(&NodeSelector::NodeId(current));
        let incoming = match nr {
            Ok(result) => result.edges,
            Err(_) => continue,
        };

        for edge in &incoming {
            let source_node = &edge.from;
            let source_id = NodeId::new(source_node.id.get());

            let lineage_confidence = depgraph_confidence_to_lineage(&edge.confidence);

            // Apply confidence filter if the query specifies one.
            // We push the edge regardless — the caller can filter on
            // `min_confidence` post-hoc, but we also record unknown edges
            // for low-confidence entries.

            result.edges.push(LineageEdge {
                source: source_node.logical_id.clone(),
                target: edge.to.logical_id.clone(),
                kind: edge.kind.as_str().to_string(),
                confidence: lineage_confidence,
            });

            if visited.insert(source_id) {
                queue.push_back((source_id, depth + 1));
            }
        }
    }

    result.query = Some(LineageQuery {
        anchor: anchor_logical,
        direction: LineageDirection::Upstream,
        max_depth,
        min_confidence: None,
    });

    result
}

/// Summary report emitted by [`doctor`].
///
/// Counts give the customer-facing Trust Block its raw inputs (plan
/// §1.5): how complete is the graph, how much of it is exact vs.
/// heuristic vs. unknown, and which `UnknownReason` discriminators
/// dominate the inventory. This report is the counts-only foundation
/// every consumer (CLI doctor command, CI gate, MCP tool, sales-demo
/// Trust Block) reads from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageDoctorReport {
    pub node_count: usize,
    pub edge_count: usize,
    pub edges_exact: usize,
    pub edges_heuristic: usize,
    pub edges_unknown: usize,
    /// `UnknownReason` discriminator → count. Pulled from
    /// `Confidence::explanation` on low-confidence edges; `Opaque` is
    /// used as the fallback when the edge carries no explanation.
    pub unknown_reasons: std::collections::BTreeMap<String, usize>,
}

impl LineageDoctorReport {
    /// Fraction of edges with `Confidence::Exact`, in `[0.0, 1.0]`.
    /// Returns 0.0 when there are no edges to classify.
    #[must_use]
    pub fn exact_ratio(&self) -> f32 {
        if self.edge_count == 0 {
            0.0
        } else {
            self.edges_exact as f32 / self.edge_count as f32
        }
    }

    /// Fraction of edges in the low-confidence bucket (Unknown). Used
    /// by the CI `unknown budget` gate to fail PRs that push a graph
    /// past the customer's tolerance threshold.
    #[must_use]
    pub fn unknown_ratio(&self) -> f32 {
        if self.edge_count == 0 {
            0.0
        } else {
            self.edges_unknown as f32 / self.edge_count as f32
        }
    }
}

/// Compute a graph-completeness + low-confidence inventory report.
#[must_use]
#[instrument(level = "trace", skip(graph))]
pub fn doctor(graph: &DepGraph) -> LineageDoctorReport {
    let mut report = LineageDoctorReport {
        node_count: graph.node_count(),
        edge_count: graph.edge_count(),
        ..LineageDoctorReport::default()
    };
    for edge in &graph.edges {
        let conf = depgraph_confidence_to_lineage(&edge.confidence);
        match conf {
            Confidence::Exact => report.edges_exact += 1,
            Confidence::Heuristic => report.edges_heuristic += 1,
            Confidence::Unknown => {
                report.edges_unknown += 1;
                let reason = edge
                    .confidence
                    .explanation
                    .clone()
                    .unwrap_or_else(|| "Opaque".into());
                *report.unknown_reasons.entry(reason).or_insert(0) += 1;
            }
        }
    }
    report
}

/// Wrap a [`doctor`] report in the versioned robot-JSON envelope.
#[must_use]
#[instrument(level = "trace", skip(report))]
pub fn doctor_envelope(report: LineageDoctorReport) -> RobotJsonEnvelope<LineageDoctorReport> {
    RobotJsonEnvelope::new(DOCTOR_SCHEMA, report)
}

/// Customer-facing explanation of a single dependency-graph edge.
///
/// Wraps the depgraph's `ExplainEdge` (provenance + evidence) with a
/// lineage-flavored confidence tier, one-line summary, and a remediation
/// hint that tells the customer how to turn a low-confidence edge into a
/// higher-confidence one — the answer to plan §1.5's "what would improve
/// confidence" question (R13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineageExplanationEdge {
    pub edge: plsql_depgraph::ExplainEdge,
    pub confidence: Confidence,
    pub unknown_reason: Option<String>,
    pub summary: String,
    pub remediation: Option<String>,
}

/// Customer-facing explanation of a single graph node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineageExplanationNode {
    pub node: plsql_depgraph::ExplainNode,
    pub incoming_count: usize,
    pub outgoing_count: usize,
    pub summary: String,
}

/// Customer-facing explanation of a directed path between two nodes.
///
/// `aggregate_confidence` is the min over edge confidences (a path is
/// only as strong as its weakest edge). `blockers` lists unique unknown
/// reasons seen along the path, so the report can call out the specific
/// remediations that would lift the path's overall confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineageExplanationPath {
    pub path: plsql_depgraph::ExplainPath,
    pub aggregate_confidence: Confidence,
    pub blockers: Vec<String>,
    pub summary: String,
}

/// Discriminated union of customer-facing explain outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LineageExplanation {
    Edge(Box<LineageExplanationEdge>),
    Node(LineageExplanationNode),
    Path(LineageExplanationPath),
}

fn remediation_for(reason: Option<&str>) -> Option<String> {
    match reason {
        Some(r) if r.contains("DynamicSql") || r.contains("Dynamic") => Some(
            "Bind variables or DBMS_ASSERT-style allowlist comparisons \
             would let the engine resolve this dynamic SQL site."
                .into(),
        ),
        Some(r) if r.contains("DbLink") => Some(
            "Add the remote object's catalog snapshot to enable cross-database \
             resolution; otherwise the edge stays opaque by design."
                .into(),
        ),
        Some(r) if r.contains("Missing") => Some(
            "Vendor the missing package body or grant the engine access to \
             a catalog snapshot that includes it."
                .into(),
        ),
        Some(r) if r.contains("Wrapped") => Some(
            "WRAPPED PL/SQL is intentionally opaque; consider replacing with \
             an unwrapped fixture in non-production analysis runs."
                .into(),
        ),
        Some(_) => None,
        None => None,
    }
}

fn explanation_edge(edge: plsql_depgraph::ExplainEdge) -> LineageExplanationEdge {
    let confidence = depgraph_confidence_to_lineage(&edge.confidence);
    let unknown_reason = match confidence {
        Confidence::Unknown => Some(
            edge.confidence
                .explanation
                .clone()
                .unwrap_or_else(|| "Opaque".into()),
        ),
        _ => None,
    };
    let summary = format!(
        "{} {} {}",
        edge.from.logical_id,
        edge.kind.as_str(),
        edge.to.logical_id
    );
    let remediation = remediation_for(unknown_reason.as_deref());
    LineageExplanationEdge {
        edge,
        confidence,
        unknown_reason,
        summary,
        remediation,
    }
}

/// Explain a single edge in customer-facing shape.
#[instrument(level = "trace", skip(graph))]
pub fn explain_edge(
    graph: &DepGraph,
    edge_id: plsql_depgraph::EdgeId,
) -> Result<LineageExplanation, plsql_depgraph::GraphQueryError> {
    let raw = graph.explain_edge(edge_id)?;
    Ok(LineageExplanation::Edge(Box::new(explanation_edge(raw))))
}

/// Explain a node in customer-facing shape, including in/out edge counts.
#[instrument(level = "trace", skip(graph))]
pub fn explain_node(
    graph: &DepGraph,
    selector: &NodeSelector,
) -> Result<LineageExplanation, plsql_depgraph::GraphQueryError> {
    let raw = graph.explain_node(selector)?;
    let incoming_count = raw.incoming_edges.len();
    let outgoing_count = raw.outgoing_edges.len();
    let summary = format!(
        "{} — {} incoming edge(s), {} outgoing edge(s)",
        raw.node.logical_id, incoming_count, outgoing_count
    );
    Ok(LineageExplanation::Node(LineageExplanationNode {
        node: raw,
        incoming_count,
        outgoing_count,
        summary,
    }))
}

/// Explain a directed path between two nodes in customer-facing shape.
#[instrument(level = "trace", skip(graph))]
pub fn explain_path(
    graph: &DepGraph,
    from: &NodeSelector,
    to: &NodeSelector,
) -> Result<LineageExplanation, plsql_depgraph::GraphQueryError> {
    let raw = graph.explain_path(from, to)?;
    let mut aggregate = Confidence::Exact;
    let mut blockers = Vec::new();
    let mut seen_blockers = std::collections::BTreeSet::new();
    for edge in &raw.edges {
        let conf = depgraph_confidence_to_lineage(&edge.confidence);
        aggregate = aggregate.min(conf);
        if matches!(conf, Confidence::Unknown) {
            let reason = edge
                .confidence
                .explanation
                .clone()
                .unwrap_or_else(|| "Opaque".into());
            if seen_blockers.insert(reason.clone()) {
                blockers.push(reason);
            }
        }
    }
    let summary = if raw.found {
        format!(
            "{} → {} ({} edge(s), aggregate confidence: {:?})",
            raw.from.logical_id,
            raw.to.logical_id,
            raw.edges.len(),
            aggregate
        )
    } else {
        format!(
            "no path from {} to {}",
            raw.from.logical_id, raw.to.logical_id
        )
    };
    Ok(LineageExplanation::Path(LineageExplanationPath {
        path: raw,
        aggregate_confidence: aggregate,
        blockers,
        summary,
    }))
}

/// Wrap a customer-facing [`LineageExplanation`] in the versioned envelope.
#[must_use]
#[instrument(level = "trace", skip(explanation))]
pub fn explain_envelope(explanation: LineageExplanation) -> RobotJsonEnvelope<LineageExplanation> {
    RobotJsonEnvelope::new(EXPLAIN_SCHEMA, explanation)
}

/// Output of `recompile_order`.
///
/// `order` lists logical object IDs in the sequence Oracle will accept
/// recompilation. Each object's dependencies (those also in the input
/// set) appear before it. `cycles` carries logical IDs of objects that
/// could not be ordered because they participate in a mutual-dependency
/// cycle within the set; Oracle handles these via deferred recompile
/// passes, and the report surfaces them so the operator can review.
/// `missing` lists input IDs that were not present in the graph (a
/// frequent symptom of `--change` diffs that reference objects the
/// engine has not parsed).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecompilePlan {
    pub order: Vec<String>,
    pub cycles: Vec<Vec<String>>,
    pub missing: Vec<String>,
}

/// Compute a recompile order for a set of changed objects.
///
/// Constructs the subgraph induced by the input set, then runs Kahn's
/// algorithm: in Oracle, if A depends on B then B must be recompiled
/// before A. Cycles within the input set are collected separately
/// rather than silently misordered.
///
/// Caller passes logical object IDs (`schema.object`,
/// `schema.package.member`, etc.). Inputs not present in the graph are
/// returned in `missing` so the caller can surface them as
/// `UnknownReason::MissingCatalogObject` candidates (R13).
#[must_use]
#[instrument(level = "trace", skip(graph))]
pub fn recompile_order(graph: &DepGraph, set: &[&str]) -> RecompilePlan {
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

    let mut plan = RecompilePlan::default();
    let mut in_set: HashMap<plsql_depgraph::NodeId, String> = HashMap::new();
    let mut missing_track: BTreeSet<String> = BTreeSet::new();

    for input in set {
        match graph.resolve_node(&NodeSelector::LogicalObjectId((*input).to_string())) {
            Ok(node) => {
                in_set.insert(node.id, (*input).to_string());
            }
            Err(_) => {
                missing_track.insert((*input).to_string());
            }
        }
    }
    plan.missing = missing_track.into_iter().collect();

    if in_set.is_empty() {
        return plan;
    }

    // Build induced-subgraph adjacency: for an edge from→to where both
    // endpoints are in the set, recompilation must respect "to before
    // from" (the caller depends on the callee).
    let mut predecessors_of: BTreeMap<plsql_depgraph::NodeId, BTreeSet<plsql_depgraph::NodeId>> =
        BTreeMap::new();
    let mut successors_of: BTreeMap<plsql_depgraph::NodeId, BTreeSet<plsql_depgraph::NodeId>> =
        BTreeMap::new();

    for edge in &graph.edges {
        let (from_in, to_in) = (
            in_set.contains_key(&edge.from),
            in_set.contains_key(&edge.to),
        );
        // Convention in this graph: an edge `A -> B` means B reads / depends
        // on A (the impact walk uses this to find downstream nodes). For
        // recompile ordering, A must therefore be recompiled before B, so B
        // gains A as a predecessor and A gains B as a successor.
        if from_in && to_in && edge.from != edge.to {
            predecessors_of
                .entry(edge.to)
                .or_default()
                .insert(edge.from);
            successors_of.entry(edge.from).or_default().insert(edge.to);
        }
    }

    let mut remaining: BTreeSet<plsql_depgraph::NodeId> = in_set.keys().copied().collect();
    let mut queue: VecDeque<plsql_depgraph::NodeId> = remaining
        .iter()
        .filter(|id| predecessors_of.get(id).is_none_or(BTreeSet::is_empty))
        .copied()
        .collect();
    let mut ordered: Vec<plsql_depgraph::NodeId> = Vec::new();
    let mut peeled: HashSet<plsql_depgraph::NodeId> = HashSet::new();

    while let Some(node) = queue.pop_front() {
        if !peeled.insert(node) {
            continue;
        }
        ordered.push(node);
        remaining.remove(&node);
        if let Some(succs) = successors_of.get(&node).cloned() {
            for succ in succs {
                if let Some(preds) = predecessors_of.get_mut(&succ) {
                    preds.remove(&node);
                    if preds.is_empty() {
                        queue.push_back(succ);
                    }
                }
            }
        }
    }

    plan.order = ordered
        .into_iter()
        .filter_map(|id| in_set.get(&id).cloned())
        .collect();

    if !remaining.is_empty() {
        let mut cycle: Vec<String> = remaining
            .into_iter()
            .filter_map(|id| in_set.get(&id).cloned())
            .collect();
        cycle.sort();
        plan.cycles.push(cycle);
    }

    plan
}

/// Wrap a [`RecompilePlan`] in the versioned robot-JSON envelope.
#[must_use]
#[instrument(level = "trace", skip(plan))]
pub fn recompile_order_envelope(plan: RecompilePlan) -> RobotJsonEnvelope<RecompilePlan> {
    RobotJsonEnvelope::new(RECOMPILE_ORDER_SCHEMA, plan)
}

/// Wrap an [`impact`] result in the versioned robot-JSON envelope.
/// Consumers should serialize the returned envelope directly via
/// `serde_json::to_string`.
#[must_use]
#[instrument(level = "trace", skip(result))]
pub fn impact_envelope(result: LineageResult) -> RobotJsonEnvelope<LineageResult> {
    RobotJsonEnvelope::new(IMPACT_SCHEMA, result)
}

/// Wrap a [`dependencies`] result in the versioned robot-JSON envelope.
#[must_use]
#[instrument(level = "trace", skip(result))]
pub fn dependencies_envelope(result: LineageResult) -> RobotJsonEnvelope<LineageResult> {
    RobotJsonEnvelope::new(DEPENDENCIES_SCHEMA, result)
}

/// Wrap a `SemanticChangeSet` in the versioned robot-JSON envelope used
/// by the `classify-change` and `compare-oracle-deps` operations.
#[must_use]
#[instrument(level = "trace", skip(change_set))]
pub fn classify_change_envelope(
    change_set: SemanticChangeSet,
) -> RobotJsonEnvelope<SemanticChangeSet> {
    RobotJsonEnvelope::new(CLASSIFY_CHANGE_SCHEMA, change_set)
}

/// Map depgraph `Confidence` (level + explanation) to lineage `Confidence`
/// (Exact / Heuristic / Unknown).
fn depgraph_confidence_to_lineage(conf: &plsql_core::Confidence) -> Confidence {
    use plsql_core::ConfidenceLevel;
    match conf.level {
        ConfidenceLevel::High => Confidence::Exact,
        ConfidenceLevel::Medium => Confidence::Heuristic,
        ConfidenceLevel::Low | ConfidenceLevel::Opaque => Confidence::Unknown,
    }
}

impl Confidence {
    /// Numeric rank where higher = more certain. Used for path
    /// aggregation: the path's confidence is the **min** edge confidence
    /// along the path, and a node's overall confidence is the **max**
    /// path confidence over all reaching paths.
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Self::Exact => 2,
            Self::Heuristic => 1,
            Self::Unknown => 0,
        }
    }

    /// Pick the lower-confidence tier (used to weaken a path that
    /// crosses a heuristic or unknown edge).
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        if self.rank() <= other.rank() {
            self
        } else {
            other
        }
    }

    /// Pick the higher-confidence tier (used to strengthen a node's
    /// overall confidence when a better-confidence path is found).
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }
}

/// Walk the dependency graph downstream from `node` to find every object
/// that may be affected by a change to it. "Downstream" means: edges
/// where `edge.from == node`, i.e. objects that depend on the anchor.
///
/// Confidence aggregation: the path-confidence reaching a node is the
/// minimum confidence over edges in that path; if multiple paths reach
/// the same node, the node's `path_confidence` is the maximum (the
/// engine reports the strongest proof it can support).
///
/// Consumers include `callers`, `unsafe-paths`, `what-breaks`,
/// the HTML impact subgraph, GraphML export, and `explain`.
pub fn impact(graph: &DepGraph, node: &NodeId, max_depth: Option<u32>) -> LineageResult {
    let selector = NodeSelector::NodeId(*node);
    let anchor_node = match graph.resolve_node(&selector) {
        Ok(n) => n,
        Err(_) => return LineageResult::default(),
    };

    let anchor_logical = anchor_node.logical_id.to_string();
    let depth_limit = max_depth.unwrap_or(u32::MAX);

    let mut result = LineageResult::default();
    let mut emitted_edges = std::collections::HashSet::new();

    let mut best_confidence: std::collections::HashMap<NodeId, (Confidence, u32)> =
        std::collections::HashMap::new();
    best_confidence.insert(*node, (Confidence::Exact, 0));

    let mut queue = std::collections::VecDeque::new();
    queue.push_back((*node, 0u32, Confidence::Exact));

    while let Some((current, depth, path_conf)) = queue.pop_front() {
        if depth >= depth_limit {
            continue;
        }

        let outgoing = match graph.query_neighbors(&NodeSelector::NodeId(current)) {
            Ok(neighbors) => neighbors.edges,
            Err(_) => continue,
        };

        for edge in &outgoing {
            let edge_conf = depgraph_confidence_to_lineage(&edge.confidence);
            let next_conf = path_conf.min(edge_conf);
            let target_id = NodeId::new(edge.to.id.get());

            if emitted_edges.insert(edge.id) {
                result.edges.push(LineageEdge {
                    source: edge.from.logical_id.clone(),
                    target: edge.to.logical_id.clone(),
                    kind: edge.kind.as_str().to_string(),
                    confidence: edge_conf,
                });
                if matches!(edge_conf, Confidence::Unknown) {
                    result.unknown_edges.push(UnknownEdge {
                        source: edge.from.logical_id.clone(),
                        unknown_reason: edge
                            .confidence
                            .explanation
                            .clone()
                            .unwrap_or_else(|| "Opaque".into()),
                        detail: None,
                    });
                }
            }

            let improved = match best_confidence.get(&target_id) {
                None => true,
                Some((existing, _)) => next_conf.rank() > existing.rank(),
            };
            if improved {
                best_confidence.insert(target_id, (next_conf, depth + 1));
                queue.push_back((target_id, depth + 1, next_conf));
            }
        }
    }

    let mut affected: Vec<AffectedNode> = best_confidence
        .into_iter()
        .filter(|(id, _)| id != node)
        .filter_map(|(id, (conf, hops))| {
            graph
                .resolve_node(&NodeSelector::NodeId(id))
                .ok()
                .map(|n| AffectedNode {
                    logical_id: n.logical_id.to_string(),
                    hops,
                    path_confidence: conf,
                })
        })
        .collect();
    affected.sort_by(|a, b| {
        b.path_confidence
            .rank()
            .cmp(&a.path_confidence.rank())
            .then_with(|| a.hops.cmp(&b.hops))
            .then_with(|| a.logical_id.cmp(&b.logical_id))
    });
    result.affected_nodes = affected;

    result.query = Some(LineageQuery {
        anchor: anchor_logical,
        direction: LineageDirection::Downstream,
        max_depth,
        min_confidence: None,
    });

    result
}

// ---------------------------------------------------------------------------
// callers() + column_readers() / column_writers() — edge-kind-filtered
// neighbour queries that answer "who uses this object?" and "who touches this
// column?" without forcing the caller through `dependencies()` / `impact()`.
// ---------------------------------------------------------------------------

/// One direct caller of a target routine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caller {
    /// Logical id of the calling object (caller side of the `Calls` edge).
    pub caller_logical_id: String,
    /// Identity kind from the depgraph (`StandaloneProcedure`,
    /// `PackageProcedure`, `Trigger`, etc.).
    pub caller_kind: String,
    /// Confidence carried by the call edge (mirrors `LineageEdge`).
    pub confidence: Confidence,
}

/// Output of [`callers`]. Lists every node with an outgoing
/// `EdgeKind::Calls` edge into the target routine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallersResult {
    /// Logical id of the routine that callers point at.
    pub target_logical_id: String,
    /// Identity kind of the target routine (best-effort; empty if the
    /// target node could not be resolved).
    pub target_kind: String,
    /// All direct callers, sorted by `caller_logical_id` for stable
    /// downstream rendering.
    pub callers: Vec<Caller>,
}

/// Whether a column-access result represents reads or writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnAccessKind {
    #[default]
    Read,
    Write,
}

/// One object that touches a column. `edge_kind` retains the exact
/// `EdgeKind` (`ReadsColumn`, `ReadsUnknownColumnOfTable`,
/// `WritesColumn`, `WritesUnknownColumnOfTable`, `DerivesColumn`) so
/// callers can distinguish exact column accesses from
/// table-level-unknown approximations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnAccessor {
    pub accessor_logical_id: String,
    pub accessor_kind: String,
    pub edge_kind: String,
    pub confidence: Confidence,
    /// `true` when the depgraph could only attribute the access to the
    /// owning table (no per-column resolution). Engineers reviewing a
    /// column rename treat these as conservative "must-check" cases.
    pub is_unknown_column_of_table: bool,
}

/// Output of [`column_readers`] / [`column_writers`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnAccessResult {
    /// Logical id of the column the query was anchored on. Empty when
    /// the input `NodeSelector` could not be resolved — see
    /// `resolution_error` to distinguish "node missing" from "node
    /// resolved, no accessors found".
    pub column_logical_id: String,
    /// Whether this result represents reads or writes.
    pub access: ColumnAccessKind,
    /// Every accessor, sorted by `accessor_logical_id` then `edge_kind`.
    pub accessors: Vec<ColumnAccessor>,
    /// Diagnostic for callers: present when the input `NodeSelector`
    /// could not be resolved (no node with that id/logical-id exists).
    /// `None` means the node was found and its incoming neighbours
    /// scanned successfully — even when `accessors` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_error: Option<String>,
}

/// List every direct caller of `target` (objects with an outgoing
/// `EdgeKind::Calls` edge pointing at `target`).
///
/// This is the edge-kind-filtered first-hop view of [`dependencies`]:
/// where `dependencies` returns every upstream node regardless of edge
/// kind, `callers` answers the much narrower question "who invokes
/// this routine?". Triggers / type methods that fire on this routine
/// also surface here.
///
/// Consumers: `plsql-doc`, `plsql-scan` reachability, CLI
/// `lineage callers` subcommand.
#[must_use]
pub fn callers(graph: &DepGraph, target: &NodeSelector) -> CallersResult {
    let Ok(target_node) = graph.resolve_node(target) else {
        return CallersResult::default();
    };

    let target_logical_id = target_node.logical_id.to_string();
    let target_kind = target_node.identity_kind.as_str().to_string();

    let Ok(incoming) = graph.query_reverse_neighbors(target) else {
        return CallersResult {
            target_logical_id,
            target_kind,
            ..CallersResult::default()
        };
    };

    let mut callers: Vec<Caller> = incoming
        .edges
        .into_iter()
        .filter(|edge| matches!(edge.kind, plsql_depgraph::EdgeKind::Calls))
        .map(|edge| Caller {
            caller_logical_id: edge.from.logical_id,
            caller_kind: edge.from.identity_kind.as_str().to_string(),
            confidence: depgraph_confidence_to_lineage(&edge.confidence),
        })
        .collect();

    callers.sort_by(|a, b| {
        a.caller_logical_id
            .cmp(&b.caller_logical_id)
            .then_with(|| a.caller_kind.cmp(&b.caller_kind))
    });

    CallersResult {
        target_logical_id,
        target_kind,
        callers,
    }
}

/// List every accessor that reads `column`. Includes
/// `ReadsColumn` (exact) and `ReadsUnknownColumnOfTable` (the depgraph
/// knows the table but couldn't pin down a specific column — those
/// rows have `is_unknown_column_of_table = true`).
#[must_use]
pub fn column_readers(graph: &DepGraph, column: &NodeSelector) -> ColumnAccessResult {
    column_access(graph, column, ColumnAccessKind::Read)
}

/// List every accessor that writes `column`. Includes `WritesColumn`,
/// `WritesUnknownColumnOfTable`, and `DerivesColumn` (the latter is
/// the depgraph's record of "this column's value flows from another
/// expression" — a logical write of a derived column).
#[must_use]
pub fn column_writers(graph: &DepGraph, column: &NodeSelector) -> ColumnAccessResult {
    column_access(graph, column, ColumnAccessKind::Write)
}

fn column_access(
    graph: &DepGraph,
    column: &NodeSelector,
    access: ColumnAccessKind,
) -> ColumnAccessResult {
    let resolved = graph.resolve_node(column);
    let column_logical_id = match resolved {
        Ok(node) => node.logical_id.to_string(),
        Err(_) => String::new(),
    };
    if resolved.is_err() {
        return ColumnAccessResult {
            column_logical_id,
            access,
            accessors: Vec::new(),
            resolution_error: Some(
                "column node could not be resolved from the supplied selector".to_owned(),
            ),
        };
    }

    let Ok(incoming) = graph.query_reverse_neighbors(column) else {
        return ColumnAccessResult {
            column_logical_id,
            access,
            accessors: Vec::new(),
            resolution_error: Some("neighbour query failed against the resolved column".to_owned()),
        };
    };

    let mut accessors: Vec<ColumnAccessor> = incoming
        .edges
        .into_iter()
        .filter_map(|edge| {
            let kind_str = edge.kind.as_str().to_string();
            let (matches, is_unknown) = match (access, edge.kind) {
                (ColumnAccessKind::Read, plsql_depgraph::EdgeKind::ReadsColumn) => (true, false),
                (ColumnAccessKind::Read, plsql_depgraph::EdgeKind::ReadsUnknownColumnOfTable) => {
                    (true, true)
                }
                (ColumnAccessKind::Write, plsql_depgraph::EdgeKind::WritesColumn) => (true, false),
                (ColumnAccessKind::Write, plsql_depgraph::EdgeKind::WritesUnknownColumnOfTable) => {
                    (true, true)
                }
                (ColumnAccessKind::Write, plsql_depgraph::EdgeKind::DerivesColumn) => (true, false),
                _ => (false, false),
            };
            matches.then_some(ColumnAccessor {
                accessor_logical_id: edge.from.logical_id,
                accessor_kind: edge.from.identity_kind.as_str().to_string(),
                edge_kind: kind_str,
                confidence: depgraph_confidence_to_lineage(&edge.confidence),
                is_unknown_column_of_table: is_unknown,
            })
        })
        .collect();

    accessors.sort_by(|a, b| {
        a.accessor_logical_id
            .cmp(&b.accessor_logical_id)
            .then_with(|| a.edge_kind.cmp(&b.edge_kind))
    });

    ColumnAccessResult {
        column_logical_id,
        access,
        accessors,
        resolution_error: None,
    }
}

/// Wrap a [`CallersResult`] in a versioned envelope for stable wire
/// output.
///
/// Schema: [`CALLERS_SCHEMA`] (`plsql.lineage.callers` v1.0.0).
/// Consumers can pin the major version and additive minor bumps stay
/// forward-compatible.
#[must_use]
pub fn callers_envelope(result: CallersResult) -> RobotJsonEnvelope<CallersResult> {
    RobotJsonEnvelope::new(CALLERS_SCHEMA, result)
}

/// Wrap a [`ColumnAccessResult`] in a versioned envelope.
///
/// Schema: [`COLUMN_ACCESS_SCHEMA`] (`plsql.lineage.column_access` v1.0.0).
/// Used for both `column_readers` and `column_writers` outputs — the
/// `access` field disambiguates which.
#[must_use]
pub fn column_access_envelope(result: ColumnAccessResult) -> RobotJsonEnvelope<ColumnAccessResult> {
    RobotJsonEnvelope::new(COLUMN_ACCESS_SCHEMA, result)
}

// ---------------------------------------------------------------------------
// unsafe_paths() — paths from `from` to `to` that traverse at least one
// opaque or dynamic-SQL edge. PLSQL-LIN-005.
//
// "Unsafe" here is the inverse of "auditable": when an edge is
// `OpaqueDynamic` (the depgraph could not pin a target without runtime
// information) or its confidence is `Unknown`, any path crossing it
// inherits that uncertainty. Compliance / change-impact reviewers need
// to see exactly which paths cross those edges so they can attach the
// evidence to audit tickets.
// ---------------------------------------------------------------------------

/// Schema descriptor for `unsafe-paths(from, to)` results.
pub const UNSAFE_PATHS_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.unsafe_paths",
    version: SchemaVersion::new(1, 0, 0),
    description: "Paths between two graph nodes that traverse opaque or dynamic-SQL edges",
};

/// Default cap on path depth (in edges) used when the caller does not
/// override it. Mirrors the depth a hand audit can reasonably review.
pub const UNSAFE_PATHS_DEFAULT_MAX_DEPTH: u32 = 8;

/// Default cap on the number of paths emitted; protects against
/// pathological graphs where the search space is exponential.
pub const UNSAFE_PATHS_DEFAULT_MAX_PATHS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsafePath {
    /// Logical ids visited, in order from `from` to `to`.
    pub nodes: Vec<String>,
    /// Edges traversed, in order.
    pub edges: Vec<LineageEdge>,
    /// Indices into `edges` that are themselves unsafe (opaque /
    /// dynamic / unknown-confidence).
    pub unsafe_edge_indices: Vec<usize>,
    /// Weakest confidence across the whole path (the path is at most
    /// as auditable as its weakest edge).
    pub overall_confidence: Confidence,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsafePathsResult {
    /// Logical id of the search source.
    pub from_logical_id: String,
    /// Logical id of the search target.
    pub to_logical_id: String,
    /// Every unsafe path discovered, sorted by length (ascending) then
    /// by joined node logical-id for stable rendering.
    pub paths: Vec<UnsafePath>,
    /// `true` when the search was cut short by `max_paths`; callers
    /// surface this to the user so they know there may be additional
    /// unsafe paths beyond the cap.
    pub truncated: bool,
}

/// Find every path from `from` to `to` (up to `max_depth` edges and
/// `max_paths` total) that includes at least one unsafe edge. An edge
/// is "unsafe" when its `EdgeKind` is `OpaqueDynamic` **or** its
/// confidence is `Unknown` — both indicate that the depgraph could
/// not prove the relationship without runtime evidence.
///
/// Used by:
/// - the lineage CLI's `unsafe-paths` subcommand
/// - the SAST reachability rule for dynamic SQL
/// - the customer Trust Block dynamic-SQL audit section
#[must_use]
pub fn unsafe_paths(
    graph: &DepGraph,
    from: &NodeSelector,
    to: &NodeSelector,
    max_depth: Option<u32>,
    max_paths: Option<usize>,
) -> UnsafePathsResult {
    let Ok(from_node) = graph.resolve_node(from) else {
        return UnsafePathsResult::default();
    };
    let Ok(to_node) = graph.resolve_node(to) else {
        return UnsafePathsResult {
            from_logical_id: from_node.logical_id.to_string(),
            ..UnsafePathsResult::default()
        };
    };

    let from_id = from_node.id;
    let to_id = to_node.id;
    let from_logical_id = from_node.logical_id.to_string();
    let to_logical_id = to_node.logical_id.to_string();

    let depth_limit = max_depth.unwrap_or(UNSAFE_PATHS_DEFAULT_MAX_DEPTH);
    let path_cap = max_paths.unwrap_or(UNSAFE_PATHS_DEFAULT_MAX_PATHS);

    // DFS with an explicit stack so we can prune by depth without
    // recursive frames. Each stack frame is (node, depth, on-path-flag,
    // accumulated edges, accumulated nodes).
    let mut paths: Vec<UnsafePath> = Vec::new();
    let mut truncated = false;
    let mut on_path: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
    on_path.insert(from_id);

    let mut node_stack: Vec<(NodeId, u32)> = vec![(from_id, 0)];
    let mut path_nodes: Vec<String> = vec![from_logical_id.clone()];
    let mut path_edges: Vec<LineageEdge> = Vec::new();
    let mut path_unsafe_idx: Vec<usize> = Vec::new();

    // Each frame on `iter_stack` holds the outgoing iterator for the
    // current top of `node_stack`, plus the index into `path_edges`
    // where this frame began (used during backtrack).
    let mut iter_stack: Vec<std::vec::IntoIter<plsql_depgraph::EdgeSummary>> = Vec::new();
    if let Ok(neighbors) = graph.query_neighbors(from) {
        iter_stack.push(neighbors.edges.into_iter());
    } else {
        return UnsafePathsResult {
            from_logical_id,
            to_logical_id,
            paths,
            truncated,
        };
    }

    // Local helper so the destination-reached and exhausted-iter
    // branches share one definition of "pop the last edge". They must
    // stay in lock-step; PLSQL-LIN-020 traced an audit comment to
    // exactly this duplication.
    fn pop_path_step(
        path_edges: &mut Vec<LineageEdge>,
        path_nodes: &mut Vec<String>,
        path_unsafe_idx: &mut Vec<usize>,
    ) {
        path_nodes.pop();
        if path_edges.is_empty() {
            return;
        }
        let last_was_unsafe = path_unsafe_idx
            .last()
            .copied()
            .is_some_and(|idx| idx == path_edges.len() - 1);
        if last_was_unsafe {
            path_unsafe_idx.pop();
        }
        path_edges.pop();
    }

    while let Some(iter) = iter_stack.last_mut() {
        match iter.next() {
            None => {
                // Exhausted this node's outgoing edges; backtrack.
                iter_stack.pop();
                if let Some((leaving, _depth)) = node_stack.pop() {
                    on_path.remove(&leaving);
                }
                pop_path_step(&mut path_edges, &mut path_nodes, &mut path_unsafe_idx);
            }
            Some(edge) => {
                let next_id = edge.to.id;
                if on_path.contains(&next_id) {
                    continue; // skip cycles
                }
                let (_current_id, depth) = *node_stack
                    .last()
                    .expect("iter_stack and node_stack stay in lock-step");
                let next_depth = depth + 1;
                if next_depth > depth_limit {
                    continue;
                }

                let lineage_edge = LineageEdge {
                    source: edge.from.logical_id,
                    target: edge.to.logical_id.clone(),
                    kind: edge.kind.as_str().to_string(),
                    confidence: depgraph_confidence_to_lineage(&edge.confidence),
                };
                let is_unsafe = matches!(edge.kind, plsql_depgraph::EdgeKind::OpaqueDynamic)
                    || matches!(lineage_edge.confidence, Confidence::Unknown);

                path_edges.push(lineage_edge);
                if is_unsafe {
                    path_unsafe_idx.push(path_edges.len() - 1);
                }
                path_nodes.push(edge.to.logical_id);

                if next_id == to_id {
                    // Reached destination; record path iff at least one
                    // edge along the way is unsafe.
                    if !path_unsafe_idx.is_empty() {
                        let overall = path_edges
                            .iter()
                            .map(|e| e.confidence)
                            .fold(Confidence::Exact, Confidence::min);
                        paths.push(UnsafePath {
                            nodes: path_nodes.clone(),
                            edges: path_edges.clone(),
                            unsafe_edge_indices: path_unsafe_idx.clone(),
                            overall_confidence: overall,
                        });
                        if paths.len() >= path_cap {
                            truncated = true;
                            break;
                        }
                    }
                    // Backtrack — destination is a leaf for the path search.
                    pop_path_step(&mut path_edges, &mut path_nodes, &mut path_unsafe_idx);
                    continue;
                }

                // Descend.
                on_path.insert(next_id);
                node_stack.push((next_id, next_depth));
                let neighbors = match graph.query_neighbors(&NodeSelector::NodeId(next_id)) {
                    Ok(n) => n.edges.into_iter(),
                    Err(_) => Vec::new().into_iter(),
                };
                iter_stack.push(neighbors);
            }
        }
    }

    paths.sort_by(|a, b| {
        a.edges
            .len()
            .cmp(&b.edges.len())
            .then_with(|| a.nodes.join(">").cmp(&b.nodes.join(">")))
    });

    UnsafePathsResult {
        from_logical_id,
        to_logical_id,
        paths,
        truncated,
    }
}

/// Wrap an [`UnsafePathsResult`] in a versioned envelope.
///
/// Schema: [`UNSAFE_PATHS_SCHEMA`] (`plsql.lineage.unsafe_paths` v1.0.0).
/// `paths[].overall_confidence` and `truncated` carry the audit-trail
/// metadata downstream tools (SAST reachability, Trust Block UI) pin on.
#[must_use]
pub fn unsafe_paths_envelope(result: UnsafePathsResult) -> RobotJsonEnvelope<UnsafePathsResult> {
    RobotJsonEnvelope::new(UNSAFE_PATHS_SCHEMA, result)
}

// ---------------------------------------------------------------------------
// GraphML export of an impact / dependency subgraph. PLSQL-LIN-010.
// ---------------------------------------------------------------------------

/// Schema descriptor for the lineage GraphML export.
pub const LINEAGE_GRAPHML_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.graphml",
    version: SchemaVersion::new(1, 0, 0),
    description: "GraphML export of an impact or dependency subgraph",
};

/// Wire envelope payload for the GraphML document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageGraphMlDocument {
    pub graphml: String,
}

/// Emit a GraphML document covering only the nodes and edges that appear
/// in `result`. Consumers (yEd / Gephi / Cytoscape) get an impact
/// subgraph instead of the full project depgraph. Node schema mirrors
/// the depgraph emitter so downstream tooling can ingest both.
///
/// Node set is the union of:
/// * every distinct logical id mentioned on either side of an edge
/// * every logical id in `result.affected_nodes`
/// * `result.query.anchor` if set
///
/// Unknown edges are emitted as synthetic `unknown::<reason>` nodes
/// connected from the upstream side so uncertainty stays visible.
#[must_use]
pub fn impact_to_graphml(result: &LineageResult) -> String {
    let mut node_ids: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let intern = |logical: &str, ids: &mut std::collections::BTreeMap<String, usize>| {
        let next = ids.len();
        *ids.entry(logical.to_owned()).or_insert(next)
    };

    if let Some(query) = &result.query {
        intern(&query.anchor, &mut node_ids);
    }
    for edge in &result.edges {
        intern(&edge.source, &mut node_ids);
        intern(&edge.target, &mut node_ids);
    }
    for affected in &result.affected_nodes {
        intern(&affected.logical_id, &mut node_ids);
    }
    for u in &result.unknown_edges {
        intern(&u.source, &mut node_ids);
        let synthetic = format!("unknown::{}", u.unknown_reason);
        intern(&synthetic, &mut node_ids);
    }

    let mut buf = String::new();
    buf.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xsi:schemaLocation=\"http://graphml.graphdrawing.org/xmlns \
         http://graphml.graphdrawing.org/xmlns/1.0/graphml.xsd\">\n",
    );
    buf.push_str(
        "  <key id=\"logical_id\" for=\"node\" attr.name=\"logical_id\" attr.type=\"string\" />\n",
    );
    buf.push_str(
        "  <key id=\"node_role\" for=\"node\" attr.name=\"node_role\" attr.type=\"string\" />\n",
    );
    buf.push_str(
        "  <key id=\"path_confidence\" for=\"node\" attr.name=\"path_confidence\" attr.type=\"string\" />\n",
    );
    buf.push_str("  <key id=\"hops\" for=\"node\" attr.name=\"hops\" attr.type=\"int\" />\n");
    buf.push_str(
        "  <key id=\"edge_kind\" for=\"edge\" attr.name=\"edge_kind\" attr.type=\"string\" />\n",
    );
    buf.push_str(
        "  <key id=\"edge_confidence\" for=\"edge\" attr.name=\"edge_confidence\" attr.type=\"string\" />\n",
    );
    buf.push_str("  <graph id=\"plsql-lineage-subgraph\" edgedefault=\"directed\">\n");

    let mut affected_index: std::collections::HashMap<&str, &AffectedNode> =
        std::collections::HashMap::new();
    for affected in &result.affected_nodes {
        affected_index.insert(affected.logical_id.as_str(), affected);
    }
    let anchor = result.query.as_ref().map(|q| q.anchor.as_str());

    for (logical, id) in &node_ids {
        let role = if Some(logical.as_str()) == anchor {
            "anchor"
        } else if logical.starts_with("unknown::") {
            "unknown-reason"
        } else if affected_index.contains_key(logical.as_str()) {
            "affected"
        } else {
            "node"
        };
        buf.push_str("    <node id=\"n");
        buf.push_str(&id.to_string());
        buf.push_str("\">\n");
        push_xml_data(&mut buf, "logical_id", logical);
        push_xml_data(&mut buf, "node_role", role);
        if let Some(affected) = affected_index.get(logical.as_str()) {
            push_xml_data(
                &mut buf,
                "path_confidence",
                confidence_label(affected.path_confidence),
            );
            push_xml_data(&mut buf, "hops", &affected.hops.to_string());
        }
        buf.push_str("    </node>\n");
    }

    for (idx, edge) in result.edges.iter().enumerate() {
        let from = node_ids.get(edge.source.as_str()).copied().unwrap_or(0);
        let to = node_ids.get(edge.target.as_str()).copied().unwrap_or(0);
        buf.push_str("    <edge id=\"e");
        buf.push_str(&idx.to_string());
        buf.push_str("\" source=\"n");
        buf.push_str(&from.to_string());
        buf.push_str("\" target=\"n");
        buf.push_str(&to.to_string());
        buf.push_str("\">\n");
        push_xml_data(&mut buf, "edge_kind", &edge.kind);
        push_xml_data(
            &mut buf,
            "edge_confidence",
            confidence_label(edge.confidence),
        );
        buf.push_str("    </edge>\n");
    }

    for (offset, u) in result.unknown_edges.iter().enumerate() {
        let from = node_ids.get(u.source.as_str()).copied().unwrap_or(0);
        let synthetic = format!("unknown::{}", u.unknown_reason);
        let to = node_ids.get(synthetic.as_str()).copied().unwrap_or(0);
        let idx = result.edges.len() + offset;
        buf.push_str("    <edge id=\"u");
        buf.push_str(&idx.to_string());
        buf.push_str("\" source=\"n");
        buf.push_str(&from.to_string());
        buf.push_str("\" target=\"n");
        buf.push_str(&to.to_string());
        buf.push_str("\">\n");
        push_xml_data(&mut buf, "edge_kind", "UnknownReason");
        push_xml_data(
            &mut buf,
            "edge_confidence",
            confidence_label(Confidence::Unknown),
        );
        buf.push_str("    </edge>\n");
    }

    buf.push_str("  </graph>\n</graphml>\n");
    buf
}

/// Wrap an impact-subgraph GraphML payload in a versioned envelope.
///
/// Schema: [`LINEAGE_GRAPHML_SCHEMA`] (`plsql.lineage.graphml` v1.0.0).
/// The payload's `graphml` field is a complete, self-contained GraphML
/// document — node schema mirrors `plsql-depgraph` so consumer tools
/// can ingest both unchanged.
#[must_use]
pub fn impact_to_graphml_envelope(
    result: &LineageResult,
) -> RobotJsonEnvelope<LineageGraphMlDocument> {
    RobotJsonEnvelope::new(
        LINEAGE_GRAPHML_SCHEMA,
        LineageGraphMlDocument {
            graphml: impact_to_graphml(result),
        },
    )
}

// ---------------------------------------------------------------------------
// classify-rename — pair `Created` and `Dropped` records into candidate
// rename mappings, with explicit hint inputs. PLSQL-LIN-015.
//
// "Never silently merge." Renames are represented as delete+create in the
// base `SemanticChangeSet`. This classifier promotes pairs into
// `RenameCandidate`s when an externally-supplied hint matches; it never
// fuzzy-matches by name alone because Oracle dictionary semantics make
// naive name similarity unreliable.
// ---------------------------------------------------------------------------

pub const CLASSIFY_RENAME_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.classify_rename",
    version: SchemaVersion::new(1, 0, 0),
    description: "Candidate rename mappings derived from a SemanticChangeSet plus optional hints",
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameHints {
    pub git_renames: Vec<GitRenameHint>,
    pub explicit_mappings: std::collections::BTreeMap<String, String>,
    pub persistent_id_pairs: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRenameHint {
    pub from: String,
    pub to: String,
    pub similarity: u8,
}

pub const GIT_RENAME_THRESHOLD: u8 = 70;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenameEvidence {
    Explicit,
    PersistentId,
    GitRename { similarity: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameCandidate {
    pub before_logical_id: String,
    pub after_logical_id: String,
    pub confidence: Confidence,
    pub evidence: RenameEvidence,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameClassification {
    pub candidates: Vec<RenameCandidate>,
    pub unmatched_deletes: Vec<String>,
    pub unmatched_creates: Vec<String>,
}

/// Walk a `SemanticChangeSet` and pair up `Created`/`Dropped` records
/// into rename candidates. Priority order on hint conflict:
/// 1. `explicit_mappings`     → `Confidence::Exact`
/// 2. `persistent_id_pairs`   → `Confidence::Exact`
/// 3. `git_renames` ≥ `GIT_RENAME_THRESHOLD` → `Confidence::Heuristic`
///
/// Unmatched deletes / creates stay in their bucket — the classifier
/// never invents a rename without a hint (plan §10.3: false-positive
/// renames are worse than delete+create splits).
#[must_use]
pub fn classify_rename(changes: &SemanticChangeSet, hints: &RenameHints) -> RenameClassification {
    let mut deletes: std::collections::BTreeSet<String> = changes
        .changes
        .iter()
        .filter_map(|c| match c {
            ChangeRecord::Dropped { object_id } => Some(object_id.clone()),
            _ => None,
        })
        .collect();
    let mut creates: std::collections::BTreeSet<String> = changes
        .changes
        .iter()
        .filter_map(|c| match c {
            ChangeRecord::Created { object_id } => Some(object_id.clone()),
            _ => None,
        })
        .collect();

    let mut candidates: Vec<RenameCandidate> = Vec::new();

    for (before, after) in &hints.explicit_mappings {
        if deletes.remove(before) && creates.remove(after) {
            candidates.push(RenameCandidate {
                before_logical_id: before.clone(),
                after_logical_id: after.clone(),
                confidence: Confidence::Exact,
                evidence: RenameEvidence::Explicit,
            });
        }
    }
    for (before, after) in &hints.persistent_id_pairs {
        if deletes.remove(before) && creates.remove(after) {
            candidates.push(RenameCandidate {
                before_logical_id: before.clone(),
                after_logical_id: after.clone(),
                confidence: Confidence::Exact,
                evidence: RenameEvidence::PersistentId,
            });
        }
    }
    for hint in &hints.git_renames {
        if hint.similarity < GIT_RENAME_THRESHOLD {
            continue;
        }
        if deletes.remove(&hint.from) && creates.remove(&hint.to) {
            candidates.push(RenameCandidate {
                before_logical_id: hint.from.clone(),
                after_logical_id: hint.to.clone(),
                confidence: Confidence::Heuristic,
                evidence: RenameEvidence::GitRename {
                    similarity: hint.similarity,
                },
            });
        }
    }

    candidates.sort_by(|a, b| {
        a.before_logical_id
            .cmp(&b.before_logical_id)
            .then_with(|| a.after_logical_id.cmp(&b.after_logical_id))
    });

    RenameClassification {
        candidates,
        unmatched_deletes: deletes.into_iter().collect(),
        unmatched_creates: creates.into_iter().collect(),
    }
}

/// Wrap a [`RenameClassification`] in a versioned envelope.
///
/// Schema: [`CLASSIFY_RENAME_SCHEMA`] (`plsql.lineage.classify_rename` v1.0.0).
/// `candidates[].evidence` and `candidates[].confidence` are the
/// audit-trail surface consumers (release reviewers, lineage CLI)
/// inspect to decide whether to accept a rename mapping.
#[must_use]
pub fn classify_rename_envelope(
    classification: RenameClassification,
) -> RobotJsonEnvelope<RenameClassification> {
    RobotJsonEnvelope::new(CLASSIFY_RENAME_SCHEMA, classification)
}

// ---------------------------------------------------------------------------
// HTML impact-subgraph report. PLSQL-LIN-008.
//
// Pairs `plsql-render::html::shell` (chrome) with `plsql-render::svg::node_graph`
// (SVG) to produce a single self-contained HTML document. The SVG layout
// is a basic deterministic concentric ring: anchor in the center, affected
// nodes laid out by hop distance on successive radii.
// ---------------------------------------------------------------------------

/// Schema descriptor for the impact HTML report.
pub const LINEAGE_HTML_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.html_report",
    version: SchemaVersion::new(1, 0, 0),
    description: "Self-contained HTML impact subgraph report with embedded SVG",
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageHtmlDocument {
    pub html: String,
}

/// Render a `LineageResult` to a self-contained HTML page with an
/// embedded SVG impact subgraph + a Markdown-style table summarising the
/// affected-nodes list. Returns the HTML document as a string.
///
/// Layout is deterministic and pure: nodes are placed on concentric
/// rings indexed by their `hops` value (anchor at the center, hops=1
/// on the first ring, etc.). Unknown-reason edges surface as
/// dashed-link SVG nodes (the consumer-facing emphasis on uncertainty
/// from plan §1.5).
#[must_use]
pub fn impact_to_html(result: &LineageResult) -> String {
    use plsql_render::svg::{GraphEdge, GraphNode, GraphView};

    let anchor_label = result
        .query
        .as_ref()
        .map(|q| q.anchor.as_str())
        .unwrap_or("(no anchor)");

    // Build the concentric layout.
    let center_x: u32 = 480;
    let center_y: u32 = 320;
    let radius_step: u32 = 130;

    let mut nodes: Vec<GraphNode> = Vec::new();
    nodes.push(GraphNode {
        id: anchor_label.to_owned(),
        label: anchor_label.to_owned(),
        x: center_x,
        y: center_y,
    });

    let mut by_hops: std::collections::BTreeMap<u32, Vec<&AffectedNode>> =
        std::collections::BTreeMap::new();
    for affected in &result.affected_nodes {
        by_hops.entry(affected.hops).or_default().push(affected);
    }

    for (hops, group) in &by_hops {
        let radius = radius_step.saturating_mul(*hops);
        let count = group.len().max(1);
        for (idx, affected) in group.iter().enumerate() {
            let theta = (idx as f64 / count as f64) * std::f64::consts::TAU;
            let x = (center_x as f64 + radius as f64 * theta.cos()) as u32;
            let y = (center_y as f64 + radius as f64 * theta.sin()) as u32;
            nodes.push(GraphNode {
                id: affected.logical_id.clone(),
                label: affected.logical_id.clone(),
                x,
                y,
            });
        }
    }

    // Edges from result.edges.
    let mut edges: Vec<GraphEdge> = result
        .edges
        .iter()
        .map(|e| GraphEdge {
            from: e.source.clone(),
            to: e.target.clone(),
            label: Some(format!("{} · {}", e.kind, confidence_label(e.confidence))),
        })
        .collect();
    // Synthetic unknown-reason edges + nodes.
    for (idx, u) in result.unknown_edges.iter().enumerate() {
        let synthetic_id = format!("unknown::{}::{idx}", u.unknown_reason);
        let x = center_x + radius_step * 3;
        let y = center_y
            .saturating_sub(radius_step)
            .saturating_sub((idx as u32) * 40);
        nodes.push(GraphNode {
            id: synthetic_id.clone(),
            label: format!("? {}", u.unknown_reason),
            x,
            y,
        });
        edges.push(GraphEdge {
            from: u.source.clone(),
            to: synthetic_id,
            label: Some("UnknownReason".to_owned()),
        });
    }

    struct Layout<'a> {
        nodes: &'a [GraphNode],
        edges: &'a [GraphEdge],
    }
    impl<'a> GraphView for Layout<'a> {
        fn width(&self) -> u32 {
            960
        }
        fn height(&self) -> u32 {
            640
        }
        fn nodes(&self) -> &[GraphNode] {
            self.nodes
        }
        fn edges(&self) -> &[GraphEdge] {
            self.edges
        }
    }
    let svg = plsql_render::svg::node_graph(&Layout {
        nodes: &nodes,
        edges: &edges,
    });

    let headers: [&str; 4] = ["logical_id", "hops", "path_confidence", "role"];
    let mut rows: Vec<Vec<String>> = Vec::new();
    rows.push(vec![
        anchor_label.to_owned(),
        "0".to_owned(),
        "exact".to_owned(),
        "anchor".to_owned(),
    ]);
    for affected in &result.affected_nodes {
        rows.push(vec![
            affected.logical_id.clone(),
            affected.hops.to_string(),
            confidence_label(affected.path_confidence).to_owned(),
            "affected".to_owned(),
        ]);
    }
    let table_md = plsql_render::markdown::table(&headers, &rows);

    let body = format!(
        "<h1>Impact subgraph</h1>\n\
         <p>Anchor: <code>{anchor}</code>. \
         Edges: {n_edges}. Affected nodes: {n_affected}. \
         Unknown edges: {n_unknown}.</p>\n\
         <section aria-label=\"impact-graph\">{svg}</section>\n\
         <section aria-label=\"affected-table\"><pre>{table}</pre></section>\n",
        anchor = escape_html(anchor_label),
        n_edges = result.edges.len(),
        n_affected = result.affected_nodes.len(),
        n_unknown = result.unknown_edges.len(),
        svg = svg,
        table = escape_html(&table_md),
    );

    plsql_render::html::shell("PL/SQL Impact Report", body)
}

/// Render an HTML impact report **augmented with the catalog
/// cross-check** ("Oracle sees / engine sees / uncertain" framing
/// from plan §1.5). Wraps `impact_to_html` and splices in a
/// three-column comparison table at the bottom.
///
/// Uses the same SVG / summary as `impact_to_html` plus a
/// `<section aria-label="oracle-vs-engine">` listing the
/// `CompareOracleDepsReport` categories.
#[must_use]
pub fn impact_to_html_with_compare(
    result: &LineageResult,
    compare: &CompareOracleDepsReport,
) -> String {
    let base = impact_to_html(result);

    let mut rows: Vec<Vec<String>> = Vec::new();
    rows.push(vec![
        format!("agreements: {}", compare.agreements),
        format!("oracle deps: {}", compare.oracle_dependencies),
        format!("engine edges: {}", compare.engine_edges),
    ]);
    rows.push(vec![
        "Oracle sees (engine missed)".into(),
        compare.oracle_only.len().to_string(),
        sample_three(&compare.oracle_only),
    ]);
    rows.push(vec![
        "Engine sees (Oracle does not)".into(),
        compare.engine_only.len().to_string(),
        sample_three(&compare.engine_only),
    ]);
    rows.push(vec![
        "Kind mismatch (both record, disagree)".into(),
        compare.kind_mismatches.len().to_string(),
        compare
            .kind_mismatches
            .iter()
            .take(3)
            .map(|m| {
                format!(
                    "{} -> {} ({}/{})",
                    m.from, m.to, m.engine_kind, m.oracle_kind
                )
            })
            .collect::<Vec<_>>()
            .join("; "),
    ]);
    rows.push(vec![
        "Expected gap (uncertain, not a bug)".into(),
        compare.expected_gaps.len().to_string(),
        sample_three(&compare.expected_gaps),
    ]);
    let headers = ["category", "count", "examples"];
    let compare_table = plsql_render::markdown::table(&headers, &rows);

    let insertion = format!(
        "\n  <section aria-label=\"oracle-vs-engine\">\n\
         <h2>Oracle sees / engine sees / uncertain</h2>\n\
         <pre>{table}</pre>\n\
         </section>\n",
        table = escape_html(&compare_table),
    );

    // Splice the new section in just before the closing </main>.
    if let Some(close_idx) = base.rfind("  </main>") {
        let mut out = String::with_capacity(base.len() + insertion.len());
        out.push_str(&base[..close_idx]);
        out.push_str(&insertion);
        out.push_str(&base[close_idx..]);
        out
    } else {
        base + &insertion
    }
}

fn sample_three(edges: &[CompareEdge]) -> String {
    edges
        .iter()
        .take(3)
        .map(|e| format!("{} -> {} ({})", e.from, e.to, e.kind))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Wrap an impact HTML report payload in a versioned envelope.
///
/// Schema: [`LINEAGE_HTML_SCHEMA`] (`plsql.lineage.html_report` v1.0.0).
/// The payload's `html` field is a self-contained HTML5 document with
/// embedded SVG and Markdown-style summary table — safe to serve
/// directly or to splice into a larger page wrapper.
#[must_use]
pub fn impact_to_html_envelope(result: &LineageResult) -> RobotJsonEnvelope<LineageHtmlDocument> {
    RobotJsonEnvelope::new(
        LINEAGE_HTML_SCHEMA,
        LineageHtmlDocument {
            html: impact_to_html(result),
        },
    )
}

// ---------------------------------------------------------------------------
// compare-oracle-deps — customer-facing report (PLSQL-LIN-016).
//
// Thin lineage-side wrapper over `DepGraph::cross_check_with_catalog`.
// The depgraph crate produces the raw classification; this surface
// renames the report fields into the customer-facing vocabulary plan
// §1.5 calls for ("Oracle sees / engine sees / uncertain") and
// stabilizes the JSON shape under a lineage-side schema so the
// `lineage compare-oracle-deps` CLI subcommand can pin against it.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompareOracleDepsReport {
    /// Number of (from, to) dependencies recorded by Oracle.
    pub oracle_dependencies: usize,
    /// Number of edges in our depgraph.
    pub engine_edges: usize,
    /// Dependency pairs where both sides agree.
    pub agreements: usize,
    /// Pairs Oracle records that the engine missed.
    pub oracle_only: Vec<CompareEdge>,
    /// Edges the engine has that Oracle does not record.
    pub engine_only: Vec<CompareEdge>,
    /// Pairs both record but with disagreeing kinds.
    pub kind_mismatches: Vec<KindMismatchEntry>,
    /// Edges the engine emits that ALL_DEPENDENCIES doesn't track by
    /// design (OpaqueDynamic, DbLink, Constrains, TriggersOn).
    pub expected_gaps: Vec<CompareEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompareEdge {
    pub from: String,
    pub to: String,
    /// Edge kind label (depgraph EdgeKind for engine side, Oracle
    /// dependency_kind for the Oracle-only and expected-gap rows).
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindMismatchEntry {
    pub from: String,
    pub to: String,
    pub engine_kind: String,
    pub oracle_kind: String,
}

/// Build a customer-facing comparison report from a depgraph + catalog
/// snapshot pair. Wraps `DepGraph::cross_check_with_catalog` and
/// renames the report's classification vocabulary into the "Oracle sees
/// / engine sees / uncertain" framing the customer-facing surface uses.
#[must_use]
pub fn compare_oracle_deps(
    graph: &DepGraph,
    snapshot: &plsql_catalog::CatalogSnapshot,
    interner: &plsql_core::SymbolInterner,
) -> CompareOracleDepsReport {
    use plsql_depgraph::CrossCheckMismatch;
    let cross = graph.cross_check_with_catalog(snapshot, interner);
    let mut oracle_only: Vec<CompareEdge> = Vec::new();
    let mut engine_only: Vec<CompareEdge> = Vec::new();
    let mut kind_mismatches: Vec<KindMismatchEntry> = Vec::new();
    let mut expected_gaps: Vec<CompareEdge> = Vec::new();

    for m in cross.mismatches {
        match m {
            CrossCheckMismatch::OurExtra {
                from,
                to,
                edge_kind,
                ..
            } => engine_only.push(CompareEdge {
                from,
                to,
                kind: edge_kind,
            }),
            CrossCheckMismatch::OracleOnly {
                from,
                to,
                dependency_kind,
            } => oracle_only.push(CompareEdge {
                from,
                to,
                kind: dependency_kind,
            }),
            CrossCheckMismatch::KindMismatch {
                from,
                to,
                our_kind,
                oracle_kind,
            } => kind_mismatches.push(KindMismatchEntry {
                from,
                to,
                engine_kind: our_kind,
                oracle_kind,
            }),
            CrossCheckMismatch::ExpectedGap { from, to, reason } => {
                expected_gaps.push(CompareEdge {
                    from,
                    to,
                    kind: reason,
                })
            }
        }
    }

    CompareOracleDepsReport {
        oracle_dependencies: cross.summary.total_oracle_deps,
        engine_edges: cross.summary.total_our_edges,
        agreements: cross.summary.matches,
        oracle_only,
        engine_only,
        kind_mismatches,
        expected_gaps,
    }
}

/// Wrap a [`CompareOracleDepsReport`] in a versioned envelope.
///
/// Schema: [`COMPARE_ORACLE_DEPS_SCHEMA`]
/// (`plsql.lineage.compare_oracle_deps` v1.0.0). The four categorical
/// arrays (`oracle_only`, `engine_only`, `kind_mismatches`,
/// `expected_gaps`) are the customer-facing surface — release reviewers
/// inspect them when deciding whether a depgraph snapshot is safe to
/// promote against a target environment.
#[must_use]
pub fn compare_oracle_deps_envelope(
    report: CompareOracleDepsReport,
) -> RobotJsonEnvelope<CompareOracleDepsReport> {
    RobotJsonEnvelope::new(COMPARE_ORACLE_DEPS_SCHEMA, report)
}

// ---------------------------------------------------------------------------
// detect_orphans — zero-incoming-edge classifier (PLSQL-LIN-019).
// ---------------------------------------------------------------------------

pub const ORPHAN_CANDIDATES_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.orphan_candidates",
    version: SchemaVersion::new(1, 0, 0),
    description: "Orphan-candidate report derived from depgraph zero-incoming-edge query",
};

/// Compute an orphan-candidate report from a depgraph.
///
/// Tags every node with **zero incoming edges** as a potential orphan
/// using `plsql_output::OrphanCandidate`. Tier picker:
///
/// * `HighConfidenceUnused` — no incoming, no outgoing edges
/// * `LikelyUnused`         — no incoming, has outgoing
/// * `Inconclusive`         — emitted instead when
///   `assume_incomplete_augmentation` is true (catalog grants /
///   synonyms / scheduler / DB-link aren't loaded yet, so the
///   absence of inbound edges cannot prove non-use)
#[must_use]
pub fn detect_orphans(
    graph: &DepGraph,
    assume_incomplete_augmentation: bool,
) -> plsql_output::OrphanCandidatesReport {
    use plsql_output::{OrphanCandidate, OrphanCandidatesReport, OrphanConfidenceTier};

    let mut inbound: std::collections::HashSet<plsql_depgraph::NodeId> =
        std::collections::HashSet::new();
    let mut outbound: std::collections::HashSet<plsql_depgraph::NodeId> =
        std::collections::HashSet::new();
    for edge in &graph.edges {
        inbound.insert(edge.to);
        outbound.insert(edge.from);
    }

    let mut candidates: Vec<OrphanCandidate> = Vec::new();
    let mut with_references = 0usize;
    for node in graph.nodes.values() {
        if inbound.contains(&node.id) {
            with_references += 1;
            continue;
        }
        let has_outgoing = outbound.contains(&node.id);
        let tier = if assume_incomplete_augmentation {
            OrphanConfidenceTier::Inconclusive
        } else if has_outgoing {
            OrphanConfidenceTier::LikelyUnused
        } else {
            OrphanConfidenceTier::HighConfidenceUnused
        };
        let mut evidence = vec![format!(
            "no incoming edges in depgraph (identity_kind = {})",
            node.identity_kind.as_str()
        )];
        if has_outgoing {
            evidence.push("object has outgoing references but nothing points at it".to_owned());
        }
        if assume_incomplete_augmentation {
            evidence.push(
                "catalog grants / synonyms / scheduler / DB-link augmentation not yet applied"
                    .to_owned(),
            );
        }
        candidates.push(OrphanCandidate {
            object_id: node.logical_id.to_string(),
            kind: node.identity_kind.as_str().to_owned(),
            last_used: None,
            evidence,
            confidence: tier,
        });
    }

    candidates.sort_by(|a, b| a.object_id.cmp(&b.object_id));

    OrphanCandidatesReport {
        candidates,
        objects_examined: graph.nodes.len(),
        objects_with_references: with_references,
        observation_window: None,
    }
}

/// Wrap a [`plsql_output::OrphanCandidatesReport`] in a versioned envelope.
#[must_use]
pub fn detect_orphans_envelope(
    report: plsql_output::OrphanCandidatesReport,
) -> RobotJsonEnvelope<plsql_output::OrphanCandidatesReport> {
    RobotJsonEnvelope::new(ORPHAN_CANDIDATES_SCHEMA, report)
}

/// Schema for the orphan-report doctor check.
pub const ORPHAN_DOCTOR_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.lineage.orphan_doctor",
    version: SchemaVersion::new(1, 0, 0),
    description: "Doctor check over an OrphanCandidatesReport: tier counts, freshness, audit hints",
};

/// Per-tier breakdown of an orphan-candidate report plus health flags.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrphanDoctorReport {
    pub candidates_total: usize,
    pub high_confidence_unused: usize,
    pub likely_unused: usize,
    pub maybe_unused: usize,
    pub inconclusive: usize,
    /// Whether the report carries an observation-window string.
    pub observation_window_present: bool,
    /// Plan §13.8: AUDIT statements should run for AT LEAST 30 days
    /// before any drop decision. `false` when `observation_window` is
    /// missing OR parses to a duration below the minimum.
    pub observation_window_meets_minimum: bool,
    /// Plan §13.8: only HighConfidenceUnused candidates are safe to
    /// AUDIT today. Lower-tier candidates need augmentation before any
    /// AUDIT-enable recommendation is consistent.
    pub audit_recommendation_safe: bool,
    /// Free-form remediation hints the doctor surfaces to the operator.
    pub hints: Vec<String>,
}

/// Compute a doctor check over an orphan-candidates report.
///
/// Verifies:
/// * Freshness — `observation_window` populated and at least 30 days
/// * Audit-enablement consistency — only HighConfidenceUnused candidates
///   should be recommended for AUDIT today; mixing tiers is a smell
/// * Tier distribution so operators can spot reports where everything
///   degraded to Inconclusive (suggests catalog/grant/synonym data is
///   missing rather than the estate genuinely being orphan-heavy)
#[must_use]
pub fn orphan_doctor(report: &plsql_output::OrphanCandidatesReport) -> OrphanDoctorReport {
    use plsql_output::OrphanConfidenceTier::*;
    let mut out = OrphanDoctorReport {
        candidates_total: report.candidates.len(),
        observation_window_present: report.observation_window.is_some(),
        ..OrphanDoctorReport::default()
    };
    for c in &report.candidates {
        match c.confidence {
            HighConfidenceUnused => out.high_confidence_unused += 1,
            LikelyUnused => out.likely_unused += 1,
            MaybeUnused => out.maybe_unused += 1,
            Inconclusive => out.inconclusive += 1,
        }
    }

    // Freshness: parse the observation window string heuristically.
    // Accept "30d", "60d", "90d", or "Nd" / "N day(s)" formats.
    out.observation_window_meets_minimum = report
        .observation_window
        .as_deref()
        .and_then(|w| {
            let trimmed = w.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == ' ');
            trimmed.parse::<u32>().ok()
        })
        .is_some_and(|days| days >= 30);

    // Audit recommendation safety: only HighConfidenceUnused is safe
    // for an AUDIT-enable today.
    out.audit_recommendation_safe = out.high_confidence_unused > 0
        && (out.likely_unused + out.maybe_unused + out.inconclusive == 0
            || out.high_confidence_unused
                >= out.likely_unused + out.maybe_unused + out.inconclusive);

    if !out.observation_window_present {
        out.hints.push(
            "Observation window not set on the report. Per plan §13.8 every orphan candidate \
             needs an AUDIT observation window of 30/60/90 days before any drop decision."
                .to_owned(),
        );
    } else if !out.observation_window_meets_minimum {
        out.hints.push(
            "Observation window is below the 30-day floor mandated by plan §13.8. Re-run after \
             extending the window or wait until the existing AUDIT trail accumulates."
                .to_owned(),
        );
    }
    if out.inconclusive > out.high_confidence_unused && out.inconclusive > 0 {
        out.hints.push(format!(
            "Inconclusive candidates ({}) outnumber HighConfidenceUnused ({}). This often means \
             catalog grants / synonyms / scheduler / DB-link augmentation isn't loaded — \
             investigate before treating the report as actionable.",
            out.inconclusive, out.high_confidence_unused
        ));
    }
    if out.likely_unused + out.maybe_unused > 0 {
        out.hints.push(
            "Likely/Maybe-unused candidates exist. AUDIT-enable should ONLY target the \
             HighConfidenceUnused tier today; lower tiers need augmentation before any \
             DROP decision is safe."
                .to_owned(),
        );
    }
    out
}

/// Wrap an [`OrphanDoctorReport`] in a versioned envelope.
#[must_use]
pub fn orphan_doctor_envelope(report: OrphanDoctorReport) -> RobotJsonEnvelope<OrphanDoctorReport> {
    RobotJsonEnvelope::new(ORPHAN_DOCTOR_SCHEMA, report)
}

// ---------------------------------------------------------------------------
// Orphan candidates report renderers — PLSQL-LIN-021.
//
// Per plan §1.5 + §13.8: every report MUST partition by confidence
// tier, MUST carry a Trust Block (completeness + low-confidence
// inventory), and MUST emit AUDIT statements rather than DROP scripts
// (the safety-first remediation pattern).
// ---------------------------------------------------------------------------

/// Render an [`plsql_output::OrphanCandidatesReport`] as Markdown with
/// the mandatory tier partitioning + Trust Block + AUDIT remediation
/// block. Each tier becomes its own H2 section; the Trust Block lives
/// at the top; AUDIT statements at the bottom.
#[must_use]
pub fn orphans_to_markdown(report: &plsql_output::OrphanCandidatesReport) -> String {
    let mut out = String::new();
    out.push_str("# Orphan Candidates Report\n\n");

    // Trust Block.
    out.push_str("## Trust Block\n\n");
    out.push_str(&format!(
        "- Objects examined: **{}**\n",
        report.objects_examined
    ));
    out.push_str(&format!(
        "- Objects with at least one inbound reference: **{}**\n",
        report.objects_with_references
    ));
    out.push_str(&format!(
        "- Orphan candidates surfaced: **{}**\n",
        report.candidates.len()
    ));
    if let Some(window) = &report.observation_window {
        out.push_str(&format!("- Observation window: **{window}**\n"));
    }
    out.push('\n');

    // Tier sections.
    use plsql_output::OrphanConfidenceTier;
    let tiers = [
        (
            "High confidence (unused)",
            OrphanConfidenceTier::HighConfidenceUnused,
        ),
        ("Likely unused", OrphanConfidenceTier::LikelyUnused),
        ("Maybe unused", OrphanConfidenceTier::MaybeUnused),
        ("Inconclusive", OrphanConfidenceTier::Inconclusive),
    ];
    for (label, tier) in tiers {
        let bucket: Vec<&plsql_output::OrphanCandidate> = report
            .candidates
            .iter()
            .filter(|c| std::mem::discriminant(&c.confidence) == std::mem::discriminant(&tier))
            .collect();
        out.push_str(&format!("## {label} ({})\n\n", bucket.len()));
        if bucket.is_empty() {
            out.push_str("_No candidates in this tier._\n\n");
            continue;
        }
        for c in bucket {
            out.push_str(&format!("- `{}` (`{}`)\n", c.object_id, c.kind));
            for ev in &c.evidence {
                out.push_str(&format!("  - {ev}\n"));
            }
        }
        out.push('\n');
    }

    // AUDIT block — never emit DROP. Per plan §13.8 use AUDIT-based
    // observation windows so customers can verify before any destructive
    // action.
    out.push_str("## AUDIT statements (observation, not deletion)\n\n");
    out.push_str("> Apply these AUDIT statements to confirm non-use over the configured observation window (30/60/90 days). **No DROP statements are emitted** — that decision belongs to a human reviewing AUDIT findings.\n\n");
    out.push_str("```sql\n");
    for c in &report.candidates {
        out.push_str(&format!(
            "AUDIT ALL ON {} BY ACCESS;  -- {} candidate\n",
            c.object_id,
            confidence_tier_label(c.confidence)
        ));
    }
    out.push_str("```\n");
    out
}

/// Render an [`plsql_output::OrphanCandidatesReport`] as a self-contained
/// HTML document by wrapping the Markdown rendering in
/// `plsql_render::html::shell`.
#[must_use]
pub fn orphans_to_html(report: &plsql_output::OrphanCandidatesReport) -> String {
    let md = orphans_to_markdown(report);
    let body = format!(
        "<pre data-plsql-render=\"orphan-candidates-md\">{}</pre>\n",
        escape_html(&md)
    );
    plsql_render::html::shell("PL/SQL Orphan Candidates Report", body)
}

fn confidence_tier_label(c: plsql_output::OrphanConfidenceTier) -> &'static str {
    use plsql_output::OrphanConfidenceTier::*;
    match c {
        HighConfidenceUnused => "high-confidence-unused",
        LikelyUnused => "likely-unused",
        MaybeUnused => "maybe-unused",
        Inconclusive => "inconclusive",
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn confidence_label(c: Confidence) -> &'static str {
    match c {
        Confidence::Exact => "exact",
        Confidence::Heuristic => "heuristic",
        Confidence::Unknown => "unknown",
    }
}

fn push_xml_data(buf: &mut String, key: &str, value: &str) {
    buf.push_str("      <data key=\"");
    buf.push_str(key);
    buf.push_str("\">");
    for ch in value.chars() {
        match ch {
            '&' => buf.push_str("&amp;"),
            '<' => buf.push_str("&lt;"),
            '>' => buf.push_str("&gt;"),
            '"' => buf.push_str("&quot;"),
            '\'' => buf.push_str("&apos;"),
            _ => buf.push(ch),
        }
    }
    buf.push_str("</data>\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_roundtrip_json() {
        let res = LineageResult {
            query: Some(LineageQuery {
                anchor: "hr.customers.legacy_segment".into(),
                direction: LineageDirection::Downstream,
                max_depth: Some(5),
                min_confidence: Some(Confidence::Heuristic),
            }),
            edges: vec![LineageEdge {
                source: "hr.customers.legacy_segment".into(),
                target: "hr.report_pkg.list_customers".into(),
                kind: "reads".into(),
                confidence: Confidence::Exact,
            }],
            unknown_edges: vec![UnknownEdge {
                source: "hr.report_pkg.dyn_call".into(),
                unknown_reason: "DynamicSqlOpaque".into(),
                detail: Some("EXECUTE IMMEDIATE with unbound variable".into()),
            }],
            affected_nodes: vec![],
        };
        let json = serde_json::to_string(&res).unwrap();
        let back: LineageResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.edges.len(), 1);
        assert_eq!(back.unknown_edges[0].unknown_reason, "DynamicSqlOpaque");
    }

    #[test]
    fn confidence_serializes_lowercase() {
        let json = serde_json::to_string(&Confidence::Exact).unwrap();
        assert_eq!(json, "\"exact\"");
    }

    #[test]
    fn changeset_roundtrip_json() {
        let mut cs = SemanticChangeSet::new();
        cs.old_run_id = Some("run-001".into());
        cs.new_run_id = Some("run-002".into());

        cs.push(ChangeRecord::Created {
            object_id: "billing.new_pkg".into(),
        });
        cs.push(ChangeRecord::Signature(SignatureChange {
            object_id: "billing.billing_api.process_payment".into(),
            old_signature: Some("process_payment(p_id NUMBER)".into()),
            new_signature: Some("process_payment(p_id NUMBER, p_amount NUMBER)".into()),
        }));
        cs.push(ChangeRecord::Body(BodyChange {
            object_id: "billing.billing_api".into(),
            hash_before: Some("sha256:aaa".into()),
            hash_after: Some("sha256:bbb".into()),
        }));
        cs.push(ChangeRecord::Grant(GrantChange {
            object_id: "billing.invoices".into(),
            grantee: "app_writer".into(),
            privilege: "INSERT".into(),
            action: GrantAction::Granted,
        }));
        cs.push(ChangeRecord::Column(ColumnChange {
            object_id: "billing.customers".into(),
            column_name: "LEGACY_SEGMENT".into(),
            change: ColumnChangeDetail::Dropped,
        }));
        cs.push(ChangeRecord::Synonym(SynonymChange {
            synonym_id: "billing.syn_customers".into(),
            target_before: Some("old_schema.customers".into()),
            target_after: None,
        }));
        cs.push(ChangeRecord::Type(TypeChange {
            type_id: "billing.route_t".into(),
            detail: TypeChangeDetail::AttributeAdded {
                name: "carrier_id".into(),
            },
        }));
        cs.push(ChangeRecord::Dropped {
            object_id: "billing.old_table".into(),
        });
        cs.push(ChangeRecord::Privilege(PrivilegeChange {
            object_id: "billing.invoices".into(),
            grantee: "public".into(),
            privilege: "SELECT".into(),
            action: GrantAction::Revoked,
        }));
        cs.push(ChangeRecord::Ddl(DdlChange {
            object_id: "billing.ix_invoices_customer".into(),
            object_type: "INDEX".into(),
            detail: "dropped".into(),
        }));

        // Roundtrip
        let json = serde_json::to_string_pretty(&cs).unwrap();
        let back: SemanticChangeSet = serde_json::from_str(&json).unwrap();

        assert_eq!(back.old_run_id, Some("run-001".into()));
        assert_eq!(back.new_run_id, Some("run-002".into()));
        assert_eq!(back.changes.len(), 10);
        assert_eq!(back.count_by_kind(ChangeKind::Created), 1);
        assert_eq!(back.count_by_kind(ChangeKind::Signature), 1);
        assert_eq!(back.count_by_kind(ChangeKind::Body), 1);
        assert_eq!(back.count_by_kind(ChangeKind::Grant), 1);
        assert_eq!(back.count_by_kind(ChangeKind::Column), 1);
        assert_eq!(back.count_by_kind(ChangeKind::Synonym), 1);
        assert_eq!(back.count_by_kind(ChangeKind::Type), 1);
        assert_eq!(back.count_by_kind(ChangeKind::Dropped), 1);
        assert_eq!(back.count_by_kind(ChangeKind::Privilege), 1);
        assert_eq!(back.count_by_kind(ChangeKind::Ddl), 1);
    }

    #[test]
    fn changeset_tagged_serde_envelope() {
        let rec = ChangeRecord::Signature(SignatureChange {
            object_id: "hr.pkg.proc".into(),
            old_signature: None,
            new_signature: Some("proc(p_id NUMBER)".into()),
        });
        let json = serde_json::to_string(&rec).unwrap();
        assert!(json.contains("\"kind\":\"signature\""));
        let back: ChangeRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rec);
    }

    #[test]
    fn changeset_empty_is_empty() {
        let cs = SemanticChangeSet::new();
        assert!(cs.is_empty());
        assert_eq!(cs.count_by_kind(ChangeKind::Created), 0);
    }

    // --- dependencies() tests ---

    use plsql_core::{
        Confidence as DgConfidence, ConfidenceLevel as DgConfidenceLevel, FileId, ObjectName,
        Position, Span, SymbolId,
    };
    use plsql_depgraph::{
        DepGraph as Dg, Edge as DgEdge, EdgeId as DgEdgeId, EdgeKind as DgEdgeKind,
        LogicalObjectId, Node as DgNode, NodeId as DgNodeId, NodeIdentityKind, ObjectRevisionId,
        Provenance as DgProvenance, QualifiedName, ResolutionStrategy,
    };

    /// Build a small graph: A <- B <- C (B depends on A, C depends on B).
    /// Also C <- D (D depends on C).
    fn dependency_fixture() -> Dg {
        let mut g = Dg::new();
        let make_node = |id: u64, lid: &str| -> DgNode {
            DgNode::new(
                DgNodeId::new(id),
                LogicalObjectId::new(lid),
                ObjectRevisionId::new(format!("rev:{id}")),
                QualifiedName::new(None, ObjectName::from(SymbolId::new(id + 100))),
                NodeIdentityKind::Table,
            )
        };

        g.insert_node(make_node(1, "schema.table_a"));
        g.insert_node(make_node(2, "schema.table_b"));
        g.insert_node(make_node(3, "schema.proc_c"));
        g.insert_node(make_node(4, "schema.func_d"));

        let prov = || -> DgProvenance {
            DgProvenance::new(
                FileId::new(1),
                Span::new(
                    FileId::new(1),
                    Position::new(1, 1, 0),
                    Position::new(1, 5, 4),
                ),
                ResolutionStrategy::CatalogLookup,
            )
        };

        // B depends on A (edge: A -> B)
        g.insert_edge(
            DgEdge::new(
                DgEdgeId::new(1),
                DgNodeId::new(1),
                DgNodeId::new(2),
                DgEdgeKind::Reads,
                DgConfidence::new(DgConfidenceLevel::High, None),
            ),
            prov(),
            None,
        );
        // C depends on B (edge: B -> C)
        g.insert_edge(
            DgEdge::new(
                DgEdgeId::new(2),
                DgNodeId::new(2),
                DgNodeId::new(3),
                DgEdgeKind::Calls,
                DgConfidence::new(DgConfidenceLevel::High, None),
            ),
            prov(),
            None,
        );
        // D depends on C (edge: C -> D)
        g.insert_edge(
            DgEdge::new(
                DgEdgeId::new(3),
                DgNodeId::new(3),
                DgNodeId::new(4),
                DgEdgeKind::Reads,
                DgConfidence::new(DgConfidenceLevel::Medium, None),
            ),
            prov(),
            None,
        );
        g
    }

    #[test]
    fn dependencies_returns_direct_upstream() {
        let graph = dependency_fixture();
        // What does table_b (node 2) depend on? -> table_a (node 1)
        let result = super::dependencies(&graph, &DgNodeId::new(2), Some(1));
        assert_eq!(result.edges.len(), 1);
        assert_eq!(result.edges[0].source, "schema.table_a");
        assert_eq!(result.edges[0].target, "schema.table_b");
        assert_eq!(result.edges[0].kind, "Reads");
    }

    #[test]
    fn dependencies_walks_transitively() {
        let graph = dependency_fixture();
        // What does func_d (node 4) depend on, 3 hops?
        // Direct: proc_c (node 3). Transitive: table_b (node 2), table_a (node 1).
        let result = super::dependencies(&graph, &DgNodeId::new(4), Some(3));
        // Should find 3 edges: C->D, B->C, A->B
        assert_eq!(result.edges.len(), 3);
        let sources: Vec<&str> = result.edges.iter().map(|e| e.source.as_str()).collect();
        assert!(sources.contains(&"schema.proc_c"));
        assert!(sources.contains(&"schema.table_b"));
        assert!(sources.contains(&"schema.table_a"));
    }

    #[test]
    fn dependencies_respects_max_depth() {
        let graph = dependency_fixture();
        // depth=1 from node 4: only C
        let d1 = super::dependencies(&graph, &DgNodeId::new(4), Some(1));
        assert_eq!(d1.edges.len(), 1);
        assert_eq!(d1.edges[0].source, "schema.proc_c");

        // depth=2 from node 4: C and B
        let d2 = super::dependencies(&graph, &DgNodeId::new(4), Some(2));
        assert_eq!(d2.edges.len(), 2);
    }

    #[test]
    fn dependencies_leaf_node_has_no_upstream() {
        let graph = dependency_fixture();
        // table_a (node 1) has nothing depending on it upstream
        let result = super::dependencies(&graph, &DgNodeId::new(1), Some(5));
        assert!(result.edges.is_empty());
    }

    #[test]
    fn dependencies_maps_confidence_tiers() {
        let graph = dependency_fixture();
        // func_d -> proc_c edge has Medium confidence -> Heuristic
        let result = super::dependencies(&graph, &DgNodeId::new(4), Some(1));
        assert_eq!(result.edges[0].confidence, Confidence::Heuristic);

        // proc_c -> table_b edge has High confidence -> Exact
        let result2 = super::dependencies(&graph, &DgNodeId::new(3), Some(1));
        assert_eq!(result2.edges[0].confidence, Confidence::Exact);
    }

    #[test]
    fn dependencies_unknown_node_returns_empty() {
        let graph = dependency_fixture();
        let result = super::dependencies(&graph, &DgNodeId::new(999), Some(5));
        assert!(result.edges.is_empty());
    }

    #[test]
    fn dependencies_sets_query_metadata() {
        let graph = dependency_fixture();
        let result = super::dependencies(&graph, &DgNodeId::new(2), Some(1));
        let q = result.query.as_ref().unwrap();
        assert_eq!(q.direction, LineageDirection::Upstream);
        assert_eq!(q.anchor, "schema.table_b");
        assert_eq!(q.max_depth, Some(1));
    }

    #[test]
    fn dependencies_unbounded_traversal() {
        let graph = dependency_fixture();
        // None = unbounded, should traverse entire upstream graph from node 4
        let result = super::dependencies(&graph, &DgNodeId::new(4), None);
        assert_eq!(result.edges.len(), 3);
    }

    // --- classify_dir_diff tests ---

    use std::fs;

    #[test]
    fn classify_dir_diff_created_dropped_modified() {
        let tmp = tempfile::tempdir().unwrap();
        let before = tmp.path().join("before");
        let after = tmp.path().join("after");

        // before: pkg.sql, old.sql
        fs::create_dir_all(&before).unwrap();
        fs::write(before.join("pkg.sql"), "CREATE PACKAGE pkg AS END;").unwrap();
        fs::write(before.join("old.sql"), "CREATE TABLE old (id NUMBER);").unwrap();

        // after: pkg.sql (modified), new.sql, old.sql removed
        fs::create_dir_all(&after).unwrap();
        fs::write(
            after.join("pkg.sql"),
            "CREATE PACKAGE pkg AS
  PROCEDURE p;
END;",
        )
        .unwrap();
        fs::write(after.join("new.sql"), "CREATE TABLE new_tab (id NUMBER);").unwrap();

        let cs = super::classify_dir_diff(&before, &after).unwrap();

        let created: Vec<_> = cs
            .changes
            .iter()
            .filter(|r| matches!(r, super::ChangeRecord::Created { .. }))
            .collect();
        let dropped: Vec<_> = cs
            .changes
            .iter()
            .filter(|r| matches!(r, super::ChangeRecord::Dropped { .. }))
            .collect();
        let body: Vec<_> = cs
            .changes
            .iter()
            .filter(|r| matches!(r, super::ChangeRecord::Body(_)))
            .collect();

        assert_eq!(created.len(), 1, "expected 1 created (new_tab)");
        assert_eq!(dropped.len(), 1, "expected 1 dropped (old)");
        assert_eq!(body.len(), 1, "expected 1 body change (pkg)");
    }

    #[test]
    fn classify_dir_diff_identical_dirs_produce_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("same");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.sql"), "SELECT 1 FROM dual;").unwrap();

        let cs = super::classify_dir_diff(&dir, &dir).unwrap();
        assert!(cs.is_empty(), "identical dirs should produce no changes");
    }

    #[test]
    fn classify_dir_diff_empty_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let before = tmp.path().join("empty_before");
        let after = tmp.path().join("empty_after");
        fs::create_dir_all(&before).unwrap();
        fs::create_dir_all(&after).unwrap();

        let cs = super::classify_dir_diff(&before, &after).unwrap();
        assert!(cs.is_empty());
    }

    #[test]
    fn classify_dir_diff_non_sql_files_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let before = tmp.path().join("b");
        let after = tmp.path().join("a");
        fs::create_dir_all(&before).unwrap();
        fs::create_dir_all(&after).unwrap();
        // Only .txt files — should be ignored
        fs::write(before.join("readme.txt"), "hello").unwrap();
        fs::write(after.join("readme.txt"), "world").unwrap();

        let cs = super::classify_dir_diff(&before, &after).unwrap();
        assert!(cs.is_empty(), "non-SQL files should be ignored");
    }
}

#[cfg(test)]
mod impact_tests {
    use super::*;
    use plsql_core::{ConfidenceLevel, FileId, Position, Span, SymbolId};
    use plsql_depgraph::{
        Edge, EdgeId, EdgeKind, LogicalObjectId, Node, NodeIdentityKind, ObjectRevisionId,
        Provenance, QualifiedName, ResolutionStrategy,
    };

    fn span() -> Span {
        Span::new(
            FileId::new(1),
            Position::new(1, 1, 0),
            Position::new(1, 1, 0),
        )
    }

    fn provenance() -> Provenance {
        Provenance::new(FileId::new(1), span(), ResolutionStrategy::CatalogLookup)
    }

    fn node(id: u64, logical: &str) -> Node {
        Node::new(
            NodeId::new(id),
            LogicalObjectId::new(logical),
            ObjectRevisionId::new(format!("sha256:{logical}")),
            QualifiedName::new(None, plsql_core::ObjectName::from(SymbolId::new(id))),
            NodeIdentityKind::Table,
        )
    }

    fn edge(id: u64, from: u64, to: u64, level: ConfidenceLevel) -> Edge {
        Edge::new(
            EdgeId::new(id),
            NodeId::new(from),
            NodeId::new(to),
            EdgeKind::Reads,
            plsql_core::Confidence::new(
                level,
                match level {
                    ConfidenceLevel::Medium => Some("inferred via catalog heuristic".into()),
                    ConfidenceLevel::Low | ConfidenceLevel::Opaque => {
                        Some("DynamicSqlOpaque".into())
                    }
                    _ => None,
                },
            ),
        )
    }

    /// Linear chain: anchor → A → B → C, all High-confidence.
    fn linear_chain_graph() -> DepGraph {
        let mut g = DepGraph::new();
        g.insert_node(node(1, "billing.customers"));
        g.insert_node(node(2, "billing.report_pkg"));
        g.insert_node(node(3, "billing.report_view"));
        g.insert_node(node(4, "billing.summary_job"));
        g.insert_edge(edge(1, 1, 2, ConfidenceLevel::High), provenance(), None);
        g.insert_edge(edge(2, 2, 3, ConfidenceLevel::High), provenance(), None);
        g.insert_edge(edge(3, 3, 4, ConfidenceLevel::High), provenance(), None);
        g
    }

    /// Branching graph with mixed confidences:
    ///   anchor → A (High) → B (High)
    ///   anchor → C (Medium) → B (High)
    ///   anchor → D (Opaque)
    fn branching_graph() -> DepGraph {
        let mut g = DepGraph::new();
        g.insert_node(node(1, "billing.customers"));
        g.insert_node(node(2, "billing.exact_dep"));
        g.insert_node(node(3, "billing.merge_target"));
        g.insert_node(node(4, "billing.heuristic_dep"));
        g.insert_node(node(5, "billing.opaque_dep"));
        g.insert_edge(edge(1, 1, 2, ConfidenceLevel::High), provenance(), None);
        g.insert_edge(edge(2, 2, 3, ConfidenceLevel::High), provenance(), None);
        g.insert_edge(edge(3, 1, 4, ConfidenceLevel::Medium), provenance(), None);
        g.insert_edge(edge(4, 4, 3, ConfidenceLevel::High), provenance(), None);
        g.insert_edge(edge(5, 1, 5, ConfidenceLevel::Opaque), provenance(), None);
        g
    }

    #[test]
    fn impact_walks_downstream_chain() {
        let g = linear_chain_graph();
        let res = impact(&g, &NodeId::new(1), None);
        assert_eq!(res.edges.len(), 3);
        assert_eq!(res.unknown_edges.len(), 0);

        let reached: Vec<&str> = res
            .affected_nodes
            .iter()
            .map(|n| n.logical_id.as_str())
            .collect();
        assert!(reached.contains(&"billing.report_pkg"));
        assert!(reached.contains(&"billing.report_view"));
        assert!(reached.contains(&"billing.summary_job"));
        assert!(
            res.affected_nodes
                .iter()
                .all(|n| matches!(n.path_confidence, Confidence::Exact))
        );

        let query = res.query.expect("query echoed");
        assert!(matches!(query.direction, LineageDirection::Downstream));
    }

    #[test]
    fn impact_respects_max_depth() {
        let g = linear_chain_graph();
        let res = impact(&g, &NodeId::new(1), Some(1));
        // depth=1 means we expand the anchor once, picking up the immediate hop.
        assert_eq!(res.edges.len(), 1);
        let reached: Vec<&str> = res
            .affected_nodes
            .iter()
            .map(|n| n.logical_id.as_str())
            .collect();
        assert_eq!(reached, vec!["billing.report_pkg"]);
    }

    #[test]
    fn impact_aggregates_path_confidence_by_max_of_min() {
        let g = branching_graph();
        let res = impact(&g, &NodeId::new(1), None);

        let merge_target = res
            .affected_nodes
            .iter()
            .find(|n| n.logical_id == "billing.merge_target")
            .expect("merge target reached");
        // Two paths reach merge_target:
        //   1 → 2 (High) → 3 (High)            => path conf = Exact
        //   1 → 4 (Medium) → 3 (High)          => path conf = Heuristic
        // Best path wins.
        assert!(matches!(merge_target.path_confidence, Confidence::Exact));

        let heuristic_dep = res
            .affected_nodes
            .iter()
            .find(|n| n.logical_id == "billing.heuristic_dep")
            .expect("heuristic dep reached");
        assert!(matches!(
            heuristic_dep.path_confidence,
            Confidence::Heuristic
        ));

        let opaque = res
            .affected_nodes
            .iter()
            .find(|n| n.logical_id == "billing.opaque_dep")
            .expect("opaque dep reached");
        assert!(matches!(opaque.path_confidence, Confidence::Unknown));
    }

    #[test]
    fn impact_records_opaque_edges_as_unknown_edges() {
        let g = branching_graph();
        let res = impact(&g, &NodeId::new(1), None);
        assert_eq!(res.unknown_edges.len(), 1);
        let u = &res.unknown_edges[0];
        assert_eq!(u.source, "billing.customers");
        assert_eq!(u.unknown_reason, "DynamicSqlOpaque");
    }

    #[test]
    fn impact_does_not_loop_on_cycles() {
        let mut g = DepGraph::new();
        g.insert_node(node(1, "a"));
        g.insert_node(node(2, "b"));
        g.insert_edge(edge(1, 1, 2, ConfidenceLevel::High), provenance(), None);
        g.insert_edge(edge(2, 2, 1, ConfidenceLevel::High), provenance(), None);
        let res = impact(&g, &NodeId::new(1), None);
        // Both edges visited, but no infinite loop. Anchor is not in affected_nodes.
        assert_eq!(res.edges.len(), 2);
        let reached: Vec<&str> = res
            .affected_nodes
            .iter()
            .map(|n| n.logical_id.as_str())
            .collect();
        assert_eq!(reached, vec!["b"]);
    }

    #[test]
    fn impact_returns_empty_for_unknown_anchor() {
        let g = linear_chain_graph();
        let res = impact(&g, &NodeId::new(999), None);
        assert!(res.edges.is_empty());
        assert!(res.affected_nodes.is_empty());
        assert!(res.query.is_none());
    }

    #[test]
    fn impact_result_serializes_with_affected_nodes() {
        let g = linear_chain_graph();
        let res = impact(&g, &NodeId::new(1), None);
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("affected_nodes"));
        let back: LineageResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.affected_nodes.len(), 3);
    }

    #[test]
    fn confidence_min_and_max_match_rank() {
        assert_eq!(
            Confidence::Exact.min(Confidence::Heuristic),
            Confidence::Heuristic
        );
        assert_eq!(
            Confidence::Heuristic.min(Confidence::Unknown),
            Confidence::Unknown
        );
        assert_eq!(
            Confidence::Exact.max(Confidence::Heuristic),
            Confidence::Exact
        );
        assert_eq!(
            Confidence::Unknown.max(Confidence::Heuristic),
            Confidence::Heuristic
        );
    }

    #[test]
    fn impact_envelope_round_trips_and_carries_schema_id() {
        let g = linear_chain_graph();
        let res = impact(&g, &NodeId::new(1), None);
        let envelope = impact_envelope(res);
        assert!(envelope.matches_schema(IMPACT_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.impact"));
        assert!(json.contains("affected_nodes"));
        let back: RobotJsonEnvelope<LineageResult> = serde_json::from_str(&json).unwrap();
        assert!(back.matches_schema(IMPACT_SCHEMA));
        assert_eq!(back.payload.edges.len(), 3);
    }

    #[test]
    fn dependencies_envelope_has_distinct_schema_id() {
        let g = linear_chain_graph();
        let res = dependencies(&g, &NodeId::new(4), None);
        let envelope = dependencies_envelope(res);
        assert!(envelope.matches_schema(DEPENDENCIES_SCHEMA));
        assert!(!envelope.matches_schema(IMPACT_SCHEMA));
    }

    #[test]
    fn classify_change_envelope_wraps_semantic_change_set() {
        let mut cs = SemanticChangeSet::new();
        cs.push(ChangeRecord::Created {
            object_id: "billing.new_pkg".into(),
        });
        let envelope = classify_change_envelope(cs);
        assert!(envelope.matches_schema(CLASSIFY_CHANGE_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.classify_change"));
        let back: RobotJsonEnvelope<SemanticChangeSet> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload.changes.len(), 1);
    }

    #[test]
    fn parse_unified_diff_emits_body_for_modified_plsql() {
        let diff = "diff --git a/pkg/billing.pkb b/pkg/billing.pkb\n--- a/pkg/billing.pkb\n+++ b/pkg/billing.pkb\n@@ -1,3 +1,4 @@\n line1\n-old\n+new\n+extra\n";
        let cs = parse_unified_diff(diff).unwrap();
        assert_eq!(cs.changes.len(), 1, "{:?}", cs.changes);
        match &cs.changes[0] {
            ChangeRecord::Body(bc) => {
                assert_eq!(bc.object_id, "pkg.billing");
                assert_eq!(bc.hash_before.as_deref(), Some("diff:-1"));
                assert_eq!(bc.hash_after.as_deref(), Some("diff:+2"));
            }
            other => panic!("expected Body, got {other:?}"),
        }
    }

    #[test]
    fn parse_unified_diff_emits_created_for_new_file() {
        let diff = "--- /dev/null\n+++ b/pkg/new_pkg.pks\n@@ -0,0 +1,3 @@\n+CREATE PACKAGE new_pkg\n+IS\n+END;\n";
        let cs = parse_unified_diff(diff).unwrap();
        assert_eq!(cs.changes.len(), 1);
        assert!(matches!(
            &cs.changes[0],
            ChangeRecord::Created { object_id } if object_id == "pkg.new_pkg"
        ));
    }

    #[test]
    fn parse_unified_diff_emits_dropped_for_deleted_file() {
        let diff = "--- a/pkg/old_pkg.pkb\n+++ /dev/null\n@@ -1,5 +0,0 @@\n-CREATE PACKAGE BODY old_pkg\n-IS\n-...\n";
        let cs = parse_unified_diff(diff).unwrap();
        assert_eq!(cs.changes.len(), 1);
        assert!(matches!(
            &cs.changes[0],
            ChangeRecord::Dropped { object_id } if object_id == "pkg.old_pkg"
        ));
    }

    #[test]
    fn parse_unified_diff_skips_non_plsql_files() {
        let diff = "--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-old\n+new\n";
        let cs = parse_unified_diff(diff).unwrap();
        assert!(cs.changes.is_empty(), "{:?}", cs.changes);
    }

    #[test]
    fn parse_unified_diff_rename_emits_drop_plus_create() {
        let diff = "--- a/pkg/old_name.pkb\n+++ b/pkg/new_name.pkb\n@@ -1 +1 @@\n-x\n+x\n";
        let cs = parse_unified_diff(diff).unwrap();
        assert_eq!(cs.changes.len(), 2, "{:?}", cs.changes);
        assert!(
            matches!(&cs.changes[0], ChangeRecord::Dropped { object_id } if object_id == "pkg.old_name")
        );
        assert!(
            matches!(&cs.changes[1], ChangeRecord::Created { object_id } if object_id == "pkg.new_name")
        );
    }

    #[test]
    fn parse_unified_diff_handles_multi_file_diff() {
        let diff = "--- a/pkg/a.pkb\n+++ b/pkg/a.pkb\n@@\n-x\n+y\ndiff --git a/pkg/b.pks b/pkg/b.pks\n--- a/pkg/b.pks\n+++ b/pkg/b.pks\n@@\n-x\n+y\n";
        let cs = parse_unified_diff(diff).unwrap();
        assert_eq!(cs.changes.len(), 2, "{:?}", cs.changes);
        for r in &cs.changes {
            assert!(matches!(r, ChangeRecord::Body(_)));
        }
    }

    #[test]
    fn parse_unified_diff_rejects_orphan_plus_header() {
        // A `+++ b/foo` line without a preceding `--- a/foo` is malformed.
        let diff = "+++ b/pkg/x.pkb\n@@\n+abc\n";
        let err = parse_unified_diff(diff).unwrap_err();
        assert!(matches!(err, ClassifyError::MalformedDiff { .. }), "{err}");
    }

    #[test]
    fn parse_change_file_reads_from_disk() {
        let dir =
            std::env::temp_dir().join(format!("lineage-parse-change-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("change.diff");
        std::fs::write(&p, "--- /dev/null\n+++ b/pkg/x.pks\n@@\n+y\n").unwrap();
        let cs = parse_change_file(&p).unwrap();
        assert!(matches!(
            &cs.changes[0],
            ChangeRecord::Created { object_id } if object_id == "pkg.x"
        ));
    }

    /// LAB-002 — drives `parse_change_file` against the L1 hero
    /// scenario at `corpus/lab/hero_diff/change.diff` and asserts
    /// the two Body changes that the parser layer can detect
    /// (deeper semantic classification — Signature changes — lands
    /// in the IR / SYM layer beyond LIN-007's scope).
    #[test]
    fn lab_hero_diff_parses_into_body_changes_for_employee_mgmt() {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let path = std::path::Path::new(&manifest)
            .join("..")
            .join("..")
            .join("corpus")
            .join("lab")
            .join("hero_diff")
            .join("change.diff");
        let cs = parse_change_file(&path).expect("hero diff must parse");
        let bodies: Vec<&BodyChange> = cs
            .changes
            .iter()
            .filter_map(|r| match r {
                ChangeRecord::Body(b) => Some(b),
                _ => None,
            })
            .collect();
        assert!(
            bodies
                .iter()
                .any(|b| b.object_id.ends_with("pkg_employee_mgmt")),
            "hero diff should produce a Body change for pkg_employee_mgmt; got {:?}",
            cs.changes,
        );
    }

    #[test]
    fn schema_descriptors_have_unique_ids() {
        let ids: std::collections::HashSet<_> = LINEAGE_SCHEMAS.iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), LINEAGE_SCHEMAS.len());
    }

    #[test]
    fn doctor_counts_match_graph_shape() {
        let g = linear_chain_graph();
        let report = doctor(&g);
        assert_eq!(report.node_count, 4);
        assert_eq!(report.edge_count, 3);
        assert_eq!(report.edges_exact, 3);
        assert_eq!(report.edges_heuristic, 0);
        assert_eq!(report.edges_unknown, 0);
        assert!(report.unknown_reasons.is_empty());
        assert!((report.exact_ratio() - 1.0).abs() < f32::EPSILON);
        assert_eq!(report.unknown_ratio(), 0.0);
    }

    #[test]
    fn doctor_partitions_mixed_confidence_graph() {
        let g = branching_graph();
        let report = doctor(&g);
        assert_eq!(report.node_count, 5);
        assert_eq!(report.edge_count, 5);
        assert_eq!(report.edges_exact, 3);
        assert_eq!(report.edges_heuristic, 1);
        assert_eq!(report.edges_unknown, 1);
        let count = report
            .unknown_reasons
            .get("DynamicSqlOpaque")
            .copied()
            .unwrap_or_default();
        assert_eq!(count, 1);
        assert!(report.exact_ratio() > 0.0);
        assert!(report.unknown_ratio() > 0.0);
    }

    #[test]
    fn doctor_handles_empty_graph_without_division_by_zero() {
        let g = DepGraph::new();
        let report = doctor(&g);
        assert_eq!(report.edge_count, 0);
        assert_eq!(report.exact_ratio(), 0.0);
        assert_eq!(report.unknown_ratio(), 0.0);
    }

    #[test]
    fn doctor_envelope_carries_schema_id() {
        let g = branching_graph();
        let envelope = doctor_envelope(doctor(&g));
        assert!(envelope.matches_schema(DOCTOR_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.doctor"));
        let back: RobotJsonEnvelope<LineageDoctorReport> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload.edge_count, 5);
    }

    #[test]
    fn explain_edge_returns_summary_and_confidence() {
        let g = linear_chain_graph();
        let explanation = explain_edge(&g, plsql_depgraph::EdgeId::new(1)).unwrap();
        match explanation {
            LineageExplanation::Edge(e) => {
                assert!(matches!(e.confidence, Confidence::Exact));
                assert!(e.unknown_reason.is_none());
                assert!(e.remediation.is_none());
                assert!(e.summary.contains("billing.customers"));
                assert!(e.summary.contains("billing.report_pkg"));
            }
            other => panic!("expected Edge explanation, got {other:?}"),
        }
    }

    #[test]
    fn explain_edge_reports_unknown_reason_and_remediation_for_opaque_edge() {
        let g = branching_graph();
        let explanation = explain_edge(&g, plsql_depgraph::EdgeId::new(5)).unwrap();
        match explanation {
            LineageExplanation::Edge(e) => {
                assert!(matches!(e.confidence, Confidence::Unknown));
                assert_eq!(e.unknown_reason.as_deref(), Some("DynamicSqlOpaque"));
                let remediation = e.remediation.expect("remediation provided");
                assert!(remediation.contains("Bind variables"));
            }
            other => panic!("expected Edge explanation, got {other:?}"),
        }
    }

    #[test]
    fn explain_node_counts_incoming_and_outgoing() {
        let g = linear_chain_graph();
        let selector = NodeSelector::NodeId(NodeId::new(2));
        let explanation = explain_node(&g, &selector).unwrap();
        match explanation {
            LineageExplanation::Node(n) => {
                assert_eq!(n.incoming_count, 1);
                assert_eq!(n.outgoing_count, 1);
                assert!(n.summary.contains("incoming"));
            }
            other => panic!("expected Node explanation, got {other:?}"),
        }
    }

    #[test]
    fn explain_path_aggregates_confidence_and_collects_blockers() {
        let g = branching_graph();
        let from = NodeSelector::NodeId(NodeId::new(1));
        let to = NodeSelector::NodeId(NodeId::new(5));
        let explanation = explain_path(&g, &from, &to).unwrap();
        match explanation {
            LineageExplanation::Path(p) => {
                assert!(p.path.found);
                assert!(matches!(p.aggregate_confidence, Confidence::Unknown));
                assert_eq!(p.blockers, vec!["DynamicSqlOpaque"]);
                assert!(p.summary.contains("aggregate confidence"));
            }
            other => panic!("expected Path explanation, got {other:?}"),
        }
    }

    #[test]
    fn explain_envelope_carries_schema_id() {
        let g = linear_chain_graph();
        let explanation = explain_edge(&g, plsql_depgraph::EdgeId::new(1)).unwrap();
        let envelope = explain_envelope(explanation);
        assert!(envelope.matches_schema(EXPLAIN_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.explain"));
        let back: RobotJsonEnvelope<LineageExplanation> = serde_json::from_str(&json).unwrap();
        match back.payload {
            LineageExplanation::Edge(_) => {}
            other => panic!("expected Edge variant after roundtrip, got {other:?}"),
        }
    }

    #[test]
    fn explain_path_handles_missing_route_gracefully() {
        let mut g = DepGraph::new();
        g.insert_node(node(1, "isolated.a"));
        g.insert_node(node(2, "isolated.b"));
        let from = NodeSelector::NodeId(NodeId::new(1));
        let to = NodeSelector::NodeId(NodeId::new(2));
        let explanation = explain_path(&g, &from, &to).unwrap();
        match explanation {
            LineageExplanation::Path(p) => {
                assert!(!p.path.found);
                assert!(p.summary.starts_with("no path"));
                assert!(matches!(p.aggregate_confidence, Confidence::Exact));
            }
            other => panic!("expected Path explanation, got {other:?}"),
        }
    }

    #[test]
    fn recompile_order_sorts_linear_chain_callee_first() {
        let g = linear_chain_graph();
        let plan = recompile_order(
            &g,
            &[
                "billing.report_view",
                "billing.report_pkg",
                "billing.summary_job",
                "billing.customers",
            ],
        );
        // In linear_chain_graph: customers → report_pkg → report_view → summary_job
        // Recompile order: callees before callers, so:
        //   customers (callee of report_pkg) first,
        //   report_pkg next, report_view next, summary_job last.
        assert_eq!(
            plan.order,
            vec![
                "billing.customers",
                "billing.report_pkg",
                "billing.report_view",
                "billing.summary_job",
            ]
        );
        assert!(plan.cycles.is_empty());
        assert!(plan.missing.is_empty());
    }

    #[test]
    fn recompile_order_returns_missing_for_unknown_objects() {
        let g = linear_chain_graph();
        let plan = recompile_order(
            &g,
            &[
                "billing.customers",
                "billing.nonexistent",
                "billing.report_pkg",
            ],
        );
        assert_eq!(plan.missing, vec!["billing.nonexistent"]);
        assert_eq!(plan.order.len(), 2);
        assert_eq!(plan.order[0], "billing.customers");
        assert_eq!(plan.order[1], "billing.report_pkg");
    }

    #[test]
    fn recompile_order_isolates_cycles() {
        let mut g = DepGraph::new();
        g.insert_node(node(1, "billing.a"));
        g.insert_node(node(2, "billing.b"));
        g.insert_node(node(3, "billing.c"));
        // a → b → c → a (cycle), plus standalone d
        g.insert_node(node(4, "billing.d"));
        g.insert_edge(edge(1, 1, 2, ConfidenceLevel::High), provenance(), None);
        g.insert_edge(edge(2, 2, 3, ConfidenceLevel::High), provenance(), None);
        g.insert_edge(edge(3, 3, 1, ConfidenceLevel::High), provenance(), None);

        let plan = recompile_order(&g, &["billing.a", "billing.b", "billing.c", "billing.d"]);
        // Standalone d should be ordered without a cycle.
        assert!(plan.order.contains(&"billing.d".to_string()));
        // The cycle members can't be ordered.
        assert_eq!(plan.cycles.len(), 1);
        let cycle = &plan.cycles[0];
        assert_eq!(cycle.len(), 3);
        for id in ["billing.a", "billing.b", "billing.c"] {
            assert!(cycle.contains(&id.to_string()), "cycle should include {id}");
        }
    }

    #[test]
    fn recompile_order_returns_empty_for_empty_set() {
        let g = linear_chain_graph();
        let plan = recompile_order(&g, &[]);
        assert!(plan.order.is_empty());
        assert!(plan.missing.is_empty());
        assert!(plan.cycles.is_empty());
    }

    #[test]
    fn recompile_order_ignores_edges_to_objects_outside_set() {
        let g = linear_chain_graph();
        let plan = recompile_order(&g, &["billing.summary_job", "billing.report_view"]);
        // Even with two nodes that have intervening hops, only intra-set
        // edges constrain ordering. The single direct edge
        // report_view → summary_job is in-set, so report_view comes first.
        assert_eq!(
            plan.order,
            vec!["billing.report_view", "billing.summary_job"]
        );
    }

    #[test]
    fn recompile_order_envelope_carries_schema_id() {
        let g = linear_chain_graph();
        let plan = recompile_order(&g, &["billing.customers", "billing.report_pkg"]);
        let envelope = recompile_order_envelope(plan);
        assert!(envelope.matches_schema(RECOMPILE_ORDER_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.recompile_order"));
        let back: RobotJsonEnvelope<RecompilePlan> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload.order.len(), 2);
    }

    // -----------------------------------------------------------------
    // callers() + column_readers / column_writers — PLSQL-LIN-004
    // -----------------------------------------------------------------

    fn typed_node(id: u64, logical: &str, kind: NodeIdentityKind) -> Node {
        Node::new(
            NodeId::new(id),
            LogicalObjectId::new(logical),
            ObjectRevisionId::new(format!("sha256:{logical}")),
            QualifiedName::new(None, plsql_core::ObjectName::from(SymbolId::new(id))),
            kind,
        )
    }

    fn typed_edge(id: u64, from: u64, to: u64, kind: EdgeKind, level: ConfidenceLevel) -> Edge {
        Edge::new(
            EdgeId::new(id),
            NodeId::new(from),
            NodeId::new(to),
            kind,
            plsql_core::Confidence::new(
                level,
                match level {
                    ConfidenceLevel::Medium => Some("inferred via catalog heuristic".into()),
                    ConfidenceLevel::Low | ConfidenceLevel::Opaque => {
                        Some("DynamicSqlOpaque".into())
                    }
                    _ => None,
                },
            ),
        )
    }

    /// Fixture: three callers pointing at `billing.report_pkg.run`.
    ///   billing.report_job (Calls)   →  report_pkg.run
    ///   billing.api_pkg.handler (Calls) →  report_pkg.run
    ///   billing.trigger_t (TriggersOn) →  report_pkg.run   (should NOT appear)
    ///   billing.unrelated (Reads)    →  report_pkg.run   (should NOT appear)
    fn callers_fixture() -> DepGraph {
        let mut g = DepGraph::new();
        g.insert_node(typed_node(
            10,
            "billing.report_pkg.run",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_node(typed_node(
            11,
            "billing.report_job",
            NodeIdentityKind::SchedulerJob,
        ));
        g.insert_node(typed_node(
            12,
            "billing.api_pkg.handler",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_node(typed_node(
            13,
            "billing.trigger_t",
            NodeIdentityKind::Trigger,
        ));
        g.insert_node(typed_node(14, "billing.unrelated", NodeIdentityKind::Table));

        g.insert_edge(
            typed_edge(1, 11, 10, EdgeKind::Calls, ConfidenceLevel::High),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(2, 12, 10, EdgeKind::Calls, ConfidenceLevel::Medium),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(3, 13, 10, EdgeKind::TriggersOn, ConfidenceLevel::High),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(4, 14, 10, EdgeKind::Reads, ConfidenceLevel::High),
            provenance(),
            None,
        );
        g
    }

    #[test]
    fn callers_returns_only_call_edges() {
        let g = callers_fixture();
        let result = callers(&g, &NodeSelector::NodeId(NodeId::new(10)));
        assert_eq!(result.target_logical_id, "billing.report_pkg.run");
        assert_eq!(result.target_kind, "PackageProcedure");
        assert_eq!(result.callers.len(), 2);

        // sorted by caller_logical_id
        assert_eq!(
            result.callers[0].caller_logical_id,
            "billing.api_pkg.handler"
        );
        assert_eq!(result.callers[0].caller_kind, "PackageProcedure");
        assert_eq!(result.callers[0].confidence, Confidence::Heuristic);

        assert_eq!(result.callers[1].caller_logical_id, "billing.report_job");
        assert_eq!(result.callers[1].caller_kind, "SchedulerJob");
        assert_eq!(result.callers[1].confidence, Confidence::Exact);
    }

    #[test]
    fn callers_unknown_target_returns_default() {
        let g = callers_fixture();
        let result = callers(&g, &NodeSelector::NodeId(NodeId::new(999)));
        assert_eq!(result, CallersResult::default());
    }

    #[test]
    fn callers_envelope_carries_schema_id() {
        let g = callers_fixture();
        let result = callers(&g, &NodeSelector::NodeId(NodeId::new(10)));
        let envelope = callers_envelope(result);
        assert!(envelope.matches_schema(CALLERS_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.callers"));
    }

    /// Fixture: a column with mixed accessor edges:
    ///   reader_pkg.fetch   ReadsColumn         → customers.legacy_segment   (exact)
    ///   shadow_pkg.scan    ReadsUnknownColumnOfTable → customers           (parent table)
    ///   writer_pkg.bump    WritesColumn        → customers.legacy_segment   (exact)
    ///   audit_pkg.touch    WritesUnknownColumnOfTable → customers          (parent table)
    ///   deriver_pkg.calc   DerivesColumn       → customers.legacy_segment   (exact)
    ///   unrelated_pkg.x    Calls               → customers.legacy_segment   (should NOT appear)
    fn column_access_fixture() -> DepGraph {
        let mut g = DepGraph::new();
        g.insert_node(typed_node(
            20,
            "billing.customers.legacy_segment",
            NodeIdentityKind::Column,
        ));
        g.insert_node(typed_node(21, "billing.customers", NodeIdentityKind::Table));
        g.insert_node(typed_node(
            22,
            "billing.reader_pkg.fetch",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_node(typed_node(
            23,
            "billing.shadow_pkg.scan",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_node(typed_node(
            24,
            "billing.writer_pkg.bump",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_node(typed_node(
            25,
            "billing.audit_pkg.touch",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_node(typed_node(
            26,
            "billing.deriver_pkg.calc",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_node(typed_node(
            27,
            "billing.unrelated_pkg.x",
            NodeIdentityKind::PackageProcedure,
        ));

        // Reader edges target the column directly.
        g.insert_edge(
            typed_edge(1, 22, 20, EdgeKind::ReadsColumn, ConfidenceLevel::High),
            provenance(),
            None,
        );
        // ReadsUnknownColumnOfTable points at the table, NOT the column. We
        // still expect column_readers() to surface these when queried on the
        // column because the unknown-column edge implies "some column of
        // this table was read"; matching is by parent table. To keep this
        // test focused on the per-column query, we model the unknown edge
        // as if it targeted the column itself (the depgraph emitter
        // produces both forms in practice).
        g.insert_edge(
            typed_edge(
                2,
                23,
                20,
                EdgeKind::ReadsUnknownColumnOfTable,
                ConfidenceLevel::Medium,
            ),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(3, 24, 20, EdgeKind::WritesColumn, ConfidenceLevel::High),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(
                4,
                25,
                20,
                EdgeKind::WritesUnknownColumnOfTable,
                ConfidenceLevel::Medium,
            ),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(5, 26, 20, EdgeKind::DerivesColumn, ConfidenceLevel::High),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(6, 27, 20, EdgeKind::Calls, ConfidenceLevel::High),
            provenance(),
            None,
        );
        g
    }

    #[test]
    fn column_readers_picks_up_exact_and_unknown_column_edges() {
        let g = column_access_fixture();
        let result = column_readers(&g, &NodeSelector::NodeId(NodeId::new(20)));
        assert_eq!(result.column_logical_id, "billing.customers.legacy_segment");
        assert_eq!(result.access, ColumnAccessKind::Read);
        assert_eq!(result.accessors.len(), 2);

        // sorted by accessor_logical_id
        assert_eq!(
            result.accessors[0].accessor_logical_id,
            "billing.reader_pkg.fetch"
        );
        assert_eq!(result.accessors[0].edge_kind, "ReadsColumn");
        assert!(!result.accessors[0].is_unknown_column_of_table);

        assert_eq!(
            result.accessors[1].accessor_logical_id,
            "billing.shadow_pkg.scan"
        );
        assert_eq!(result.accessors[1].edge_kind, "ReadsUnknownColumnOfTable");
        assert!(result.accessors[1].is_unknown_column_of_table);
    }

    #[test]
    fn column_writers_includes_derives_column() {
        let g = column_access_fixture();
        let result = column_writers(&g, &NodeSelector::NodeId(NodeId::new(20)));
        assert_eq!(result.access, ColumnAccessKind::Write);
        assert_eq!(result.accessors.len(), 3);

        let kinds: Vec<&str> = result
            .accessors
            .iter()
            .map(|a| a.edge_kind.as_str())
            .collect();
        // Should include exact write, derived, AND the unknown-column-of-table.
        assert!(kinds.contains(&"WritesColumn"));
        assert!(kinds.contains(&"WritesUnknownColumnOfTable"));
        assert!(kinds.contains(&"DerivesColumn"));
        // Should NOT include Calls or ReadsColumn.
        assert!(!kinds.contains(&"Calls"));
        assert!(!kinds.contains(&"ReadsColumn"));

        let unknown = result
            .accessors
            .iter()
            .find(|a| a.edge_kind == "WritesUnknownColumnOfTable")
            .unwrap();
        assert!(unknown.is_unknown_column_of_table);
    }

    #[test]
    fn column_access_envelope_carries_schema_id() {
        let g = column_access_fixture();
        let envelope =
            column_access_envelope(column_readers(&g, &NodeSelector::NodeId(NodeId::new(20))));
        assert!(envelope.matches_schema(COLUMN_ACCESS_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.column_access"));
        let back: RobotJsonEnvelope<ColumnAccessResult> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload.access, ColumnAccessKind::Read);
    }

    #[test]
    fn column_readers_unknown_column_returns_empty() {
        let g = column_access_fixture();
        let result = column_readers(&g, &NodeSelector::NodeId(NodeId::new(9999)));
        assert!(result.accessors.is_empty());
        assert_eq!(result.access, ColumnAccessKind::Read);
        assert!(result.column_logical_id.is_empty());
        assert!(
            result.resolution_error.is_some(),
            "unknown column must surface resolution_error"
        );
    }

    #[test]
    fn column_writers_unknown_column_returns_empty() {
        let g = column_access_fixture();
        let result = column_writers(&g, &NodeSelector::NodeId(NodeId::new(9999)));
        assert!(result.accessors.is_empty());
        assert_eq!(result.access, ColumnAccessKind::Write);
        assert!(result.resolution_error.is_some());
    }

    #[test]
    fn column_readers_known_column_no_accessors_returns_none_error() {
        // Build a graph with a column node that has zero incoming
        // ReadsColumn edges. resolution_error must be None — the node
        // was found, just nobody reads it.
        let mut g = DepGraph::new();
        g.insert_node(typed_node(
            80,
            "billing.lonely_col",
            NodeIdentityKind::Column,
        ));
        let result = column_readers(&g, &NodeSelector::NodeId(NodeId::new(80)));
        assert_eq!(result.column_logical_id, "billing.lonely_col");
        assert!(result.accessors.is_empty());
        assert!(result.resolution_error.is_none());
    }

    #[test]
    fn unsafe_paths_unknown_anchor_returns_default() {
        let g = unsafe_paths_fixture();
        let result = unsafe_paths(
            &g,
            &NodeSelector::NodeId(NodeId::new(9999)),
            &NodeSelector::NodeId(NodeId::new(33)),
            None,
            None,
        );
        assert_eq!(result, UnsafePathsResult::default());
    }

    #[test]
    fn unsafe_paths_unknown_destination_returns_with_from_only() {
        let g = unsafe_paths_fixture();
        let result = unsafe_paths(
            &g,
            &NodeSelector::NodeId(NodeId::new(30)),
            &NodeSelector::NodeId(NodeId::new(9999)),
            None,
            None,
        );
        assert!(result.paths.is_empty());
        assert_eq!(result.from_logical_id, "billing.from_pkg");
        assert!(result.to_logical_id.is_empty());
    }

    // -----------------------------------------------------------------
    // unsafe_paths() — PLSQL-LIN-005
    // -----------------------------------------------------------------

    /// Fixture: two paths from `from` to `to`, one safe, one unsafe.
    ///   from → exact_mid → to        (all High confidence, safe)
    ///   from → dynamic_mid → to      (first edge OpaqueDynamic, unsafe)
    fn unsafe_paths_fixture() -> DepGraph {
        let mut g = DepGraph::new();
        g.insert_node(typed_node(
            30,
            "billing.from_pkg",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_node(typed_node(31, "billing.exact_mid", NodeIdentityKind::Table));
        g.insert_node(typed_node(
            32,
            "billing.dynamic_mid",
            NodeIdentityKind::Table,
        ));
        g.insert_node(typed_node(
            33,
            "billing.to_pkg",
            NodeIdentityKind::PackageProcedure,
        ));

        // Safe path: from --Calls--> exact_mid --Calls--> to
        g.insert_edge(
            typed_edge(1, 30, 31, EdgeKind::Calls, ConfidenceLevel::High),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(2, 31, 33, EdgeKind::Calls, ConfidenceLevel::High),
            provenance(),
            None,
        );
        // Unsafe path: from --OpaqueDynamic--> dynamic_mid --Calls--> to
        g.insert_edge(
            typed_edge(3, 30, 32, EdgeKind::OpaqueDynamic, ConfidenceLevel::Low),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(4, 32, 33, EdgeKind::Calls, ConfidenceLevel::High),
            provenance(),
            None,
        );
        g
    }

    #[test]
    fn unsafe_paths_returns_only_dynamic_path() {
        let g = unsafe_paths_fixture();
        let result = unsafe_paths(
            &g,
            &NodeSelector::NodeId(NodeId::new(30)),
            &NodeSelector::NodeId(NodeId::new(33)),
            None,
            None,
        );
        assert_eq!(result.from_logical_id, "billing.from_pkg");
        assert_eq!(result.to_logical_id, "billing.to_pkg");
        assert_eq!(result.paths.len(), 1);

        let p = &result.paths[0];
        assert_eq!(
            p.nodes,
            vec!["billing.from_pkg", "billing.dynamic_mid", "billing.to_pkg"]
        );
        assert_eq!(p.edges.len(), 2);
        assert_eq!(p.unsafe_edge_indices, vec![0]);
        assert_eq!(p.overall_confidence, Confidence::Unknown);
        assert!(!result.truncated);
    }

    #[test]
    fn unsafe_paths_empty_when_only_safe_paths_exist() {
        // Graph with only the safe path: from → exact_mid → to.
        let mut g = DepGraph::new();
        g.insert_node(typed_node(
            40,
            "billing.from_pkg",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_node(typed_node(41, "billing.exact_mid", NodeIdentityKind::Table));
        g.insert_node(typed_node(
            42,
            "billing.to_pkg",
            NodeIdentityKind::PackageProcedure,
        ));
        g.insert_edge(
            typed_edge(1, 40, 41, EdgeKind::Calls, ConfidenceLevel::High),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(2, 41, 42, EdgeKind::Calls, ConfidenceLevel::High),
            provenance(),
            None,
        );

        let result = unsafe_paths(
            &g,
            &NodeSelector::NodeId(NodeId::new(40)),
            &NodeSelector::NodeId(NodeId::new(42)),
            None,
            None,
        );
        assert!(result.paths.is_empty());
        assert!(!result.truncated);
    }

    #[test]
    fn unsafe_paths_respects_max_depth() {
        // Long chain: from → A → B → C → D → to. Make edge from→A
        // OpaqueDynamic; rest High. With max_depth=2, the path is too
        // long (5 edges total) and we should find zero paths.
        let mut g = DepGraph::new();
        g.insert_node(typed_node(50, "from", NodeIdentityKind::Table));
        g.insert_node(typed_node(51, "a", NodeIdentityKind::Table));
        g.insert_node(typed_node(52, "b", NodeIdentityKind::Table));
        g.insert_node(typed_node(53, "c", NodeIdentityKind::Table));
        g.insert_node(typed_node(54, "d", NodeIdentityKind::Table));
        g.insert_node(typed_node(55, "to", NodeIdentityKind::Table));

        g.insert_edge(
            typed_edge(1, 50, 51, EdgeKind::OpaqueDynamic, ConfidenceLevel::Low),
            provenance(),
            None,
        );
        for (id, from, to) in [(2u64, 51u64, 52u64), (3, 52, 53), (4, 53, 54), (5, 54, 55)] {
            g.insert_edge(
                typed_edge(id, from, to, EdgeKind::Calls, ConfidenceLevel::High),
                provenance(),
                None,
            );
        }

        let result = unsafe_paths(
            &g,
            &NodeSelector::NodeId(NodeId::new(50)),
            &NodeSelector::NodeId(NodeId::new(55)),
            Some(2),
            None,
        );
        assert!(result.paths.is_empty());

        // With max_depth=5, we DO find the path.
        let result = unsafe_paths(
            &g,
            &NodeSelector::NodeId(NodeId::new(50)),
            &NodeSelector::NodeId(NodeId::new(55)),
            Some(5),
            None,
        );
        assert_eq!(result.paths.len(), 1);
        assert_eq!(result.paths[0].unsafe_edge_indices, vec![0]);
    }

    #[test]
    fn unsafe_paths_truncates_when_max_paths_hit() {
        // Diamond fan-out: from → mid_1..3 each with OpaqueDynamic →
        // common_to. Three distinct unsafe paths. Set max_paths=2 and
        // expect truncated=true.
        let mut g = DepGraph::new();
        g.insert_node(typed_node(60, "from", NodeIdentityKind::Table));
        g.insert_node(typed_node(64, "to", NodeIdentityKind::Table));
        for (i, mid_id) in [61u64, 62, 63].iter().enumerate() {
            g.insert_node(typed_node(
                *mid_id,
                &format!("mid_{}", i + 1),
                NodeIdentityKind::Table,
            ));
            g.insert_edge(
                typed_edge(
                    (*mid_id) * 2,
                    60,
                    *mid_id,
                    EdgeKind::OpaqueDynamic,
                    ConfidenceLevel::Low,
                ),
                provenance(),
                None,
            );
            g.insert_edge(
                typed_edge(
                    (*mid_id) * 2 + 1,
                    *mid_id,
                    64,
                    EdgeKind::Calls,
                    ConfidenceLevel::High,
                ),
                provenance(),
                None,
            );
        }

        let result = unsafe_paths(
            &g,
            &NodeSelector::NodeId(NodeId::new(60)),
            &NodeSelector::NodeId(NodeId::new(64)),
            None,
            Some(2),
        );
        assert_eq!(result.paths.len(), 2);
        assert!(result.truncated);
    }

    #[test]
    fn unsafe_paths_handles_two_consecutive_unsafe_edges() {
        // PLSQL-LIN-018 regression guard: review flagged path_unsafe_idx
        // mis-pop when two adjacent unsafe edges sit on a backtrack path.
        // Build: from --OpaqueDynamic--> a --OpaqueDynamic--> to
        // PLUS  a sibling branch: a --Calls--> sink_unrelated, so the DFS
        // explores deeper before backtracking past two consecutive unsafe
        // edges. If the backtrack mis-pops, the recorded
        // `unsafe_edge_indices` for the from→a→to path will be wrong.
        let mut g = DepGraph::new();
        g.insert_node(typed_node(70, "from", NodeIdentityKind::Table));
        g.insert_node(typed_node(71, "a", NodeIdentityKind::Table));
        g.insert_node(typed_node(72, "to", NodeIdentityKind::Table));
        g.insert_node(typed_node(73, "sink_unrelated", NodeIdentityKind::Table));
        g.insert_edge(
            typed_edge(1, 70, 71, EdgeKind::OpaqueDynamic, ConfidenceLevel::Low),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(2, 71, 72, EdgeKind::OpaqueDynamic, ConfidenceLevel::Low),
            provenance(),
            None,
        );
        g.insert_edge(
            typed_edge(3, 71, 73, EdgeKind::Calls, ConfidenceLevel::High),
            provenance(),
            None,
        );

        let result = unsafe_paths(
            &g,
            &NodeSelector::NodeId(NodeId::new(70)),
            &NodeSelector::NodeId(NodeId::new(72)),
            None,
            None,
        );
        assert_eq!(result.paths.len(), 1);
        let p = &result.paths[0];
        assert_eq!(p.edges.len(), 2);
        // Both edges are unsafe.
        assert_eq!(p.unsafe_edge_indices, vec![0, 1]);
        assert_eq!(p.overall_confidence, Confidence::Unknown);
    }

    #[test]
    fn unsafe_paths_envelope_carries_schema_id() {
        let g = unsafe_paths_fixture();
        let result = unsafe_paths(
            &g,
            &NodeSelector::NodeId(NodeId::new(30)),
            &NodeSelector::NodeId(NodeId::new(33)),
            None,
            None,
        );
        let envelope = unsafe_paths_envelope(result);
        assert!(envelope.matches_schema(UNSAFE_PATHS_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.unsafe_paths"));
    }

    // -----------------------------------------------------------------
    // impact_to_graphml() — PLSQL-LIN-010
    // -----------------------------------------------------------------

    #[test]
    fn graphml_emits_anchor_affected_and_edges() {
        let g = linear_chain_graph();
        let result = impact(&g, &NodeId::new(1), None);
        let graphml = impact_to_graphml(&result);

        assert!(graphml.contains("<?xml version=\"1.0\""));
        assert!(graphml.contains("<graphml"));
        assert!(graphml.contains("edgedefault=\"directed\""));

        // All four logical ids appear as <node> entries.
        for logical in [
            "billing.customers",
            "billing.report_pkg",
            "billing.report_view",
            "billing.summary_job",
        ] {
            assert!(
                graphml.contains(&format!(">{logical}<")),
                "missing logical_id {logical} in:\n{graphml}"
            );
        }

        // Anchor role + at least one affected role surface.
        assert!(graphml.contains(">anchor<"));
        assert!(graphml.contains(">affected<"));

        // Each edge contributes one edge_kind data.
        assert!(graphml.contains(">Reads<"));

        // Document closes cleanly.
        assert!(graphml.trim_end().ends_with("</graphml>"));
    }

    #[test]
    fn graphml_emits_unknown_reason_synthetic_nodes() {
        let g = branching_graph();
        let result = impact(&g, &NodeId::new(1), None);
        let graphml = impact_to_graphml(&result);

        // The Opaque path in branching_graph produces an unknown edge
        // whose synthetic node should appear.
        assert!(
            graphml.contains("unknown::DynamicSqlOpaque"),
            "synthetic unknown-reason node missing:\n{graphml}"
        );
        assert!(graphml.contains(">unknown-reason<"));
    }

    #[test]
    fn graphml_xml_escaping_works() {
        let result = LineageResult {
            query: Some(LineageQuery {
                anchor: "schema.has<quoted>&id".into(),
                direction: LineageDirection::Downstream,
                max_depth: None,
                min_confidence: None,
            }),
            edges: vec![LineageEdge {
                source: "schema.has<quoted>&id".into(),
                target: "schema.normal".into(),
                kind: "Reads".into(),
                confidence: Confidence::Exact,
            }],
            unknown_edges: Vec::new(),
            affected_nodes: Vec::new(),
        };
        let graphml = impact_to_graphml(&result);
        assert!(graphml.contains("schema.has&lt;quoted&gt;&amp;id"));
        assert!(!graphml.contains("<quoted>"));
    }

    #[test]
    fn graphml_envelope_carries_schema_id() {
        let g = linear_chain_graph();
        let result = impact(&g, &NodeId::new(1), None);
        let envelope = impact_to_graphml_envelope(&result);
        assert!(envelope.matches_schema(LINEAGE_GRAPHML_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.graphml"));
        let back: RobotJsonEnvelope<LineageGraphMlDocument> = serde_json::from_str(&json).unwrap();
        assert!(back.payload.graphml.contains("<graphml"));
    }

    // -----------------------------------------------------------------
    // impact_to_html() — PLSQL-LIN-008
    // -----------------------------------------------------------------

    #[test]
    fn html_report_embeds_svg_and_summary() {
        let g = linear_chain_graph();
        let result = impact(&g, &NodeId::new(1), None);
        let html = impact_to_html(&result);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>PL/SQL Impact Report</title>"));
        assert!(html.contains("<svg"));
        assert!(html.contains("Anchor:"));
        assert!(html.contains("billing.customers"));
        // Markdown table mentions every reached node.
        assert!(html.contains("billing.report_pkg"));
        assert!(html.contains("billing.summary_job"));
    }

    #[test]
    fn html_report_shows_unknown_reasons() {
        let g = branching_graph();
        let result = impact(&g, &NodeId::new(1), None);
        let html = impact_to_html(&result);

        // Synthetic unknown-reason node + UnknownReason label both surface.
        assert!(html.contains("? DynamicSqlOpaque"));
        assert!(html.contains("UnknownReason"));
    }

    #[test]
    fn html_report_escapes_html_in_anchor() {
        let result = LineageResult {
            query: Some(LineageQuery {
                anchor: "schema.evil<\"a&b\">".into(),
                direction: LineageDirection::Downstream,
                max_depth: None,
                min_confidence: None,
            }),
            ..LineageResult::default()
        };
        let html = impact_to_html(&result);
        assert!(html.contains("schema.evil&lt;&quot;a&amp;b&quot;&gt;"));
        assert!(!html.contains("<\"a&b\">"));
    }

    #[test]
    fn html_envelope_carries_schema_id() {
        let g = linear_chain_graph();
        let result = impact(&g, &NodeId::new(1), None);
        let envelope = impact_to_html_envelope(&result);
        assert!(envelope.matches_schema(LINEAGE_HTML_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.html_report"));
        let back: RobotJsonEnvelope<LineageHtmlDocument> = serde_json::from_str(&json).unwrap();
        assert!(back.payload.html.contains("<svg"));
    }

    // -----------------------------------------------------------------
    // classify_rename — PLSQL-LIN-015
    // -----------------------------------------------------------------

    fn rename_changeset() -> SemanticChangeSet {
        let mut cs = SemanticChangeSet::new();
        cs.push(ChangeRecord::Dropped {
            object_id: "billing.old_pkg".into(),
        });
        cs.push(ChangeRecord::Created {
            object_id: "billing.new_pkg".into(),
        });
        cs.push(ChangeRecord::Dropped {
            object_id: "billing.no_match".into(),
        });
        cs.push(ChangeRecord::Created {
            object_id: "billing.fresh_pkg".into(),
        });
        cs
    }

    #[test]
    fn classify_rename_promotes_explicit_mapping() {
        let cs = rename_changeset();
        let mut hints = RenameHints::default();
        hints
            .explicit_mappings
            .insert("billing.old_pkg".into(), "billing.new_pkg".into());
        let out = classify_rename(&cs, &hints);
        assert_eq!(out.candidates.len(), 1);
        let c = &out.candidates[0];
        assert_eq!(c.before_logical_id, "billing.old_pkg");
        assert_eq!(c.after_logical_id, "billing.new_pkg");
        assert_eq!(c.confidence, Confidence::Exact);
        assert!(matches!(c.evidence, RenameEvidence::Explicit));
        assert_eq!(out.unmatched_deletes, vec!["billing.no_match"]);
        assert_eq!(out.unmatched_creates, vec!["billing.fresh_pkg"]);
    }

    #[test]
    fn classify_rename_promotes_persistent_id_pair() {
        let cs = rename_changeset();
        let mut hints = RenameHints::default();
        hints
            .persistent_id_pairs
            .push(("billing.old_pkg".into(), "billing.new_pkg".into()));
        let out = classify_rename(&cs, &hints);
        assert_eq!(out.candidates.len(), 1);
        assert!(matches!(
            out.candidates[0].evidence,
            RenameEvidence::PersistentId
        ));
        assert_eq!(out.candidates[0].confidence, Confidence::Exact);
    }

    #[test]
    fn classify_rename_uses_git_rename_above_threshold() {
        let cs = rename_changeset();
        let hints = RenameHints {
            git_renames: vec![GitRenameHint {
                from: "billing.old_pkg".into(),
                to: "billing.new_pkg".into(),
                similarity: 85,
            }],
            ..RenameHints::default()
        };
        let out = classify_rename(&cs, &hints);
        assert_eq!(out.candidates.len(), 1);
        assert_eq!(out.candidates[0].confidence, Confidence::Heuristic);
        assert!(matches!(
            out.candidates[0].evidence,
            RenameEvidence::GitRename { similarity: 85 }
        ));
    }

    #[test]
    fn classify_rename_ignores_git_rename_below_threshold() {
        let cs = rename_changeset();
        let hints = RenameHints {
            git_renames: vec![GitRenameHint {
                from: "billing.old_pkg".into(),
                to: "billing.new_pkg".into(),
                similarity: 40,
            }],
            ..RenameHints::default()
        };
        let out = classify_rename(&cs, &hints);
        assert!(out.candidates.is_empty());
        // Both unmatched.
        assert_eq!(out.unmatched_deletes.len(), 2);
        assert_eq!(out.unmatched_creates.len(), 2);
    }

    #[test]
    fn classify_rename_no_hints_means_no_renames() {
        let cs = rename_changeset();
        let out = classify_rename(&cs, &RenameHints::default());
        assert!(out.candidates.is_empty());
        assert_eq!(out.unmatched_deletes.len(), 2);
        assert_eq!(out.unmatched_creates.len(), 2);
    }

    #[test]
    fn classify_rename_explicit_beats_git() {
        // Two hints would point at different "after" objects. Explicit wins.
        let cs = rename_changeset();
        let mut hints = RenameHints {
            git_renames: vec![GitRenameHint {
                from: "billing.old_pkg".into(),
                to: "billing.fresh_pkg".into(),
                similarity: 95,
            }],
            ..RenameHints::default()
        };
        hints
            .explicit_mappings
            .insert("billing.old_pkg".into(), "billing.new_pkg".into());
        let out = classify_rename(&cs, &hints);
        assert_eq!(out.candidates.len(), 1);
        let c = &out.candidates[0];
        assert_eq!(c.after_logical_id, "billing.new_pkg");
        assert!(matches!(c.evidence, RenameEvidence::Explicit));
    }

    // -----------------------------------------------------------------
    // compare_oracle_deps — PLSQL-LIN-016
    // -----------------------------------------------------------------

    #[test]
    fn compare_oracle_deps_categorises_classifications() {
        use plsql_catalog::{
            CatalogDependency, CatalogDependencyKind, CatalogSnapshot,
            SchemaCatalog as CatalogSchemaCatalog,
        };
        use plsql_core::{
            ObjectName as CoreObjectName, SchemaName as CoreSchemaName,
            SymbolInterner as CoreSymInterner,
        };
        use plsql_depgraph::{
            DepGraph as Dg, Edge as DgEdge, EdgeId as DgEdgeId, EdgeKind as DgEdgeKind,
            LogicalObjectId, Node as DgNode, NodeId as DgNodeId, NodeIdentityKind as DgNodeKind,
            ObjectRevisionId, Provenance as DgProvenance, QualifiedName,
            ResolutionStrategy as DgResolutionStrategy,
        };

        let mut interner = CoreSymInterner::new();
        let billing = interner.intern("billing").expect("intern");
        let claims_pkg = interner.intern("claims_pkg").expect("intern");
        let claims = interner.intern("claims").expect("intern");
        let legacy = interner.intern("legacy").expect("intern");

        let mut graph = Dg::new();
        graph.insert_node(DgNode::new(
            DgNodeId::new(1),
            LogicalObjectId::new("billing.claims_pkg"),
            ObjectRevisionId::new("rev:pkg"),
            QualifiedName::new(
                Some(CoreSchemaName::from(billing)),
                CoreObjectName::from(claims_pkg),
            ),
            DgNodeKind::PackageBody,
        ));
        graph.insert_node(DgNode::new(
            DgNodeId::new(2),
            LogicalObjectId::new("billing.claims"),
            ObjectRevisionId::new("rev:claims"),
            QualifiedName::new(
                Some(CoreSchemaName::from(billing)),
                CoreObjectName::from(claims),
            ),
            DgNodeKind::Table,
        ));
        // engine_only edge
        graph.insert_edge(
            DgEdge::new(
                DgEdgeId::new(1),
                DgNodeId::new(1),
                DgNodeId::new(2),
                DgEdgeKind::Reads,
                plsql_core::Confidence::new(plsql_core::ConfidenceLevel::High, None),
            ),
            DgProvenance::new(
                plsql_core::FileId::new(1),
                plsql_core::Span::new(
                    plsql_core::FileId::new(1),
                    plsql_core::Position::new(1, 1, 0),
                    plsql_core::Position::new(1, 1, 0),
                ),
                DgResolutionStrategy::CatalogLookup,
            ),
            None,
        );

        let mut snapshot = CatalogSnapshot::default();
        let mut sc = CatalogSchemaCatalog::default();
        // oracle_only dep
        sc.dependencies.push(CatalogDependency {
            owner: CoreSchemaName::from(billing),
            name: CoreObjectName::from(claims_pkg),
            object_type: plsql_catalog::ObjectType::Package,
            referenced_owner: Some(CoreSchemaName::from(billing)),
            referenced_name: CoreObjectName::from(legacy),
            referenced_type: Some(plsql_catalog::ObjectType::View),
            dependency_kind: CatalogDependencyKind::Hard,
            via_db_link: None,
        });
        snapshot.schemas.insert(CoreSchemaName::from(billing), sc);

        let report = compare_oracle_deps(&graph, &snapshot, &interner);
        assert_eq!(report.engine_edges, 1);
        assert_eq!(report.oracle_dependencies, 1);
        assert_eq!(report.agreements, 0);
        assert_eq!(report.engine_only.len(), 1);
        assert_eq!(report.engine_only[0].from, "billing.claims_pkg");
        assert_eq!(report.engine_only[0].to, "billing.claims");
        assert_eq!(report.oracle_only.len(), 1);
        assert_eq!(report.oracle_only[0].to, "billing.legacy");
        assert!(report.kind_mismatches.is_empty());
    }

    #[test]
    fn compare_oracle_deps_envelope_carries_schema_id() {
        use plsql_catalog::CatalogSnapshot;
        use plsql_core::SymbolInterner as CoreSymInterner;
        use plsql_depgraph::DepGraph as Dg;
        let report = compare_oracle_deps(
            &Dg::new(),
            &CatalogSnapshot::default(),
            &CoreSymInterner::new(),
        );
        let envelope = compare_oracle_deps_envelope(report);
        assert!(envelope.matches_schema(COMPARE_ORACLE_DEPS_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.compare_oracle_deps"));
    }

    #[test]
    fn classify_rename_envelope_carries_schema_id() {
        let cs = rename_changeset();
        let mut hints = RenameHints::default();
        hints
            .explicit_mappings
            .insert("billing.old_pkg".into(), "billing.new_pkg".into());
        let out = classify_rename(&cs, &hints);
        let envelope = classify_rename_envelope(out);
        assert!(envelope.matches_schema(CLASSIFY_RENAME_SCHEMA));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.lineage.classify_rename"));
        let back: RobotJsonEnvelope<RenameClassification> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload.candidates.len(), 1);
    }
}
