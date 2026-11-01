//! @file hour.rs
//!
//! @brief
//! UTC hour buckets and two-digit event timestamps.
//!
//! @details
//! [`HourKey`] names rolling files as `{prefix}-{yyyy-MM-dd-HH}.log`.
//! [`UtcStamp`] truncates UTC time to two fractional-second digits for
//! tracing lines. Neither type names a product.
//!
//! @responsibilities
//! - Format the hourly file-name key.
//! - Trim and write UTC timestamps with two subsecond digits.
//!
//! @behaviour
//! Fail closed. Unit tests do not open a live display or GPU.

use std::fmt;
use time::OffsetDateTime;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

/// Fractional second digits kept on UTC event timestamps.
const SUBSEC_DIGITS: u32 = 2;
/// Nanoseconds per remaining subsecond digit (`10^(9 - SUBSEC_DIGITS)`).
const SUBSEC_TICK_NS: u32 = 1_000_000_000 / 10u32.pow(SUBSEC_DIGITS);

#[derive(Debug, Clone, PartialEq, Eq)]
/// @details
/// UTC hour stamp used in `{prefix}-{hour}.log`.
///
/// @responsibilities
/// - Hold the `yyyy-MM-dd-HH` text for one UTC hour.
///
/// @behaviour
/// Hold data only. Methods on this type own behaviour.
pub(crate) struct HourKey {
    text: String,
}

#[derive(Clone, Copy, Debug)]
/// @details
/// UTC clock for tracing timestamps, truncated to [`SUBSEC_DIGITS`].
///
/// @responsibilities
/// - Trim nanoseconds to two fractional digits.
/// - Write RFC3339 UTC with a trailing space.
///
/// @behaviour
/// Hold data only. Methods on this type own behaviour.
pub(crate) struct UtcStamp;

impl HourKey {
    /// @details
    /// Build a key from the current UTC hour.
    ///
    /// @responsibilities
    /// - Build a key from the current UTC hour.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `Self`. This method does not fail.
    pub(crate) fn now_utc() -> Self {
        Self::from_time(OffsetDateTime::now_utc())
    }

    /// @details
    /// Build a key from an explicit UTC time.
    ///
    /// @responsibilities
    /// - Format year, month, day, and hour as `yyyy-MM-dd-HH`.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `Self`. This method does not fail.
    pub(crate) fn from_time(now: OffsetDateTime) -> Self {
        let text = format!(
            "{:04}-{:02}-{:02}-{:02}",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour()
        );

        Self::from_text(text)
    }

    /// @details
    /// Wrap already-formatted hour-key text.
    ///
    /// @responsibilities
    /// - Wrap already-formatted hour-key text.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `Self`. This method does not fail.
    pub(crate) fn from_text(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// @details
    /// Borrow the `yyyy-MM-dd-HH` text.
    ///
    /// @responsibilities
    /// - Borrow the `yyyy-MM-dd-HH` text.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `&str`. This method does not fail.
    pub(crate) fn as_str(&self) -> &str {
        self.text.as_str()
    }

    /// @details
    /// Build `{prefix}-{hour}.log` with prefix first.
    ///
    /// @responsibilities
    /// - Build `{prefix}-{hour}.log` with prefix first.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `String`. This method does not fail.
    pub(crate) fn file_name(prefix: &str, hour: &HourKey) -> String {
        format!("{}-{}.log", prefix, hour.text)
    }
}

impl UtcStamp {
    /// @details
    /// Truncate nanoseconds to two fractional-second digits.
    ///
    /// @responsibilities
    /// - Truncate nanoseconds to two fractional-second digits.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `OffsetDateTime`. This method does not fail. A replace failure leaves `now`
    /// unchanged.
    pub(crate) fn trim(now: OffsetDateTime) -> OffsetDateTime {
        let ns = now.nanosecond();
        let trimmed = ns / SUBSEC_TICK_NS * SUBSEC_TICK_NS;
        match now.replace_nanosecond(trimmed) {
            Ok(time) => time,
            Err(_) => now,
        }
    }

    /// @details
    /// Write RFC3339 UTC with two fractional digits and a trailing space.
    ///
    /// @responsibilities
    /// - Write RFC3339 UTC with two fractional digits and a trailing space.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// On success, returns `Ok(())`. On error, returns [`fmt::Error`] when the writer cannot accept
    /// the stamp.
    pub(crate) fn write_to(now: OffsetDateTime, writer: &mut impl fmt::Write) -> fmt::Result {
        let now = Self::trim(now);
        let hundredths = now.nanosecond() / SUBSEC_TICK_NS;

        write!(
            writer,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:02}Z ",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
            hundredths
        )
    }
}

impl FormatTime for UtcStamp {
    /// @details
    /// Write the current UTC time for a tracing event.
    ///
    /// @responsibilities
    /// - Write the current UTC time for a tracing event.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// On success, returns `Ok(())`. On error, returns [`fmt::Error`] when the writer cannot accept
    /// the stamp.
    fn format_time(&self, writer: &mut Writer<'_>) -> fmt::Result {
        Self::write_to(OffsetDateTime::now_utc(), writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /***
     * @test-list
     * 1. from_time maps unix epoch to the zero hour key.
     * 2. now_utc is at least as long as a one-digit hour key.
     * 3. file_name puts prefix first and ends in .log.
     * 4. trim keeps two subsecond digits.
     * 5. write_to prints RFC3339 with two fractional digits and a trailing space.
     */

    /// @details
    /// Unix epoch with `nanos` subseconds for stamp tests.
    ///
    /// @responsibilities
    /// - Unix epoch with `nanos` subseconds for stamp tests.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `OffsetDateTime`. This method does not fail in these tests.
    fn epoch_with_nanos(nanos: u32) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(0)
            .expect("epoch")
            .replace_nanosecond(nanos)
            .expect("nanos")
    }

    #[test]
    fn from_time_maps_unix_epoch() {
        let epoch = OffsetDateTime::from_unix_timestamp(0).expect("epoch");
        let key = HourKey::from_time(epoch);

        assert_eq!(key.as_str(), "1970-01-01-00");
    }

    #[test]
    fn now_utc_has_hour_key_length() {
        let key = HourKey::now_utc();

        assert!(key.as_str().len() >= "2026-07-31-0".len());
    }

    #[test]
    fn file_name_puts_prefix_first() {
        let key = HourKey::from_time(OffsetDateTime::from_unix_timestamp(0).expect("epoch"));
        let name = HourKey::file_name("app", &key);

        assert_eq!(name, "app-1970-01-01-00.log");
    }

    #[test]
    fn trim_keeps_two_subsecond_digits() {
        let trimmed = UtcStamp::trim(epoch_with_nanos(123_456_789));

        assert_eq!(trimmed.nanosecond(), 120_000_000);
    }

    #[test]
    fn write_to_prints_two_fractional_digits() {
        let mut out = String::new();
        UtcStamp::write_to(epoch_with_nanos(123_456_789), &mut out).expect("write");

        assert_eq!(out, "1970-01-01T00:00:00.12Z ");
    }
}
