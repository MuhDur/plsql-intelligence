#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::Write as _;

use plsql_catalog::{
    CatalogDependency, CatalogDependencyKind, CatalogSnapshot, ConstraintMetadata, ConstraintType,
    SchemaCatalog, TriggerMetadata,
};
use plsql_core::{
    ColumnName, Confidence, ConfidenceLevel, Evidence, FileId, MemberName, ObjectName, SchemaName,
    Span, SymbolInterner,
};
use plsql_output::{RobotJsonEnvelope, SchemaDescriptor, SchemaVersion};
use plsql_render::graphviz;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;

macro_rules! numeric_id {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Eq,
            PartialEq,
            Ord,
            PartialOrd,
            Hash,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            #[must_use]
            #[instrument(level = "trace")]
            pub fn new(raw: u64) -> Self {
                Self(raw)
            }

            #[must_use]
            #[instrument(level = "trace", skip(self))]
            pub fn get(self) -> u64 {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

macro_rules! string_id {
    ($name:ident) => {
        #[derive(
            Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            #[instrument(level = "trace", skip(value))]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            #[must_use]
            #[instrument(level = "trace", skip(self))]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0.as_str())
            }
        }
    };
}

numeric_id!(NodeId);
numeric_id!(EdgeId);
numeric_id!(PersistentObjectId);

string_id!(LogicalObjectId);
string_id!(ObjectRevisionId);

pub const GRAPHML_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.depgraph.graphml",
    version: SchemaVersion::new(1, 0, 0),
    description: "GraphML export for plsql-intelligence dependency graphs",
};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphMlDocument {
    pub graphml: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphMlEnvelope {
    #[serde(flatten)]
    pub envelope: RobotJsonEnvelope<GraphMlDocument>,
}

impl GraphMlEnvelope {
    #[must_use]
    #[instrument(level = "trace", skip(graphml))]
    pub fn new(graphml: String) -> Self {
        Self {
            envelope: RobotJsonEnvelope::new(GRAPHML_SCHEMA, GraphMlDocument { graphml }),
        }
    }
}

pub const QUERY_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.depgraph.query",
    version: SchemaVersion::new(1, 0, 0),
    description: "Query results for plsql-intelligence dependency graphs",
};

pub const DOCTOR_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.depgraph.doctor",
    version: SchemaVersion::new(1, 0, 0),
    description: "Doctor report for plsql-intelligence dependency graphs",
};

pub const CATALOG_CROSS_CHECK_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.depgraph.catalog_cross_check",
    version: SchemaVersion::new(1, 0, 0),
    description: "Cross-check of depgraph edges against Oracle ALL_DEPENDENCIES",
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NodeSelector {
    NodeId(NodeId),
    LogicalObjectId(String),
}

impl NodeSelector {
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn describe(&self) -> String {
        match self {
            Self::NodeId(node_id) => format!("node-id={}", node_id.get()),
            Self::LogicalObjectId(logical_id) => format!("logical-id={logical_id}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeSummary {
    pub id: NodeId,
    pub logical_id: String,
    pub revision_id: String,
    pub persistent_id: Option<u64>,
    pub identity_kind: NodeIdentityKind,
}

impl NodeSummary {
    #[must_use]
    #[instrument(level = "trace", skip(node))]
    pub fn from_node(node: &Node) -> Self {
        Self {
            id: node.id,
            logical_id: String::from(node.logical_id.as_str()),
            revision_id: String::from(node.revision_id.as_str()),
            persistent_id: node.persistent_id.map(PersistentObjectId::get),
            identity_kind: node.identity_kind,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EdgeSummary {
    pub id: EdgeId,
    pub from: NodeSummary,
    pub to: NodeSummary,
    pub kind: EdgeKind,
    pub confidence: Confidence,
    pub resolution_strategy: Option<ResolutionStrategy>,
    pub has_evidence: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NeighborhoodQueryResult {
    pub node: NodeSummary,
    pub edges: Vec<EdgeSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PathQueryResult {
    pub from: NodeSummary,
    pub to: NodeSummary,
    pub found: bool,
    pub nodes: Vec<NodeSummary>,
    pub edges: Vec<EdgeSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CycleSummary {
    pub nodes: Vec<NodeSummary>,
    pub edges: Vec<EdgeSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CycleDetectResult {
    pub cycles: Vec<CycleSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum QueryOutput {
    Neighbors(NeighborhoodQueryResult),
    ReverseNeighbors(NeighborhoodQueryResult),
    Path(PathQueryResult),
    CycleDetect(CycleDetectResult),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InvariantViolationSummary {
    pub code: String,
    pub edge_id: EdgeId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DepGraphDoctorReport {
    pub node_count: usize,
    pub edge_count: usize,
    pub nodes_without_persistent_id: usize,
    pub low_confidence_edges: Vec<EdgeSummary>,
    pub opaque_edge_count: usize,
    pub cycle_count: usize,
    pub validation_violations: Vec<InvariantViolationSummary>,
    pub edge_kind_counts: BTreeMap<String, usize>,
    pub identity_kind_counts: BTreeMap<String, usize>,
}

#[derive(Default)]
struct TarjanState {
    index: usize,
    indices: HashMap<NodeId, usize>,
    lowlinks: HashMap<NodeId, usize>,
    stack: Vec<NodeId>,
    on_stack: HashSet<NodeId>,
    components: Vec<Vec<NodeId>>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum GraphQueryError {
    #[error("no node matched selector `{selector}`")]
    NodeNotFound { selector: String },
    #[error("edge {edge_id} references missing node {missing_node_id}")]
    MissingEdgeEndpoint {
        edge_id: EdgeId,
        missing_node_id: NodeId,
    },
    #[error("edge {edge_id} not found in graph")]
    EdgeNotFound { edge_id: EdgeId },
}

#[must_use]
#[instrument(level = "trace", skip(result))]
pub fn query_envelope(result: QueryOutput) -> RobotJsonEnvelope<QueryOutput> {
    RobotJsonEnvelope::new(QUERY_SCHEMA, result)
}

#[must_use]
#[instrument(level = "trace", skip(report))]
pub fn doctor_envelope(report: DepGraphDoctorReport) -> RobotJsonEnvelope<DepGraphDoctorReport> {
    RobotJsonEnvelope::new(DOCTOR_SCHEMA, report)
}

/// Schema ID for explain robot-JSON output.
pub const EXPLAIN_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.depgraph.explain",
    description: "Detailed explanation of a dependency graph element",
    version: SchemaVersion::new(1, 0, 0),
};

/// Detailed explanation of a single edge with full provenance and evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExplainEdge {
    pub edge_id: EdgeId,
    pub from: NodeSummary,
    pub to: NodeSummary,
    pub kind: EdgeKind,
    pub confidence: Confidence,
    pub provenance: Option<Provenance>,
    pub evidence: Option<Evidence>,
}

/// Detailed explanation of a node with all connected edges.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExplainNode {
    pub node: NodeSummary,
    pub outgoing_edges: Vec<ExplainEdge>,
    pub incoming_edges: Vec<ExplainEdge>,
}

/// Detailed explanation of a path with full edge provenance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExplainPath {
    pub from: NodeSummary,
    pub to: NodeSummary,
    pub found: bool,
    pub edges: Vec<ExplainEdge>,
}

/// Top-level explain report for robot-JSON output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExplainReport {
    Edge(Box<ExplainEdge>),
    Node(ExplainNode),
    Path(ExplainPath),
}

/// Wrap an `ExplainReport` in the versioned robot-JSON envelope.
#[must_use]
pub fn explain_envelope(report: ExplainReport) -> RobotJsonEnvelope<ExplainReport> {
    RobotJsonEnvelope::new(EXPLAIN_SCHEMA, report)
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct QualifiedName {
    pub schema: Option<SchemaName>,
    pub object: ObjectName,
    pub member: Option<MemberName>,
    pub column: Option<ColumnName>,
    pub db_link: Option<String>,
}

impl QualifiedName {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new(schema: Option<SchemaName>, object: ObjectName) -> Self {
        Self {
            schema,
            object,
            member: None,
            column: None,
            db_link: None,
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_member(mut self, member: MemberName) -> Self {
        self.member = Some(member);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_column(mut self, column: ColumnName) -> Self {
        self.column = Some(column);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, db_link))]
    pub fn with_db_link(mut self, db_link: impl Into<String>) -> Self {
        self.db_link = Some(db_link.into());
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, interner))]
    pub fn render(&self, interner: &SymbolInterner) -> String {
        let mut rendered = Vec::new();

        if let Some(schema) = self.schema {
            rendered.push(resolve_symbol(interner, schema.symbol()));
        }

        rendered.push(resolve_symbol(interner, self.object.symbol()));

        if let Some(member) = self.member {
            rendered.push(resolve_symbol(interner, member.symbol()));
        }

        if let Some(column) = self.column {
            rendered.push(resolve_symbol(interner, column.symbol()));
        }

        let mut name = rendered.join(".");
        if let Some(db_link) = &self.db_link {
            name.push('@');
            name.push_str(db_link);
        }

        name
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum NodeIdentityKind {
    #[default]
    Unknown,
    SpecDeclaration,
    BodyImplementation,
    StandaloneProcedure,
    StandaloneFunction,
    LocalNestedRoutine,
    PackageSpecification,
    PackageBody,
    PackageProcedure,
    PackageFunction,
    Table,
    View,
    MaterializedView,
    Sequence,
    Type,
    TypeAttribute,
    TypeMethod,
    Trigger,
    Column,
    Constraint,
    Synonym,
    SchedulerJob,
    EditioningView,
}

impl NodeIdentityKind {
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::SpecDeclaration => "SpecDeclaration",
            Self::BodyImplementation => "BodyImplementation",
            Self::StandaloneProcedure => "StandaloneProcedure",
            Self::StandaloneFunction => "StandaloneFunction",
            Self::LocalNestedRoutine => "LocalNestedRoutine",
            Self::PackageSpecification => "PackageSpecification",
            Self::PackageBody => "PackageBody",
            Self::PackageProcedure => "PackageProcedure",
            Self::PackageFunction => "PackageFunction",
            Self::Table => "Table",
            Self::View => "View",
            Self::MaterializedView => "MaterializedView",
            Self::Sequence => "Sequence",
            Self::Type => "Type",
            Self::TypeAttribute => "TypeAttribute",
            Self::TypeMethod => "TypeMethod",
            Self::Trigger => "Trigger",
            Self::Column => "Column",
            Self::Constraint => "Constraint",
            Self::Synonym => "Synonym",
            Self::SchedulerJob => "SchedulerJob",
            Self::EditioningView => "EditioningView",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ParameterMode {
    #[default]
    In,
    Out,
    InOut,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParameterSignature {
    pub position: u32,
    pub name: Option<MemberName>,
    pub mode: ParameterMode,
    pub data_type: String,
    pub has_default: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OverloadSignature {
    pub parameters: Vec<ParameterSignature>,
    pub return_type: Option<String>,
}

impl OverloadSignature {
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn arity(&self) -> usize {
        self.parameters.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub logical_id: LogicalObjectId,
    pub revision_id: ObjectRevisionId,
    pub persistent_id: Option<PersistentObjectId>,
    pub display_name: QualifiedName,
    pub identity_kind: NodeIdentityKind,
    pub overload_signature: Option<OverloadSignature>,
}

impl Node {
    #[must_use]
    #[instrument(level = "trace", skip(logical_id, revision_id, display_name))]
    pub fn new(
        id: NodeId,
        logical_id: LogicalObjectId,
        revision_id: ObjectRevisionId,
        display_name: QualifiedName,
        identity_kind: NodeIdentityKind,
    ) -> Self {
        Self {
            id,
            logical_id,
            revision_id,
            persistent_id: None,
            display_name,
            identity_kind,
            overload_signature: None,
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_persistent_id(mut self, persistent_id: PersistentObjectId) -> Self {
        self.persistent_id = Some(persistent_id);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, overload_signature))]
    pub fn with_overload_signature(mut self, overload_signature: OverloadSignature) -> Self {
        self.overload_signature = Some(overload_signature);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum EdgeKind {
    #[default]
    Unknown,
    Calls,
    Reads,
    Writes,
    ReadsColumn,
    WritesColumn,
    DerivesColumn,
    ReadsUnknownColumnOfTable,
    WritesUnknownColumnOfTable,
    TriggersOn,
    DependsOnType,
    Constrains,
    OpaqueDynamic,
    DbLink,
    References,
}

impl EdgeKind {
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Calls => "Calls",
            Self::Reads => "Reads",
            Self::Writes => "Writes",
            Self::ReadsColumn => "ReadsColumn",
            Self::WritesColumn => "WritesColumn",
            Self::DerivesColumn => "DerivesColumn",
            Self::ReadsUnknownColumnOfTable => "ReadsUnknownColumnOfTable",
            Self::WritesUnknownColumnOfTable => "WritesUnknownColumnOfTable",
            Self::TriggersOn => "TriggersOn",
            Self::DependsOnType => "DependsOnType",
            Self::Constrains => "Constrains",
            Self::OpaqueDynamic => "OpaqueDynamic",
            Self::DbLink => "DbLink",
            Self::References => "References",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    pub confidence: Confidence,
}

impl Edge {
    #[must_use]
    #[instrument(level = "trace", skip(confidence))]
    pub fn new(
        id: EdgeId,
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
        confidence: Confidence,
    ) -> Self {
        Self {
            id,
            from,
            to,
            kind,
            confidence,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ResolutionStrategy {
    #[default]
    Unknown,
    LocalLexical,
    PackageMemberLookup,
    SameSchemaLookup,
    CatalogLookup,
    SynonymExpansion,
    PublicSynonymExpansion,
    ConstraintMetadata,
    TriggerMetadata,
    DynamicSqlInference,
    DbLinkBoundary,
    ManualMapping,
    PlScopeCrossCheck,
}

impl ResolutionStrategy {
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::LocalLexical => "LocalLexical",
            Self::PackageMemberLookup => "PackageMemberLookup",
            Self::SameSchemaLookup => "SameSchemaLookup",
            Self::CatalogLookup => "CatalogLookup",
            Self::SynonymExpansion => "SynonymExpansion",
            Self::PublicSynonymExpansion => "PublicSynonymExpansion",
            Self::ConstraintMetadata => "ConstraintMetadata",
            Self::TriggerMetadata => "TriggerMetadata",
            Self::DynamicSqlInference => "DynamicSqlInference",
            Self::DbLinkBoundary => "DbLinkBoundary",
            Self::ManualMapping => "ManualMapping",
            Self::PlScopeCrossCheck => "PlScopeCrossCheck",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub file: FileId,
    pub span: Span,
    pub parse_rule: Option<String>,
    pub resolution_strategy: ResolutionStrategy,
    pub notes: Vec<String>,
}

impl Provenance {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new(file: FileId, span: Span, resolution_strategy: ResolutionStrategy) -> Self {
        Self {
            file,
            span,
            parse_rule: None,
            resolution_strategy,
            notes: Vec::new(),
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, parse_rule))]
    pub fn with_parse_rule(mut self, parse_rule: impl Into<String>) -> Self {
        self.parse_rule = Some(parse_rule.into());
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, note))]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphInvariantViolation {
    MissingProvenance { edge_id: EdgeId },
    MissingEvidence { edge_id: EdgeId },
}

impl From<GraphInvariantViolation> for InvariantViolationSummary {
    fn from(value: GraphInvariantViolation) -> Self {
        match value {
            GraphInvariantViolation::MissingProvenance { edge_id } => Self {
                code: String::from("missing_provenance"),
                edge_id,
            },
            GraphInvariantViolation::MissingEvidence { edge_id } => Self {
                code: String::from("missing_evidence"),
                edge_id,
            },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DepGraph {
    pub nodes: HashMap<NodeId, Node>,
    pub edges: Vec<Edge>,
    pub provenance: HashMap<EdgeId, Provenance>,
    pub evidence: HashMap<EdgeId, Evidence>,
}

impl DepGraph {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new() -> Self {
        Self::default()
    }

    #[instrument(level = "trace", skip(self, node))]
    pub fn insert_node(&mut self, node: Node) -> Option<Node> {
        self.nodes.insert(node.id, node)
    }

    #[instrument(level = "trace", skip(self, edge, provenance, evidence))]
    pub fn insert_edge(&mut self, edge: Edge, provenance: Provenance, evidence: Option<Evidence>) {
        let edge_id = edge.id;
        self.edges.push(edge);
        self.provenance.insert(edge_id, provenance);

        if let Some(evidence) = evidence {
            self.evidence.insert(edge_id, evidence);
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn validate(&self) -> Vec<GraphInvariantViolation> {
        let mut violations = Vec::new();

        for edge in &self.edges {
            if !self.provenance.contains_key(&edge.id) {
                violations.push(GraphInvariantViolation::MissingProvenance { edge_id: edge.id });
            }

            if edge.confidence.level != ConfidenceLevel::High
                && !self.evidence.contains_key(&edge.id)
            {
                violations.push(GraphInvariantViolation::MissingEvidence { edge_id: edge.id });
            }
        }

        violations
    }

    #[instrument(level = "trace", skip(self, selector))]
    pub fn resolve_node(&self, selector: &NodeSelector) -> Result<&Node, GraphQueryError> {
        match selector {
            NodeSelector::NodeId(node_id) => {
                self.nodes
                    .get(node_id)
                    .ok_or_else(|| GraphQueryError::NodeNotFound {
                        selector: selector.describe(),
                    })
            }
            NodeSelector::LogicalObjectId(logical_id) => self
                .nodes
                .values()
                .find(|node| node.logical_id.as_str() == logical_id)
                .ok_or_else(|| GraphQueryError::NodeNotFound {
                    selector: selector.describe(),
                }),
        }
    }

    #[instrument(level = "trace", skip(self, selector))]
    pub fn query_neighbors(
        &self,
        selector: &NodeSelector,
    ) -> Result<NeighborhoodQueryResult, GraphQueryError> {
        let node = self.resolve_node(selector)?;
        let edges = sorted_edges(&self.edges)
            .into_iter()
            .filter(|edge| edge.from == node.id)
            .map(|edge| self.edge_summary(edge))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(NeighborhoodQueryResult {
            node: NodeSummary::from_node(node),
            edges,
        })
    }

    #[instrument(level = "trace", skip(self, selector))]
    pub fn query_reverse_neighbors(
        &self,
        selector: &NodeSelector,
    ) -> Result<NeighborhoodQueryResult, GraphQueryError> {
        let node = self.resolve_node(selector)?;
        let edges = sorted_edges(&self.edges)
            .into_iter()
            .filter(|edge| edge.to == node.id)
            .map(|edge| self.edge_summary(edge))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(NeighborhoodQueryResult {
            node: NodeSummary::from_node(node),
            edges,
        })
    }

    #[instrument(level = "trace", skip(self, from, to))]
    pub fn query_path(
        &self,
        from: &NodeSelector,
        to: &NodeSelector,
    ) -> Result<PathQueryResult, GraphQueryError> {
        let start = self.resolve_node(from)?;
        let target = self.resolve_node(to)?;

        let mut queue = VecDeque::from([start.id]);
        let mut visited = HashSet::from([start.id]);
        let mut predecessors = HashMap::<NodeId, EdgeId>::new();

        while let Some(current) = queue.pop_front() {
            if current == target.id {
                break;
            }

            for edge in sorted_edges(&self.edges)
                .into_iter()
                .filter(|edge| edge.from == current)
            {
                if visited.insert(edge.to) {
                    predecessors.insert(edge.to, edge.id);
                    queue.push_back(edge.to);
                }
            }
        }

        if !visited.contains(&target.id) {
            return Ok(PathQueryResult {
                from: NodeSummary::from_node(start),
                to: NodeSummary::from_node(target),
                found: false,
                nodes: vec![
                    NodeSummary::from_node(start),
                    NodeSummary::from_node(target),
                ],
                edges: Vec::new(),
            });
        }

        let mut path_edge_ids = Vec::new();
        let mut cursor = target.id;
        while cursor != start.id {
            let Some(edge_id) = predecessors.get(&cursor).copied() else {
                break;
            };
            path_edge_ids.push(edge_id);
            let edge = self
                .edge_by_id(edge_id)
                .ok_or(GraphQueryError::MissingEdgeEndpoint {
                    edge_id,
                    missing_node_id: cursor,
                })?;
            cursor = edge.from;
        }
        path_edge_ids.reverse();

        let mut nodes = Vec::from([NodeSummary::from_node(start)]);
        let mut edges = Vec::new();
        for edge_id in path_edge_ids {
            let edge = self
                .edge_by_id(edge_id)
                .ok_or(GraphQueryError::MissingEdgeEndpoint {
                    edge_id,
                    missing_node_id: target.id,
                })?;
            edges.push(self.edge_summary(edge)?);
            let next_node =
                self.node_by_id(edge.to)
                    .ok_or(GraphQueryError::MissingEdgeEndpoint {
                        edge_id,
                        missing_node_id: edge.to,
                    })?;
            nodes.push(NodeSummary::from_node(next_node));
        }

        Ok(PathQueryResult {
            from: NodeSummary::from_node(start),
            to: NodeSummary::from_node(target),
            found: true,
            nodes,
            edges,
        })
    }

    #[instrument(level = "trace", skip(self))]
    pub fn detect_cycles(&self) -> Result<CycleDetectResult, GraphQueryError> {
        let mut state = TarjanState::default();

        for node_id in sorted_nodes(&self.nodes).into_iter().map(|node| node.id) {
            if !state.indices.contains_key(&node_id) {
                self.strong_connect(node_id, &mut state)?;
            }
        }

        let mut cycles = state
            .components
            .into_iter()
            .filter_map(|component| self.component_cycle_summary(component).transpose())
            .collect::<Result<Vec<_>, _>>()?;

        cycles.sort_by_key(|cycle| cycle.nodes.first().map(|node| node.id).unwrap_or_default());
        Ok(CycleDetectResult { cycles })
    }

    #[instrument(level = "trace", skip(self))]
    pub fn doctor_report(&self) -> Result<DepGraphDoctorReport, GraphQueryError> {
        let low_confidence_edges = sorted_edges(&self.edges)
            .into_iter()
            .filter(|edge| edge.confidence.level != ConfidenceLevel::High)
            .map(|edge| self.edge_summary(edge))
            .collect::<Result<Vec<_>, _>>()?;

        let opaque_edge_count = self
            .edges
            .iter()
            .filter(|edge| edge.confidence.level == ConfidenceLevel::Opaque)
            .count();
        let nodes_without_persistent_id = self
            .nodes
            .values()
            .filter(|node| node.persistent_id.is_none())
            .count();

        let mut edge_kind_counts = BTreeMap::<String, usize>::new();
        for edge in &self.edges {
            *edge_kind_counts
                .entry(String::from(edge.kind.as_str()))
                .or_default() += 1;
        }

        let mut identity_kind_counts = BTreeMap::<String, usize>::new();
        for node in self.nodes.values() {
            *identity_kind_counts
                .entry(String::from(node.identity_kind.as_str()))
                .or_default() += 1;
        }

        let validation_violations = self
            .validate()
            .into_iter()
            .map(InvariantViolationSummary::from)
            .collect::<Vec<_>>();
        let cycle_count = self.detect_cycles()?.cycles.len();

        Ok(DepGraphDoctorReport {
            node_count: self.node_count(),
            edge_count: self.edge_count(),
            nodes_without_persistent_id,
            low_confidence_edges,
            opaque_edge_count,
            cycle_count,
            validation_violations,
            edge_kind_counts,
            identity_kind_counts,
        })
    }

    fn node_by_id(&self, node_id: NodeId) -> Option<&Node> {
        self.nodes.get(&node_id)
    }

    fn edge_by_id(&self, edge_id: EdgeId) -> Option<&Edge> {
        self.edges.iter().find(|edge| edge.id == edge_id)
    }

    fn edge_summary(&self, edge: &Edge) -> Result<EdgeSummary, GraphQueryError> {
        let from = self
            .node_by_id(edge.from)
            .ok_or(GraphQueryError::MissingEdgeEndpoint {
                edge_id: edge.id,
                missing_node_id: edge.from,
            })?;
        let to = self
            .node_by_id(edge.to)
            .ok_or(GraphQueryError::MissingEdgeEndpoint {
                edge_id: edge.id,
                missing_node_id: edge.to,
            })?;

        Ok(EdgeSummary {
            id: edge.id,
            from: NodeSummary::from_node(from),
            to: NodeSummary::from_node(to),
            kind: edge.kind,
            confidence: edge.confidence.clone(),
            resolution_strategy: self
                .provenance
                .get(&edge.id)
                .map(|provenance| provenance.resolution_strategy),
            has_evidence: self.evidence.contains_key(&edge.id),
        })
    }

    fn strong_connect(
        &self,
        node_id: NodeId,
        state: &mut TarjanState,
    ) -> Result<(), GraphQueryError> {
        state.indices.insert(node_id, state.index);
        state.lowlinks.insert(node_id, state.index);
        state.index += 1;
        state.stack.push(node_id);
        state.on_stack.insert(node_id);

        for edge in sorted_edges(&self.edges)
            .into_iter()
            .filter(|edge| edge.from == node_id)
        {
            let neighbor = edge.to;
            if !state.indices.contains_key(&neighbor) {
                self.strong_connect(neighbor, state)?;

                let neighbor_lowlink = state.lowlinks.get(&neighbor).copied().ok_or(
                    GraphQueryError::MissingEdgeEndpoint {
                        edge_id: edge.id,
                        missing_node_id: neighbor,
                    },
                )?;
                let current_lowlink = state.lowlinks.get(&node_id).copied().ok_or(
                    GraphQueryError::MissingEdgeEndpoint {
                        edge_id: edge.id,
                        missing_node_id: node_id,
                    },
                )?;
                if neighbor_lowlink < current_lowlink {
                    state.lowlinks.insert(node_id, neighbor_lowlink);
                }
            } else if state.on_stack.contains(&neighbor) {
                let neighbor_index = state.indices.get(&neighbor).copied().ok_or(
                    GraphQueryError::MissingEdgeEndpoint {
                        edge_id: edge.id,
                        missing_node_id: neighbor,
                    },
                )?;
                let current_lowlink = state.lowlinks.get(&node_id).copied().ok_or(
                    GraphQueryError::MissingEdgeEndpoint {
                        edge_id: edge.id,
                        missing_node_id: node_id,
                    },
                )?;
                if neighbor_index < current_lowlink {
                    state.lowlinks.insert(node_id, neighbor_index);
                }
            }
        }

        let node_index =
            state
                .indices
                .get(&node_id)
                .copied()
                .ok_or(GraphQueryError::NodeNotFound {
                    selector: format!("node-id={}", node_id.get()),
                })?;
        let node_lowlink =
            state
                .lowlinks
                .get(&node_id)
                .copied()
                .ok_or(GraphQueryError::NodeNotFound {
                    selector: format!("node-id={}", node_id.get()),
                })?;

        if node_lowlink == node_index {
            let mut component = Vec::new();
            while let Some(current) = state.stack.pop() {
                state.on_stack.remove(&current);
                component.push(current);
                if current == node_id {
                    break;
                }
            }
            component.sort_unstable();
            state.components.push(component);
        }

        Ok(())
    }

    fn component_cycle_summary(
        &self,
        component: Vec<NodeId>,
    ) -> Result<Option<CycleSummary>, GraphQueryError> {
        let component_nodes = component
            .iter()
            .copied()
            .map(|node_id| {
                self.node_by_id(node_id).map(NodeSummary::from_node).ok_or(
                    GraphQueryError::NodeNotFound {
                        selector: format!("node-id={}", node_id.get()),
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let component_set = component.iter().copied().collect::<HashSet<_>>();
        let component_edges = sorted_edges(&self.edges)
            .into_iter()
            .filter(|edge| component_set.contains(&edge.from) && component_set.contains(&edge.to))
            .map(|edge| self.edge_summary(edge))
            .collect::<Result<Vec<_>, _>>()?;

        let is_cycle = component_nodes.len() > 1
            || component_edges
                .iter()
                .any(|edge| edge.from.id == edge.to.id);
        if !is_cycle {
            return Ok(None);
        }

        Ok(Some(CycleSummary {
            nodes: component_nodes,
            edges: component_edges,
        }))
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, interner))]
    pub fn to_graphml(&self, interner: &SymbolInterner) -> String {
        let mut graphml = String::from(concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\" ",
            "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" ",
            "xsi:schemaLocation=\"http://graphml.graphdrawing.org/xmlns ",
            "http://graphml.graphdrawing.org/xmlns/1.0/graphml.xsd\">\n",
            "  <key id=\"node_label\" for=\"node\" attr.name=\"label\" attr.type=\"string\" />\n",
            "  <key id=\"node_kind\" for=\"node\" attr.name=\"identity_kind\" attr.type=\"string\" />\n",
            "  <key id=\"logical_id\" for=\"node\" attr.name=\"logical_id\" attr.type=\"string\" />\n",
            "  <key id=\"revision_id\" for=\"node\" attr.name=\"revision_id\" attr.type=\"string\" />\n",
            "  <key id=\"persistent_id\" for=\"node\" attr.name=\"persistent_id\" attr.type=\"string\" />\n",
            "  <key id=\"edge_kind\" for=\"edge\" attr.name=\"kind\" attr.type=\"string\" />\n",
            "  <key id=\"confidence_level\" for=\"edge\" attr.name=\"confidence_level\" attr.type=\"string\" />\n",
            "  <key id=\"confidence_explanation\" for=\"edge\" attr.name=\"confidence_explanation\" attr.type=\"string\" />\n",
            "  <key id=\"resolution_strategy\" for=\"edge\" attr.name=\"resolution_strategy\" attr.type=\"string\" />\n",
            "  <key id=\"parse_rule\" for=\"edge\" attr.name=\"parse_rule\" attr.type=\"string\" />\n",
            "  <key id=\"evidence_count\" for=\"edge\" attr.name=\"evidence_count\" attr.type=\"int\" />\n",
            "  <graph id=\"plsql-depgraph\" edgedefault=\"directed\">\n"
        ));

        for node in sorted_nodes(&self.nodes) {
            let persistent_id = persistent_id_text(node.persistent_id);
            let _ = write!(
                graphml,
                concat!(
                    "    <node id=\"n{node_id}\">\n",
                    "      <data key=\"node_label\">{label}</data>\n",
                    "      <data key=\"node_kind\">{kind}</data>\n",
                    "      <data key=\"logical_id\">{logical_id}</data>\n",
                    "      <data key=\"revision_id\">{revision_id}</data>\n",
                    "      <data key=\"persistent_id\">{persistent_id}</data>\n",
                    "    </node>\n"
                ),
                node_id = node.id.get(),
                label = escape_xml(&node.display_name.render(interner)),
                kind = escape_xml(node.identity_kind.as_str()),
                logical_id = escape_xml(node.logical_id.as_str()),
                revision_id = escape_xml(node.revision_id.as_str()),
                persistent_id = escape_xml(&persistent_id),
            );
        }

        for edge in sorted_edges(&self.edges) {
            let provenance = self.provenance.get(&edge.id);
            let confidence_explanation = edge.confidence.explanation.as_deref().unwrap_or("");
            let parse_rule = provenance
                .and_then(|entry| entry.parse_rule.as_deref())
                .unwrap_or("");
            let resolution_strategy = provenance
                .map(|entry| entry.resolution_strategy.as_str())
                .unwrap_or(ResolutionStrategy::Unknown.as_str());
            let evidence_count = usize::from(self.evidence.contains_key(&edge.id));

            let _ = write!(
                graphml,
                concat!(
                    "    <edge id=\"e{edge_id}\" source=\"n{from_id}\" target=\"n{to_id}\">\n",
                    "      <data key=\"edge_kind\">{kind}</data>\n",
                    "      <data key=\"confidence_level\">{confidence_level}</data>\n",
                    "      <data key=\"confidence_explanation\">{confidence_explanation}</data>\n",
                    "      <data key=\"resolution_strategy\">{resolution_strategy}</data>\n",
                    "      <data key=\"parse_rule\">{parse_rule}</data>\n",
                    "      <data key=\"evidence_count\">{evidence_count}</data>\n",
                    "    </edge>\n"
                ),
                edge_id = edge.id.get(),
                from_id = edge.from.get(),
                to_id = edge.to.get(),
                kind = escape_xml(edge.kind.as_str()),
                confidence_level = escape_xml(confidence_level_name(edge.confidence.level)),
                confidence_explanation = escape_xml(confidence_explanation),
                resolution_strategy = escape_xml(resolution_strategy),
                parse_rule = escape_xml(parse_rule),
                evidence_count = evidence_count,
            );
        }

        graphml.push_str("  </graph>\n</graphml>\n");
        graphml
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, interner))]
    pub fn graphml_envelope(&self, interner: &SymbolInterner) -> GraphMlEnvelope {
        GraphMlEnvelope::new(self.to_graphml(interner))
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, interner))]
    pub fn to_dot(&self, interner: &SymbolInterner) -> String {
        let mut dot = String::from(concat!(
            "digraph plsql_depgraph {\n",
            "  rankdir=LR;\n",
            "  graph [fontname=\"Helvetica\"];\n",
            "  node [shape=box, style=\"rounded\", fontname=\"Helvetica\"];\n",
            "  edge [fontname=\"Helvetica\"];\n"
        ));

        for node in sorted_nodes(&self.nodes) {
            let mut label = node.display_name.render(interner);
            label.push('\n');
            label.push_str(node.identity_kind.as_str());
            let _ = writeln!(
                dot,
                "  {} [label=\"{}\"] ;",
                graphviz::quote_id(dot_node_id(node.id)),
                graphviz::escape_label(&label),
            );
        }

        for edge in sorted_edges(&self.edges) {
            let mut label = String::from(edge.kind.as_str());
            label.push_str(" (");
            label.push_str(confidence_level_name(edge.confidence.level));
            label.push(')');
            let _ = writeln!(
                dot,
                "  {} -> {} [label=\"{}\"] ;",
                graphviz::quote_id(dot_node_id(edge.from)),
                graphviz::quote_id(dot_node_id(edge.to)),
                graphviz::escape_label(&label),
            );
        }

        dot.push_str("}\n");
        dot
    }

    /// Explain a single edge by edge id — returns full provenance and evidence.
    #[instrument(level = "trace", skip(self))]
    pub fn explain_edge(&self, edge_id: EdgeId) -> Result<ExplainEdge, GraphQueryError> {
        let edge = self
            .edges
            .iter()
            .find(|e| e.id == edge_id)
            .ok_or(GraphQueryError::EdgeNotFound { edge_id })?;

        let from = self
            .node_by_id(edge.from)
            .ok_or(GraphQueryError::MissingEdgeEndpoint {
                edge_id: edge.id,
                missing_node_id: edge.from,
            })?;
        let to = self
            .node_by_id(edge.to)
            .ok_or(GraphQueryError::MissingEdgeEndpoint {
                edge_id: edge.id,
                missing_node_id: edge.to,
            })?;

        Ok(ExplainEdge {
            edge_id: edge.id,
            from: NodeSummary::from_node(from),
            to: NodeSummary::from_node(to),
            kind: edge.kind,
            confidence: edge.confidence.clone(),
            provenance: self.provenance.get(&edge.id).cloned(),
            evidence: self.evidence.get(&edge.id).cloned(),
        })
    }

    /// Explain a node — identity plus all connected edges with full provenance.
    #[instrument(level = "trace", skip(self))]
    pub fn explain_node(&self, selector: &NodeSelector) -> Result<ExplainNode, GraphQueryError> {
        let node = self.resolve_node(selector)?;

        let outgoing = sorted_edges(&self.edges)
            .into_iter()
            .filter(|e| e.from == node.id)
            .map(|e| self.explain_edge(e.id))
            .collect::<Result<Vec<_>, _>>()?;

        let incoming = sorted_edges(&self.edges)
            .into_iter()
            .filter(|e| e.to == node.id)
            .map(|e| self.explain_edge(e.id))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ExplainNode {
            node: NodeSummary::from_node(node),
            outgoing_edges: outgoing,
            incoming_edges: incoming,
        })
    }

    /// Explain a path — all edges along the shortest directed path with full provenance.
    #[instrument(level = "trace", skip(self, from, to))]
    pub fn explain_path(
        &self,
        from: &NodeSelector,
        to: &NodeSelector,
    ) -> Result<ExplainPath, GraphQueryError> {
        let path_result = self.query_path(from, to)?;

        let explained_edges = path_result
            .edges
            .iter()
            .map(|summary| self.explain_edge(summary.id))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ExplainPath {
            from: path_result.from,
            to: path_result.to,
            found: path_result.found,
            edges: explained_edges,
        })
    }

    /// Compare every edge in `self` against Oracle's `ALL_DEPENDENCIES`
    /// rows from `snapshot`, classifying each pair as a match,
    /// `OurExtra`, `OracleOnly`, `KindMismatch`, or `ExpectedGap`.
    ///
    /// Used by:
    /// - `lineage compare-oracle-deps` customer report
    /// - completeness gates in `plsql-engine` doctor output
    #[must_use]
    #[instrument(level = "trace", skip(self, snapshot, interner))]
    pub fn cross_check_with_catalog(
        &self,
        snapshot: &CatalogSnapshot,
        interner: &SymbolInterner,
    ) -> CatalogCrossCheckReport {
        catalog_cross_check(self, snapshot, interner)
    }

    /// Record a DB-link dependency edge.
    ///
    /// A `name@dblink` reference crosses into a remote database
    /// whose metadata is unreachable from offline analysis, so
    /// the edge is recorded with `ConfidenceLevel::Opaque` and a
    /// mandatory `Evidence` row naming the link. The edge target
    /// is the `to` node the caller minted for the remote object
    /// placeholder; `db_link_name` is surfaced in the evidence so
    /// reports can group remote dependencies by link.
    ///
    /// Returns the `EdgeId` so the caller can correlate it with
    /// the originating `DbLinkReference`.
    #[instrument(level = "trace", skip(self, span))]
    pub fn record_db_link_edge(
        &mut self,
        edge_id: EdgeId,
        from: NodeId,
        to: NodeId,
        db_link_name: &str,
        file: FileId,
        span: Span,
    ) -> EdgeId {
        let edge = Edge::new(
            edge_id,
            from,
            to,
            EdgeKind::DbLink,
            Confidence::new(ConfidenceLevel::Opaque, None),
        );
        let provenance = Provenance::new(file, span, ResolutionStrategy::default())
            .with_note(format!("db-link reference via @{db_link_name}"));
        let evidence = Evidence::new(
            "DEP008",
            format!(
                "remote object reached through database link {db_link_name:?}; remote metadata is opaque to offline analysis (R13 DbLinkRemoteObject)"
            ),
        );
        self.insert_edge(edge, provenance, Some(evidence));
        edge_id
    }
}

/// One mismatch between depgraph edges and `ALL_DEPENDENCIES` rows.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CrossCheckMismatch {
    /// Our depgraph has an edge that Oracle's dictionary does not.
    /// Often legitimate (e.g. dynamic-SQL inferences, source-derived
    /// edges that haven't been compiled yet), but worth flagging.
    OurExtra {
        from: String,
        to: String,
        edge_kind: String,
        confidence: String,
    },
    /// Oracle records a dependency that our depgraph does not.
    /// Indicates we missed something in source analysis or that the
    /// catalog reflects state we haven't loaded (different edition,
    /// dropped + recreated, etc.).
    OracleOnly {
        from: String,
        to: String,
        dependency_kind: String,
    },
    /// Both sides record the dependency but disagree on its kind.
    KindMismatch {
        from: String,
        to: String,
        our_kind: String,
        oracle_kind: String,
    },
    /// Difference we have an explicit reason to expect, not a bug.
    ExpectedGap {
        from: String,
        to: String,
        reason: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CrossCheckSummary {
    pub total_oracle_deps: usize,
    pub total_our_edges: usize,
    pub matches: usize,
    pub our_extras: usize,
    pub oracle_onlies: usize,
    pub kind_mismatches: usize,
    pub expected_gaps: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogCrossCheckReport {
    pub summary: CrossCheckSummary,
    pub mismatches: Vec<CrossCheckMismatch>,
}

/// Edge kinds whose absence from `ALL_DEPENDENCIES` is *expected*
/// rather than a bug. `OpaqueDynamic` (we resolved via runtime
/// information Oracle doesn't track), `DbLink` (Oracle records the
/// remote side opaquely), and `Constrains` / `TriggersOn` (carried in
/// other dictionary views, not `ALL_DEPENDENCIES`) all fall here.
fn edge_kind_is_expected_gap(kind: EdgeKind) -> bool {
    matches!(
        kind,
        EdgeKind::OpaqueDynamic | EdgeKind::DbLink | EdgeKind::Constrains | EdgeKind::TriggersOn
    )
}

fn render_owner_name(
    owner: SchemaName,
    name: ObjectName,
    interner: &SymbolInterner,
) -> Option<String> {
    let owner = interner.resolve(owner.symbol())?;
    let name = interner.resolve(name.symbol())?;
    Some(format!("{owner}.{name}").to_lowercase())
}

fn dep_keys(
    dep: &CatalogDependency,
    snapshot_owner_fallback: Option<SchemaName>,
    interner: &SymbolInterner,
) -> Option<(String, String)> {
    let from = render_owner_name(dep.owner, dep.name, interner)?;
    let ref_owner = dep.referenced_owner.or(snapshot_owner_fallback)?;
    let to = render_owner_name(ref_owner, dep.referenced_name, interner)?;
    Some((from, to))
}

fn normalised_object_key(logical_id: &str) -> String {
    // Reduce 3-part names (`schema.package.member`) to the object level
    // (`schema.package`) since Oracle's ALL_DEPENDENCIES does not track
    // within-package dependencies. Single-part names pass through.
    let parts: Vec<&str> = logical_id.split('.').collect();
    let key = match parts.len() {
        0 => String::new(),
        1 => parts[0].to_owned(),
        _ => format!("{}.{}", parts[0], parts[1]),
    };
    key.to_lowercase()
}

fn catalog_cross_check(
    graph: &DepGraph,
    snapshot: &CatalogSnapshot,
    interner: &SymbolInterner,
) -> CatalogCrossCheckReport {
    let mut summary = CrossCheckSummary::default();
    let mut mismatches: Vec<CrossCheckMismatch> = Vec::new();

    // Build catalog dep key set keyed by (from, to). CatalogDependency
    // rows live on each SchemaCatalog; iterate per schema.
    use std::collections::BTreeMap;
    let mut catalog_index: BTreeMap<(String, String), CatalogDependencyKind> = BTreeMap::new();
    for (owner, schema_catalog) in &snapshot.schemas {
        for dep in &schema_catalog.dependencies {
            let Some((from, to)) = dep_keys(dep, Some(*owner), interner) else {
                continue;
            };
            summary.total_oracle_deps += 1;
            catalog_index.insert((from, to), dep.dependency_kind);
        }
    }

    // Build our edge key map: (from, to) → (EdgeKind, Confidence).
    let mut our_index: BTreeMap<(String, String), (EdgeKind, ConfidenceLevel)> = BTreeMap::new();
    for edge in &graph.edges {
        summary.total_our_edges += 1;
        let Some(from_node) = graph.nodes.get(&edge.from) else {
            continue;
        };
        let Some(to_node) = graph.nodes.get(&edge.to) else {
            continue;
        };
        let from = normalised_object_key(from_node.logical_id.as_str());
        let to = normalised_object_key(to_node.logical_id.as_str());
        our_index.insert((from, to), (edge.kind, edge.confidence.level));
    }

    // Walk our edges against the catalog index.
    let our_keys: std::collections::BTreeSet<_> = our_index.keys().cloned().collect();
    let catalog_keys: std::collections::BTreeSet<_> = catalog_index.keys().cloned().collect();

    for key in our_keys.intersection(&catalog_keys) {
        let (kind, _conf) = our_index[key];
        let oracle_kind = catalog_index[key];
        let our_label = kind.as_str();
        let oracle_label = catalog_dep_kind_label(oracle_kind);
        if catalog_kinds_compatible(kind, oracle_kind) {
            summary.matches += 1;
        } else {
            summary.kind_mismatches += 1;
            mismatches.push(CrossCheckMismatch::KindMismatch {
                from: key.0.clone(),
                to: key.1.clone(),
                our_kind: our_label.to_owned(),
                oracle_kind: oracle_label.to_owned(),
            });
        }
    }

    for key in our_keys.difference(&catalog_keys) {
        let (kind, conf) = our_index[key];
        if edge_kind_is_expected_gap(kind) {
            summary.expected_gaps += 1;
            mismatches.push(CrossCheckMismatch::ExpectedGap {
                from: key.0.clone(),
                to: key.1.clone(),
                reason: format!(
                    "Edge kind {} is not tracked by ALL_DEPENDENCIES",
                    kind.as_str()
                ),
            });
        } else {
            summary.our_extras += 1;
            mismatches.push(CrossCheckMismatch::OurExtra {
                from: key.0.clone(),
                to: key.1.clone(),
                edge_kind: kind.as_str().to_owned(),
                confidence: confidence_level_name(conf).to_owned(),
            });
        }
    }
    for key in catalog_keys.difference(&our_keys) {
        let oracle_kind = catalog_index[key];
        summary.oracle_onlies += 1;
        mismatches.push(CrossCheckMismatch::OracleOnly {
            from: key.0.clone(),
            to: key.1.clone(),
            dependency_kind: catalog_dep_kind_label(oracle_kind).to_owned(),
        });
    }

    mismatches.sort_by(|a, b| {
        let key = |m: &CrossCheckMismatch| match m {
            CrossCheckMismatch::OurExtra { from, to, .. }
            | CrossCheckMismatch::OracleOnly { from, to, .. }
            | CrossCheckMismatch::KindMismatch { from, to, .. }
            | CrossCheckMismatch::ExpectedGap { from, to, .. } => (from.clone(), to.clone()),
        };
        key(a).cmp(&key(b))
    });

    CatalogCrossCheckReport {
        summary,
        mismatches,
    }
}

fn catalog_dep_kind_label(kind: CatalogDependencyKind) -> &'static str {
    match kind {
        CatalogDependencyKind::Hard => "HARD",
        CatalogDependencyKind::Reference => "REF",
        CatalogDependencyKind::Extended => "EXTENDED",
        CatalogDependencyKind::Other => "OTHER",
    }
}

fn catalog_kinds_compatible(ours: EdgeKind, oracle: CatalogDependencyKind) -> bool {
    use CatalogDependencyKind::*;
    use EdgeKind::*;
    matches!(
        (ours, oracle),
        (
            Calls | Reads | Writes | ReadsColumn | WritesColumn | DerivesColumn,
            Hard
        ) | (ReadsUnknownColumnOfTable | WritesUnknownColumnOfTable, Hard)
            | (DependsOnType, Hard)
            | (References | Constrains, Reference)
            | (_, Extended | Other)
    )
}

#[must_use]
pub fn catalog_cross_check_envelope(
    report: CatalogCrossCheckReport,
) -> RobotJsonEnvelope<CatalogCrossCheckReport> {
    RobotJsonEnvelope::new(CATALOG_CROSS_CHECK_SCHEMA, report)
}

#[must_use]
#[instrument(level = "trace", skip(snapshot))]
pub fn extract_trigger_edges(snapshot: &CatalogSnapshot) -> DepGraph {
    let mut builder = CatalogGraphBuilder::default();

    for (_schema_name, schema) in sorted_schema_catalogs(snapshot) {
        let mut triggers = schema.triggers.values().collect::<Vec<_>>();
        triggers.sort_by_key(|trigger| trigger.common.name.symbol());

        for trigger in triggers {
            builder.add_trigger_edge(trigger);
        }
    }

    builder.finish()
}

#[must_use]
#[instrument(level = "trace", skip(snapshot))]
pub fn extract_constraint_edges(snapshot: &CatalogSnapshot) -> DepGraph {
    let mut builder = CatalogGraphBuilder::default();

    for (_schema_name, schema) in sorted_schema_catalogs(snapshot) {
        let mut constraints = schema.constraints.values().collect::<Vec<_>>();
        constraints.sort_by_key(|constraint| constraint.name.symbol());

        for constraint in constraints {
            builder.add_constraint_edge(constraint);
        }
    }

    builder.finish()
}

#[derive(Default)]
struct CatalogGraphBuilder {
    graph: DepGraph,
    next_node_id: u64,
    next_edge_id: u64,
    nodes_by_logical_id: HashMap<String, NodeId>,
}

impl CatalogGraphBuilder {
    fn finish(self) -> DepGraph {
        self.graph
    }

    fn add_trigger_edge(&mut self, trigger: &TriggerMetadata) {
        let trigger_id = self.ensure_catalog_node(
            catalog_trigger_logical_id(trigger),
            trigger_revision_id(trigger),
            QualifiedName::new(Some(trigger.common.owner), trigger.common.name),
            NodeIdentityKind::Trigger,
        );
        let target_id = self.ensure_catalog_node(
            catalog_table_logical_id(trigger.target_owner, trigger.target_name),
            catalog_table_revision_id(trigger.target_owner, trigger.target_name),
            QualifiedName::new(Some(trigger.target_owner), trigger.target_name),
            NodeIdentityKind::Table,
        );

        let note = format!(
            "catalog trigger metadata from schema symbol {}",
            trigger.common.owner.symbol().get()
        );
        self.insert_catalog_edge(
            trigger_id,
            target_id,
            EdgeKind::TriggersOn,
            ResolutionStrategy::TriggerMetadata,
            note,
        );
    }

    fn add_constraint_edge(&mut self, constraint: &ConstraintMetadata) {
        if constraint.constraint_type != ConstraintType::ForeignKey {
            return;
        }

        let Some(referenced_table_owner) = constraint.referenced_table_owner else {
            return;
        };
        let Some(referenced_table_name) = constraint.referenced_table_name else {
            return;
        };

        let constraint_id = self.ensure_catalog_node(
            catalog_constraint_logical_id(constraint.table_owner, constraint),
            constraint_revision_id(constraint),
            QualifiedName::new(
                Some(constraint.table_owner),
                ObjectName::from(constraint.name.symbol()),
            ),
            NodeIdentityKind::Constraint,
        );
        let target_id = self.ensure_catalog_node(
            catalog_table_logical_id(referenced_table_owner, referenced_table_name),
            catalog_table_revision_id(referenced_table_owner, referenced_table_name),
            QualifiedName::new(Some(referenced_table_owner), referenced_table_name),
            NodeIdentityKind::Table,
        );

        let note = format!(
            "catalog foreign key metadata from schema symbol {}",
            constraint.table_owner.symbol().get()
        );
        self.insert_catalog_edge(
            constraint_id,
            target_id,
            EdgeKind::Constrains,
            ResolutionStrategy::ConstraintMetadata,
            note,
        );
    }

    fn ensure_catalog_node(
        &mut self,
        logical_id: String,
        revision_id: String,
        display_name: QualifiedName,
        identity_kind: NodeIdentityKind,
    ) -> NodeId {
        if let Some(&node_id) = self.nodes_by_logical_id.get(logical_id.as_str()) {
            return node_id;
        }

        self.next_node_id += 1;
        let node_id = NodeId::new(self.next_node_id);
        self.graph.insert_node(Node::new(
            node_id,
            LogicalObjectId::new(logical_id.clone()),
            ObjectRevisionId::new(revision_id),
            display_name,
            identity_kind,
        ));
        self.nodes_by_logical_id.insert(logical_id, node_id);
        node_id
    }

    fn insert_catalog_edge(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
        resolution_strategy: ResolutionStrategy,
        note: String,
    ) {
        self.next_edge_id += 1;
        self.graph.insert_edge(
            Edge::new(
                EdgeId::new(self.next_edge_id),
                from,
                to,
                kind,
                Confidence::new(ConfidenceLevel::High, None),
            ),
            catalog_provenance(resolution_strategy, note),
            None,
        );
    }
}

#[must_use]
fn sorted_schema_catalogs(snapshot: &CatalogSnapshot) -> Vec<(SchemaName, &SchemaCatalog)> {
    let mut schemas = snapshot
        .schemas
        .iter()
        .map(|(schema_name, schema)| (*schema_name, schema))
        .collect::<Vec<_>>();
    schemas.sort_by_key(|(schema_name, _)| schema_name.symbol());
    schemas
}

#[instrument(level = "trace", skip(interner))]
fn resolve_symbol(interner: &SymbolInterner, symbol: plsql_core::SymbolId) -> String {
    interner
        .resolve(symbol)
        .map(String::from)
        .unwrap_or_else(|| format!("#{}", symbol.get()))
}

#[must_use]
#[instrument(level = "trace")]
fn confidence_level_name(level: ConfidenceLevel) -> &'static str {
    match level {
        ConfidenceLevel::High => "High",
        ConfidenceLevel::Medium => "Medium",
        ConfidenceLevel::Low => "Low",
        ConfidenceLevel::Opaque => "Opaque",
    }
}

#[must_use]
fn catalog_provenance(resolution_strategy: ResolutionStrategy, note: String) -> Provenance {
    Provenance::new(FileId::new(0), Span::default(), resolution_strategy)
        .with_parse_rule("catalog_snapshot")
        .with_note(note)
}

#[must_use]
fn catalog_table_logical_id(owner: SchemaName, table_name: ObjectName) -> String {
    format!(
        "catalog:schema#{}.table#{}",
        owner.symbol().get(),
        table_name.symbol().get()
    )
}

#[must_use]
fn catalog_table_revision_id(owner: SchemaName, table_name: ObjectName) -> String {
    format!(
        "catalog-table-rev:schema#{}.table#{}",
        owner.symbol().get(),
        table_name.symbol().get()
    )
}

#[must_use]
fn catalog_trigger_logical_id(trigger: &TriggerMetadata) -> String {
    format!(
        "catalog:schema#{}.trigger#{}",
        trigger.common.owner.symbol().get(),
        trigger.common.name.symbol().get()
    )
}

#[must_use]
fn trigger_revision_id(trigger: &TriggerMetadata) -> String {
    let event_vector = trigger
        .events
        .iter()
        .map(|event| format!("{event:?}"))
        .collect::<Vec<_>>()
        .join(",");
    let when_clause = trigger.when_clause.as_deref().unwrap_or("no-when-clause");
    let body_hash = trigger
        .body_hash
        .as_ref()
        .map(|hash| hash.as_str())
        .unwrap_or("no-body-hash");
    format!(
        "trigger-rev:{body_hash}:owner#{owner}:target#{target_owner}.{target_name}:timing={timing:?}:level={level:?}:events[{event_vector}]:when={when_clause}",
        owner = trigger.common.owner.symbol().get(),
        target_owner = trigger.target_owner.symbol().get(),
        target_name = trigger.target_name.symbol().get(),
        timing = trigger.timing,
        level = trigger.level,
        event_vector = event_vector,
        when_clause = when_clause,
    )
}

#[must_use]
fn catalog_constraint_logical_id(
    schema_name: SchemaName,
    constraint: &ConstraintMetadata,
) -> String {
    format!(
        "catalog:schema#{}.constraint#{}",
        schema_name.symbol().get(),
        constraint.name.symbol().get()
    )
}

#[must_use]
fn constraint_revision_id(constraint: &ConstraintMetadata) -> String {
    let column_vector = constraint
        .columns
        .iter()
        .map(|column| column.symbol().get().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let referenced_vector = constraint
        .referenced_columns
        .iter()
        .map(|column| column.symbol().get().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let referenced_owner = constraint
        .referenced_table_owner
        .map(|owner| owner.symbol().get().to_string())
        .unwrap_or_else(|| String::from("unknown-owner"));
    let referenced_table = constraint
        .referenced_table_name
        .map(|name| name.symbol().get().to_string())
        .unwrap_or_else(|| String::from("unknown-table"));
    format!(
        "constraint-rev:{constraint_type:?}:owner#{owner}:table#{table}:ref#{referenced_owner}.{referenced_table}:cols[{column_vector}]:refcols[{referenced_vector}]:deferrable={deferrable:?}:initially_deferred={initially_deferred:?}",
        constraint_type = constraint.constraint_type,
        owner = constraint.table_owner.symbol().get(),
        table = constraint.table_name.symbol().get(),
        referenced_owner = referenced_owner,
        referenced_table = referenced_table,
        column_vector = column_vector,
        referenced_vector = referenced_vector,
        deferrable = constraint.deferrable,
        initially_deferred = constraint.initially_deferred,
    )
}

#[must_use]
#[instrument(level = "trace", skip(value))]
fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[must_use]
#[instrument(level = "trace")]
fn dot_node_id(node_id: NodeId) -> String {
    let mut rendered = String::from("n");
    let _ = write!(rendered, "{}", node_id.get());
    rendered
}

#[must_use]
#[instrument(level = "trace")]
fn persistent_id_text(persistent_id: Option<PersistentObjectId>) -> String {
    let Some(persistent_id) = persistent_id else {
        return String::new();
    };

    let mut rendered = String::new();
    let _ = write!(rendered, "{}", persistent_id.get());
    rendered
}

#[must_use]
#[instrument(level = "trace", skip(nodes))]
fn sorted_nodes(nodes: &HashMap<NodeId, Node>) -> Vec<&Node> {
    let mut sorted = nodes.values().collect::<Vec<_>>();
    sorted.sort_by_key(|node| node.id);
    sorted
}

#[must_use]
#[instrument(level = "trace", skip(edges))]
fn sorted_edges(edges: &[Edge]) -> Vec<&Edge> {
    let mut sorted = edges.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|edge| edge.id);
    sorted
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        CATALOG_CROSS_CHECK_SCHEMA, CatalogCrossCheckReport, CrossCheckMismatch, CycleDetectResult,
        DepGraph, EXPLAIN_SCHEMA, Edge, EdgeId, EdgeKind, ExplainReport, GRAPHML_SCHEMA,
        GraphInvariantViolation, LogicalObjectId, Node, NodeId, NodeIdentityKind, NodeSelector,
        ObjectRevisionId, ParameterMode, ParameterSignature, Provenance, QualifiedName,
        ResolutionStrategy, catalog_cross_check_envelope, explain_envelope,
        extract_constraint_edges, extract_trigger_edges,
    };
    use plsql_catalog::{
        CatalogDependency, CatalogDependencyKind, CatalogSnapshot, ConstraintMetadata,
        ConstraintName, ConstraintType, Hash as CatalogHash, ObjectCommon, ObjectStatus,
        ObjectType, SchemaCatalog, TriggerEvent, TriggerLevel, TriggerMetadata, TriggerName,
        TriggerTiming,
    };
    use plsql_core::{
        Confidence, ConfidenceLevel, Evidence, FileId, MemberName, ObjectName, Position,
        SchemaName, Span, SymbolId, SymbolInterner,
    };
    use serde_json::json;

    fn sample_query_graph() -> DepGraph {
        let mut graph = DepGraph::new();

        graph.insert_node(Node::new(
            NodeId::new(1),
            LogicalObjectId::new("billing.claims_pkg.calculate/1"),
            ObjectRevisionId::new("sha256:pkg"),
            QualifiedName::new(None, ObjectName::from(SymbolId::new(10))),
            NodeIdentityKind::PackageProcedure,
        ));
        graph.insert_node(Node::new(
            NodeId::new(2),
            LogicalObjectId::new("billing.claims"),
            ObjectRevisionId::new("sha256:claims"),
            QualifiedName::new(None, ObjectName::from(SymbolId::new(11))),
            NodeIdentityKind::Table,
        ));
        graph.insert_node(Node::new(
            NodeId::new(3),
            LogicalObjectId::new("billing.claim_audit"),
            ObjectRevisionId::new("sha256:audit"),
            QualifiedName::new(None, ObjectName::from(SymbolId::new(12))),
            NodeIdentityKind::Table,
        ));

        graph.insert_edge(
            Edge::new(
                EdgeId::new(1),
                NodeId::new(1),
                NodeId::new(2),
                EdgeKind::Reads,
                Confidence::new(ConfidenceLevel::High, None),
            ),
            Provenance::new(
                FileId::new(1),
                Span::new(
                    FileId::new(1),
                    Position::new(1, 1, 0),
                    Position::new(1, 10, 9),
                ),
                ResolutionStrategy::CatalogLookup,
            ),
            None,
        );
        graph.insert_edge(
            Edge::new(
                EdgeId::new(2),
                NodeId::new(2),
                NodeId::new(3),
                EdgeKind::Writes,
                Confidence::new(
                    ConfidenceLevel::Medium,
                    Some(String::from(
                        "materialized view refresh edge inferred from metadata",
                    )),
                ),
            ),
            Provenance::new(
                FileId::new(1),
                Span::new(
                    FileId::new(1),
                    Position::new(2, 1, 10),
                    Position::new(2, 10, 19),
                ),
                ResolutionStrategy::CatalogLookup,
            ),
            Some(Evidence::new(
                "DEP003",
                "refresh target confirmed from catalog",
            )),
        );
        graph.insert_edge(
            Edge::new(
                EdgeId::new(3),
                NodeId::new(3),
                NodeId::new(2),
                EdgeKind::Reads,
                Confidence::new(ConfidenceLevel::High, None),
            ),
            Provenance::new(
                FileId::new(1),
                Span::new(
                    FileId::new(1),
                    Position::new(3, 1, 20),
                    Position::new(3, 10, 29),
                ),
                ResolutionStrategy::CatalogLookup,
            ),
            None,
        );

        graph
    }

    #[test]
    fn qualified_name_renders_schema_member_and_db_link() {
        let mut interner = SymbolInterner::new();
        let schema = interner
            .intern_schema_name("billing")
            .expect("schema symbol should intern");
        let object = ObjectName::from(interner.intern("claims_pkg").expect("object symbol"));
        let member = MemberName::from(interner.intern("calculate").expect("member symbol"));

        let name = QualifiedName::new(Some(schema), object)
            .with_member(member)
            .with_db_link("REMOTE");

        assert_eq!(
            name.render(&interner),
            "billing.claims_pkg.calculate@REMOTE"
        );
    }

    #[test]
    fn qualified_name_render_edge_shapes() {
        let mut interner = SymbolInterner::new();
        let schema = interner
            .intern_schema_name("hr")
            .expect("schema symbol should intern");
        let table = ObjectName::from(interner.intern("employees").expect("object symbol"));
        let column =
            plsql_core::ColumnName::from(interner.intern("salary").expect("column symbol"));
        let bare = ObjectName::from(interner.intern("dual").expect("object symbol"));

        // Object only — no schema/member/column/db_link.
        assert_eq!(QualifiedName::new(None, bare).render(&interner), "dual");

        // Column WITHOUT member: the member position is skipped, not
        // back-filled — `schema.table.column`, not `schema.table..column`
        // (table column-level dependency FQN).
        assert_eq!(
            QualifiedName::new(Some(schema), table)
                .with_column(column)
                .render(&interner),
            "hr.employees.salary"
        );

        // db_link with no schema attaches to the bare object.
        assert_eq!(
            QualifiedName::new(None, bare)
                .with_db_link("REMOTE")
                .render(&interner),
            "dual@REMOTE"
        );
    }

    #[test]
    fn overload_signature_reports_arity() {
        let mut interner = SymbolInterner::new();
        let parameter = ParameterSignature {
            position: 1,
            name: interner.intern("p_claim_id").map(MemberName::from),
            mode: ParameterMode::In,
            data_type: String::from("NUMBER"),
            has_default: false,
        };
        let overload = super::OverloadSignature {
            parameters: vec![parameter],
            return_type: Some(String::from("VARCHAR2")),
        };

        assert_eq!(overload.arity(), 1);
    }

    #[test]
    fn dep_graph_validation_enforces_provenance_and_evidence_rules() {
        let mut interner = SymbolInterner::new();
        let schema = interner
            .intern_schema_name("billing")
            .expect("schema symbol should intern");
        let object = ObjectName::from(interner.intern("claims_pkg").expect("object symbol"));
        let logical_id = LogicalObjectId::new("billing.claims_pkg.calculate/1");
        let revision_id = ObjectRevisionId::new("sha256:1234");

        let node = Node::new(
            NodeId::new(1),
            logical_id,
            revision_id,
            QualifiedName::new(Some(schema), object),
            NodeIdentityKind::PackageProcedure,
        );

        let edge = Edge::new(
            EdgeId::new(10),
            NodeId::new(1),
            NodeId::new(2),
            EdgeKind::OpaqueDynamic,
            Confidence::new(
                ConfidenceLevel::Low,
                Some(String::from("dynamic SQL target only partially inferred")),
            ),
        );

        let mut graph = DepGraph::new();
        graph.insert_node(node);
        graph.edges.push(edge);

        assert_eq!(
            graph.validate(),
            vec![
                GraphInvariantViolation::MissingProvenance {
                    edge_id: EdgeId::new(10)
                },
                GraphInvariantViolation::MissingEvidence {
                    edge_id: EdgeId::new(10)
                }
            ]
        );
    }

    #[test]
    fn insert_edge_stores_provenance_and_evidence() {
        let mut graph = DepGraph::new();
        let provenance = Provenance::new(
            FileId::new(7),
            Span::new(
                FileId::new(7),
                Position::new(12, 3, 120),
                Position::new(12, 22, 139),
            ),
            ResolutionStrategy::DynamicSqlInference,
        )
        .with_parse_rule("call_statement");
        let evidence = Evidence::new("DEP001", "call edge inferred from execute immediate")
            .with_attribute("candidate_count", json!(2));

        graph.insert_edge(
            Edge::new(
                EdgeId::new(11),
                NodeId::new(1),
                NodeId::new(2),
                EdgeKind::Calls,
                Confidence::new(
                    ConfidenceLevel::Medium,
                    Some(String::from("secondary parse found one viable callee")),
                ),
            ),
            provenance,
            Some(evidence),
        );

        assert!(graph.validate().is_empty());
        assert_eq!(graph.edge_count(), 1);
        assert_eq!(
            graph
                .provenance
                .get(&EdgeId::new(11))
                .and_then(|p| p.parse_rule.as_deref()),
            Some("call_statement")
        );
    }

    #[test]
    fn resolve_symbol_falls_back_to_symbol_id_when_not_in_interner() {
        let interner = SymbolInterner::new();
        let object = ObjectName::from(SymbolId::new(42));
        let name = QualifiedName::new(Some(SchemaName::from(SymbolId::new(7))), object);

        assert_eq!(name.render(&interner), "#7.#42");
    }

    #[test]
    fn query_path_finds_directed_chain_by_logical_id() {
        let graph = sample_query_graph();
        let path = graph
            .query_path(
                &NodeSelector::LogicalObjectId(String::from("billing.claims_pkg.calculate/1")),
                &NodeSelector::LogicalObjectId(String::from("billing.claim_audit")),
            )
            .expect("path query should succeed");

        assert!(path.found);
        assert_eq!(path.nodes.len(), 3);
        assert_eq!(path.edges.len(), 2);
        assert_eq!(path.edges[0].id, EdgeId::new(1));
        assert_eq!(path.edges[1].id, EdgeId::new(2));
    }

    #[test]
    fn detect_cycles_returns_strongly_connected_component_summaries() {
        let graph = sample_query_graph();
        let cycle_report = graph
            .detect_cycles()
            .expect("cycle detection should succeed");

        assert_eq!(
            cycle_report,
            CycleDetectResult {
                cycles: vec![super::CycleSummary {
                    nodes: vec![
                        super::NodeSummary {
                            id: NodeId::new(2),
                            logical_id: String::from("billing.claims"),
                            revision_id: String::from("sha256:claims"),
                            persistent_id: None,
                            identity_kind: NodeIdentityKind::Table,
                        },
                        super::NodeSummary {
                            id: NodeId::new(3),
                            logical_id: String::from("billing.claim_audit"),
                            revision_id: String::from("sha256:audit"),
                            persistent_id: None,
                            identity_kind: NodeIdentityKind::Table,
                        },
                    ],
                    edges: vec![
                        graph
                            .edge_summary(graph.edge_by_id(EdgeId::new(2)).expect("edge 2"))
                            .expect("edge 2 summary"),
                        graph
                            .edge_summary(graph.edge_by_id(EdgeId::new(3)).expect("edge 3"))
                            .expect("edge 3 summary"),
                    ],
                }],
            }
        );
    }

    #[test]
    fn graphml_export_round_trips_through_output_envelope() {
        let mut interner = SymbolInterner::new();
        let schema = interner
            .intern_schema_name("billing")
            .expect("schema symbol should intern");
        let package = ObjectName::from(interner.intern("claims_pkg").expect("object symbol"));
        let table = ObjectName::from(interner.intern("claims").expect("table symbol"));

        let caller = Node::new(
            NodeId::new(1),
            LogicalObjectId::new("billing.claims_pkg.calculate/1"),
            ObjectRevisionId::new("sha256:caller"),
            QualifiedName::new(Some(schema), package),
            NodeIdentityKind::PackageProcedure,
        );
        let callee = Node::new(
            NodeId::new(2),
            LogicalObjectId::new("billing.claims"),
            ObjectRevisionId::new("sha256:table"),
            QualifiedName::new(Some(schema), table),
            NodeIdentityKind::Table,
        );
        let provenance = Provenance::new(
            FileId::new(3),
            Span::new(
                FileId::new(3),
                Position::new(5, 2, 40),
                Position::new(5, 18, 56),
            ),
            ResolutionStrategy::CatalogLookup,
        )
        .with_parse_rule("select_statement");

        let mut graph = DepGraph::new();
        graph.insert_node(caller);
        graph.insert_node(callee);
        graph.insert_edge(
            Edge::new(
                EdgeId::new(5),
                NodeId::new(1),
                NodeId::new(2),
                EdgeKind::Reads,
                Confidence::new(
                    ConfidenceLevel::Medium,
                    Some(String::from("catalog lookup confirmed target table")),
                ),
            ),
            provenance,
            Some(Evidence::new(
                "DEP003",
                "table read edge confirmed by semantic SQL model",
            )),
        );

        let graphml = graph.to_graphml(&interner);
        let envelope = graph.graphml_envelope(&interner);

        assert!(graphml.contains("<graphml"));
        assert!(graphml.contains("billing.claims_pkg"));
        assert!(graphml.contains("Reads"));
        assert!(graphml.contains("select_statement"));
        assert!(graphml.contains("catalog lookup confirmed target table"));
        assert!(envelope.envelope.matches_schema(GRAPHML_SCHEMA));
        assert_eq!(envelope.envelope.payload.graphml, graphml);
    }

    #[test]
    fn dot_export_uses_graphviz_safe_labels() {
        let mut interner = SymbolInterner::new();
        let schema = interner
            .intern_schema_name("reporting")
            .expect("schema symbol should intern");
        let object = ObjectName::from(interner.intern("mv_sales").expect("object symbol"));
        let column = ObjectName::from(interner.intern("sales").expect("table symbol"));

        let mut graph = DepGraph::new();
        graph.insert_node(Node::new(
            NodeId::new(1),
            LogicalObjectId::new("reporting.mv_sales"),
            ObjectRevisionId::new("sha256:mv"),
            QualifiedName::new(Some(schema), object),
            NodeIdentityKind::MaterializedView,
        ));
        graph.insert_node(Node::new(
            NodeId::new(2),
            LogicalObjectId::new("reporting.sales"),
            ObjectRevisionId::new("sha256:tab"),
            QualifiedName::new(Some(schema), column),
            NodeIdentityKind::Table,
        ));
        graph.insert_edge(
            Edge::new(
                EdgeId::new(7),
                NodeId::new(1),
                NodeId::new(2),
                EdgeKind::Writes,
                Confidence::new(ConfidenceLevel::High, None),
            ),
            Provenance::new(
                FileId::new(9),
                Span::new(
                    FileId::new(9),
                    Position::new(1, 1, 0),
                    Position::new(1, 15, 14),
                ),
                ResolutionStrategy::TriggerMetadata,
            ),
            None,
        );

        let dot = graph.to_dot(&interner);

        assert!(dot.starts_with("digraph plsql_depgraph"));
        assert!(dot.contains("\"n1\" [label=\"reporting.mv_sales\\nMaterializedView\"]"));
        assert!(dot.contains("\"n1\" -> \"n2\" [label=\"Writes (High)\"]"));
    }

    #[test]
    fn extract_trigger_edges_builds_catalog_backed_trigger_to_table_edge() {
        let mut interner = SymbolInterner::new();
        let billing = interner
            .intern_schema_name("billing")
            .expect("billing schema should intern");
        let claims = ObjectName::from(interner.intern("claims").expect("claims table"));
        let trigger_name = ObjectName::from(
            interner
                .intern("claims_audit_trg")
                .expect("trigger name should intern"),
        );

        let trigger = TriggerMetadata {
            common: ObjectCommon {
                owner: billing,
                name: trigger_name,
                object_type: ObjectType::Trigger,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            target_owner: billing,
            target_name: claims,
            timing: TriggerTiming::Before,
            level: TriggerLevel::Row,
            events: vec![TriggerEvent::Insert, TriggerEvent::Update],
            when_clause: Some(String::from("new.status = 'OPEN'")),
            body_hash: Some(CatalogHash::new("body-sha256")),
        };

        let mut schema = SchemaCatalog::default();
        schema
            .triggers
            .insert(TriggerName::from(trigger_name.symbol()), trigger);

        let snapshot = CatalogSnapshot {
            schemas: HashMap::from([(billing, schema)]),
            ..CatalogSnapshot::default()
        };

        let graph = extract_trigger_edges(&snapshot);

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
        assert!(graph.validate().is_empty());

        let edge = graph
            .edges
            .first()
            .expect("graph should contain one trigger edge");
        assert_eq!(edge.kind, EdgeKind::TriggersOn);
        assert_eq!(edge.confidence.level, ConfidenceLevel::High);

        let trigger_node = graph
            .nodes
            .get(&edge.from)
            .expect("trigger node should exist");
        let table_node = graph.nodes.get(&edge.to).expect("table node should exist");
        let provenance = graph
            .provenance
            .get(&edge.id)
            .expect("trigger edge should have provenance");

        assert_eq!(trigger_node.identity_kind, NodeIdentityKind::Trigger);
        assert_eq!(
            trigger_node.display_name.render(&interner),
            "billing.claims_audit_trg"
        );
        assert!(
            trigger_node
                .revision_id
                .as_str()
                .contains("timing=Before:level=Row")
        );
        assert!(
            trigger_node
                .revision_id
                .as_str()
                .contains("when=new.status = 'OPEN'")
        );
        assert_eq!(table_node.identity_kind, NodeIdentityKind::Table);
        assert_eq!(table_node.display_name.render(&interner), "billing.claims");
        assert_eq!(
            provenance.resolution_strategy,
            ResolutionStrategy::TriggerMetadata
        );
        assert_eq!(provenance.parse_rule.as_deref(), Some("catalog_snapshot"));
        assert_eq!(provenance.notes.len(), 1);
    }

    #[test]
    fn extract_constraint_edges_only_keeps_resolved_foreign_keys() {
        let mut interner = SymbolInterner::new();
        let billing = interner
            .intern_schema_name("billing")
            .expect("billing schema should intern");
        let refdata = interner
            .intern_schema_name("refdata")
            .expect("refdata schema should intern");
        let claims = ObjectName::from(interner.intern("claims").expect("claims table"));
        let policies = ObjectName::from(interner.intern("policies").expect("policies table"));
        let fk_name = ConstraintName::from(interner.intern("fk_claim_policy").expect("fk name"));
        let pk_name = ConstraintName::from(interner.intern("pk_claims").expect("pk name"));
        let dangling_name =
            ConstraintName::from(interner.intern("fk_missing").expect("dangling fk name"));
        let claim_policy_column =
            plsql_core::ColumnName::from(interner.intern("policy_id").expect("policy id"));
        let policy_id_column =
            plsql_core::ColumnName::from(interner.intern("id").expect("policy pk"));

        let mut schema = SchemaCatalog::default();
        schema.constraints.insert(
            fk_name,
            ConstraintMetadata {
                name: fk_name,
                table_owner: billing,
                table_name: claims,
                constraint_type: ConstraintType::ForeignKey,
                columns: vec![claim_policy_column],
                referenced_table_owner: Some(refdata),
                referenced_table_name: Some(policies),
                referenced_columns: vec![policy_id_column],
                deferrable: Some(true),
                initially_deferred: Some(false),
                ..ConstraintMetadata::default()
            },
        );
        schema.constraints.insert(
            pk_name,
            ConstraintMetadata {
                name: pk_name,
                table_owner: billing,
                table_name: claims,
                constraint_type: ConstraintType::PrimaryKey,
                columns: vec![policy_id_column],
                ..ConstraintMetadata::default()
            },
        );
        schema.constraints.insert(
            dangling_name,
            ConstraintMetadata {
                name: dangling_name,
                table_owner: billing,
                table_name: claims,
                constraint_type: ConstraintType::ForeignKey,
                columns: vec![claim_policy_column],
                referenced_table_owner: Some(refdata),
                referenced_table_name: None,
                ..ConstraintMetadata::default()
            },
        );

        let snapshot = CatalogSnapshot {
            schemas: HashMap::from([(billing, schema)]),
            ..CatalogSnapshot::default()
        };

        let graph = extract_constraint_edges(&snapshot);

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
        assert!(graph.validate().is_empty());

        let edge = graph
            .edges
            .first()
            .expect("graph should contain one constraint edge");
        assert_eq!(edge.kind, EdgeKind::Constrains);
        assert_eq!(edge.confidence.level, ConfidenceLevel::High);

        let constraint_node = graph
            .nodes
            .get(&edge.from)
            .expect("constraint node should exist");
        let table_node = graph.nodes.get(&edge.to).expect("table node should exist");
        let provenance = graph
            .provenance
            .get(&edge.id)
            .expect("constraint edge should have provenance");

        assert_eq!(constraint_node.identity_kind, NodeIdentityKind::Constraint);
        assert_eq!(
            constraint_node.display_name.render(&interner),
            "billing.fk_claim_policy"
        );
        let expected_ref_fragment =
            format!("ref#{}.{}", refdata.symbol().get(), policies.symbol().get());
        assert!(
            constraint_node
                .revision_id
                .as_str()
                .contains(&expected_ref_fragment)
        );
        assert!(
            constraint_node
                .revision_id
                .as_str()
                .contains("deferrable=Some(true):initially_deferred=Some(false)")
        );
        assert_eq!(table_node.identity_kind, NodeIdentityKind::Table);
        assert_eq!(
            table_node.display_name.render(&interner),
            "refdata.policies"
        );
        assert_eq!(
            provenance.resolution_strategy,
            ResolutionStrategy::ConstraintMetadata
        );
        assert_eq!(provenance.parse_rule.as_deref(), Some("catalog_snapshot"));
        assert_eq!(provenance.notes.len(), 1);
    }

    #[test]
    fn explain_edge_returns_full_provenance_and_evidence() {
        let graph = sample_query_graph();
        let edge = &graph.edges[0];
        let report = graph.explain_edge(edge.id).unwrap();

        assert_eq!(report.edge_id, edge.id);
        assert!(report.provenance.is_some(), "provenance should be present");

        let prov = report.provenance.unwrap();
        assert_eq!(prov.resolution_strategy, ResolutionStrategy::CatalogLookup);
    }

    #[test]
    fn explain_node_returns_all_connected_edges() {
        let graph = sample_query_graph();
        let report = graph
            .explain_node(&NodeSelector::NodeId(NodeId::new(1)))
            .unwrap();

        // Node 1 has outgoing edges
        assert!(!report.outgoing_edges.is_empty());
        // All edges should have provenance
        for edge in &report.outgoing_edges {
            assert!(edge.provenance.is_some());
        }
    }

    #[test]
    fn explain_path_returns_edges_with_provenance() {
        let graph = sample_query_graph();
        let from = NodeSelector::NodeId(NodeId::new(1));
        let to = NodeSelector::NodeId(NodeId::new(2));
        let report = graph.explain_path(&from, &to).unwrap();

        assert!(report.found);
        assert!(!report.edges.is_empty());
        for edge in &report.edges {
            assert!(edge.provenance.is_some());
        }
    }

    #[test]
    fn explain_edge_not_found_returns_error() {
        let graph = sample_query_graph();
        let result = graph.explain_edge(EdgeId::new(999));
        assert!(result.is_err());
    }

    #[test]
    fn explain_report_roundtrips_through_json() {
        let graph = sample_query_graph();
        let report = ExplainReport::Edge(Box::new(graph.explain_edge(EdgeId::new(1)).unwrap()));
        let json = serde_json::to_string(&report).unwrap();
        let parsed: ExplainReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, parsed);
    }

    #[test]
    fn explain_envelope_has_correct_schema() {
        let graph = sample_query_graph();
        let report = ExplainReport::Edge(Box::new(graph.explain_edge(EdgeId::new(1)).unwrap()));
        let envelope = explain_envelope(report);
        assert_eq!(envelope.schema_id, EXPLAIN_SCHEMA.id);
    }

    // -----------------------------------------------------------------
    // catalog_cross_check_with_catalog — PLSQL-DEP-014
    // -----------------------------------------------------------------

    fn build_cross_check_fixture() -> (DepGraph, CatalogSnapshot, SymbolInterner) {
        let mut interner = SymbolInterner::new();
        let billing = interner.intern("billing").expect("intern");
        let pkg = interner.intern("claims_pkg").expect("intern");
        let claims = interner.intern("claims").expect("intern");
        let audit = interner.intern("claim_audit").expect("intern");
        let archived = interner.intern("archived_claims").expect("intern");

        let mut graph = DepGraph::new();
        graph.insert_node(Node::new(
            NodeId::new(1),
            LogicalObjectId::new("billing.claims_pkg"),
            ObjectRevisionId::new("sha256:pkg"),
            QualifiedName::new(Some(SchemaName::from(billing)), ObjectName::from(pkg)),
            NodeIdentityKind::PackageBody,
        ));
        graph.insert_node(Node::new(
            NodeId::new(2),
            LogicalObjectId::new("billing.claims"),
            ObjectRevisionId::new("sha256:claims"),
            QualifiedName::new(Some(SchemaName::from(billing)), ObjectName::from(claims)),
            NodeIdentityKind::Table,
        ));
        graph.insert_node(Node::new(
            NodeId::new(3),
            LogicalObjectId::new("billing.claim_audit"),
            ObjectRevisionId::new("sha256:audit"),
            QualifiedName::new(Some(SchemaName::from(billing)), ObjectName::from(audit)),
            NodeIdentityKind::Table,
        ));
        graph.insert_node(Node::new(
            NodeId::new(4),
            LogicalObjectId::new("billing.archived_claims"),
            ObjectRevisionId::new("sha256:archived"),
            QualifiedName::new(Some(SchemaName::from(billing)), ObjectName::from(archived)),
            NodeIdentityKind::Table,
        ));

        // billing.claims_pkg --Reads--> billing.claims (will match HARD dep)
        // billing.claims_pkg --OpaqueDynamic--> billing.claim_audit (expected gap)
        // billing.claims_pkg --Reads--> billing.archived_claims (our extra; not in catalog)
        for (id, from, to, kind) in [
            (1u64, 1u64, 2u64, EdgeKind::Reads),
            (2, 1, 3, EdgeKind::OpaqueDynamic),
            (3, 1, 4, EdgeKind::Reads),
        ] {
            graph.insert_edge(
                Edge::new(
                    EdgeId::new(id),
                    NodeId::new(from),
                    NodeId::new(to),
                    kind,
                    Confidence::new(ConfidenceLevel::High, None),
                ),
                Provenance::new(
                    FileId::new(1),
                    Span::new(
                        FileId::new(1),
                        Position::new(1, 1, 0),
                        Position::new(1, 1, 0),
                    ),
                    ResolutionStrategy::CatalogLookup,
                ),
                None,
            );
        }

        // Catalog snapshot: two deps.
        //  billing.claims_pkg → billing.claims  (HARD — matches our Reads edge)
        //  billing.claims_pkg → billing.legacy_view  (HARD — oracle-only)
        let mut snapshot = CatalogSnapshot::default();
        let mut sc = SchemaCatalog::default();
        let legacy = interner.intern("legacy_view").expect("intern");
        sc.dependencies.push(CatalogDependency {
            owner: SchemaName::from(billing),
            name: ObjectName::from(pkg),
            object_type: ObjectType::Package,
            referenced_owner: Some(SchemaName::from(billing)),
            referenced_name: ObjectName::from(claims),
            referenced_type: Some(ObjectType::Table),
            dependency_kind: CatalogDependencyKind::Hard,
            via_db_link: None,
        });
        sc.dependencies.push(CatalogDependency {
            owner: SchemaName::from(billing),
            name: ObjectName::from(pkg),
            object_type: ObjectType::Package,
            referenced_owner: Some(SchemaName::from(billing)),
            referenced_name: ObjectName::from(legacy),
            referenced_type: Some(ObjectType::View),
            dependency_kind: CatalogDependencyKind::Hard,
            via_db_link: None,
        });
        snapshot.schemas.insert(SchemaName::from(billing), sc);

        (graph, snapshot, interner)
    }

    #[test]
    fn cross_check_classifies_match_extra_oracle_only_and_expected_gap() {
        let (graph, snapshot, interner) = build_cross_check_fixture();
        let report = graph.cross_check_with_catalog(&snapshot, &interner);
        assert_eq!(report.summary.total_oracle_deps, 2);
        assert_eq!(report.summary.total_our_edges, 3);
        assert_eq!(report.summary.matches, 1);
        assert_eq!(report.summary.our_extras, 1);
        assert_eq!(report.summary.oracle_onlies, 1);
        assert_eq!(report.summary.expected_gaps, 1);
        assert_eq!(report.summary.kind_mismatches, 0);

        // Mismatch shapes.
        let kinds: Vec<&str> = report
            .mismatches
            .iter()
            .map(|m| match m {
                CrossCheckMismatch::OurExtra { .. } => "our_extra",
                CrossCheckMismatch::OracleOnly { .. } => "oracle_only",
                CrossCheckMismatch::KindMismatch { .. } => "kind_mismatch",
                CrossCheckMismatch::ExpectedGap { .. } => "expected_gap",
            })
            .collect();
        assert!(kinds.contains(&"our_extra"));
        assert!(kinds.contains(&"oracle_only"));
        assert!(kinds.contains(&"expected_gap"));
    }

    #[test]
    fn cross_check_envelope_carries_schema_id() {
        let (graph, snapshot, interner) = build_cross_check_fixture();
        let report = graph.cross_check_with_catalog(&snapshot, &interner);
        let envelope = catalog_cross_check_envelope(report);
        assert_eq!(envelope.schema_id, CATALOG_CROSS_CHECK_SCHEMA.id);
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.depgraph.catalog_cross_check"));
        let back: super::RobotJsonEnvelope<CatalogCrossCheckReport> =
            serde_json::from_str(&json).unwrap();
        assert!(back.payload.summary.total_oracle_deps >= 1);
    }

    #[test]
    fn cross_check_kind_mismatch_classifies_as_mismatch() {
        // Build a small graph + snapshot where the SAME (from, to)
        // exists on both sides but with incompatible kinds:
        // depgraph says `Calls` (caller→callee), catalog says
        // `Reference` (FK-style). Per catalog_kinds_compatible, `Calls`
        // only matches `Hard`, so this should land in `KindMismatch`.
        let mut interner = SymbolInterner::new();
        let billing = interner.intern("billing").expect("intern");
        let caller = interner.intern("caller_pkg").expect("intern");
        let callee = interner.intern("callee_t").expect("intern");

        let mut graph = DepGraph::new();
        graph.insert_node(Node::new(
            NodeId::new(1),
            LogicalObjectId::new("billing.caller_pkg"),
            ObjectRevisionId::new("sha256:caller"),
            QualifiedName::new(Some(SchemaName::from(billing)), ObjectName::from(caller)),
            NodeIdentityKind::PackageBody,
        ));
        graph.insert_node(Node::new(
            NodeId::new(2),
            LogicalObjectId::new("billing.callee_t"),
            ObjectRevisionId::new("sha256:callee"),
            QualifiedName::new(Some(SchemaName::from(billing)), ObjectName::from(callee)),
            NodeIdentityKind::Table,
        ));
        graph.insert_edge(
            Edge::new(
                EdgeId::new(1),
                NodeId::new(1),
                NodeId::new(2),
                EdgeKind::Calls,
                Confidence::new(ConfidenceLevel::High, None),
            ),
            Provenance::new(
                FileId::new(1),
                Span::new(
                    FileId::new(1),
                    Position::new(1, 1, 0),
                    Position::new(1, 1, 0),
                ),
                ResolutionStrategy::CatalogLookup,
            ),
            None,
        );

        let mut snapshot = CatalogSnapshot::default();
        let mut sc = SchemaCatalog::default();
        sc.dependencies.push(CatalogDependency {
            owner: SchemaName::from(billing),
            name: ObjectName::from(caller),
            object_type: ObjectType::Package,
            referenced_owner: Some(SchemaName::from(billing)),
            referenced_name: ObjectName::from(callee),
            referenced_type: Some(ObjectType::Table),
            dependency_kind: CatalogDependencyKind::Reference,
            via_db_link: None,
        });
        snapshot.schemas.insert(SchemaName::from(billing), sc);

        let report = graph.cross_check_with_catalog(&snapshot, &interner);
        assert_eq!(report.summary.kind_mismatches, 1);
        assert_eq!(report.summary.matches, 0);
        assert_eq!(report.summary.our_extras, 0);
        assert_eq!(report.summary.oracle_onlies, 0);
        let mismatch = report
            .mismatches
            .iter()
            .find(|m| matches!(m, CrossCheckMismatch::KindMismatch { .. }))
            .expect("KindMismatch expected");
        match mismatch {
            CrossCheckMismatch::KindMismatch {
                our_kind,
                oracle_kind,
                ..
            } => {
                assert_eq!(our_kind, "Calls");
                assert_eq!(oracle_kind, "REF");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn record_db_link_edge_is_opaque_with_evidence() {
        let mut graph = sample_query_graph();
        let before = graph.edge_count();
        let id = graph.record_db_link_edge(
            EdgeId::new(900),
            NodeId::new(1),
            NodeId::new(2),
            "HR_REMOTE",
            FileId::new(1),
            Span::new(
                FileId::new(1),
                Position::new(3, 1, 40),
                Position::new(3, 20, 60),
            ),
        );
        assert_eq!(id, EdgeId::new(900));
        assert_eq!(graph.edge_count(), before + 1);
        let edge = graph
            .edges
            .iter()
            .find(|e| e.id == EdgeId::new(900))
            .expect("db-link edge present");
        assert_eq!(edge.kind, EdgeKind::DbLink);
        assert_eq!(edge.confidence.level, ConfidenceLevel::Opaque);
        // Evidence is mandatory for non-High edges; the validator
        // must therefore see no MissingEvidence violation.
        let ev = graph.evidence.get(&EdgeId::new(900)).expect("evidence");
        assert_eq!(ev.code, "DEP008");
        assert!(ev.summary.contains("HR_REMOTE"));
        assert!(graph.validate().is_empty(), "{:?}", graph.validate());
    }

    #[test]
    fn record_db_link_edge_notes_link_in_provenance() {
        let mut graph = sample_query_graph();
        graph.record_db_link_edge(
            EdgeId::new(901),
            NodeId::new(1),
            NodeId::new(3),
            "ANALYTICS_LINK",
            FileId::new(1),
            Span::new(
                FileId::new(1),
                Position::new(4, 1, 70),
                Position::new(4, 10, 80),
            ),
        );
        let prov = graph.provenance.get(&EdgeId::new(901)).expect("provenance");
        assert!(prov.notes.iter().any(|n| n.contains("ANALYTICS_LINK")));
    }
}
