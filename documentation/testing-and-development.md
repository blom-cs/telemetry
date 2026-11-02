# Testing and development

Tests live in the module they exercise, under `#[cfg(test)]`. There is no `tests/` integration suite and no example binary. The public behaviour is covered by those unit tests and by one doctest on `telemetry.rs`.

## What the tests cover

Each module keeps a short `@test-list` comment next to its tests. The lists below match the functions in the tree.

### `error.rs`

Each variant is constructed and checked against itself and against `code()`.

| Test | Asserts |
| --- | --- |
| `empty_prefix_code` | `EmptyPrefix` is code 1. |
| `empty_directory_code` | `EmptyDirectory` is code 2. |
| `subscriber_install_failed_code` | `SubscriberInstallFailed` is code 3. |
| `create_directory_failed_code` | `CreateDirectoryFailed` is code 4. |
| `open_log_file_failed_code` | `OpenLogFileFailed` is code 5. |

### `hour.rs`

Time values are fixed at the Unix epoch so the assertions do not depend on the clock, except `now_utc_has_hour_key_length`, which only checks that the live key is at least as long as a one-digit hour string.

| Test | Asserts |
| --- | --- |
| `from_time_maps_unix_epoch` | Epoch formats as `1970-01-01-00`. |
| `now_utc_has_hour_key_length` | The current key is at least 12 characters. |
| `file_name_puts_prefix_first` | `app` and the epoch key produce `app-1970-01-01-00.log`. |
| `trim_keeps_two_subsecond_digits` | 123,456,789 ns becomes 120,000,000 ns. |
| `with_nanosecond_keeps_original_when_replace_fails` | An out-of-range nanosecond leaves the original time unchanged. |
| `write_to_prints_two_fractional_digits` | That instant prints `1970-01-01T00:00:00.12Z `. |

### `prefix.rs`

A local `CaptureWriter` implements `Write` and `MakeWriter` over a shared `Mutex<Vec<u8>>`. The tests build a fmt subscriber with `EventPrefix` and drive it through `tracing::subscriber::with_default`, so they do not install the global subscriber.

| Test | Asserts |
| --- | --- |
| `event_line_includes_prefix_argument` | The captured line contains `[kiosk] `. |
| `event_timestamp_has_two_fractional_digits` | The timestamp token ends in `Z` and has a two-digit fraction. |
| `capture_writer_flush_is_ok` | `CaptureWriter::flush` returns `Ok`. |
| `capture_writer_write_fails_when_lock_is_poisoned` | A poisoned buffer lock makes `write` return an error. |

### `rolling.rs`

Filesystem tests use `TestPath::unique`, which joins the process temp directory with `tel-{pid}-{nanos}-{tag}`. A `FailWrite` stub returns an error from both `write` and `flush`.

| Test | Asserts |
| --- | --- |
| `path_puts_prefix_first` | The file name is `app-2026-07-31-20.log`. |
| `write_and_flush_succeed` | A real write and flush leave an `app-*.log` file in the directory. |
| `make_writer_writes` | The `MakeWriter` clone accepts a write. |
| `write_after_hour_key_change_rotates` | Forcing a stale hour key and writing rotates back to the current hour. |
| `open_under_file_path_is_create_directory_failed` | A parent path that is a file maps to `CreateDirectoryFailed`. |
| `rotate_to_same_hour_is_ok` | Rotation to the stored key succeeds and keeps the file. |
| `rotate_into_blocked_directory_fails` | Pointing the directory at a file and rotating returns an error. |
| `open_when_log_path_is_directory_is_open_log_file_failed` | A log path that is a directory maps to `OpenLogFileFailed`. |
| `write_fails_when_rotation_cannot_open_next_file` | A stale key plus a blocked directory fails the write. |
| `write_fails_when_file_write_fails` | A swapped-in `FailWrite` fails the write. |
| `flush_fails_when_file_flush_fails` | A swapped-in `FailWrite` fails the flush. |
| `map_lock_ok_is_ok` | A successful lock result passes through. |
| `map_lock_poison_is_io_error` | A poisoned lock becomes an `io::Error`. |

### `telemetry.rs`

| Test | Asserts |
| --- | --- |
| `telemetry_init_succeeds_once` | The first `Telemetry::new` succeeds, the three macros can be called, and the second `new` returns `SubscriberInstallFailed`. |
| `empty_prefix_is_empty_prefix` | `""` returns `EmptyPrefix` and does not install a subscriber. |
| `empty_directory_is_empty_directory` | An empty path returns `EmptyDirectory` and does not install a subscriber. |
| `create_directory_failure_maps` | A directory whose parent is a file returns `CreateDirectoryFailed`. |

The success test passes `../tests/logs`. `cargo test` runs with the package directory as the working directory, so that path is a `tests/logs` directory next to this repository. The test creates it when the parent exists.

The doctest on `Telemetry::new` repeats the startup sequence against `../documentation/logs` in its own process. Doctests do not share the unit-test process, so they do not collide with `telemetry_init_succeeds_once`.

## How to run the tests

From the package directory:

```bash
cargo test
```

That builds the library, runs every module test, and runs the doctest. Use a filter when you want one module:

```bash
cargo test --lib hour::
cargo test --lib rolling::
cargo test --doc
```

`telemetry_init_succeeds_once` calls `try_init`, which succeeds once per process. The other tests in this crate either avoid the global subscriber or return before install, so the default `cargo test` schedule is safe. A new test that expects `Telemetry::new` to succeed in the same process as `telemetry_init_succeeds_once` will see `SubscriberInstallFailed` if the success test runs first. Prefer `with_default` for formatter tests, as `prefix.rs` does. Reserve a real `Telemetry::new` for a single test that owns the process-global subscriber.

Rolling-file tests create directories under the system temp directory and leave them there. They do not delete those directories on success.

## Local quality checks

Formatting is `rustfmt`, configured in `rustfmt.toml` for the 2024 style edition. The line width is 100, indentation is four spaces, and newlines are Unix. The file enables field-init shorthand, `?` shorthand, merged derives, nested-paren removal, and import and module reordering. Unstable rustfmt options are omitted so `cargo fmt` works on stable.

```bash
cargo fmt --all -- --check
cargo fmt --all
```

Clippy is expected to be clean for every target, with warnings denied:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

A full debug build of every target:

```bash
cargo build --all-targets --all-features
```

Release builds use `opt-level` 3, as set in `Cargo.toml`.

## Coverage

Coverage is measured with `cargo-llvm-cov` on nightly, because the tool needs the nightly `llvm-tools-preview` component. The CI invocation is:

```bash
cargo +nightly llvm-cov --all-features --workspace --branch \
  --json --summary-only --output-path target/llvm-cov-summary.json
```

The job reads the workspace `totals` object. It adds the covered and total counts for lines, functions, regions, and branches into one pair, then applies a single comparison: `covered * 100 < 80 * count`. A file or a metric can sit below 80 percent as long as that combined total does not. 80 percent passes. An empty total fails.

Install the tools the workflow uses when you want the same command locally: a nightly toolchain with `llvm-tools-preview`, and `cargo-llvm-cov`.

The rolling tests cover the poison path, the failing writer, and the blocked rotation, and every `TelemetryError` variant has a direct assertion. The coverage job fails when the combined total described above falls below 80 percent.

## Dependency and license policy

`deny.toml` is the cargo-deny configuration.

- Licenses are limited to MIT, Apache-2.0, and Unicode-3.0, with a detection confidence of 0.8. Private crates are not exempt.
- Advisory ignores are empty.
- Duplicate crate versions warn. Wildcard requirements are allowed. No crate is on the explicit deny list.
- Registries are limited to crates.io. Git dependencies are not on the allow list. Unknown registries and unknown git sources warn.
- `cargo deny check` runs advisories, licenses, bans, and sources.

`cargo audit` checks the lockfile against the RustSec advisory database. Run both before changing dependencies:

```bash
cargo deny check
cargo audit
```

When you add a crate, keep the license inside the allow list, or the deny job fails. Update `Cargo.lock` in the same change. The daily audit workflow also runs when `Cargo.toml`, `Cargo.lock`, `deny.toml`, `.cargo/audit.toml`, or the audit workflow file changes.

## Continuous integration

Workflows live under `.github/workflows/`.

### `ci-cd.yml`

The workflow runs on pushes to `main`, `develop`, and `feature/*`, on pull requests into `main` and `develop`, and on manual dispatch. The runner is `ubuntu-latest`. `RUST_BACKTRACE` is set to 1. The workflow sets the `x86_64-unknown-linux-gnu` linker flags to `cc` so the job uses the system compiler on the runner.

Jobs:

| Job | Needs | What it runs |
| --- | --- | --- |
| Build | nothing | `cargo build --all-targets --all-features` |
| Lint and format | Build | `cargo fmt --all -- --check`, then `cargo clippy --all-targets --all-features -- -D warnings` |
| Coverage | Build and Lint | The nightly `cargo llvm-cov` command described above |
| Security | nothing | `cargo deny check` and `cargo audit` |

Build and Lint export `SQLX_OFFLINE=true`. This crate does not read that variable. The workflow name is CI/CD. The jobs stop at build, lint, coverage, and security. There is no publish or deploy step.

Rust is installed with `dtolnay/rust-toolchain` at stable for build, lint, and security. Coverage installs stable and then nightly with `llvm-tools-preview`. Cargo home is cached with `Swatinem/rust-cache` on the build and lint jobs.

### `audit.yml`

A second security workflow runs `cargo deny check` and `cargo audit` on a daily schedule at 00:00 UTC, on manual dispatch, and on pushes that touch the manifest, the lockfile, `deny.toml`, `.cargo/audit.toml`, or the workflow file.

## Day-to-day change pattern

1. Change the module that owns the behaviour. Keep `hour`, `prefix`, and `rolling` crate-private unless a product truly needs the type.
2. Add the test in that module. For anything that installs a subscriber, use `with_default` unless the test is specifically about `try_init`.
3. Keep error discriminants stable. A new failure mode gets a new variant at the next code, and a unit test for that code.
4. Run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`.
5. If dependencies changed, run `cargo deny check` and `cargo audit`, and commit `Cargo.lock`.

The source files use a fixed comment header (`@file`, `@brief`, `@details`, `@responsibilities`, `@behaviour`) and the same tags on public items. New items should follow that header so the style stays consistent across the crate.
