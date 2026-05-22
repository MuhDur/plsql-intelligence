#![forbid(unsafe_code)]

//! Privilege and authorization model for PL/SQL analysis.
//!
//! Models authorization-relevant semantics by combining source-code annotations
//! (`AUTHID`, `ACCESSIBLE BY`) with catalog-derived grants and roles.
//!
//! This crate is Layer 2 of the dependency graph — it depends on `plsql-core`
//! and `plsql-catalog`.

mod ambiguity_feed;
mod doctor;
mod model;
mod resolve;

pub use ambiguity_feed::{
    AMBIGUITY_EVIDENCE_CODE, AmbiguityFeedEntry, ambiguity_feed, confidence_ceiling_for,
    downgrade_confidence,
};
pub use doctor::{
    AuthidDistribution, DoctorReasonRow, PrivilegeDoctorReport, PrivilegePosture, doctor_report,
};
pub use model::*;
pub use resolve::*;
