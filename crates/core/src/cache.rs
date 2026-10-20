//! Design cache.
//!
//! The cache is a bounded LRU over encoded payloads. It exists so an agent
//! that explicitly asks for a cached read gets a fast answer, and so repeated
//! metadata reads do not hammer the editor. A cached answer is always
//! labelled: the envelope reports `freshness: cache_hit` and the age of the
//! entry.
//!
//! Cache keys bind the source, the grant epoch, the requested nodes, the
//! field projection, the export settings, and the schema version, so a
//! revoked grant can never be served from a stale entry.
//!
//! Public signatures in this file are frozen for the v0.1 implementation round.

use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::{SessionId, SnapshotId};

use crate::limits::ResourceLimits;

/// Which source a cache entry belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SourceKey {
    /// A live session.
    Session(SessionId),
    /// An immutable snapshot.
    Snapshot(SnapshotId),
}

/// Key of a cache entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Source of the entry.
    pub source: SourceKey,
    /// Grant epoch in force when the entry was written.
    pub grant_epoch: u64,
    /// Node ids the entry covers.
    pub node_ids: Vec<String>,
    /// Field projection of the request.
    pub fields: Vec<String>,
    /// Export settings, for export entries.
    pub export_settings: Option<String>,
    /// Schema version of the payload.
    pub schema_version: String,
}

/// One cached entry.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// The cached payload.
    pub value: serde_json::Value,
    /// When the data was observed.
    pub observed_at: DateTime<Utc>,
    /// When the entry was written.
    pub inserted_at: Instant,
    /// When the entry expires.
    pub expires_at: Instant,
    /// Size of the entry in bytes.
    pub bytes: u64,
    /// Grant epoch of the entry.
    pub grant_epoch: u64,
}

impl CacheEntry {
    /// Age of the entry.
    pub fn age(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.inserted_at)
    }

    /// Whether the entry is still valid.
    pub fn is_fresh(&self, now: Instant) -> bool {
        now < self.expires_at
    }
}

/// Counters of the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    /// Entries currently held.
    pub entries: usize,
    /// Bytes currently held.
    pub bytes: u64,
    /// Entries evicted because of the byte budget.
    pub evictions: u64,
    /// Lookups served from the cache.
    pub hits: u64,
    /// Lookups that missed.
    pub misses: u64,
}

/// The design cache.
#[derive(Debug)]
pub struct Cache {
    inner: RwLock<CacheInner>,
    limits: ResourceLimits,
}

#[derive(Debug, Default)]
struct CacheInner {
    entries: HashMap<CacheKey, CacheEntry>,
    /// Keys in use order, the most recently used one first.
    order: Vec<CacheKey>,
    bytes: u64,
    evictions: u64,
    hits: u64,
    misses: u64,
}

impl Cache {
    /// Create an empty cache.
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            inner: RwLock::new(CacheInner::default()),
            limits,
        }
    }

    /// Look up an entry.
    ///
    /// An expired entry is never returned; it is dropped and counted as a
    /// miss.
    pub fn get(&self, key: &CacheKey, now: Instant) -> Option<CacheEntry> {
        let mut inner = self.write_inner();
        let entry = inner.entries.get(key).cloned();
        let Some(entry) = entry else {
            inner.misses += 1;
            return None;
        };
        if !entry.is_fresh(now) {
            inner.entries.remove(key);
            inner.bytes = inner.bytes.saturating_sub(entry.bytes);
            remove_key(&mut inner.order, key);
            inner.misses += 1;
            return None;
        }
        inner.hits += 1;
        touch(&mut inner.order, key);
        Some(entry)
    }

    /// Store an entry.
    ///
    /// Fails with `PAYLOAD_TOO_LARGE` when a single value exceeds the byte
    /// budget of the cache.
    pub fn put(
        &self,
        key: CacheKey,
        value: serde_json::Value,
        ttl: Duration,
        now: Instant,
    ) -> Result<(), FigError> {
        let encoded = serde_json::to_vec(&value).map_err(|error| {
            FigError::new(
                ErrorCode::InternalError,
                format!("the cache payload could not be encoded: {error}"),
            )
        })?;
        let bytes = encoded.len() as u64;
        if bytes > self.limits.max_cache_bytes {
            return Err(FigError::new(
                ErrorCode::PayloadTooLarge,
                format!(
                    "a cache entry of {bytes} bytes exceeds the budget of {} bytes",
                    self.limits.max_cache_bytes
                ),
            ));
        }

        let mut inner = self.write_inner();
        if let Some(previous) = inner.entries.remove(&key) {
            inner.bytes = inner.bytes.saturating_sub(previous.bytes);
            remove_key(&mut inner.order, &key);
        }
        let entry = CacheEntry {
            value,
            observed_at: Utc::now(),
            inserted_at: now,
            expires_at: now + ttl,
            bytes,
            grant_epoch: key.grant_epoch,
        };
        inner.bytes += bytes;
        inner.entries.insert(key.clone(), entry);
        inner.order.insert(0, key.clone());

        // Evict the least recently used entries until the budget holds.
        while inner.bytes > self.limits.max_cache_bytes {
            let Some(victim) = inner.order.pop() else {
                break;
            };
            if let Some(entry) = inner.entries.remove(&victim) {
                inner.bytes = inner.bytes.saturating_sub(entry.bytes);
            }
            inner.evictions += 1;
        }
        Ok(())
    }

    /// Drop every entry written under a grant epoch.
    pub fn invalidate_grant(&self, grant_epoch: u64) -> usize {
        let mut inner = self.write_inner();
        let victims: Vec<CacheKey> = inner
            .entries
            .keys()
            .filter(|key| key.grant_epoch == grant_epoch)
            .cloned()
            .collect();
        let removed = victims.len();
        for victim in victims {
            remove_entry(&mut inner, &victim);
        }
        removed
    }

    /// Drop every entry of a source.
    pub fn invalidate_source(&self, source: &SourceKey) -> usize {
        let mut inner = self.write_inner();
        let victims: Vec<CacheKey> = inner
            .entries
            .keys()
            .filter(|key| &key.source == source)
            .cloned()
            .collect();
        let removed = victims.len();
        for victim in victims {
            remove_entry(&mut inner, &victim);
        }
        removed
    }

    /// Remove expired entries.
    pub fn evict_expired(&self, now: Instant) -> usize {
        let mut inner = self.write_inner();
        let victims: Vec<CacheKey> = inner
            .entries
            .iter()
            .filter(|(_, entry)| !entry.is_fresh(now))
            .map(|(key, _)| key.clone())
            .collect();
        let removed = victims.len();
        for victim in victims {
            remove_entry(&mut inner, &victim);
        }
        removed
    }

    /// Counters of the cache.
    pub fn stats(&self) -> CacheStats {
        let inner = self.read_inner();
        CacheStats {
            entries: inner.entries.len(),
            bytes: inner.bytes,
            evictions: inner.evictions,
            hits: inner.hits,
            misses: inner.misses,
        }
    }

    /// Read the cache state.
    fn read_inner(&self) -> RwLockReadGuard<'_, CacheInner> {
        match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Write the cache state.
    fn write_inner(&self) -> RwLockWriteGuard<'_, CacheInner> {
        match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Remove an entry and its bytes from the cache.
fn remove_entry(inner: &mut CacheInner, key: &CacheKey) {
    if let Some(entry) = inner.entries.remove(key) {
        inner.bytes = inner.bytes.saturating_sub(entry.bytes);
    }
    remove_key(&mut inner.order, key);
}

/// Drop a key from the use order.
fn remove_key(order: &mut Vec<CacheKey>, key: &CacheKey) {
    if let Some(index) = order.iter().position(|candidate| candidate == key) {
        order.remove(index);
    }
}

/// Move a key to the front of the use order.
fn touch(order: &mut Vec<CacheKey>, key: &CacheKey) {
    if let Some(index) = order.iter().position(|candidate| candidate == key) {
        let key = order.remove(index);
        order.insert(0, key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cache() -> Cache {
        Cache::new(ResourceLimits::default())
    }

    fn key(epoch: u64, node: &str) -> CacheKey {
        CacheKey {
            source: SourceKey::Session(SessionId::new()),
            grant_epoch: epoch,
            node_ids: vec![node.to_string()],
            fields: vec!["name".to_string()],
            export_settings: None,
            schema_version: "1.0".to_string(),
        }
    }

    #[test]
    fn a_stored_entry_is_served_again() {
        let cache = cache();
        let now = Instant::now();
        let key = key(1, "1:2");
        cache
            .put(
                key.clone(),
                json!({"name": "cart"}),
                Duration::from_secs(2),
                now,
            )
            .expect("stores");
        let entry = cache.get(&key, now).expect("a hit");
        assert_eq!(entry.value["name"], "cart");
        assert_eq!(entry.grant_epoch, 1);
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn an_unknown_key_is_a_miss() {
        let cache = cache();
        let now = Instant::now();
        assert!(cache.get(&key(1, "1:2"), now).is_none());
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);
    }

    #[test]
    fn an_expired_entry_is_not_returned() {
        let cache = cache();
        let now = Instant::now();
        let key = key(1, "1:2");
        cache
            .put(
                key.clone(),
                json!({"name": "cart"}),
                Duration::from_millis(10),
                now,
            )
            .expect("stores");
        let later = now + Duration::from_millis(50);
        assert!(cache.get(&key, later).is_none());
        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 0, "the expired entry is dropped");
    }

    #[test]
    fn evict_expired_drops_the_stale_entries() {
        let cache = cache();
        let now = Instant::now();
        let fresh = key(1, "1:2");
        let stale = key(1, "1:3");
        cache
            .put(fresh.clone(), json!({"a": 1}), Duration::from_secs(60), now)
            .expect("stores");
        cache
            .put(
                stale.clone(),
                json!({"a": 2}),
                Duration::from_millis(5),
                now,
            )
            .expect("stores");
        let later = now + Duration::from_millis(50);
        assert_eq!(cache.evict_expired(later), 1);
        assert_eq!(cache.stats().entries, 1);
        assert!(cache.get(&fresh, later).is_some());
    }

    #[test]
    fn a_value_larger_than_the_budget_is_refused() {
        let limits = ResourceLimits {
            max_cache_bytes: 32,
            ..ResourceLimits::default()
        };
        let cache = Cache::new(limits);
        let error = cache
            .put(
                key(1, "1:2"),
                json!({"payload": "x".repeat(128)}),
                Duration::from_secs(60),
                Instant::now(),
            )
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::PayloadTooLarge);
        assert_eq!(cache.stats().entries, 0);
    }

    #[test]
    fn the_least_recently_used_entry_is_evicted() {
        // Two entries fit, three do not.
        let limits = ResourceLimits {
            max_cache_bytes: 120,
            ..ResourceLimits::default()
        };
        let cache = Cache::new(limits);
        let now = Instant::now();
        let first = key(1, "1:2");
        let second = key(1, "1:3");
        let third = key(1, "1:4");
        let payload = json!({"payload": "x".repeat(40)});
        cache
            .put(first.clone(), payload.clone(), Duration::from_secs(60), now)
            .expect("stores");
        cache
            .put(
                second.clone(),
                payload.clone(),
                Duration::from_secs(60),
                now,
            )
            .expect("stores");
        // Touch the first entry so the second one becomes the oldest.
        assert!(cache.get(&first, now).is_some());
        cache
            .put(third.clone(), payload, Duration::from_secs(60), now)
            .expect("stores");
        let stats = cache.stats();
        assert_eq!(stats.evictions, 1);
        assert_eq!(stats.entries, 2);
        assert!(cache.get(&first, now).is_some());
        assert!(
            cache.get(&second, now).is_none(),
            "the oldest entry is gone"
        );
        assert!(cache.get(&third, now).is_some());
    }

    #[test]
    fn invalidating_a_grant_drops_its_entries() {
        let cache = cache();
        let now = Instant::now();
        let old = key(1, "1:2");
        let new = key(2, "1:2");
        cache
            .put(old.clone(), json!({"a": 1}), Duration::from_secs(60), now)
            .expect("stores");
        cache
            .put(new.clone(), json!({"a": 2}), Duration::from_secs(60), now)
            .expect("stores");
        assert_eq!(cache.invalidate_grant(1), 1);
        assert!(cache.get(&old, now).is_none());
        assert!(cache.get(&new, now).is_some());
        assert_eq!(cache.stats().entries, 1);
    }

    #[test]
    fn invalidating_a_source_drops_its_entries() {
        let cache = cache();
        let now = Instant::now();
        let session = SessionId::new();
        let mut session_key = key(1, "1:2");
        session_key.source = SourceKey::Session(session);
        let mut snapshot_key = key(1, "1:2");
        snapshot_key.source = SourceKey::Snapshot(SnapshotId::new());
        cache
            .put(
                session_key.clone(),
                json!({"a": 1}),
                Duration::from_secs(60),
                now,
            )
            .expect("stores");
        cache
            .put(
                snapshot_key.clone(),
                json!({"a": 2}),
                Duration::from_secs(60),
                now,
            )
            .expect("stores");
        assert_eq!(cache.invalidate_source(&session_key.source), 1);
        assert!(cache.get(&session_key, now).is_none());
        assert!(cache.get(&snapshot_key, now).is_some());
    }

    #[test]
    fn the_key_is_used_as_given() {
        let cache = cache();
        let now = Instant::now();
        let mut first = key(1, "1:2");
        first.node_ids = vec!["1:2".to_string(), "1:3".to_string()];
        let mut second = first.clone();
        second.node_ids = vec!["1:3".to_string(), "1:2".to_string()];
        cache
            .put(first.clone(), json!({"a": 1}), Duration::from_secs(60), now)
            .expect("stores");
        cache
            .put(
                second.clone(),
                json!({"a": 2}),
                Duration::from_secs(60),
                now,
            )
            .expect("stores");
        assert_eq!(cache.stats().entries, 2, "the order of node ids matters");
        assert_eq!(cache.get(&first, now).expect("hit").value["a"], 1);
        assert_eq!(cache.get(&second, now).expect("hit").value["a"], 2);
    }

    #[test]
    fn a_replaced_entry_does_not_count_twice() {
        let cache = cache();
        let now = Instant::now();
        let key = key(1, "1:2");
        cache
            .put(key.clone(), json!({"a": 1}), Duration::from_secs(60), now)
            .expect("stores");
        let first_bytes = cache.stats().bytes;
        cache
            .put(key.clone(), json!({"a": 2}), Duration::from_secs(60), now)
            .expect("stores");
        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.bytes, first_bytes);
    }

    #[test]
    fn the_expiry_is_the_write_time_plus_the_ttl() {
        let cache = cache();
        let now = Instant::now();
        let key = key(1, "1:2");
        cache
            .put(key.clone(), json!({"a": 1}), Duration::from_secs(2), now)
            .expect("stores");
        let entry = cache.get(&key, now).expect("hit");
        assert_eq!(entry.expires_at, now + Duration::from_secs(2));
        assert_eq!(entry.age(now), Duration::ZERO);
    }
}
