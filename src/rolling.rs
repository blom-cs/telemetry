//! @file rolling.rs
//!
//! @brief
//! Hourly rolling log file writer.
//!
//! @details
//! [`RollingLog`] is a cloneable [`MakeWriter`]. It creates
//! `{prefix}-{yyyy-MM-dd-HH}.log` under the given directory, rotates when
//! the UTC hour changes, and maps mkdir or open failures to
//! [`TelemetryError`].
//!
//! @responsibilities
//! - Create the directory and open the current hour file.
//! - Rotate, write, and flush under a mutex.
//! - Map poison and I/O failures to [`io::Error`] on the write path.
//!
//! @behaviour
//! Fail closed. Unit tests do not open a live display or GPU.

use std::fs::{OpenOptions, create_dir_all};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::error::TelemetryError;
use crate::hour::HourKey;
use tracing_subscriber::fmt::MakeWriter;

/// @details
/// Mutex payload: prefix, directory, current hour, and open file.
///
/// @responsibilities
/// - Hold prefix, directory, current hour, and the open file.
///
/// @behaviour
/// Hold data only. Methods on this type own behaviour.
struct Inner {
    prefix: String,
    dir: PathBuf,
    hour_key: String,
    file: Box<dyn Write + Send>,
}

impl Inner {
    /// @details
    /// Open the current UTC hour file under `dir`.
    ///
    /// @responsibilities
    /// - Open the current UTC hour file under `dir`.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(Self)`. On error, returns [`TelemetryError::CreateDirectoryFailed`]
    /// or [`TelemetryError::OpenLogFileFailed`].
    fn open(prefix: &str, dir: &Path) -> Result<Self, TelemetryError> {
        let hour = HourKey::now_utc();

        Self::open_hour(prefix, dir, hour.as_str())
    }

    /// @details
    /// Create `dir` and open `{prefix}-{hour_key}.log`.
    ///
    /// @responsibilities
    /// - Create `dir` and open `{prefix}-{hour_key}.log`.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(Self)`. On error, returns [`TelemetryError::CreateDirectoryFailed`]
    /// or [`TelemetryError::OpenLogFileFailed`].
    fn open_hour(prefix: &str, dir: &Path, hour_key: &str) -> Result<Self, TelemetryError> {
        create_dir_all(dir).map_err(|_| TelemetryError::CreateDirectoryFailed)?;
        let path = Self::path(prefix, dir, hour_key);
        let file = Self::open_file(&path).map_err(|_| TelemetryError::OpenLogFileFailed)?;

        Ok(Self {
            prefix: String::from(prefix),
            dir: dir.to_path_buf(),
            hour_key: String::from(hour_key),
            file,
        })
    }

    /// @details
    /// Join `dir` with `{prefix}-{hour_key}.log`.
    ///
    /// @responsibilities
    /// - Join `dir` with `{prefix}-{hour_key}.log`.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `PathBuf`. This method does not fail.
    fn path(prefix: &str, dir: &Path, hour_key: &str) -> PathBuf {
        dir.join(HourKey::file_name(prefix, &HourKey::from_text(hour_key)))
    }

    /// @details
    /// Open `path` for append, creating it when missing.
    ///
    /// @responsibilities
    /// - Open `path` for append, creating it when missing.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(Box<dyn Write + Send>)`. On error, returns [`io::Error`] when create
    /// or open fails.
    fn open_file(path: &Path) -> io::Result<Box<dyn Write + Send>> {
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(file) => Ok(Box::new(file)),
            Err(err) => Err(err),
        }
    }

    /// @details
    /// Rotate to the current UTC hour when the stored key is stale.
    ///
    /// @responsibilities
    /// - Rotate to the current UTC hour when the stored key is stale.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(())`. On error, returns [`io::Error`] when the next hour file cannot
    /// be opened.
    fn rotate_if_needed(&mut self) -> io::Result<()> {
        self.rotate_to(String::from(HourKey::now_utc().as_str()))
    }

    /// @details
    /// Open the file for `key` when it differs from the stored hour.
    ///
    /// @responsibilities
    /// - Open the file for `key` when it differs from the stored hour.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(())`. On error, returns [`io::Error`] when the next hour file cannot
    /// be opened.
    fn rotate_to(&mut self, key: String) -> io::Result<()> {
        if key == self.hour_key {
            return Ok(());
        }
        let path = Self::path(&self.prefix, &self.dir, &key);
        self.file = Self::open_file(&path)?;
        self.hour_key = key;

        Ok(())
    }

    /// @details
    /// Rotate if needed, then write `buf`.
    ///
    /// @responsibilities
    /// - Rotate if needed, then write `buf`.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(usize)`. On error, returns [`io::Error`] on rotate or write failure.
    fn write_bytes(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.rotate_if_needed()?;

        self.file.write(buf)
    }

    /// @details
    /// Flush the open file.
    ///
    /// @responsibilities
    /// - Flush the open file.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(())`. On error, returns [`io::Error`] when flush fails.
    fn flush_file(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// @details
/// Cloneable hourly writer used as a tracing [`MakeWriter`].
///
/// @responsibilities
/// - Share one mutex-protected inner file across clones.
///
/// @behaviour
/// Hold data only. Methods on this type own behaviour.
pub(crate) struct RollingLog {
    locked: Arc<Mutex<Inner>>,
}

impl RollingLog {
    /// @details
    /// Open the current hour file for `prefix` under `dir`.
    ///
    /// @responsibilities
    /// - Open the current hour file for `prefix` under `dir`.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(Self)`. On error, returns [`TelemetryError::CreateDirectoryFailed`]
    /// or [`TelemetryError::OpenLogFileFailed`].
    pub(crate) fn open(prefix: &str, dir: &Path) -> Result<Self, TelemetryError> {
        let inner = Inner::open(prefix, dir)?;

        Ok(Self {
            locked: Arc::new(Mutex::new(inner)),
        })
    }

    /// @details
    /// Map a mutex lock result to [`io::Error`] on poison.
    ///
    /// @responsibilities
    /// - Map a mutex lock result to [`io::Error`] on poison.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// On success, returns `Ok(T)`. On error, returns [`io::Error`] when the lock is poisoned.
    fn map_lock<T>(result: Result<T, PoisonError<T>>) -> io::Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(_) => Err(io::Error::other("log writer lock poisoned")),
        }
    }

    /// @details
    /// Lock the inner file and run `op`.
    ///
    /// @responsibilities
    /// - Lock the inner file and run `op`.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(R)`. On error, returns [`io::Error`] on poison or when `op` fails.
    fn with_inner<R>(&self, op: impl FnOnce(&mut Inner) -> io::Result<R>) -> io::Result<R> {
        let mut inner = Self::map_lock(self.locked.lock())?;

        op(&mut inner)
    }
}

impl Clone for RollingLog {
    /// @details
    /// Clone the shared inner mutex.
    ///
    /// @responsibilities
    /// - Clone the shared inner mutex.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `Self`. This method does not fail.
    fn clone(&self) -> Self {
        Self {
            locked: Arc::clone(&self.locked),
        }
    }
}

impl Write for RollingLog {
    /// @details
    /// Rotate if needed and write `buf`.
    ///
    /// @responsibilities
    /// - Rotate if needed and write `buf`.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(usize)`. On error, returns [`io::Error`] on poison, rotate, or write
    /// failure.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.with_inner(|inner| inner.write_bytes(buf))
    }

    /// @details
    /// Flush the open file.
    ///
    /// @responsibilities
    /// - Flush the open file.
    ///
    /// @behaviour
    /// Fail closed. Does not unwrap on the product path.
    ///
    /// @return
    /// On success, returns `Ok(())`. On error, returns [`io::Error`] on poison or flush failure.
    fn flush(&mut self) -> io::Result<()> {
        self.with_inner(|inner| inner.flush_file())
    }
}

impl<'a> MakeWriter<'a> for RollingLog {
    type Writer = RollingLog;

    /// @details
    /// Clone this writer for a tracing layer.
    ///
    /// @responsibilities
    /// - Clone this writer for a tracing layer.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `Self::Writer`. This method does not fail.
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    /***
     * @test-list
     * 1. path puts prefix first and ends in .log.
     * 2. write and flush succeed.
     * 3. make_writer writes.
     * 4. write after an hour-key change rotates.
     * 5. open under a file path is CreateDirectoryFailed.
     * 6. rotate to the same hour is ok.
     * 7. rotate into a blocked directory fails.
     * 8. open when the log path is a directory is OpenLogFileFailed.
     * 9. write fails when rotation cannot open the next file.
     * 10. write fails when the file write fails.
     * 11. flush fails when the file flush fails.
     * 12. map_lock ok is ok.
     * 13. map_lock poison is an io error.
     */

    struct FailWrite;

    impl Write for FailWrite {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let _ = buf;

            Err(io::Error::other("write"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush"))
        }
    }

    /// @details
    /// Temp-dir helper for rolling-file tests.
    ///
    /// @responsibilities
    /// - Build a unique path under the process temp directory.
    ///
    /// @behaviour
    /// Hold data only. Methods on this type own behaviour.
    struct TestPath;

    impl TestPath {
        /// @details
        /// Join a unique `tel-{pid}-{nanos}-{tag}` name under temp.
        ///
        /// @responsibilities
        /// - Join a unique `tel-{pid}-{nanos}-{tag}` name under temp.
        ///
        /// @behaviour
        /// Completes without I/O unless callers have already opened a backend.
        ///
        /// @return
        /// Returns `PathBuf`. This method does not fail.
        fn unique(tag: &str) -> PathBuf {
            std::env::temp_dir().join(format!(
                "tel-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
                tag
            ))
        }
    }

    #[test]
    fn path_puts_prefix_first() {
        let path = Inner::path("app", Path::new("/tmp"), "2026-07-31-20");
        let name = path.file_name().and_then(|s| s.to_str());

        assert_eq!(name, Some("app-2026-07-31-20.log"));
    }

    /// @details
    /// True when `dir` contains an `app-*.log` file.
    ///
    /// @responsibilities
    /// - True when `dir` contains an `app-*.log` file.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `bool`. This method does not fail. A read_dir error is treated as no match.
    fn has_prefixed_log(dir: &Path) -> bool {
        fs::read_dir(dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .any(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.starts_with("app-") && n.ends_with(".log"))
                    .unwrap_or(false)
            })
    }

    #[test]
    fn write_and_flush_succeed() {
        let dir = TestPath::unique("wf");
        create_dir_all(&dir).expect("dir");
        let mut log = RollingLog::open("app", &dir).expect("open");
        log.write_all(b"hello\n").expect("write");
        log.flush().expect("flush");

        assert!(has_prefixed_log(&dir));
    }

    #[test]
    fn make_writer_writes() {
        let dir = TestPath::unique("mw");
        create_dir_all(&dir).expect("dir");
        let log = RollingLog::open("app", &dir).expect("open");
        let mut writer = log.make_writer();
        let written = writer.write_all(b"world\n");

        assert!(written.is_ok());
    }

    #[test]
    fn write_after_hour_key_change_rotates() {
        let dir = TestPath::unique("rot");
        create_dir_all(&dir).expect("dir");
        let log = RollingLog::open("app", &dir).expect("open");
        let key = log
            .with_inner(|inner| {
                inner.hour_key = String::from("force-rotate");
                inner.rotate_if_needed()?;
                Ok(inner.hour_key.clone())
            })
            .expect("rotate");

        assert_ne!(key, "force-rotate");
    }

    #[test]
    fn open_under_file_path_is_create_directory_failed() {
        let blocker = TestPath::unique("file");
        fs::write(&blocker, b"x").expect("blocker");
        let result = RollingLog::open("app", &blocker.join("child"));

        assert_eq!(result.err(), Some(TelemetryError::CreateDirectoryFailed));
    }

    #[test]
    fn rotate_to_same_hour_is_ok() {
        let dir = TestPath::unique("ok");
        let mut inner = Inner::open_hour("app", &dir, "a").expect("open");
        let result = inner.rotate_to(String::from("a"));

        assert!(result.is_ok());
    }

    #[test]
    fn rotate_into_blocked_directory_fails() {
        let blocker = TestPath::unique("file");
        fs::write(&blocker, b"x").expect("blocker");
        let dir = TestPath::unique("ok");
        let mut inner = Inner::open_hour("app", &dir, "a").expect("open");
        inner.dir = blocker;
        let result = inner.rotate_to(String::from("b"));

        assert!(result.is_err());
    }

    #[test]
    fn open_when_log_path_is_directory_is_open_log_file_failed() {
        let dir = TestPath::unique("openfile");
        create_dir_all(&dir).expect("dir");
        let blocked = Inner::path("app", &dir, "blocked");
        create_dir_all(&blocked).expect("log path is a directory");
        let result = Inner::open_hour("app", &dir, "blocked");

        assert_eq!(result.err(), Some(TelemetryError::OpenLogFileFailed));
    }

    #[test]
    fn write_fails_when_rotation_cannot_open_next_file() {
        let blocker = TestPath::unique("file");
        fs::write(&blocker, b"x").expect("blocker");
        let dir = TestPath::unique("openfile");
        create_dir_all(&dir).expect("dir");
        let mut inner = Inner::open_hour("app", &dir, "w").expect("state");
        inner.hour_key = String::from("stale");
        inner.dir = blocker;
        let result = inner.write_bytes(b"x");

        assert!(result.is_err());
    }

    #[test]
    fn write_fails_when_file_write_fails() {
        let dir = TestPath::unique("openfile");
        create_dir_all(&dir).expect("dir");
        let mut inner = Inner::open_hour("app", &dir, "w2").expect("state");
        inner.hour_key = String::from(HourKey::now_utc().as_str());
        inner.file = Box::new(FailWrite);
        let result = inner.write_bytes(b"x");

        assert!(result.is_err());
    }

    #[test]
    fn flush_fails_when_file_flush_fails() {
        let dir = TestPath::unique("openfile");
        create_dir_all(&dir).expect("dir");
        let log = RollingLog::open("app", &dir).expect("open for flush fail");
        log.with_inner(|inner| {
            inner.file = Box::new(FailWrite);
            Ok(())
        })
        .expect("swap file");
        let mut writer = log.make_writer();
        let result = writer.flush();

        assert!(result.is_err());
    }

    #[test]
    fn map_lock_ok_is_ok() {
        let result = RollingLog::map_lock(Ok(()));

        assert!(result.is_ok());
    }

    #[test]
    fn map_lock_poison_is_io_error() {
        let result = RollingLog::map_lock::<()>(Err(PoisonError::new(())));

        assert!(result.is_err());
    }
}
