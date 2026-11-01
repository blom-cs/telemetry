//! @file telemetry.rs
//!
//! @brief
//! Process-wide tracing install and log macros.
//!
//! @details
//! Construct once in `main` and keep the value alive for the process:
//!
//! ```
//! use telemetry::telemetry::{Telemetry, TelemetryError};
//!
//! # fn main() -> Result<(), TelemetryError> {
//! let _telemetry = Telemetry::new("app", "../documentation/logs")?;
//! telemetry::telemetry_info!("telemetry ready");
//! telemetry::telemetry_warn!("something odd");
//! telemetry::telemetry_error!(code = 1u32, "failure");
//! # Ok(())
//! # }
//! ```
//!
//! Log files: `{prefix}-<yyyy-MM-dd-HH>.log` under the given directory (hourly UTC).
//! Level defaults to INFO; override with `RUST_LOG`. Second init fails closed.
//!
//! @responsibilities
//! - Validate prefix and directory.
//! - Open the rolling writer and install stdout plus file layers.
//! - Export [`telemetry_info!`], [`telemetry_warn!`], and [`telemetry_error!`].
//!
//! @behaviour
//! Fail closed. Unit tests do not open a live display or GPU.

use std::path::Path;

use crate::prefix::EventPrefix;
use crate::rolling::RollingLog;
use tracing_appender::non_blocking;
use tracing_subscriber::fmt as tracing_fmt;
use tracing_subscriber::{EnvFilter, prelude::*};

pub use crate::error::TelemetryError;

#[derive(Debug)]
/// @details
/// Process-wide tracing subscriber owner. Keep alive for the process
/// lifetime so the worker guard flushes.
///
/// @responsibilities
/// - Own the non-blocking file-writer guard.
///
/// @behaviour
/// Hold data only. Methods on this type own behaviour.
pub struct Telemetry {
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

impl Telemetry {
    /// @details
    /// Install process-wide logging under `log_directory` (once per process).
    ///
    /// `prefix` is written in brackets at the start of every log line and is
    /// the stem of the rolling file name.
    ///
    /// @responsibilities
    /// - Reject an empty prefix or directory.
    /// - Open the rolling writer and install the subscriber.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(Self)`. On error, returns [`TelemetryError::EmptyPrefix`],
    /// [`TelemetryError::EmptyDirectory`], [`TelemetryError::CreateDirectoryFailed`],
    /// [`TelemetryError::OpenLogFileFailed`], or [`TelemetryError::SubscriberInstallFailed`].
    pub fn new(prefix: &str, log_directory: impl AsRef<Path>) -> Result<Self, TelemetryError> {
        if prefix.is_empty() {
            return Err(TelemetryError::EmptyPrefix);
        }
        let dir = log_directory.as_ref();
        if dir.as_os_str().is_empty() {
            return Err(TelemetryError::EmptyDirectory);
        }
        let rolling = RollingLog::open(prefix, dir)?;
        let (non_blocking_file, guard) = non_blocking(rolling);
        let format = EventPrefix::from_arg(prefix);
        Self::install(format, non_blocking_file)?;

        Ok(Self { _guard: guard })
    }

    /// @details
    /// Install stdout and file fmt layers with `format`.
    ///
    /// @responsibilities
    /// - Install stdout and file fmt layers with `format`.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(())`. On error, returns [`TelemetryError::SubscriberInstallFailed`]
    /// when a subscriber is already installed.
    fn install(
        format: EventPrefix,
        file: tracing_appender::non_blocking::NonBlocking,
    ) -> Result<(), TelemetryError> {
        match tracing_subscriber::registry()
            .with(
                tracing_fmt::layer()
                    .event_format(format.clone())
                    .with_writer(std::io::stdout)
                    .with_ansi(true),
            )
            .with(
                tracing_fmt::layer()
                    .event_format(format)
                    .with_writer(file)
                    .with_ansi(false),
            )
            .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
            .try_init()
        {
            Ok(()) => Ok(()),
            Err(_) => Err(TelemetryError::SubscriberInstallFailed),
        }
    }
}

impl Drop for Telemetry {
    /// @details
    /// Log shutdown so the worker guard can flush.
    ///
    /// @responsibilities
    /// - Log shutdown so the worker guard can flush.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns nothing. This method does not fail.
    fn drop(&mut self) {
        tracing::info!("Telemetry shutting down - flushing logs");
    }
}

/// @details
/// Log an INFO event through the global tracing subscriber.
///
/// @responsibilities
/// - Log an INFO event through the global tracing subscriber.
///
/// @behaviour
/// Forwards to [`tracing::info`].
#[macro_export]
macro_rules! telemetry_info {
    ($($arg:tt)+) => {
        tracing::info!($($arg)+)
    };
}

/// @details
/// Log a WARN event through the global tracing subscriber.
///
/// @responsibilities
/// - Log a WARN event through the global tracing subscriber.
///
/// @behaviour
/// Forwards to [`tracing::warn`].
#[macro_export]
macro_rules! telemetry_warn {
    ($($arg:tt)+) => {
        tracing::warn!($($arg)+)
    };
}

/// @details
/// Log an ERROR event through the global tracing subscriber.
///
/// @responsibilities
/// - Log an ERROR event through the global tracing subscriber.
///
/// @behaviour
/// Forwards to [`tracing::error`].
#[macro_export]
macro_rules! telemetry_error {
    ($($arg:tt)+) => {
        tracing::error!($($arg)+)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    /***
     * @test-list
     * 1. Telemetry::new succeeds once then fails closed.
     * 2. Empty prefix is EmptyPrefix.
     * 3. Empty directory is EmptyDirectory.
     * 4. Create directory failure maps to CreateDirectoryFailed.
     */

    #[test]
    fn telemetry_init_succeeds_once() {
        let first = Telemetry::new("app", "../tests/logs");
        telemetry_info!("Test info message.");
        telemetry_warn!("Test warn message.");
        telemetry_error!("Test error message.");
        let second = Telemetry::new("app", "../tests/logs");

        assert!(first.is_ok());
        assert_eq!(second.err(), Some(TelemetryError::SubscriberInstallFailed));
    }

    #[test]
    fn empty_prefix_is_empty_prefix() {
        let result = Telemetry::new("", "../tests/logs");

        assert_eq!(result.err(), Some(TelemetryError::EmptyPrefix));
    }

    #[test]
    fn empty_directory_is_empty_directory() {
        let result = Telemetry::new("app", "");

        assert_eq!(result.err(), Some(TelemetryError::EmptyDirectory));
    }

    #[test]
    fn create_directory_failure_maps() {
        let blocker = std::env::temp_dir().join(format!(
            "tel-block-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&blocker, b"x").expect("blocker");
        let result = Telemetry::new("app", blocker.join("child"));

        assert_eq!(result.err(), Some(TelemetryError::CreateDirectoryFailed));
    }
}
