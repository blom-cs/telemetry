//! @file error.rs
//!
//! @brief
//! Stable [`TelemetryError`] codes for logging install and file I/O.
//!
//! @details
//! Variants match the returns from [`super::telemetry::Telemetry::new`] and
//! the rolling writer. Discriminants are stable (`1..=5`). This type does
//! not implement [`std::fmt::Display`].
//!
//! @responsibilities
//! - Name empty prefix, empty directory, subscriber install, directory
//!   create, and log-file open failures.
//! - Keep numeric codes stable.
//!
//! @behaviour
//! Fail closed. Unit tests do not open a live display or GPU.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// @details
/// Fail-closed telemetry errors. Discriminants are stable (`1..=5`).
///
/// Use [`TelemetryError::code`] in logs. Map each install or file failure
/// to the matching variant.
///
/// @responsibilities
/// - Identify prefix, directory, subscriber, mkdir, and open failures.
///
/// @behaviour
/// Hold data only. Methods on this type own behaviour.
pub enum TelemetryError {
    /// Prefix string is empty.
    EmptyPrefix = 1,
    /// Log directory path is empty.
    EmptyDirectory = 2,
    /// Tracing subscriber is already installed.
    SubscriberInstallFailed = 3,
    /// Creating the log directory failed.
    CreateDirectoryFailed = 4,
    /// Opening or creating the log file failed.
    OpenLogFileFailed = 5,
}

impl TelemetryError {
    /// @details
    /// Stable numeric code for diagnostics.
    ///
    /// Equals the discriminant (`1..=5`).
    ///
    /// @responsibilities
    /// - Stable numeric code for diagnostics.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `u8`. This method does not fail.
    pub fn code(&self) -> u8 {
        *self as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /***
     * @test-list
     * 1. EmptyPrefix code.
     * 2. EmptyDirectory code.
     * 3. SubscriberInstallFailed code.
     * 4. CreateDirectoryFailed code.
     * 5. OpenLogFileFailed code.
     */

    #[test]
    fn empty_prefix_code() {
        let err = TelemetryError::EmptyPrefix;

        assert_eq!(err, TelemetryError::EmptyPrefix);
        assert_eq!(err.code(), 1);
    }

    #[test]
    fn empty_directory_code() {
        let err = TelemetryError::EmptyDirectory;

        assert_eq!(err, TelemetryError::EmptyDirectory);
        assert_eq!(err.code(), 2);
    }

    #[test]
    fn subscriber_install_failed_code() {
        let err = TelemetryError::SubscriberInstallFailed;

        assert_eq!(err, TelemetryError::SubscriberInstallFailed);
        assert_eq!(err.code(), 3);
    }

    #[test]
    fn create_directory_failed_code() {
        let err = TelemetryError::CreateDirectoryFailed;

        assert_eq!(err, TelemetryError::CreateDirectoryFailed);
        assert_eq!(err.code(), 4);
    }

    #[test]
    fn open_log_file_failed_code() {
        let err = TelemetryError::OpenLogFileFailed;

        assert_eq!(err, TelemetryError::OpenLogFileFailed);
        assert_eq!(err.code(), 5);
    }
}
