//! Oracle date/time wrapper types with a configurable backend
//! (`PLSQL-BG-016`).
//!
//! Plan §12.3 maps Oracle's temporal types to dedicated Rust wrappers
//! instead of eagerly normalizing into `chrono` types. The reason: Oracle's
//! `TIMESTAMP WITH LOCAL TIME ZONE` is a session-local value; silently
//! mapping it to `chrono::Local` in generated server code is a foot-gun
//! — different sessions see different wall-clock values for the same row.
//!
//! Backend choice (which underlying library actually stores the value) is
//! configurable per generated wrapper crate via [`DateTimeBackend`]:
//!
//! - [`DateTimeBackend::Chrono`] — default; uses `chrono::NaiveDateTime` /
//!   `chrono::DateTime<chrono::FixedOffset>`. Microsecond resolution.
//! - [`DateTimeBackend::Time`] — uses `time::PrimitiveDateTime` /
//!   `time::OffsetDateTime`. Nanosecond resolution.
//! - [`DateTimeBackend::Strings`] — keeps the raw Oracle text. Lossless
//!   pass-through; downstream code converts as needed.
//!
//! The wrapper types preserve the Oracle semantics literally:
//! - `OracleDateTime` — `DATE`: date + time, no fractional seconds, no
//!   timezone. Calling `to_chrono` widens to `NaiveDateTime` with zeroed
//!   fractional seconds.
//! - `OracleTimestamp` — `TIMESTAMP(p)`: date + time + fractional seconds
//!   up to driver-supported precision, no timezone.
//! - `OracleTimestampTz` — `TIMESTAMP WITH TIME ZONE`: preserves the
//!   offset/region. Opt-in `into_utc()` performs the normalization
//!   explicitly.
//! - `OracleTimestampLtz` — `TIMESTAMP WITH LOCAL TIME ZONE`: documented
//!   as "session-local"; the type carries the session offset alongside
//!   the timestamp so server-side rendering is explicit.
//!
//! Per plan §12.3 footnote, none of these types implements `From<chrono::Local>`
//! or any equivalent — converting to a wall-clock-local representation is
//! always opt-in.

use serde::{Deserialize, Serialize};

/// Which underlying library the generated wrappers should use.
///
/// The bindings generator records this choice in `.plsql-bindgen.toml` so
/// every emitted wrapper crate is consistent.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateTimeBackend {
    #[default]
    Chrono,
    Time,
    /// Lossless pass-through — keep the raw Oracle textual representation
    /// (e.g. `'2026-05-01 13:14:15.123456'` / `'2026-05-01 13:14:15.123456 +02:00'`).
    Strings,
}

impl DateTimeBackend {
    /// Stable identifier the generator writes into `.plsql-bindgen.toml`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chrono => "chrono",
            Self::Time => "time",
            Self::Strings => "strings",
        }
    }

    /// Whether this backend preserves sub-microsecond fractional seconds.
    /// Oracle 23ai+ supports `TIMESTAMP(9)`; only `Time` and `Strings` can
    /// represent it without loss.
    #[must_use]
    pub fn preserves_nanoseconds(self) -> bool {
        matches!(self, Self::Time | Self::Strings)
    }
}

/// `DATE`: date + time, no fractional seconds, no timezone.
///
/// Stored as the (date, time-of-day) pair in microseconds since
/// `1970-01-01T00:00:00`. The bindings generator chooses how this serializes
/// based on [`DateTimeBackend`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct OracleDateTime {
    /// Unix-epoch seconds (UTC interpretation is the user's responsibility
    /// per Oracle's DATE semantics — DATE itself has no timezone).
    pub epoch_seconds: i64,
}

impl OracleDateTime {
    /// Build from a unix-epoch second count.
    #[must_use]
    pub fn from_unix_seconds(epoch_seconds: i64) -> Self {
        Self { epoch_seconds }
    }
}

/// `TIMESTAMP(p)`: date + time + fractional seconds, no timezone.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct OracleTimestamp {
    pub epoch_seconds: i64,
    /// Fractional seconds in nanoseconds.
    pub nanoseconds: u32,
    /// Declared precision (`TIMESTAMP(p)`, p in 0..=9). When the backend
    /// can't represent the requested precision, the wrapper rounds.
    pub precision: u8,
}

/// `TIMESTAMP WITH TIME ZONE`: preserves offset/region.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct OracleTimestampTz {
    pub epoch_seconds: i64,
    pub nanoseconds: u32,
    pub precision: u8,
    /// Offset from UTC in seconds. `0` represents UTC.
    pub offset_seconds: i32,
}

impl OracleTimestampTz {
    /// Normalize the timestamp into UTC. The wrapper type itself never does
    /// this silently — see plan §12.3 ("never blindly mapped to
    /// chrono::Local in generated server code"); callers opt in.
    #[must_use]
    pub fn into_utc(self) -> Self {
        Self {
            offset_seconds: 0,
            ..self
        }
    }
}

/// `TIMESTAMP WITH LOCAL TIME ZONE`: session-local; the type carries the
/// session offset so server-side rendering is explicit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct OracleTimestampLtz {
    pub epoch_seconds: i64,
    pub nanoseconds: u32,
    pub precision: u8,
    /// Session offset captured at row construction. Different sessions can
    /// observe different values from the same row (Oracle's documented
    /// LTZ semantics); the wrapper records the session offset explicitly so
    /// server-side rendering doesn't lose context.
    pub session_offset_seconds: i32,
}

/// `INTERVAL YEAR TO MONTH` — Rust has no first-class equivalent.
/// Custom type carrying the literal `(years, months)` pair.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct IntervalYM {
    pub years: i32,
    pub months: i32,
}

impl IntervalYM {
    #[must_use]
    pub fn new(years: i32, months: i32) -> Self {
        Self { years, months }
    }

    /// Total month count `years*12 + months`. Convenient for arithmetic.
    #[must_use]
    pub fn total_months(self) -> i32 {
        self.years.saturating_mul(12).saturating_add(self.months)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_str_round_trips() {
        for backend in [
            DateTimeBackend::Chrono,
            DateTimeBackend::Time,
            DateTimeBackend::Strings,
        ] {
            let label = backend.as_str();
            let parsed: DateTimeBackend =
                serde_json::from_value(serde_json::Value::String(label.into())).expect("parse");
            assert_eq!(parsed, backend);
        }
    }

    #[test]
    fn only_time_and_strings_preserve_nanos() {
        assert!(!DateTimeBackend::Chrono.preserves_nanoseconds());
        assert!(DateTimeBackend::Time.preserves_nanoseconds());
        assert!(DateTimeBackend::Strings.preserves_nanoseconds());
    }

    #[test]
    fn oracle_date_time_from_unix_seconds() {
        let dt = OracleDateTime::from_unix_seconds(1_777_641_255);
        assert_eq!(dt.epoch_seconds, 1_777_641_255);
    }

    #[test]
    fn oracle_timestamp_tz_into_utc_zeros_offset() {
        let original = OracleTimestampTz {
            epoch_seconds: 1_777_641_255,
            nanoseconds: 123_456_789,
            precision: 9,
            offset_seconds: 3600,
        };
        let utc = original.into_utc();
        assert_eq!(utc.epoch_seconds, 1_777_641_255);
        assert_eq!(utc.offset_seconds, 0);
        assert_eq!(utc.nanoseconds, 123_456_789);
        assert_eq!(utc.precision, 9);
    }

    #[test]
    fn oracle_timestamp_ltz_keeps_session_offset_for_explicit_rendering() {
        let row = OracleTimestampLtz {
            epoch_seconds: 1_777_641_255,
            nanoseconds: 0,
            precision: 6,
            session_offset_seconds: -7 * 3600,
        };
        // The wrapper never silently normalizes; the session offset is
        // preserved so server code can render against it explicitly.
        assert_eq!(row.session_offset_seconds, -7 * 3600);
    }

    #[test]
    fn interval_ym_total_months_handles_negative_intervals() {
        assert_eq!(IntervalYM::new(2, 5).total_months(), 29);
        assert_eq!(IntervalYM::new(-1, -3).total_months(), -15);
        // Saturates on overflow.
        let huge = IntervalYM::new(i32::MAX, 0);
        assert_eq!(huge.total_months(), i32::MAX);
    }

    #[test]
    fn timestamp_precision_is_preserved_in_struct() {
        let ts = OracleTimestamp {
            epoch_seconds: 0,
            nanoseconds: 123_000_000,
            precision: 3,
        };
        assert_eq!(ts.precision, 3);
        assert_eq!(ts.nanoseconds, 123_000_000);
    }
}
