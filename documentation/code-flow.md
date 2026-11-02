# Code flow

This document follows one successful startup, one log event, one hour change, and the shutdown path. Names below match the functions in `src/`.

## Startup

A product calls `Telemetry::new` once, usually from `main`, and stores the result for the life of the process.

```
caller
  │
  ▼
Telemetry::new(prefix, log_directory)
  │
  ├─ prefix.is_empty() ──────────────► Err(EmptyPrefix)
  ├─ directory path is empty ────────► Err(EmptyDirectory)
  │
  ▼
RollingLog::open
  │
  ▼
Inner::open → HourKey::now_utc → Inner::open_hour
  │
  ├─ create_dir_all(dir) ────────────► Err(CreateDirectoryFailed)
  ├─ open {prefix}-{hour}.log append ► Err(OpenLogFileFailed)
  │
  ▼
tracing_appender::non_blocking(rolling)
  │
  ├─ returns NonBlocking  (queue writer, used by the file layer)
  └─ returns WorkerGuard  (owned later by Telemetry)
  │
  ▼
EventPrefix::from_arg(prefix)
  │
  ▼
Telemetry::install
  │
  ├─ fmt layer: EventPrefix clone, stdout, ANSI on
  ├─ fmt layer: EventPrefix, NonBlocking, ANSI off
  ├─ EnvFilter::from_default_env().add_directive(INFO)
  └─ registry.try_init()
        │
        ├─ Ok ──► Telemetry { _guard }
        └─ Err ─► Err(SubscriberInstallFailed)
```

`RollingLog::open` runs before `try_init`. A second `Telemetry::new` that fails at install has already created or opened the current hour file. That attempt's worker guard is dropped with the error, which shuts down the second worker. The subscriber installed by the first success stays in place.

`try_init` comes from `SubscriberInitExt`. It first calls `set_global_default`. On success, and because the `tracing-log` feature is enabled by default, it then calls `LogTracer::builder().init()`, which registers this subscriber as the destination for the `log` crate. A `log` logger that is already set makes that second step fail. The tracing subscriber is left installed, `Telemetry::new` returns `SubscriberInstallFailed`, and the worker guard is dropped, so the file writer stops while stdout output from the installed subscriber continues.

### Validation

`EmptyPrefix` is only the empty string. A prefix of spaces is accepted and is written as `[   ] `.

`EmptyDirectory` is only a path whose `OsStr` is empty. A relative path, an absolute path, and a path that does not exist yet are all accepted. A missing directory is created. If a parent component is a file, `create_dir_all` fails and the error is `CreateDirectoryFailed`. If the computed log path exists as a directory, open fails and the error is `OpenLogFileFailed`.

### Filter construction

`EnvFilter::from_default_env` reads `RUST_LOG`. When the variable is unset, that constructor starts from a global `ERROR` directive. `add_directive(INFO)` then runs.

In tracing-subscriber, two directives with the same target and the same field list compare equal, and the later one replaces the earlier one. A bare level is that kind of directive. The added `INFO` directive therefore replaces a global level that came from an unset variable or from a `RUST_LOG` value that is only a level.

Directives that name a target are more specific, so they stay. For an event, the most specific matching directive wins.

| `RUST_LOG` | Events with no more specific directive | Named target |
| --- | --- | --- |
| unset | INFO and more severe | INFO and more severe |
| `debug`, `warn`, or `error` | INFO and more severe | INFO and more severe |
| `my_crate=debug` | INFO and more severe | `my_crate` at DEBUG and more severe |
| `my_crate=warn` | INFO and more severe | `my_crate` at WARN and more severe |

`telemetry_info!`, `telemetry_warn!`, and `telemetry_error!` all pass the default filter. `tracing::debug!` and `tracing::trace!` pass only when a matching directive enables them.

## Emitting an event

```
telemetry_info!(...)        or any tracing event
        │
        ▼
global subscriber (the registry installed by try_init)
        │
        ├─ EnvFilter rejects the event, and both layers skip it
        │
        └─ EnvFilter accepts the event
              │
              ├─ stdout layer
              │     EventPrefix::format_event
              │       write "[prefix] "
              │       default format with UtcStamp::format_time
              │       write the line to stdout on this thread
              │
              └─ file layer
                    EventPrefix::format_event
                      write "[prefix] "
                      default format with UtcStamp::format_time
                      enqueue the bytes on the non-blocking writer
                            │
                            ▼
                      worker thread
                            │
                            ▼
                      RollingLog::write
                            │
                            ▼
                      Inner::write_bytes
```

`UtcStamp::format_time` calls `UtcStamp::write_to(OffsetDateTime::now_utc())`. `write_to` truncates nanoseconds down to a 10,000,000 ns boundary (two fractional digits) and writes `yyyy-MM-ddTHH:MM:SS.ssZ` plus a trailing space. The default full formatter then writes the level, any active spans from root to leaf, the target, the event fields, and a newline.

The two layers format the event separately. Each calls `UtcStamp` when that layer runs.

## Rotation

`Inner::write_bytes` calls `rotate_if_needed` before every write. That reads `HourKey::now_utc` and calls `rotate_to`.

When the new key equals the stored key, `rotate_to` returns immediately and the existing file stays open.

When the key differs, `rotate_to` builds `{dir}/{prefix}-{new-hour}.log`, opens it for append, and on success swaps the file and stores the new key. The previous file is dropped when the `Box<dyn Write>` is replaced, which closes it. `rotate_to` does not call `create_dir_all`. If the directory is gone, or the new path cannot be opened, the write returns `io::Error` and the stored key stays on the old hour so a later write can try again.

`flush` locks the same mutex and flushes the open file. It does not rotate.

## Lock poisoning

`RollingLog::with_inner` locks the mutex and maps a poison error to `io::Error` with the message `log writer lock poisoned`. The poisoned guard is not recovered. Later writes and flushes fail the same way until the process replaces the `Telemetry` value, which this API does not do in place: a second `Telemetry::new` returns `SubscriberInstallFailed` while the first subscriber is still installed.

## Shutdown

When `Telemetry` is dropped, its `Drop` implementation logs `Telemetry shutting down - flushing logs` through the still-installed subscriber. Field drop then runs and drops `WorkerGuard`. The guard sends shutdown to the worker and joins it, which writes lines still queued. An abrupt exit that skips destructors, such as `std::process::exit` or a fatal signal, skips that flush. Lines still in the queue can be lost. Lines already written to the file stay there.

The global subscriber remains installed after the guard is dropped. Further tracing events still enter the subscriber. The file worker is gone, so file output stops with the guard. Standard output remains a normal stdout write.

## Macros and direct tracing calls

The macros expand to the corresponding `tracing` macros and add no fields of their own. `telemetry_error!(code = 1u32, "failure")` is a normal tracing event with a `code` field. The formatter prints that field in the message portion of the line.

Code that already uses `tracing::info!` or spans reaches the same subscriber. After a successful `try_init`, `log` records do too, through the `LogTracer` installed with the subscriber. The crate adds no separate logger configuration beyond that call.
