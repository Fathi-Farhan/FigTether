//! Asset store.
//!
//! Exported assets are held in memory under an opaque id. The id is bound to
//! the grant that produced it, so a revoked grant invalidates its assets
//! immediately and an agent can never address an asset it did not earn.
//!
//! Public signatures in this file are frozen for the v0.1 implementation round.

use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::{AssetId, GrantId};
use sha2::{Digest, Sha256};

use crate::limits::ResourceLimits;

/// One stored asset.
#[derive(Debug, Clone)]
pub struct AssetEntry {
    /// Opaque id of the asset.
    pub asset_id: AssetId,
    /// Grant that produced the asset.
    pub grant_id: GrantId,
    /// MIME type.
    pub mime: String,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Raw bytes.
    pub bytes: Vec<u8>,
    /// Content hash.
    pub hash: String,
    /// When the asset was stored.
    pub stored_at: DateTime<Utc>,
}

/// Storage of exported assets.
pub trait AssetStore: Send + Sync {
    /// Store an asset and return its opaque id.
    fn put(
        &self,
        grant_id: GrantId,
        mime: &str,
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    ) -> Result<AssetId, figtether_model::errors::FigError>;

    /// Read an asset.
    fn get(&self, asset_id: &AssetId) -> Option<AssetEntry>;

    /// Drop every asset produced by a grant.
    fn invalidate_grant(&self, grant_id: &GrantId) -> usize;

    /// Number of stored assets and total bytes.
    fn stats(&self) -> (usize, u64);
}

/// In-memory asset store with a byte budget.
#[derive(Debug)]
pub struct InMemoryAssetStore {
    inner: RwLock<AssetStoreInner>,
    limits: ResourceLimits,
}

#[derive(Debug, Default)]
struct AssetStoreInner {
    entries: HashMap<AssetId, AssetEntry>,
    /// Assets in use order, the most recently used one first.
    order: Vec<AssetId>,
    bytes: u64,
}

impl InMemoryAssetStore {
    /// Create an empty store.
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            inner: RwLock::new(AssetStoreInner::default()),
            limits,
        }
    }

    /// Read the store state.
    fn read_inner(&self) -> RwLockReadGuard<'_, AssetStoreInner> {
        match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Write the store state.
    fn write_inner(&self) -> RwLockWriteGuard<'_, AssetStoreInner> {
        match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl AssetStore for InMemoryAssetStore {
    fn put(
        &self,
        grant_id: GrantId,
        mime: &str,
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    ) -> Result<AssetId, figtether_model::errors::FigError> {
        let size = bytes.len() as u64;
        if size > self.limits.max_asset_bytes {
            return Err(FigError::new(
                ErrorCode::PayloadTooLarge,
                format!(
                    "an asset of {size} bytes exceeds the asset cap of {} bytes",
                    self.limits.max_asset_bytes
                ),
            ));
        }

        let mut inner = self.write_inner();
        // Make room first: the least recently used assets leave the store.
        while inner.bytes.saturating_add(size) > self.limits.max_cache_bytes {
            let Some(victim) = inner.order.pop() else {
                break;
            };
            if let Some(entry) = inner.entries.remove(&victim) {
                inner.bytes = inner.bytes.saturating_sub(entry.bytes.len() as u64);
            }
        }
        if inner.bytes.saturating_add(size) > self.limits.max_cache_bytes {
            return Err(FigError::new(
                ErrorCode::PayloadTooLarge,
                format!(
                    "an asset of {size} bytes does not fit into the budget of {} bytes",
                    self.limits.max_cache_bytes
                ),
            ));
        }

        let asset_id = AssetId::new();
        let entry = AssetEntry {
            asset_id,
            grant_id,
            mime: mime.to_string(),
            width,
            height,
            hash: format!("sha256:{}", sha256_hex(&bytes)),
            bytes,
            stored_at: Utc::now(),
        };
        inner.bytes += size;
        inner.entries.insert(asset_id, entry);
        inner.order.insert(0, asset_id);
        Ok(asset_id)
    }

    fn get(&self, asset_id: &AssetId) -> Option<AssetEntry> {
        let mut inner = self.write_inner();
        let entry = inner.entries.get(asset_id).cloned()?;
        if let Some(index) = inner.order.iter().position(|id| id == asset_id) {
            let id = inner.order.remove(index);
            inner.order.insert(0, id);
        }
        Some(entry)
    }

    fn invalidate_grant(&self, grant_id: &GrantId) -> usize {
        let mut inner = self.write_inner();
        let victims: Vec<AssetId> = inner
            .entries
            .iter()
            .filter(|(_, entry)| &entry.grant_id == grant_id)
            .map(|(asset_id, _)| *asset_id)
            .collect();
        let removed = victims.len();
        for victim in victims {
            if let Some(entry) = inner.entries.remove(&victim) {
                inner.bytes = inner.bytes.saturating_sub(entry.bytes.len() as u64);
            }
            if let Some(index) = inner.order.iter().position(|id| id == &victim) {
                inner.order.remove(index);
            }
        }
        removed
    }

    fn stats(&self) -> (usize, u64) {
        let inner = self.read_inner();
        (inner.entries.len(), inner.bytes)
    }
}

/// The hex encoded sha256 of a byte slice.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> InMemoryAssetStore {
        InMemoryAssetStore::new(ResourceLimits::default())
    }

    #[test]
    fn an_asset_is_stored_and_read_back() {
        let store = store();
        let grant = GrantId::new();
        let id = store
            .put(grant, "image/png", 4, 4, vec![1, 2, 3, 4])
            .expect("stores");
        let entry = store.get(&id).expect("known asset");
        assert_eq!(entry.mime, "image/png");
        assert_eq!(entry.width, 4);
        assert_eq!(entry.height, 4);
        assert_eq!(entry.bytes, vec![1, 2, 3, 4]);
        assert_eq!(entry.grant_id, grant);
    }

    #[test]
    fn the_hash_is_the_sha256_of_the_bytes() {
        let store = store();
        let id = store
            .put(GrantId::new(), "image/png", 1, 1, b"hello".to_vec())
            .expect("stores");
        let entry = store.get(&id).expect("known asset");
        // The prefix matches the convention of the response envelope, so a
        // client can compare an asset hash with a content hash.
        assert_eq!(
            entry.hash,
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn the_id_is_opaque() {
        let store = store();
        let first = store
            .put(GrantId::new(), "image/png", 1, 1, b"hello".to_vec())
            .expect("stores");
        let second = store
            .put(GrantId::new(), "image/png", 1, 1, b"hello".to_vec())
            .expect("stores");
        assert_ne!(first, second, "the same bytes get different ids");
        assert!(store.get(&AssetId::new()).is_none());
    }

    #[test]
    fn an_asset_over_the_single_asset_cap_is_refused() {
        let limits = ResourceLimits {
            max_asset_bytes: 16,
            max_cache_bytes: 1024,
            ..ResourceLimits::default()
        };
        let store = InMemoryAssetStore::new(limits);
        let error = store
            .put(GrantId::new(), "image/png", 1, 1, vec![0; 17])
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::PayloadTooLarge);
        assert_eq!(store.stats().0, 0);
    }

    #[test]
    fn the_store_evicts_the_least_recently_used_asset() {
        let limits = ResourceLimits {
            max_asset_bytes: 64,
            max_cache_bytes: 100,
            ..ResourceLimits::default()
        };
        let store = InMemoryAssetStore::new(limits);
        let first = store
            .put(GrantId::new(), "image/png", 1, 1, vec![0; 40])
            .expect("stores");
        let second = store
            .put(GrantId::new(), "image/png", 1, 1, vec![1; 40])
            .expect("stores");
        // Touch the first asset so the second one becomes the oldest.
        assert!(store.get(&first).is_some());
        let third = store
            .put(GrantId::new(), "image/png", 1, 1, vec![2; 40])
            .expect("stores");
        assert!(store.get(&first).is_some());
        assert!(store.get(&second).is_none(), "the oldest asset is gone");
        assert!(store.get(&third).is_some());
        let (count, bytes) = store.stats();
        assert_eq!(count, 2);
        assert_eq!(bytes, 80);
    }

    #[test]
    fn an_asset_that_does_not_fit_the_budget_is_refused() {
        let limits = ResourceLimits {
            max_asset_bytes: 64,
            max_cache_bytes: 16,
            ..ResourceLimits::default()
        };
        let store = InMemoryAssetStore::new(limits);
        let error = store
            .put(GrantId::new(), "image/png", 1, 1, vec![0; 32])
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::PayloadTooLarge);
        assert_eq!(store.stats().0, 0);
    }

    #[test]
    fn invalidating_a_grant_drops_its_assets() {
        let store = store();
        let grant = GrantId::new();
        let other = GrantId::new();
        let kept = store
            .put(other, "image/png", 1, 1, vec![0; 8])
            .expect("stores");
        let dropped = store
            .put(grant, "image/png", 1, 1, vec![1; 8])
            .expect("stores");
        assert_eq!(store.invalidate_grant(&grant), 1);
        assert!(store.get(&dropped).is_none());
        assert!(store.get(&kept).is_some());
        let (count, bytes) = store.stats();
        assert_eq!(count, 1);
        assert_eq!(bytes, 8);
    }

    #[test]
    fn the_stats_count_the_stored_bytes() {
        let store = store();
        store
            .put(GrantId::new(), "image/png", 1, 1, vec![0; 10])
            .expect("stores");
        store
            .put(GrantId::new(), "image/png", 1, 1, vec![0; 5])
            .expect("stores");
        let (count, bytes) = store.stats();
        assert_eq!(count, 2);
        assert_eq!(bytes, 15);
    }
}
