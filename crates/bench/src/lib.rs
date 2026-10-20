//! FigTether offline domain benchmarks.
//!
//! This crate carries the workload profiles W1 to W5 of
//! `docs/06-testing/04-benchmark-plan.md` as [Criterion](https://bheisler.github.io/criterion.rs/)
//! benchmarks, plus the release evidence report generator.
//!
//! # Scope: offline domain benchmarks, not editor benchmarks
//!
//! Every benchmark in this crate runs **in process** and **without a Figma
//! editor**. The measured code is the domain half of FigTether: the cache
//! lookup, the scheduler admission, the bridge framing, the normalizer, the
//! response shaping, and the asset store.
//!
//! The domain half is not a copy. The benchmarks drive the shipped
//! `figtether-core` implementation over the real contract types:
//!
//! | Part of the domain | Shipped type |
//! | ------------------ | ------------ |
//! | design cache | `figtether_core::Cache` with `figtether_core::cache::CacheKey` |
//! | request queue | `figtether_core::Scheduler` with `figtether_core::scheduler::Job` |
//! | normalizer | `figtether_core::normalize_document` |
//! | asset store | `figtether_core::InMemoryAssetStore` implementing `figtether_core::assets::AssetStore` |
//! | resource contract | `figtether_core::limits::ResourceLimits` |
//! | scope index | `figtether_core::scope::NodeIndex` |
//!
//! The response envelope is the contract type of the model crate and the
//! bridge frames are the contract types of the protocol crate, so the bytes
//! the benchmark measures are the bytes the product answers with.
//!
//! The editor half of the system is absent, so every stage that only the
//! editor can produce is reported as `NOT_MEASURED` instead of being
//! estimated:
//!
//! | Stage            | Measured here | Why |
//! | ---------------- | ------------- | --- |
//! | `queue_ms`       | yes           | daemon scheduler admission and grant |
//! | `transport_ms`   | yes           | bridge frame encoding and decoding |
//! | `plugin_ms`      | no            | needs a running Figma plugin (N02, N06) |
//! | `normalization_ms` | yes         | daemon side model build and validation |
//! | `shaping_ms`     | yes           | daemon side envelope shaping and encoding |
//! | `render_ms`      | no            | editor export render (N03) |
//! | `e2e_ms`         | yes           | the in-process path, editor time excluded |
//!
//! The editor dependent stages, the plugin handler and the editor export
//! render, still need a Figma Desktop session. They stay `NOT_MEASURED`
//! until such a session exists, and no estimate ever replaces them.
//!
//! No number produced by this crate is a published FigTether performance
//! claim. The full protocol of the benchmark plan, including the real editor
//! session, the frozen device profile, the commit, and the review step, is
//! required before any number leaves this repository. See `benchmarks/README.md`.
//!
//! # Determinism
//!
//! Every fixture is generated from a fixed seed by the generator in
//! [`fixtures`], so two runs of the same benchmark see byte identical inputs.
//! Timings are reported as distributions; no benchmark asserts a timing
//! threshold, because a shared CI machine is not a measurement device.
//!
//! # The guard against measuring a copy
//!
//! The integration test `tests/shipped_core.rs` fails when the benchmark
//! stops using the shipped core: it asserts that the domain holds the
//! `figtether_core` types and that a cache entry written through
//! `figtether_core::Cache` is visible to the benchmark reader.

pub mod domain;
pub mod evidence;
pub mod fixtures;
pub mod host;
pub mod results;
pub mod rss;
pub mod stats;
pub mod taxonomy;

pub use domain::{CachedReadOutcome, Domain, ExportOutcome, FreshReadOutcome, StageValue};
pub use evidence::{
    render_report, Denominators, ErrorSummary, MeasurementBundle, Metric, ReleaseEvidence,
    RssSummary, Target, NOT_MEASURED,
};
pub use fixtures::{export_fixture, medium_fixture, small_fixture, Fixture};
pub use host::HostContext;
pub use stats::Distribution;
