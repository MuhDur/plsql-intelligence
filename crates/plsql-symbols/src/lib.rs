#![forbid(unsafe_code)]

//! Symbol table and name resolution for `plsql-intelligence`.
//!
//! Layer 2 of the dependency graph (plan.md §9). This crate hosts the
//! [`DeclTable`] — the canonical store of every `Declaration` discovered
//! during analysis — and the reference resolution strategies that map
//! name-use sites to those declarations.
//!
//! Introduces [`DeclTable`] plus a registration API the IR lowering
//! passes feed into. Reference-resolution strategies live alongside;
//! overload resolution and catalog cross-checking are layered on top.

mod catalog_feed;
mod db_link;
mod doctor;
mod dynamic_sql;
mod dynamic_sql_confidence;
mod dynamic_sql_shape;
mod overload;
mod plscope_diff;
mod report;
mod resolve_anchor;
mod resolve_refs;
mod table;

pub use catalog_feed::{
    CatalogBackedAnchor, CatalogColumnFact, CatalogIndexedColumnFact, CatalogResolutionSource,
    CatalogSynonymFact, follow_catalog_synonym, resolve_anchor_with_catalog,
    resolve_catalog_overload,
};
pub use db_link::{DbLinkReference, DbLinkRegistry};
pub use doctor::{
    StrategyHistogramRow, SymbolPosture, SymbolResolutionDoctorReport, UnresolvedHistogramRow,
    doctor_report,
};
pub use dynamic_sql::{
    CandidateObject, DbmsAssertCall, DynamicSqlEvidence, OpacityReason, recognise_dynamic_sql,
};
pub use dynamic_sql_confidence::{dynamic_sql_confidence_level, score_dynamic_sql_edge};
pub use dynamic_sql_shape::{EnrichedDynamicSql, enrich_dynamic_sql};
pub use overload::{
    BindFailure, CallArg, OverloadResolution, ParamSig, RoutineSignature, resolve_overload,
};
pub use plscope_diff::{
    AgreedReference, MismatchedReference, OurReference, PlScopeDiff, PlScopeDiffSummary,
    PlScopeReference, diff_plscope,
};
pub use report::{
    Evidence, ResolutionOutcome, ResolutionReport, StrategyResult, StrategyTraceEntry,
    confidence_for_strategy, report_from_resolved,
};
pub use resolve_anchor::{AnchorKind, AnchorResolutionFailure, ResolvedAnchor, resolve_anchor};
pub use resolve_refs::{
    ResolutionScope, ResolutionStrategy, ResolvedRef, UnresolvedReason, resolve_reference,
};
pub use table::{DeclEntry, DeclTable};
