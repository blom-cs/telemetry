//! @file prefix.rs
//!
//! @brief
//! Prefix tracing events with the caller-supplied string.
//!
//! @details
//! [`EventPrefix`] implements [`FormatEvent`]. Each line starts with
//! `[prefix] `, then the default tracing format timed by [`UtcStamp`].
//!
//! @responsibilities
//! - Own the prefix string.
//! - Write the prefix, then the timed event body.
//!
//! @behaviour
//! Fail closed. Unit tests do not open a live display or GPU.

use crate::hour::UtcStamp;
use std::fmt;
use tracing::Event;
use tracing::Subscriber;
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::registry::LookupSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
/// @details
/// Holds the prefix so [`FormatEvent`] can write it on each line.
///
/// @responsibilities
/// - Hold the caller-supplied prefix.
///
/// @behaviour
/// Hold data only. Methods on this type own behaviour.
pub(crate) struct EventPrefix {
    prefix: String,
}

impl EventPrefix {
    /// @details
    /// Copy `prefix` into an event formatter.
    ///
    /// @responsibilities
    /// - Copy `prefix` into an event formatter.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// Returns `Self`. This method does not fail.
    pub(crate) fn from_arg(prefix: &str) -> Self {
        Self {
            prefix: String::from(prefix),
        }
    }
}

impl<S, N> FormatEvent<S, N> for EventPrefix
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    /// @details
    /// Write `[prefix] ` then the default timed event line.
    ///
    /// @responsibilities
    /// - Write `[prefix] ` then the default timed event line.
    ///
    /// @behaviour
    /// Completes without I/O unless callers have already opened a backend.
    ///
    /// @return
    /// On success, returns `Ok(())`. On error, returns [`fmt::Error`] when the writer cannot accept
    /// the line.
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        write!(&mut writer, "[{}] ", self.prefix)?;

        tracing_subscriber::fmt::format()
            .with_timer(UtcStamp)
            .format_event(ctx, writer, event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;
    /***
     * @test-list
     * 1. Event lines include the prefix argument.
     * 2. Event timestamps keep two fractional digits.
     */

    #[derive(Clone)]
    struct CaptureWriter {
        buf: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut dest = match self.buf.lock() {
                Ok(dest) => dest,
                Err(_) => return Err(io::Error::other("capture lock")),
            };
            dest.extend_from_slice(buf);

            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureWriter;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn event_line_includes_prefix_argument() {
        let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::with_capacity(128)));
        let writer = CaptureWriter {
            buf: Arc::clone(&buf),
        };
        let subscriber = tracing_subscriber::fmt()
            .event_format(EventPrefix::from_arg("kiosk"))
            .with_writer(writer)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("prefixed-line");
        });
        let bytes = buf.lock().expect("capture lock").clone();
        let text = String::from_utf8(bytes).expect("utf8 log");

        assert!(text.contains("[kiosk] "));
    }

    #[test]
    fn event_timestamp_has_two_fractional_digits() {
        let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::with_capacity(128)));
        let writer = CaptureWriter {
            buf: Arc::clone(&buf),
        };
        let subscriber = tracing_subscriber::fmt()
            .event_format(EventPrefix::from_arg("kiosk"))
            .with_writer(writer)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("stamp");
        });
        let bytes = buf.lock().expect("capture lock").clone();
        let text = String::from_utf8(bytes).expect("utf8 log");
        let stamp = text.split_whitespace().nth(1).unwrap_or("");

        assert!(
            stamp
                .strip_suffix('Z')
                .and_then(|s| s.rsplit_once('.'))
                .is_some_and(|(_, frac)| frac.len() == 2),
            "expected two fractional digits in {stamp:?} from {text:?}"
        );
    }
}
