//! FigTether asset store crate.
//!
//! The concrete store used by the daemon lives in `figtether-core`; this
//! crate exists so the daemon and the CLI can depend on the asset layer
//! without pulling the whole domain crate into their dependency graph.

pub use figtether_core::assets::{AssetEntry, AssetStore, InMemoryAssetStore};
