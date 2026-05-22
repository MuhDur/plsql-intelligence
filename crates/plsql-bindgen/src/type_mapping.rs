//! Oracle → Rust type mapping per plan §12.3 (`PLSQL-BG-002`).
//!
//! Centralizes every entry from the §12.3 table so the wrapper emitter
//! (`PLSQL-BG-004` and later) never invents a mapping locally. Unsupported
//! types are surfaced as `BindingDiagnostic`s using the codes catalog from
//! `PLSQL-BG-011`.

use crate::{BindingDiagnostic, BindingDiagnosticCode, DiagnosticSpan, RustTypeRef};

/// Parsed view of an Oracle column / parameter type — the input shape the
/// mapper consumes. The bindings generator produces this from the catalog
/// snapshot's `DataTypeRef`. Keeping the parsed shape distinct from the raw
/// dictionary string lets the mapper reason about precision / scale and
/// custom types without re-running the dictionary regex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OracleType {
    /// `NUMBER(n,0)` and `NUMBER(n,m)`. `precision = None` means the column
    /// was declared without precision (`NUMBER`), which Oracle treats as
    /// "full range" — we conservatively map to `Decimal`.
    Number {
        precision: Option<u32>,
        scale: i32,
    },
    BinaryFloat,
    BinaryDouble,
    Varchar2 {
        length: u32,
    },
    Char {
        length: u32,
    },
    Clob,
    Blob,
    Date,
    Timestamp {
        precision: u32,
    },
    TimestampWithTimeZone {
        precision: u32,
    },
    TimestampWithLocalTimeZone {
        precision: u32,
    },
    IntervalDayToSecond,
    IntervalYearToMonth,
    Raw {
        length: u32,
    },
    /// SQL `BOOLEAN` (Oracle 23ai+).
    SqlBoolean23ai,
    /// PL/SQL `BOOLEAN` parameter (predates SQL BOOLEAN; not always
    /// driver-bindable).
    PlsqlBoolean,
    XmlType,
    /// `JSON` (Oracle 21c+).
    Json21cPlus,
    RefCursor,
    /// Custom user-defined OBJECT type — `owner.name`. Generator emits a
    /// struct with this Rust path.
    ObjectType {
        owner: String,
        name: String,
    },
    NestedTable {
        element: Box<OracleType>,
    },
    Varray {
        element: Box<OracleType>,
    },
    /// Associative array (PL/SQL only). Indexed by `key_type`, valued by
    /// `value_type`. Maps cleanly only when the driver supports it.
    AssociativeArray {
        key_type: Box<OracleType>,
        value_type: Box<OracleType>,
    },
}

/// Outcome of mapping an Oracle type to its Rust counterpart.
///
/// `Supported(RustTypeRef)` carries the Rust path the generator should
/// emit. `UnsupportedConstruct(BindingDiagnostic)` carries the
/// diagnostic the generator should attach to the corresponding routine /
/// column so the user sees both a stable code AND an actionable workaround.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeMapping {
    Supported(RustTypeRef),
    UnsupportedConstruct(BindingDiagnostic),
}

impl TypeMapping {
    /// Convenience constructor — non-nullable Rust type.
    pub fn supported(path: impl Into<String>) -> Self {
        Self::Supported(RustTypeRef {
            path: path.into(),
            nullable: false,
        })
    }

    /// Whether this mapping ended in an unsupported-construct diagnostic.
    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::UnsupportedConstruct(_))
    }
}

/// Driver-capability knobs (`PLSQL-BG-017` / oracle-4nq6).
///
/// Some Oracle constructs are only bindable on certain
/// driver+server combinations. Today only PL/SQL `BOOLEAN` swings on
/// this — 23ai with rust-oracle 0.6+ binds it natively; older OCI or
/// older Oracle either silently widens to `NUMBER(1)` (bad) or
/// refuses (also bad). The mapper takes a [`DriverCapability`] and
/// emits the right diagnostic at code-gen time so the wrapper is
/// always honest about whether the call will work at runtime.
///
/// Future knobs (REF cursor row-shape inference, JSON 21c+ vs CLOB
/// fallback, etc.) land here without breaking the existing call
/// sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriverCapability {
    /// Whether the target driver+server can bind PL/SQL `BOOLEAN`
    /// natively. `false` = generator emits `BG_UNSUPPORTED_BOOLEAN`;
    /// `true` = generator emits a `bool` wrapper that round-trips
    /// through the driver's native PL/SQL boolean binding.
    pub pl_sql_boolean_bindable: bool,
}

impl DriverCapability {
    /// Conservative default: assume PL/SQL `BOOLEAN` is NOT bindable.
    /// Matches the rust-oracle 0.6.x default on Oracle 19c — the
    /// safer posture for older shops. A caller running on 23ai with
    /// a recent OCI flips the bit via [`Self::oracle_23ai`].
    #[must_use]
    pub fn conservative() -> Self {
        Self {
            pl_sql_boolean_bindable: false,
        }
    }

    /// Oracle 23ai + rust-oracle 0.6+ defaults: PL/SQL `BOOLEAN` is
    /// natively bindable.
    #[must_use]
    pub fn oracle_23ai() -> Self {
        Self {
            pl_sql_boolean_bindable: true,
        }
    }
}

impl Default for DriverCapability {
    fn default() -> Self {
        Self::conservative()
    }
}

/// Static map from `OracleType` to either a Rust path (in the wrapper's
/// generated namespace) or a `BindingDiagnostic`. Implements the §12.3
/// table verbatim.
///
/// Backward-compat shim: keeps the historical 3-arg signature.
/// `PLSQL-BG-017` callers should prefer
/// [`map_oracle_type_with_capability`] which honours driver-specific
/// behavior for PL/SQL `BOOLEAN`.
#[must_use]
pub fn map_oracle_type(
    oracle_type: &OracleType,
    span: Option<DiagnosticSpan>,
    routine: Option<&str>,
) -> TypeMapping {
    map_oracle_type_with_capability(oracle_type, span, routine, DriverCapability::conservative())
}

/// Capability-aware variant of [`map_oracle_type`]. Honours
/// [`DriverCapability::pl_sql_boolean_bindable`] for the PL/SQL
/// `BOOLEAN` arm; identical to the legacy function for every other
/// type. (PLSQL-BG-017 / oracle-4nq6.)
#[must_use]
pub fn map_oracle_type_with_capability(
    oracle_type: &OracleType,
    span: Option<DiagnosticSpan>,
    routine: Option<&str>,
    capability: DriverCapability,
) -> TypeMapping {
    if let OracleType::PlsqlBoolean = oracle_type {
        if capability.pl_sql_boolean_bindable {
            return TypeMapping::supported("bool");
        }
    }
    legacy_map(oracle_type, span, routine)
}

fn legacy_map(
    oracle_type: &OracleType,
    span: Option<DiagnosticSpan>,
    routine: Option<&str>,
) -> TypeMapping {
    match oracle_type {
        OracleType::Number { precision, scale } => map_number(*precision, *scale),
        OracleType::BinaryFloat => TypeMapping::supported("f32"),
        OracleType::BinaryDouble => TypeMapping::supported("f64"),
        OracleType::Varchar2 { .. } | OracleType::Char { .. } | OracleType::Clob => {
            TypeMapping::supported("String")
        }
        OracleType::Blob | OracleType::Raw { .. } => TypeMapping::supported("Vec<u8>"),
        OracleType::Date => TypeMapping::supported("crate::oracle_types::OracleDateTime"),
        OracleType::Timestamp { .. } => {
            TypeMapping::supported("crate::oracle_types::OracleTimestamp")
        }
        OracleType::TimestampWithTimeZone { .. } => {
            TypeMapping::supported("crate::oracle_types::OracleTimestampTz")
        }
        OracleType::TimestampWithLocalTimeZone { .. } => {
            TypeMapping::supported("crate::oracle_types::OracleTimestampLtz")
        }
        OracleType::IntervalDayToSecond => TypeMapping::supported("chrono::Duration"),
        OracleType::IntervalYearToMonth => {
            TypeMapping::supported("crate::oracle_types::IntervalYM")
        }
        OracleType::SqlBoolean23ai => TypeMapping::supported("bool"),
        OracleType::PlsqlBoolean => {
            // PL/SQL BOOLEAN binding is driver-dependent. Generator decides
            // per-target whether to emit a `bool` or this diagnostic at
            // emission time; the mapper itself never silently widens.
            TypeMapping::UnsupportedConstruct(BindingDiagnostic::new_unsupported(
                BindingDiagnosticCode::PlSqlBoolean,
                routine.map(String::from),
                span,
            ))
        }
        OracleType::XmlType => TypeMapping::supported("String"),
        OracleType::Json21cPlus => TypeMapping::supported("serde_json::Value"),
        OracleType::RefCursor => {
            TypeMapping::UnsupportedConstruct(BindingDiagnostic::new_unsupported(
                BindingDiagnosticCode::RefCursor,
                routine.map(String::from),
                span,
            ))
        }
        OracleType::ObjectType { owner, name } => TypeMapping::supported(format!(
            "crate::types::{}::{}",
            owner.to_ascii_lowercase(),
            name
        )),
        OracleType::NestedTable { element } => map_collection("Vec", element, span, routine),
        OracleType::Varray { element } => map_collection("Vec", element, span, routine),
        OracleType::AssociativeArray {
            key_type,
            value_type,
        } => map_associative_array(key_type, value_type, span, routine),
    }
}

fn map_number(precision: Option<u32>, scale: i32) -> TypeMapping {
    if scale != 0 {
        return TypeMapping::supported("rust_decimal::Decimal");
    }
    match precision {
        Some(p) if p <= 18 => TypeMapping::supported("i64"),
        _ => TypeMapping::supported("rust_decimal::Decimal"),
    }
}

fn map_collection(
    wrapper: &str,
    element: &OracleType,
    span: Option<DiagnosticSpan>,
    routine: Option<&str>,
) -> TypeMapping {
    let element_mapping = map_oracle_type(element, span.clone(), routine);
    match element_mapping {
        TypeMapping::Supported(rust_type) => TypeMapping::supported(format!(
            "{wrapper}<{element_path}>",
            element_path = rust_type.path
        )),
        TypeMapping::UnsupportedConstruct(diag) => TypeMapping::UnsupportedConstruct(diag),
    }
}

fn map_associative_array(
    key_type: &OracleType,
    value_type: &OracleType,
    span: Option<DiagnosticSpan>,
    routine: Option<&str>,
) -> TypeMapping {
    let key_mapping = map_oracle_type(key_type, span.clone(), routine);
    let value_mapping = map_oracle_type(value_type, span.clone(), routine);
    match (key_mapping, value_mapping) {
        (TypeMapping::Supported(key_type), TypeMapping::Supported(value_type)) => {
            TypeMapping::supported(format!(
                "std::collections::HashMap<{key}, {value}>",
                key = key_type.path,
                value = value_type.path
            ))
        }
        _ => TypeMapping::UnsupportedConstruct(BindingDiagnostic::new_unsupported(
            BindingDiagnosticCode::AssociativeArray,
            routine.map(String::from),
            span,
        )),
    }
}

/// Apply nullability to a mapping: `Option<T>` when nullable, `T` unchanged
/// otherwise. The mapper itself produces `nullable: false`; callers wrap.
#[must_use]
pub fn with_nullable(mapping: TypeMapping, nullable: bool) -> TypeMapping {
    match mapping {
        TypeMapping::Supported(rust_type) if nullable && !rust_type.path.starts_with("Option<") => {
            TypeMapping::Supported(RustTypeRef {
                path: format!("Option<{}>", rust_type.path),
                nullable: true,
            })
        }
        other => other,
    }
}

/// Wrap a mapping in `Defaulted<…>` for parameters with a `DEFAULT` clause.
/// Per plan §12.3, this is independent from nullability:
/// - Nullable & defaulted → `Defaulted<Option<T>>`.
/// - Defaulted only → `Defaulted<T>`.
#[must_use]
pub fn with_defaulted(mapping: TypeMapping, defaulted: bool) -> TypeMapping {
    if !defaulted {
        return mapping;
    }
    match mapping {
        TypeMapping::Supported(rust_type) => TypeMapping::Supported(RustTypeRef {
            path: format!("Defaulted<{}>", rust_type.path),
            nullable: rust_type.nullable,
        }),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_supported(mapping: TypeMapping, expected: &str) {
        match mapping {
            TypeMapping::Supported(rust_type) => assert_eq!(rust_type.path, expected),
            TypeMapping::UnsupportedConstruct(diag) => {
                panic!("expected {expected}, got diagnostic {diag:?}")
            }
        }
    }

    fn assert_unsupported(mapping: TypeMapping, expected_code: &str) {
        match mapping {
            TypeMapping::Supported(rust_type) => {
                panic!("expected unsupported {expected_code}, got {rust_type:?}")
            }
            TypeMapping::UnsupportedConstruct(diag) => assert_eq!(diag.code, expected_code),
        }
    }

    #[test]
    fn number_precision_and_scale_drive_mapping() {
        assert_supported(
            map_oracle_type(
                &OracleType::Number {
                    precision: Some(10),
                    scale: 0,
                },
                None,
                None,
            ),
            "i64",
        );
        assert_supported(
            map_oracle_type(
                &OracleType::Number {
                    precision: Some(18),
                    scale: 0,
                },
                None,
                None,
            ),
            "i64",
        );
        // Past 18 digits → Decimal (i64 max precision).
        assert_supported(
            map_oracle_type(
                &OracleType::Number {
                    precision: Some(19),
                    scale: 0,
                },
                None,
                None,
            ),
            "rust_decimal::Decimal",
        );
        // Non-zero scale → Decimal regardless of precision.
        assert_supported(
            map_oracle_type(
                &OracleType::Number {
                    precision: Some(10),
                    scale: 2,
                },
                None,
                None,
            ),
            "rust_decimal::Decimal",
        );
        // Precisionless NUMBER → conservative Decimal.
        assert_supported(
            map_oracle_type(
                &OracleType::Number {
                    precision: None,
                    scale: 0,
                },
                None,
                None,
            ),
            "rust_decimal::Decimal",
        );
    }

    #[test]
    fn float_types_map_to_native_rust() {
        assert_supported(map_oracle_type(&OracleType::BinaryFloat, None, None), "f32");
        assert_supported(
            map_oracle_type(&OracleType::BinaryDouble, None, None),
            "f64",
        );
    }

    #[test]
    fn string_family_maps_to_string() {
        assert_supported(
            map_oracle_type(&OracleType::Varchar2 { length: 100 }, None, None),
            "String",
        );
        assert_supported(
            map_oracle_type(&OracleType::Char { length: 10 }, None, None),
            "String",
        );
        assert_supported(map_oracle_type(&OracleType::Clob, None, None), "String");
        assert_supported(map_oracle_type(&OracleType::XmlType, None, None), "String");
    }

    #[test]
    fn binary_family_maps_to_vec_u8() {
        assert_supported(map_oracle_type(&OracleType::Blob, None, None), "Vec<u8>");
        assert_supported(
            map_oracle_type(&OracleType::Raw { length: 16 }, None, None),
            "Vec<u8>",
        );
    }

    #[test]
    fn temporal_types_map_to_oracle_wrappers() {
        assert_supported(
            map_oracle_type(&OracleType::Date, None, None),
            "crate::oracle_types::OracleDateTime",
        );
        assert_supported(
            map_oracle_type(&OracleType::Timestamp { precision: 6 }, None, None),
            "crate::oracle_types::OracleTimestamp",
        );
        assert_supported(
            map_oracle_type(
                &OracleType::TimestampWithTimeZone { precision: 6 },
                None,
                None,
            ),
            "crate::oracle_types::OracleTimestampTz",
        );
        assert_supported(
            map_oracle_type(
                &OracleType::TimestampWithLocalTimeZone { precision: 6 },
                None,
                None,
            ),
            "crate::oracle_types::OracleTimestampLtz",
        );
    }

    #[test]
    fn intervals_split_between_chrono_and_custom_type() {
        assert_supported(
            map_oracle_type(&OracleType::IntervalDayToSecond, None, None),
            "chrono::Duration",
        );
        assert_supported(
            map_oracle_type(&OracleType::IntervalYearToMonth, None, None),
            "crate::oracle_types::IntervalYM",
        );
    }

    #[test]
    fn json_and_sql_boolean_map_to_native_types() {
        assert_supported(
            map_oracle_type(&OracleType::SqlBoolean23ai, None, None),
            "bool",
        );
        assert_supported(
            map_oracle_type(&OracleType::Json21cPlus, None, None),
            "serde_json::Value",
        );
    }

    #[test]
    fn plsql_boolean_emits_unsupported_diagnostic() {
        assert_unsupported(
            map_oracle_type(&OracleType::PlsqlBoolean, None, Some("LEGACY_PROC")),
            "BG_UNSUPPORTED_BOOLEAN",
        );
    }

    #[test]
    fn ref_cursor_emits_unsupported_diagnostic() {
        assert_unsupported(
            map_oracle_type(&OracleType::RefCursor, None, Some("FIND_BY_DEPT")),
            "BG_UNSUPPORTED_REF_CURSOR",
        );
    }

    #[test]
    fn object_type_produces_namespaced_struct_path() {
        assert_supported(
            map_oracle_type(
                &OracleType::ObjectType {
                    owner: String::from("BILLING"),
                    name: String::from("AddressT"),
                },
                None,
                None,
            ),
            "crate::types::billing::AddressT",
        );
    }

    #[test]
    fn nested_table_and_varray_map_through_element() {
        assert_supported(
            map_oracle_type(
                &OracleType::NestedTable {
                    element: Box::new(OracleType::Varchar2 { length: 30 }),
                },
                None,
                None,
            ),
            "Vec<String>",
        );
        assert_supported(
            map_oracle_type(
                &OracleType::Varray {
                    element: Box::new(OracleType::BinaryDouble),
                },
                None,
                None,
            ),
            "Vec<f64>",
        );
    }

    #[test]
    fn associative_array_maps_to_hashmap_when_both_sides_supported() {
        assert_supported(
            map_oracle_type(
                &OracleType::AssociativeArray {
                    key_type: Box::new(OracleType::Varchar2 { length: 30 }),
                    value_type: Box::new(OracleType::Number {
                        precision: Some(10),
                        scale: 0,
                    }),
                },
                None,
                None,
            ),
            "std::collections::HashMap<String, i64>",
        );
    }

    #[test]
    fn associative_array_emits_unsupported_diagnostic_when_value_unsupported() {
        assert_unsupported(
            map_oracle_type(
                &OracleType::AssociativeArray {
                    key_type: Box::new(OracleType::Varchar2 { length: 30 }),
                    value_type: Box::new(OracleType::RefCursor),
                },
                None,
                None,
            ),
            "BG_UNSUPPORTED_ASSOC_ARRAY",
        );
    }

    #[test]
    fn nested_table_of_unsupported_element_emits_diagnostic() {
        assert_unsupported(
            map_oracle_type(
                &OracleType::NestedTable {
                    element: Box::new(OracleType::RefCursor),
                },
                None,
                None,
            ),
            "BG_UNSUPPORTED_REF_CURSOR",
        );
    }

    #[test]
    fn with_nullable_wraps_supported_in_option() {
        let mapping = map_oracle_type(&OracleType::Varchar2 { length: 100 }, None, None);
        let nullable = with_nullable(mapping, true);
        assert_supported(nullable, "Option<String>");
    }

    #[test]
    fn with_nullable_is_idempotent_for_already_option_types() {
        let mapping = TypeMapping::Supported(RustTypeRef {
            path: String::from("Option<i64>"),
            nullable: true,
        });
        let again = with_nullable(mapping, true);
        assert_supported(again, "Option<i64>");
    }

    #[test]
    fn with_defaulted_layers_defaulted_around_nullable_correctly() {
        let mapping = map_oracle_type(&OracleType::Varchar2 { length: 100 }, None, None);
        let nullable = with_nullable(mapping, true);
        let defaulted = with_defaulted(nullable, true);
        assert_supported(defaulted, "Defaulted<Option<String>>");
    }

    #[test]
    fn unsupported_mapping_does_not_get_wrapped_in_nullable_or_defaulted() {
        let mapping = map_oracle_type(&OracleType::RefCursor, None, None);
        let after = with_defaulted(with_nullable(mapping, true), true);
        assert!(after.is_unsupported());
    }

    // -----------------------------------------------------------------
    // PLSQL-BG-017 / oracle-4nq6 — BOOLEAN driver-capability tests.
    // -----------------------------------------------------------------

    #[test]
    fn sql_boolean_23ai_always_maps_to_bool_regardless_of_capability() {
        // SQL `BOOLEAN` is the native 23ai+ datatype; binding is
        // driver-version-stable. Both capability modes map to `bool`.
        assert_supported(
            map_oracle_type_with_capability(
                &OracleType::SqlBoolean23ai,
                None,
                None,
                DriverCapability::conservative(),
            ),
            "bool",
        );
        assert_supported(
            map_oracle_type_with_capability(
                &OracleType::SqlBoolean23ai,
                None,
                None,
                DriverCapability::oracle_23ai(),
            ),
            "bool",
        );
    }

    #[test]
    fn plsql_boolean_under_conservative_capability_emits_diagnostic() {
        let mapping = map_oracle_type_with_capability(
            &OracleType::PlsqlBoolean,
            None,
            None,
            DriverCapability::conservative(),
        );
        match mapping {
            TypeMapping::UnsupportedConstruct(diag) => {
                assert_eq!(diag.code, "BG_UNSUPPORTED_BOOLEAN");
                assert!(diag.suggested_workaround.is_some());
                assert!(
                    diag.suggested_workaround
                        .as_deref()
                        .unwrap()
                        .contains("NUMBER(1)")
                );
            }
            other => panic!("expected UnsupportedConstruct, got {other:?}"),
        }
    }

    #[test]
    fn plsql_boolean_under_oracle_23ai_capability_maps_to_bool() {
        // The 23ai driver bit flips the mapping: no diagnostic,
        // wrapper emits a `bool` parameter.
        assert_supported(
            map_oracle_type_with_capability(
                &OracleType::PlsqlBoolean,
                None,
                None,
                DriverCapability::oracle_23ai(),
            ),
            "bool",
        );
    }

    #[test]
    fn legacy_map_oracle_type_uses_conservative_capability_for_back_compat() {
        // The 3-arg shim must keep emitting the diagnostic for
        // existing callers that haven't migrated to the new
        // capability-aware signature.
        let mapping = map_oracle_type(&OracleType::PlsqlBoolean, None, None);
        assert!(mapping.is_unsupported());
    }

    #[test]
    fn driver_capability_default_is_conservative() {
        let default = DriverCapability::default();
        let conservative = DriverCapability::conservative();
        assert_eq!(default, conservative);
        assert!(!default.pl_sql_boolean_bindable);
    }

    #[test]
    fn driver_capability_constructors_are_distinct() {
        // Smoke-test that we'd notice a regression where the two
        // constructors drift to producing the same posture.
        assert_ne!(
            DriverCapability::conservative().pl_sql_boolean_bindable,
            DriverCapability::oracle_23ai().pl_sql_boolean_bindable
        );
    }
}
