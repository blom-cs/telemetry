# Architecture

`telemetry` is a Rust library crate, version 0.0.1, edition 2024, licensed under MIT. It installs one process-wide tracing subscriber that writes the same events to standard output and to an hourly log file. A product constructs `Telemetry` once at startup and keeps that value alive until shutdown.

In this crate, telemetry means the tracing events of one process, written to standard output and to hourly log files.

## Modules

| Module | Visibility | Role |
| --- | --- | --- |
| `src/lib.rs` | crate root | Declares the modules and re-exports `TelemetryError`. |
| `src/error.rs` | public | Stable error codes for install and file setup. |
| `src/telemetry.rs` | public | Subscriber install, the `Telemetry` guard, and the log macros. |
| `src/rolling.rs` | private | Hourly file writer shared by the background logging thread. |
| `src/hour.rs` | private | UTC hour key for file names, and the two-digit event timestamp. |
| `src/prefix.rs` | private | Event formatter that writes the caller prefix, then the timed line. |

`hour`, `prefix`, and `rolling` stay crate-private. Callers use `Telemetry`, `TelemetryError`, and the three macros.

## Public API

`Telemetry::new(prefix, log_directory)` is the only constructor. `prefix` is a non-empty string. `log_directory` is a non-empty path. On success the function returns a `Telemetry` value whose only field is the non-blocking writer guard.

`TelemetryError` is a `Copy` enum. Discriminants are fixed from 1 through 5. `TelemetryError::code` returns that discriminant as a `u8`. The type does not implement `Display` or `std::error::Error`. Callers match the variant or log the numeric code.

| Variant | Code | When it is returned |
| --- | --- | --- |
| `EmptyPrefix` | 1 | `prefix` is an empty string. |
| `EmptyDirectory` | 2 | The directory path is empty. |
| `SubscriberInstallFailed` | 3 | `try_init` failed. A tracing subscriber or a `log` logger is already installed, or the install failed for another reason. |
| `CreateDirectoryFailed` | 4 | Creating the log directory failed. |
| `OpenLogFileFailed` | 5 | Opening or creating the current hour file failed. |

The same enum is available as `telemetry::TelemetryError` and as `telemetry::telemetry::TelemetryError`.

Three macros re-export the matching tracing macros:

- `telemetry_info!` forwards to `tracing::info!`
- `telemetry_warn!` forwards to `tracing::warn!`
- `telemetry_error!` forwards to `tracing::error!`

They are `#[macro_export]` macros, so they live at the crate root. Any other `tracing` event in the process reaches the same subscriber after a successful install. The crate does not define its own span types. When an event is emitted inside a span, the default tracing formatter includes that span in the line.

## Dependencies

Direct dependencies, from `Cargo.toml`:

| Crate | Requirement | Use in this crate |
| --- | --- | --- |
| `tracing` 0.1 | default features | Events and the global subscriber. |
| `tracing-subscriber` 0.3 | `env-filter`, `fmt` | Registry, format layers, and `RUST_LOG`. |
| `tracing-appender` 0.2 | default features | Background thread and bounded queue for the file writer. |
| `time` 0.3 | `std` only, default features off | `OffsetDateTime` for the UTC hour and the event stamp. |

`time` is limited to the `std` feature so the crate does not pull in formatting macros or serde. The locked versions are recorded in `Cargo.lock`.

## Ownership

```
Telemetry
  └── WorkerGuard
        └── background thread writes to RollingLog
              └── Arc<Mutex<Inner>>
                    ├── prefix: String
                    ├── dir: PathBuf
                    ├── hour_key: String
                    └── file: Box<dyn Write + Send>
```

`Telemetry` exists so the `WorkerGuard` stays alive. Dropping the guard shuts the worker down and flushes lines still in the queue. The guard must be bound to a named value. Binding it to `_` drops it immediately, and the file worker exits while the process is still running.

`RollingLog` is cloneable. Clones share one mutex. After install, the tracing-appender worker is the writer that calls `RollingLog`. The mutex serializes rotation, write, and flush.

Each format layer owns an `EventPrefix`. The stdout layer receives a clone. The file layer receives the original. `EventPrefix` holds the prefix string and does not touch the file.

## Subscriber shape

`Telemetry::new` builds one `tracing_subscriber` registry and installs it with `try_init`, which sets the process-global default subscriber. The registry has three layers:

1. A fmt layer on `std::io::stdout`, with ANSI color enabled. Formatting and writing happen on the thread that emits the event.
2. A fmt layer on a non-blocking writer, with ANSI color disabled. The emitting thread enqueues the formatted line. A dedicated thread named by tracing-appender performs the file write.
3. An `EnvFilter` built from `RUST_LOG`, with a global `INFO` directive added afterwards.

Both fmt layers use `EventPrefix` as their event formatter. The filter applies to both layers.

The non-blocking writer uses the tracing-appender defaults: a bounded queue of 128,000 lines, lossy when the queue is full. A line that cannot be enqueued is dropped, and the appender increments its internal drop counter. This crate does not read that counter. Standard output is separate from that queue. A slow terminal can still block the emitting thread.

## Files and time

The log directory is created with `create_dir_all` during open. The current file name is `{prefix}-{yyyy-MM-dd-HH}.log`. The hour key is UTC, taken from `OffsetDateTime::now_utc`, and formatted as four-digit year, two-digit month, two-digit day, and two-digit hour.

The file is opened with `create(true)` and `append(true)`. An existing file for that hour is extended. The crate does not delete, compress, or size-cap older files. Two processes that use the same prefix and directory append to the same hour file. There is no cross-process lock, so lines from those processes can interleave.

Rotation is checked when the worker writes, by comparing the stored hour key with the current UTC hour. A change opens the new hour file and replaces the stored key. The directory is not created again during rotation.

The timestamp printed on a line is formatted on the emitting thread, before the bytes are queued. The hour used to choose the file is the UTC hour at the moment the worker writes. A line stamped in one hour can land in the next hour's file when the queue crosses the hour boundary.

`UtcStamp` truncates subseconds to two digits (hundredths of a second) and writes RFC 3339 UTC with a trailing space, for example `2026-11-01T19:12:03.12Z `.

## Line layout

Each line starts with the prefix in brackets and a space. The rest is the tracing subscriber's default full format, timed by `UtcStamp`:

```
[app] 2026-11-01T19:12:03.12Z  INFO my_app: telemetry ready
```

The level word is padded to five characters, so `INFO` and `WARN` have a leading space. Span context is present when the event is emitted inside one or more spans, and it is written between the level and the target. The target is the tracing target, which defaults to the module path. The stdout layer may wrap the level and the target in ANSI color. The file layer writes the same text with ANSI color disabled.

## Failure behaviour

Setup fails closed. An empty prefix or an empty directory returns before any directory is created and before a subscriber is installed. A directory or file error returns before `try_init`.

`try_init` sets the tracing subscriber and then installs `tracing_log::LogTracer`, because `tracing-subscriber` is built with its default features. `LogTracer` forwards records from the `log` crate into this subscriber. If a tracing subscriber is already installed, `try_init` returns before touching the `log` logger. If the tracing subscriber is set and the `log` logger is already taken, `try_init` still returns an error. This crate maps every `try_init` error to `SubscriberInstallFailed` and drops the worker guard for that attempt. In the second case the tracing subscriber stays installed, stdout keeps working, and the file worker has already been shut down by the dropped guard.

On the write path, a poisoned mutex becomes an `io::Error`. A rotation that cannot open the next file also returns an `io::Error` from `Write`. The product path does not unwrap these results. The tracing fmt layer receives the `io::Error` from the writer.

`Telemetry`'s `Drop` emits `Telemetry shutting down - flushing logs` at info level, then drops the guard so the worker can flush.

## Build profiles

`Cargo.toml` sets `opt-level` to 0 for the dev profile and to 3 for the release profile. The crate is a library. There is no binary target and no feature flags. The only configured rustc lint is `unexpected_cfgs` at warning level for `cfg(coverage_nightly)`. The source does not currently branch on that configuration.
