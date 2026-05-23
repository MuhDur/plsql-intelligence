//! Struct emission for OBJECT types and records.
//!
//! The bindings generator emits a Rust `struct` for each PL/SQL ADT
//! the analysed corpus exposes:
//!
//! * `CREATE TYPE … AS OBJECT (…)` — a SQL-level object type with
//!   named attributes (`OBJECT-TYPES-REFERENCE.md` Core Model
//!   table, /oracle skill routing). Each attribute becomes a Rust
//!   struct field.
//! * `TYPE … IS RECORD (…)` — a PL/SQL record type declared inside
//!   a package or subprogram. Each field becomes a Rust struct
//!   field, same shape.
//! * `…%ROWTYPE` — synthesised from the row source's columns
//!   (resolved by `plsql_symbols::resolve_anchor`).
//!
//! This module defines the IR shape (`StructBinding`) plus a small
//! builder API the emitter consumes. It does not emit Rust code
//! itself — that work lives downstream of `BindingPlan`.
//!
//! References:
//! * `~/.claude/skills/oracle/OBJECT-TYPES-REFERENCE.md` Core Model
//! * Oracle Object-Relational Developer's Guide
//!   <https://docs.oracle.com/en/database/oracle/oracle-database/26/adobj/about-oracle-objects.html>
//! * PL/SQL Language Reference §13 (records / collections)

use serde::{Deserialize, Serialize};

use crate::RustTypeRef;

/// One Rust struct to emit for a PL/SQL ADT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructBinding {
    /// Original PL/SQL identifier (e.g. `EMPLOYEE_T`, `EMP_REC`,
    /// `EMPLOYEES_ROWTYPE`). Surfaced verbatim so the emitter can
    /// derive its Rust path consistently with the plan-wide naming
    /// convention.
    pub plsql_name: String,
    /// Categorical origin — drives the doc-comment header the
    /// emitter inserts above the struct, and the audit log.
    pub origin: StructOrigin,
    /// Schema-qualified PL/SQL identifier for OBJECT types
    /// (`SCHEMA.NAME`). `None` for records / rowtypes that live
    /// inside a package or subprogram and aren't independently
    /// addressable from SQL.
    pub schema_qualified: Option<String>,
    /// Ordered list of fields. PL/SQL preserves attribute order, so
    /// we do too — the field order participates in object-type
    /// constructor calls.
    pub fields: Vec<StructFieldBinding>,
    /// Constructor signature for OBJECT types (none for records).
    /// `None` indicates the emitter should not generate an explicit
    /// `new(…)` helper because PL/SQL doesn't expose one (records
    /// initialise field-by-field).
    pub constructor: Option<ConstructorBinding>,
}

/// One field of an emitted struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructFieldBinding {
    /// Original PL/SQL field name. The emitter snake-cases this for
    /// the Rust identifier and preserves the original in a
    /// `#[serde(rename = "...")]` attribute so JSON round-trips
    /// against the Oracle wire form unchanged.
    pub plsql_name: String,
    /// Resolved Rust type for the field. Nullability follows
    /// PL/SQL's NOT NULL constraint when the source declaration
    /// carries one.
    pub rust_type: RustTypeRef,
    /// True when the source PL/SQL attribute / column is declared
    /// `NOT NULL`. The emitter uses this to choose between
    /// `T` and `Option<T>` independently of driver round-tripping.
    pub not_null: bool,
}

/// Constructor for an OBJECT type. PL/SQL object types support a
/// system-defined constructor `T(a1, a2, …)` that takes positional
/// arguments in attribute-declaration order. The emitter mirrors
/// this with a Rust `T::new(a1, a2, …)` associated function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructorBinding {
    /// Positional argument list. Names mirror the field names so
    /// the emitter can produce a builder pattern when an attribute
    /// has a non-trivial default expression.
    pub params: Vec<StructFieldBinding>,
}

/// Why this struct was emitted. Drives doc comments and audit
/// metadata; the emitter chooses different header text per origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructOrigin {
    /// SQL-level `CREATE TYPE … AS OBJECT`.
    ObjectType,
    /// PL/SQL `TYPE x IS RECORD (…)`.
    Record,
    /// PL/SQL `…%ROWTYPE` — synthesised from a row source.
    Rowtype,
}

/// Build a `StructBinding` for an OBJECT type from its attribute
/// list. Constructor is synthesised automatically — every OBJECT
/// type has the system-defined constructor.
#[must_use]
pub fn struct_for_object_type(
    plsql_name: impl Into<String>,
    schema_qualified: Option<String>,
    attributes: Vec<StructFieldBinding>,
) -> StructBinding {
    let constructor = Some(ConstructorBinding {
        params: attributes.clone(),
    });
    StructBinding {
        plsql_name: plsql_name.into(),
        origin: StructOrigin::ObjectType,
        schema_qualified,
        fields: attributes,
        constructor,
    }
}

/// Build a `StructBinding` for a PL/SQL record. No constructor —
/// records initialise field-by-field.
#[must_use]
pub fn struct_for_record(
    plsql_name: impl Into<String>,
    fields: Vec<StructFieldBinding>,
) -> StructBinding {
    StructBinding {
        plsql_name: plsql_name.into(),
        origin: StructOrigin::Record,
        schema_qualified: None,
        fields,
        constructor: None,
    }
}

/// Build a `StructBinding` for a `%ROWTYPE` anchor. The caller
/// projects the resolved column list into `StructFieldBinding`s
/// via the type-mapping layer before invoking this.
#[must_use]
pub fn struct_for_rowtype(
    plsql_name: impl Into<String>,
    source_schema_qualified: Option<String>,
    fields: Vec<StructFieldBinding>,
) -> StructBinding {
    StructBinding {
        plsql_name: plsql_name.into(),
        origin: StructOrigin::Rowtype,
        schema_qualified: source_schema_qualified,
        fields,
        constructor: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, path: &str, not_null: bool) -> StructFieldBinding {
        StructFieldBinding {
            plsql_name: name.into(),
            rust_type: RustTypeRef {
                path: path.into(),
                nullable: !not_null,
            },
            not_null,
        }
    }

    #[test]
    fn object_type_struct_has_constructor_with_same_field_order() {
        let s = struct_for_object_type(
            "EMPLOYEE_T",
            Some("HR.EMPLOYEE_T".into()),
            vec![
                field("ID", "i64", true),
                field("NAME", "String", false),
                field("HIRE_DATE", "chrono::NaiveDate", false),
            ],
        );
        assert_eq!(s.origin, StructOrigin::ObjectType);
        assert_eq!(s.schema_qualified.as_deref(), Some("HR.EMPLOYEE_T"));
        assert_eq!(s.fields.len(), 3);
        let ctor = s.constructor.expect("OBJECT types ship a constructor");
        assert_eq!(ctor.params.len(), 3);
        assert_eq!(ctor.params[0].plsql_name, "ID");
        assert_eq!(ctor.params[2].plsql_name, "HIRE_DATE");
    }

    #[test]
    fn record_struct_has_no_constructor() {
        let s = struct_for_record(
            "EMP_REC",
            vec![field("ID", "i64", true), field("SAL", "f64", false)],
        );
        assert_eq!(s.origin, StructOrigin::Record);
        assert!(s.constructor.is_none());
        assert!(s.schema_qualified.is_none());
    }

    #[test]
    fn rowtype_struct_marks_origin_and_keeps_source_schema() {
        let s = struct_for_rowtype(
            "EMPLOYEES_ROWTYPE",
            Some("HR.EMPLOYEES".into()),
            vec![field("ID", "i64", true), field("SALARY", "f64", false)],
        );
        assert_eq!(s.origin, StructOrigin::Rowtype);
        assert_eq!(s.schema_qualified.as_deref(), Some("HR.EMPLOYEES"));
        assert!(s.constructor.is_none());
    }

    #[test]
    fn not_null_flag_round_trips() {
        let s = struct_for_record(
            "REC",
            vec![
                field("REQUIRED", "i64", true),
                field("OPTIONAL", "String", false),
            ],
        );
        assert!(s.fields[0].not_null);
        assert!(!s.fields[1].not_null);
        assert!(!s.fields[0].rust_type.nullable);
        assert!(s.fields[1].rust_type.nullable);
    }

    #[test]
    fn struct_binding_serialises_round_trip() {
        let s = struct_for_object_type("X", None, vec![field("A", "i64", true)]);
        let json = serde_json::to_string(&s).unwrap();
        let back: StructBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        // origin field uses snake_case in the wire form.
        assert!(json.contains("\"origin\":\"object_type\""));
    }

    #[test]
    fn empty_field_list_is_allowed() {
        // An OBJECT type with no attributes is legal Oracle (think
        // marker types) — the emitter should be able to represent it.
        let s = struct_for_object_type("MARKER_T", None, vec![]);
        assert!(s.fields.is_empty());
        let ctor = s.constructor.unwrap();
        assert!(ctor.params.is_empty());
    }
}
