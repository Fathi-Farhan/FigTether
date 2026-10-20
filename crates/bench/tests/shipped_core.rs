//! The guard that keeps the benchmark on the shipped core.
//!
//! A benchmark that measures a copy of the product is not evidence about the
//! product. The tests of this file fail as soon as the benchmark stops using
//! the implementations of `figtether-core`:
//!
//! - the type guards fail to compile when the domain holds a local copy
//!   instead of the shipped cache, scheduler, or asset store;
//! - the behavioural guards fail when the benchmark reads or normalizes with
//!   anything but the shipped implementation.

use std::time::{Duration, Instant};

use figtether_bench::domain::{metadata_fields, Domain, ReadRequest};
use figtether_bench::fixtures::small_fixture;
use figtether_core::assets::AssetStore;
use figtether_core::cache::{CacheKey, SourceKey};
use figtether_core::scheduler::SchedulerStats;
use figtether_core::{Cache, InMemoryAssetStore, Scheduler};
use figtether_model::ids::SessionId;

/// Accept the shipped cache and nothing else.
fn is_shipped_cache(_: &Cache) {}

/// Accept the shipped scheduler and nothing else.
fn is_shipped_scheduler(_: &Scheduler) {}

/// Accept the shipped asset store and nothing else.
fn is_shipped_asset_store(_: &InMemoryAssetStore) {}

/// Accept the shipped asset store trait and nothing else.
fn is_shipped_asset_store_trait(_: &dyn AssetStore) {}

#[test]
fn the_domain_holds_the_shipped_core_types() {
    let domain = Domain::new();
    is_shipped_cache(&domain.cache);
    is_shipped_scheduler(&domain.scheduler);
    is_shipped_asset_store(&domain.assets);
    is_shipped_asset_store_trait(&domain.assets);
    let stats: SchedulerStats = domain.scheduler.stats();
    assert_eq!(stats.queued, 0);
}

#[test]
fn a_cache_entry_written_through_the_shipped_cache_is_visible_to_the_benchmark_reader() {
    let fixture = small_fixture();
    let session = SessionId::new();
    let mut domain = Domain::from_fixture(&fixture);
    let node_ids = fixture.node_ids();
    let key = domain.cache_key(&session, &node_ids, &metadata_fields());

    // Write through the shipped cache API.
    let cache: &Cache = &domain.cache;
    cache
        .put(
            key.clone(),
            serde_json::json!({ "nodes": [] }),
            Duration::from_secs(60),
            Instant::now(),
        )
        .expect("the shipped cache stores");

    // The benchmark reader must see the entry the shipped cache holds.
    let outcome = domain
        .cached_read(&session, &key)
        .expect("the cached read succeeds");
    assert!(outcome.hit, "the entry of the shipped cache is visible");
    assert_eq!(
        domain.cache.stats().hits,
        1,
        "the shipped cache counted the hit itself"
    );
    assert_eq!(
        domain.cache.stats().entries,
        1,
        "the shipped cache holds the entry"
    );
}

#[test]
fn the_benchmark_normalizes_with_the_shipped_normalizer() {
    let fixture = small_fixture();
    let mut domain = Domain::from_fixture(&fixture);
    let page = fixture.pages(domain.limits.max_batch_node_ids)[0].clone();
    let request = ReadRequest {
        session_id: SessionId::new(),
        client_id: "guard-client".to_string(),
        node_ids: page,
    };
    let outcome = domain
        .fresh_read(&request)
        .expect("the fresh read succeeds");
    assert!(
        outcome
            .unavailable_fields
            .contains(&"effects.radius: capability_unavailable".to_string()),
        "the shipped normalizer reports a field with its reason, got {:?}",
        outcome.unavailable_fields
    );
    assert!(
        outcome
            .unavailable_fields
            .iter()
            .all(|field| !field.starts_with("1:")),
        "the shipped normalizer reports a field once, not once per node, got {:?}",
        outcome.unavailable_fields
    );
}

#[test]
fn the_cache_key_of_the_benchmark_is_a_shipped_cache_key() {
    let fixture = small_fixture();
    let domain = Domain::from_fixture(&fixture);
    let session = SessionId::new();
    let key: CacheKey = domain.cache_key(&session, &["1:0".to_string()], &metadata_fields());
    assert_eq!(key.source, SourceKey::Session(session));
    assert_eq!(key.schema_version, figtether_model::SCHEMA_VERSION);
}
