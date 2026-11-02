# Documentation

These notes describe the `telemetry` crate as it is implemented in this repository.

| Document | Contents |
| --- | --- |
| [architecture.md](architecture.md) | Crate layout, public API, ownership, and runtime boundaries. |
| [code-flow.md](code-flow.md) | Path from `Telemetry::new` through formatting, filtering, and hourly rotation. |
| [testing-and-development.md](testing-and-development.md) | Unit tests, local checks, coverage gates, dependency policy, and CI. |

The [project README](../README.md) covers how to depend on the crate and how to call it from a product.
