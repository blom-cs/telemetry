//! @file lib.rs
//!
//! @brief
//! Process-wide telemetry package.
//!
//! @details
//! Hosts fail-closed logging: [`TelemetryError`] codes, rolling files, a
//! prefixing event formatter, and [`telemetry::Telemetry`]. Product
//! constructs [`telemetry::Telemetry`] once in `main` and keeps it alive.
//!
//! @responsibilities
//! - Declare error, hour, prefix, rolling, and telemetry modules.
//! - Re-export [`TelemetryError`].
//!
//! @behaviour
//! Fail closed. Unit tests do not open a live display or GPU.

pub mod error;
mod hour;
mod prefix;
mod rolling;
#[allow(clippy::module_inception)]
pub mod telemetry;

pub use error::TelemetryError;
