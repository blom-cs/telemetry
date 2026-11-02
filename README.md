# telemetry

`telemetry` is a Rust library that installs process-wide logging for one product. A single constructor writes tracing events to standard output and to an hourly log file, prefixes every line with a caller-supplied name, and returns a stable numeric code when setup fails.

The crate is the shared logging package for products that want the same startup sequence, the same file layout, and the same failure codes. Version 0.0.1 is a library target on Rust edition 2024. It is licensed under the MIT license. Copyright 2026 Dawid Blom.

## Why use it

- One call in `main` installs logging for the whole process. Info, warn, and error macros are provided. Any other `tracing` event in the process uses the same subscriber.
- Every line starts with a bracketed prefix, so logs from this process stay recognizable in a shared terminal or a shared directory.
- The same event is written to the terminal, with ANSI color, and to a plain-text file.
- Files are named `{prefix}-{yyyy-MM-dd-HH}.log` and roll forward when the UTC hour changes. Rotation is part of the writer. There is no separate rotation process.
- File writes run on a background thread, so the emitting thread enqueues a line instead of waiting on disk. The value returned by `Telemetry::new` owns the flush guard. Dropping it at shutdown writes the lines still in the queue.
- Setup fails closed. An empty prefix, an empty directory, a directory that cannot be created, a file that cannot be opened, or a subscriber that is already installed each return `TelemetryError` with a fixed code from 1 to 5. A second install does not replace the first subscriber.
- The level defaults to INFO. A `RUST_LOG` directive that names a target narrows or widens that target without a code change.
- The dependency set is small: `tracing`, `tracing-subscriber`, `tracing-appender`, and `time`. Licenses accepted by the repository policy are MIT, Apache-2.0, and Unicode-3.0.

Further detail is in [documentation/architecture.md](documentation/architecture.md), [documentation/code-flow.md](documentation/code-flow.md), and [documentation/testing-and-development.md](documentation/testing-and-development.md).

## Requirements

- A stable Rust toolchain that can compile edition 2024.
- A writable log directory. The crate creates the directory when it is missing.
- This process must not already have a global tracing subscriber installed.

## Add the dependency

The crate is consumed as a path dependency from the product that embeds it:

```toml
[dependencies]
telemetry = { path = "../telemetry" }
```

Adjust the path to where this repository sits relative to the product.

## Startup

Construct `Telemetry` once in `main` and bind it to a name that lives for the whole process. The binding owns a worker guard. If the guard is dropped early, queued file lines are flushed and later file output stops.

```rust
use telemetry::telemetry::{Telemetry, TelemetryError};

fn main() -> Result<(), TelemetryError> {
    let telemetry = Telemetry::new("app", "logs")?;

    telemetry::telemetry_info!("telemetry ready");
    telemetry::telemetry_warn!("something odd");
    telemetry::telemetry_error!(code = 1u32, "failure");

    // Keep `telemetry` alive until shutdown.
    run(telemetry)
}

fn run(_telemetry: Telemetry) -> Result<(), TelemetryError> {
    Ok(())
}
```

`prefix` is written in brackets at the start of every line and is the stem of the file name. `log_directory` is the directory that receives the files. Relative paths are resolved from the process working directory.

After a successful return, events can be emitted from any thread. The macros expand to the matching `tracing` macros:

| Macro | Level |
| --- | --- |
| `telemetry_info!` | INFO |
| `telemetry_warn!` | WARN |
| `telemetry_error!` | ERROR |

Field syntax supported by `tracing` works unchanged. `telemetry_error!(code = 1u32, "failure")` records a `code` field on the event.

Code that already calls `tracing::info!`, enters spans, or logs through the `log` facade bridged by tracing reaches the same subscriber. The default formatter includes active spans on the line.

## Log files and line format

Files are created under the directory you pass in:

```
logs/app-2026-11-01-19.log
```

The hour is UTC. The file is opened in append mode, so a restart during the same hour continues the same file. Older hour files are left in place. The crate does not compress them or delete them.

A file line looks like this:

```
[app] 2026-11-01T19:12:03.12Z  INFO my_app: telemetry ready
```

The timestamp is RFC 3339 UTC with two fractional digits. The level is padded to five characters, which is why `INFO` has a leading space. The tracing target and the message follow the tracing subscriber's default full format. When the event is inside a span, the span names are written between the level and the target. Standard output uses the same text with ANSI color enabled. The file uses the same text with ANSI color disabled.

The timestamp is taken when the line is formatted. The file name is chosen when the background thread writes the line. A line emitted at the end of an hour can be written into the next hour's file if it is still queued when the hour changes.

## Levels

With `RUST_LOG` unset, INFO, WARN, and ERROR are recorded. DEBUG and TRACE are filtered out.

`Telemetry::new` builds the filter from `RUST_LOG` and then adds a global INFO directive. A `RUST_LOG` value that is only a global level is replaced by that INFO directive, so the base level stays INFO. A directive that names a target is kept and wins for that target.

```bash
# DEBUG from one crate, INFO everywhere else
RUST_LOG=my_crate=debug cargo run

# WARN and ERROR from one noisy target, INFO everywhere else
RUST_LOG=my_crate::net=warn cargo run
```

Several target directives can be comma-separated, using the normal `RUST_LOG` syntax.

## Errors

`Telemetry::new` returns `Result<Telemetry, TelemetryError>`. Match the variant, or log `error.code()`. The numeric codes are part of the interface and stay fixed.

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `EmptyPrefix` | The prefix string is empty. |
| 2 | `EmptyDirectory` | The directory path is empty. |
| 3 | `SubscriberInstallFailed` | A global tracing subscriber is already installed, a `log` logger is already installed, or `try_init` failed for another reason. |
| 4 | `CreateDirectoryFailed` | The log directory could not be created. |
| 5 | `OpenLogFileFailed` | The current hour file could not be opened or created. |

`TelemetryError` does not implement `Display`. The code is the supported diagnostic.

A failed second call still opens the current hour file before install is attempted, then drops that attempt's worker. The first subscriber remains the process subscriber.

## Shutdown

Drop `Telemetry` during normal shutdown. `Drop` logs `Telemetry shutting down - flushing logs` and then drops the worker guard, which flushes the queue. `std::process::exit` and a fatal signal skip destructors, so lines still queued at that moment are not written. Lines already flushed to the file remain on disk.

## Operational limits

- One `Telemetry` install per process. Install it before other libraries set the global tracing subscriber or the global `log` logger. A `log` logger that is already set makes `try_init` fail after the tracing subscriber has been installed, and this crate then drops the file worker while reporting `SubscriberInstallFailed`.
- The file queue holds up to 128,000 lines. When it is full, further file lines are dropped until space is available. Standard output is not on that queue.
- Standard output is written on the emitting thread. A blocked terminal can stall that thread.
- Rotation happens on write. A process that emits nothing across an hour boundary creates the next file on the next event.
- The crate does not ship logs off the machine, and it does not apply a retention policy. Removing old files is the operator's job.
- Two processes that share a prefix and a directory append to the same hour file without a cross-process lock.

## Develop

From this repository:

```bash
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

Dependency changes also need:

```bash
cargo deny check
cargo audit
```

Continuous integration runs build, format, clippy, coverage, and the same security checks. Line and function coverage are required at 100 percent. The coverage job also collects branch coverage. The testing notes describe each unit test, the coverage command, and the workflow files.

## License

MIT. See [LICENSE](LICENSE).
