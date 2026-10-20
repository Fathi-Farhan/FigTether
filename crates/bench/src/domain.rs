//! The in-process domain the benchmarks measure.
//!
//! This module is the offline stand-in for the daemon side of FigTether. It
//! wires the shipped `figtether-core` implementation to the contract types of
//! the model and the protocol crates, the way
//! `docs/02-architecture/01-system-design.md` describes them:
//!
//! | Stage | Implementation here | Source of truth |
//! | ----- | ------------------- | --------------- |
//! | queue | `figtether_core::Scheduler` | `crates/core/src/scheduler.rs` |
//! | cache | `figtether_core::Cache` | `crates/core/src/cache.rs` |
//! | normalization | `figtether_core::normalize_document` | `crates/core/src/normalize.rs` |
//! | response shaping | `figtether_model::Envelope` | the envelope contract of the model crate |
//! | transport | `figtether_protocol::BridgeFrame` and `figtether_protocol::ChunkAssembler` | the bridge protocol crate |
//! | asset store | `figtether_core::InMemoryAssetStore` | `crates/core/src/assets.rs` |
//!
//! The scheduler, the cache, the normalizer, and the asset store are the
//! shipped implementation, not a copy of it. A number measured here is
//! therefore a number about the code the daemon runs, with the editor still
//! absent.
//!
//! The editor is absent. Every stage that only a running Figma plugin could
//! produce, `plugin_ms` and the editor part of `render_ms`, is reported as
//! [`StageValue::not_measured`] and never estimated.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use figtether_core::assets::AssetStore;
use figtether_core::cache::{CacheKey, SourceKey};
use figtether_core::limits::ResourceLimits;
use figtether_core::metrics::{Lane, Metrics};
use figtether_core::scheduler::{Job, JobKind, JobStatus, Priority};
use figtether_core::scope::NodeIndex;
use figtether_core::{Cache, InMemoryAssetStore, Scheduler};
use figtether_model::envelope::{Coverage, Envelope, Observation, SourceRef};
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::{AssetId, GrantId, JobId, ObservationId, SessionId, SnapshotId};
use figtether_model::normalized::{NodeSummary, NormalizedNode};
use figtether_model::{Consistency, Freshness};
use figtether_protocol::plugin::{PluginDocument, PluginNode};
use figtether_protocol::{
    sha256_hex, BridgeFrame, BridgeOp, ChunkAssembler, ChunkPayload, MessageType,
};
use serde::{Deserialize, Serialize};

use crate::fixtures::Fixture;

/// The field projection the benchmark profiles use for metadata reads.
///
/// The projection is fixed before the run, exactly as the plan requires, and
/// it is the projection that keeps a metadata response inside the 8 KiB
/// metadata budget.
pub const METADATA_FIELDS: [&str; 6] = [
    "id",
    "node_type",
    "name",
    "visible",
    "locked",
    "child_count",
];

/// The fixed field projection of the metadata reads, as owned strings.
pub fn metadata_fields() -> Vec<String> {
    METADATA_FIELDS
        .iter()
        .map(|field| (*field).to_string())
        .collect()
}

/// Size of one chunk of an export transfer.
pub const EXPORT_CHUNK_BYTES: usize = 64 * 1024;

/// Size of the payload stand in of one export.
pub const EXPORT_PAYLOAD_BYTES: usize = 256 * 1024;

/// One stage duration that may not be measurable in the offline domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageValue {
    /// The measured duration in milliseconds, when the stage was measured.
    pub value: Option<f64>,
    /// Why the stage was not measured, when it was not.
    pub reason: Option<String>,
}

impl StageValue {
    /// A measured stage.
    pub fn measured(value: f64) -> Self {
        Self {
            value: Some(value),
            reason: None,
        }
    }

    /// A stage that could not be measured offline.
    pub fn not_measured(reason: impl Into<String>) -> Self {
        Self {
            value: None,
            reason: Some(reason.into()),
        }
    }

    /// Whether the stage carries a measurement.
    pub fn is_measured(&self) -> bool {
        self.value.is_some()
    }

    /// The measured value, or zero when the stage was not measured.
    pub fn value_or_zero(&self) -> f64 {
        self.value.unwrap_or(0.0)
    }

    /// Render the stage for a report.
    pub fn render(&self) -> String {
        match (self.value, self.reason.as_ref()) {
            (Some(value), _) => crate::stats::format_number(value),
            (None, Some(reason)) => format!("{} ({reason})", crate::NOT_MEASURED),
            (None, None) => crate::NOT_MEASURED.to_string(),
        }
    }
}

/// Milliseconds between two instants.
fn elapsed_ms(start: Instant, end: Instant) -> f64 {
    end.saturating_duration_since(start).as_secs_f64() * 1000.0
}

/// Project normalized nodes onto the metadata field projection.
pub fn project_metadata(nodes: &[NormalizedNode]) -> Vec<NodeSummary> {
    nodes.iter().map(|node| node.summary.clone()).collect()
}

/// One cached metadata read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedReadOutcome {
    /// Whether the cache answered.
    pub hit: bool,
    /// End to end latency of the read in milliseconds.
    pub e2e_ms: f64,
    /// Size of the encoded response in bytes.
    pub response_bytes: usize,
    /// Size of the encoded metadata payload in bytes.
    pub metadata_bytes: usize,
    /// Age of the cache entry in milliseconds, when the read was a hit.
    pub cache_age_ms: Option<u64>,
}

/// One fresh read through the scheduler and the normalizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshReadOutcome {
    /// Queue wait in milliseconds.
    pub queue_ms: StageValue,
    /// Bridge framing in milliseconds.
    pub transport_ms: StageValue,
    /// Editor handler time. Never measured offline.
    pub plugin_ms: StageValue,
    /// Model build and validation in milliseconds.
    pub normalization_ms: StageValue,
    /// Response shaping in milliseconds.
    pub shaping_ms: StageValue,
    /// End to end latency in milliseconds, editor time excluded.
    pub e2e_ms: f64,
    /// Size of the encoded response in bytes.
    pub response_bytes: usize,
    /// Size of the encoded metadata payload in bytes.
    pub metadata_bytes: usize,
    /// Whether the request reached a running plugin. Never measured offline.
    pub reached_plugin: Option<bool>,
    /// Whether the encoded response stayed inside the byte budget.
    pub within_budget: bool,
    /// The fields the shipped normalizer could not provide, with their reason.
    ///
    /// The list is the one of the normalized response coverage, so it is the
    /// vocabulary of `figtether_core::normalize_document` and not a local
    /// rendering of it.
    pub unavailable_fields: Vec<String>,
}

/// One export through the asset store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOutcome {
    /// Queue wait in milliseconds.
    pub queue_ms: StageValue,
    /// Chunked transfer and reassembly in milliseconds.
    pub transport_ms: StageValue,
    /// Editor render time. Never measured offline.
    pub render_ms: StageValue,
    /// Preparation of the offline payload stand in in milliseconds.
    pub payload_prepare_ms: StageValue,
    /// Asset store write in milliseconds.
    pub store_ms: StageValue,
    /// Asset store read in milliseconds.
    pub retrieve_ms: StageValue,
    /// End to end latency in milliseconds, editor render excluded.
    pub e2e_ms: f64,
    /// Size of the stored asset in bytes.
    pub asset_bytes: usize,
    /// Opaque asset id.
    pub asset_id: AssetId,
}

/// One snapshot read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotReadOutcome {
    /// End to end latency in milliseconds.
    pub e2e_ms: f64,
    /// Size of the encoded response in bytes.
    pub response_bytes: usize,
}

/// A fresh read request.
#[derive(Debug, Clone)]
pub struct ReadRequest {
    /// Session the request belongs to.
    pub session_id: SessionId,
    /// Client that submitted the request.
    pub client_id: String,
    /// Node ids to read.
    pub node_ids: Vec<String>,
}

/// An export request.
#[derive(Debug, Clone)]
pub struct ExportRequest {
    /// Session the request belongs to.
    pub session_id: SessionId,
    /// Client that submitted the request.
    pub client_id: String,
    /// Node to export.
    pub node_id: String,
    /// Pixel width of the export.
    pub width: u32,
    /// Pixel height of the export.
    pub height: u32,
    /// Export scale.
    pub scale: f64,
    /// Export format, for example `PNG`.
    pub format: String,
}

/// The in-process domain.
///
/// The cache, the scheduler, and the asset store are the shipped types of
/// `figtether-core`. The benchmark never re-implements them, so a number
/// measured here is a number about the daemon code path.
#[derive(Debug)]
pub struct Domain {
    /// Resource contract in force.
    pub limits: ResourceLimits,
    /// Counters of the lanes the benchmark records itself.
    ///
    /// The shipped scheduler owns its own counters, because the shipped
    /// `Scheduler` takes its metrics by value. The scheduler counters are
    /// read with [`Scheduler::stats`].
    pub metrics: Arc<Metrics>,
    /// The design cache of the daemon.
    pub cache: Cache,
    /// The request queue of the daemon.
    pub scheduler: Scheduler,
    /// The asset store of the daemon.
    pub assets: InMemoryAssetStore,
    /// Scope index of the observed nodes.
    pub index: NodeIndex,
    /// The document the mock plugin serves.
    pub document: PluginDocument,
    /// Captured snapshots.
    pub snapshots: HashMap<SnapshotId, Vec<NormalizedNode>>,
    /// Grant in force.
    pub grant_id: GrantId,
}

impl Default for Domain {
    fn default() -> Self {
        Self::new()
    }
}

impl Domain {
    /// Create a domain with an empty document.
    pub fn new() -> Self {
        Self::with_document(PluginDocument {
            document_label: "empty".to_string(),
            page_name: "Page 1".to_string(),
            nodes: Vec::new(),
            capabilities: Default::default(),
        })
    }

    /// Create a domain serving the given document.
    pub fn with_document(document: PluginDocument) -> Self {
        let limits = ResourceLimits::default();
        let metrics = Arc::new(Metrics::new());
        let mut index = NodeIndex::new();
        for node in &document.nodes {
            if let Some(parent) = &node.parent_id {
                index.insert(node.id.clone(), parent.clone());
            }
        }
        Self {
            cache: Cache::new(limits.clone()),
            scheduler: Scheduler::new(limits.clone(), Metrics::new()),
            assets: InMemoryAssetStore::new(limits.clone()),
            limits,
            metrics,
            index,
            document,
            snapshots: HashMap::new(),
            grant_id: GrantId::new(),
        }
    }

    /// Build a domain from a fixture.
    pub fn from_fixture(fixture: &Fixture) -> Self {
        Self::with_document(fixture.document.clone())
    }

    /// Build the cache key of a metadata projection.
    pub fn cache_key(
        &self,
        session: &SessionId,
        node_ids: &[String],
        fields: &[String],
    ) -> CacheKey {
        CacheKey {
            source: SourceKey::Session(*session),
            grant_epoch: 0,
            node_ids: node_ids.to_vec(),
            fields: fields.to_vec(),
            export_settings: None,
            schema_version: figtether_model::SCHEMA_VERSION.to_string(),
        }
    }

    /// Warm the cache with the metadata projection of every page of a fixture.
    pub fn warm_cache(
        &mut self,
        session: &SessionId,
        fixture: &Fixture,
        ttl: Duration,
    ) -> Result<usize, FigError> {
        let (nodes, _) = figtether_core::normalize_document(&fixture.document);
        let mut warmed = 0;
        for page in fixture.pages(self.limits.max_batch_node_ids) {
            let subset: Vec<NormalizedNode> = nodes
                .iter()
                .filter(|node| page.contains(&node.summary.id))
                .cloned()
                .collect();
            let projection = project_metadata(&subset);
            let key = self.cache_key(session, &page, &metadata_fields());
            self.cache.put(
                key,
                serde_json::json!({ "nodes": projection }),
                ttl,
                Instant::now(),
            )?;
            warmed += 1;
        }
        Ok(warmed)
    }

    /// Perform one cached metadata read.
    pub fn cached_read(
        &mut self,
        session: &SessionId,
        key: &CacheKey,
    ) -> Result<CachedReadOutcome, FigError> {
        let started = Instant::now();
        let now = Instant::now();
        let Some(entry) = self.cache.get(key, now) else {
            return Ok(CachedReadOutcome {
                hit: false,
                e2e_ms: elapsed_ms(started, Instant::now()),
                response_bytes: 0,
                metadata_bytes: 0,
                cache_age_ms: None,
            });
        };
        let metadata_bytes = serde_json::to_vec(&entry.value)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        let envelope = self.shape_envelope(
            session,
            entry.value.clone(),
            Coverage {
                root_ids: key.node_ids.clone(),
                returned_nodes: 0,
                truncated: false,
                next_cursor: None,
                unavailable_fields: Vec::new(),
            },
            Consistency::Cached,
            Freshness::CacheHit,
            Some(entry.age(now).as_millis() as u64),
        );
        let bytes = serde_json::to_vec(&envelope).unwrap_or_default();
        self.metrics.record_request(Lane::CacheHit);
        let e2e_ms = elapsed_ms(started, Instant::now());
        self.metrics
            .record_latency(Duration::from_secs_f64(e2e_ms / 1000.0));
        Ok(CachedReadOutcome {
            hit: true,
            e2e_ms,
            response_bytes: bytes.len(),
            metadata_bytes,
            cache_age_ms: Some(entry.age(now).as_millis() as u64),
        })
    }

    /// Perform one fresh read through the scheduler and the normalizer.
    pub fn fresh_read(&mut self, request: &ReadRequest) -> Result<FreshReadOutcome, FigError> {
        let started = Instant::now();
        if !self.limits.batch_is_valid(request.node_ids.len()) {
            return Err(FigError::new(
                ErrorCode::InvalidLimit,
                format!(
                    "a batch of {} node ids exceeds the limit of {}",
                    request.node_ids.len(),
                    self.limits.max_batch_node_ids
                ),
            ));
        }
        let enqueued_at = Instant::now();
        let job = Job {
            job_id: JobId::new(),
            kind: JobKind::Read,
            session_id: Some(request.session_id),
            client_id: request.client_id.clone(),
            params: serde_json::json!({ "node_ids": request.node_ids }),
            deadline: enqueued_at + self.limits.read_timeout,
            enqueued_at,
            priority: Priority::SmallRead,
            attempt: 0,
        };
        let job_id = self.scheduler.submit(job)?;
        let granted_at = Instant::now();
        let Some(job) = self.scheduler.next_ready() else {
            self.scheduler.complete(&job_id, JobStatus::TimedOut);
            return Err(
                FigError::new(ErrorCode::Timeout, "the read expired in the queue")
                    .with_request_id(job_id.to_string()),
            );
        };
        let queue_ms = elapsed_ms(job.enqueued_at, granted_at);
        match self.run_fresh_read(request, &job_id, queue_ms, started) {
            Ok(outcome) => {
                self.scheduler.complete(&job_id, JobStatus::Completed);
                Ok(outcome)
            }
            Err(error) => {
                // The job leaves the scheduler on every path, otherwise it
                // would hold the session lane of the shipped scheduler.
                self.scheduler.complete(&job_id, JobStatus::Failed);
                Err(error)
            }
        }
    }

    /// Run the body of a fresh read after the scheduler granted the job.
    fn run_fresh_read(
        &self,
        request: &ReadRequest,
        job_id: &JobId,
        queue_ms: f64,
        started: Instant,
    ) -> Result<FreshReadOutcome, FigError> {
        let transport_started = Instant::now();
        let frame = BridgeFrame::new(
            MessageType::Request,
            job_id.to_string(),
            serde_json::json!({
                "op": BridgeOp::ReadNodes.as_str(),
                "params": { "node_ids": request.node_ids },
            }),
        )
        .with_session(request.session_id)
        .with_deadline_ms(self.limits.read_timeout.as_millis() as u64);
        let encoded = frame.to_bytes().map_err(|error| {
            FigError::new(ErrorCode::PayloadTooLarge, error.to_string())
                .with_request_id(job_id.to_string())
        })?;
        BridgeFrame::from_bytes(&encoded).map_err(|error| {
            FigError::new(ErrorCode::ProtocolMismatch, error.to_string())
                .with_request_id(job_id.to_string())
        })?;
        let transport_ms = elapsed_ms(transport_started, Instant::now());

        let page = self.page_of(&request.node_ids);

        let normalization_started = Instant::now();
        let (nodes, unavailable) = figtether_core::normalize_document(&page);
        let normalization_ms = elapsed_ms(normalization_started, Instant::now());

        let shaping_started = Instant::now();
        let projection = project_metadata(&nodes);
        let payload = serde_json::json!({ "nodes": projection });
        let metadata_bytes = serde_json::to_vec(&payload)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        let coverage = Coverage {
            root_ids: request.node_ids.clone(),
            returned_nodes: projection.len() as u32,
            truncated: false,
            next_cursor: None,
            unavailable_fields: unavailable.clone(),
        };
        let envelope = self.shape_envelope(
            &request.session_id,
            payload,
            coverage,
            Consistency::Fresh,
            Freshness::FreshRead,
            None,
        );
        let bytes = serde_json::to_vec(&envelope).unwrap_or_default();
        let shaping_ms = elapsed_ms(shaping_started, Instant::now());

        self.metrics.record_request(Lane::FreshRead);
        let e2e_ms = elapsed_ms(started, Instant::now());
        self.metrics
            .record_latency(Duration::from_secs_f64(e2e_ms / 1000.0));
        Ok(FreshReadOutcome {
            queue_ms: StageValue::measured(queue_ms),
            transport_ms: StageValue::measured(transport_ms),
            plugin_ms: StageValue::not_measured("no Figma plugin session in the offline domain"),
            normalization_ms: StageValue::measured(normalization_ms),
            shaping_ms: StageValue::measured(shaping_ms),
            e2e_ms,
            response_bytes: bytes.len(),
            metadata_bytes,
            reached_plugin: None,
            within_budget: bytes.len() <= self.limits.max_json_response_bytes,
            unavailable_fields: unavailable,
        })
    }

    /// Perform one export through the asset store.
    pub fn export_asset(&mut self, request: &ExportRequest) -> Result<ExportOutcome, FigError> {
        let started = Instant::now();
        if !self.limits.pixels_are_valid(request.width, request.height) {
            return Err(FigError::new(
                ErrorCode::InvalidLimit,
                format!(
                    "an export of {}x{} exceeds the pixel cap",
                    request.width, request.height
                ),
            ));
        }
        if !self.limits.scale_is_valid(request.scale) {
            return Err(FigError::new(
                ErrorCode::InvalidLimit,
                format!(
                    "export scale {} is outside the accepted range",
                    request.scale
                ),
            ));
        }
        let enqueued_at = Instant::now();
        let job = Job {
            job_id: JobId::new(),
            kind: JobKind::Export,
            session_id: Some(request.session_id),
            client_id: request.client_id.clone(),
            params: serde_json::json!({ "node_id": request.node_id }),
            deadline: enqueued_at + self.limits.export_timeout,
            enqueued_at,
            priority: Priority::Export,
            attempt: 0,
        };
        let job_id = self.scheduler.submit(job)?;
        let granted_at = Instant::now();
        let Some(job) = self.scheduler.next_ready() else {
            self.scheduler.complete(&job_id, JobStatus::TimedOut);
            return Err(
                FigError::new(ErrorCode::Timeout, "the export expired in the queue")
                    .with_request_id(job_id.to_string()),
            );
        };
        let queue_ms = elapsed_ms(job.enqueued_at, granted_at);
        match self.run_export(request, &job_id, queue_ms, started) {
            Ok(outcome) => {
                self.scheduler.complete(&job_id, JobStatus::Completed);
                Ok(outcome)
            }
            Err(error) => {
                // The job leaves the scheduler on every path, otherwise it
                // would hold the export lane of the shipped scheduler.
                self.scheduler.complete(&job_id, JobStatus::Failed);
                Err(error)
            }
        }
    }

    /// Run the body of an export after the scheduler granted the job.
    fn run_export(
        &self,
        request: &ExportRequest,
        job_id: &JobId,
        queue_ms: f64,
        started: Instant,
    ) -> Result<ExportOutcome, FigError> {
        let prepare_started = Instant::now();
        let payload =
            synthetic_export_bytes(request.width, request.height, request.node_id.as_bytes());
        let payload_prepare_ms = elapsed_ms(prepare_started, Instant::now());

        let transport_started = Instant::now();
        let mut assembler = ChunkAssembler::new(job_id.to_string(), self.limits.max_transfer_bytes);
        let total = payload.len() as u64;
        let mut offset = 0usize;
        let mut sequence = 0u32;
        let mut assembled = Vec::new();
        while offset < payload.len() {
            let end = (offset + EXPORT_CHUNK_BYTES).min(payload.len());
            let is_last = end == payload.len();
            let chunk = ChunkPayload {
                transfer_id: job_id.to_string(),
                sequence,
                bytes: payload[offset..end].to_vec(),
                total_bytes: total,
                checksum: if is_last {
                    Some(sha256_hex(&payload))
                } else {
                    None
                },
            };
            if let Some(done) = assembler
                .accept(&chunk)
                .map_err(|error| FigError::new(ErrorCode::ProtocolMismatch, error.to_string()))?
            {
                assembled = done;
            }
            offset = end;
            sequence += 1;
        }
        let transport_ms = elapsed_ms(transport_started, Instant::now());

        let store_started = Instant::now();
        let mime = if request.format.eq_ignore_ascii_case("svg") {
            "image/svg+xml"
        } else {
            "image/png"
        };
        let asset_id = self.assets.put(
            self.grant_id,
            mime,
            request.width,
            request.height,
            assembled,
        )?;
        let store_ms = elapsed_ms(store_started, Instant::now());

        let retrieve_started = Instant::now();
        let entry = self.assets.get(&asset_id);
        let retrieve_ms = elapsed_ms(retrieve_started, Instant::now());
        let Some(entry) = entry else {
            return Err(
                FigError::new(ErrorCode::InternalError, "the asset was not stored")
                    .with_request_id(job_id.to_string()),
            );
        };

        self.metrics.record_request(Lane::Export);
        let e2e_ms = elapsed_ms(started, Instant::now());
        self.metrics
            .record_latency(Duration::from_secs_f64(e2e_ms / 1000.0));
        Ok(ExportOutcome {
            queue_ms: StageValue::measured(queue_ms),
            transport_ms: StageValue::measured(transport_ms),
            render_ms: StageValue::not_measured("no Figma editor render in the offline domain"),
            payload_prepare_ms: StageValue::measured(payload_prepare_ms),
            store_ms: StageValue::measured(store_ms),
            retrieve_ms: StageValue::measured(retrieve_ms),
            e2e_ms,
            asset_bytes: entry.bytes.len(),
            asset_id,
        })
    }

    /// Capture a snapshot of the current document.
    pub fn capture_snapshot(&mut self, snapshot_id: SnapshotId) -> usize {
        let (nodes, _) = figtether_core::normalize_document(&self.document);
        let count = nodes.len();
        self.snapshots.insert(snapshot_id, nodes);
        count
    }

    /// Read a captured snapshot.
    pub fn snapshot_read(
        &mut self,
        session: &SessionId,
        snapshot_id: &SnapshotId,
    ) -> Result<SnapshotReadOutcome, FigError> {
        let started = Instant::now();
        let Some(nodes) = self.snapshots.get(snapshot_id) else {
            return Err(FigError::new(
                ErrorCode::CursorExpired,
                "the snapshot is not known to this daemon",
            ));
        };
        let projection = project_metadata(nodes);
        let envelope = self.shape_envelope(
            session,
            serde_json::json!({ "nodes": projection }),
            Coverage {
                root_ids: Vec::new(),
                returned_nodes: projection.len() as u32,
                truncated: false,
                next_cursor: None,
                unavailable_fields: Vec::new(),
            },
            Consistency::BestEffort,
            Freshness::FreshRead,
            None,
        );
        let bytes = serde_json::to_vec(&envelope).unwrap_or_default();
        self.metrics.record_request(Lane::Snapshot);
        Ok(SnapshotReadOutcome {
            e2e_ms: elapsed_ms(started, Instant::now()),
            response_bytes: bytes.len(),
        })
    }

    /// The plugin document of the requested node ids.
    fn page_of(&self, node_ids: &[String]) -> PluginDocument {
        let nodes: Vec<PluginNode> = self
            .document
            .nodes
            .iter()
            .filter(|node| node_ids.contains(&node.id))
            .cloned()
            .collect();
        PluginDocument {
            document_label: self.document.document_label.clone(),
            page_name: self.document.page_name.clone(),
            nodes,
            capabilities: self.document.capabilities.clone(),
        }
    }

    /// Build the response envelope of a read.
    fn shape_envelope(
        &self,
        session: &SessionId,
        data: serde_json::Value,
        coverage: Coverage,
        consistency: Consistency,
        freshness: Freshness,
        cache_age_ms: Option<u64>,
    ) -> Envelope {
        shape_envelope(
            session,
            data,
            coverage,
            consistency,
            freshness,
            cache_age_ms,
        )
    }
}

/// Build the response envelope of a read.
///
/// The envelope is the daemon side response rendering: the provenance, the
/// observation window, the coverage, and the content hash.
pub fn shape_envelope(
    session: &SessionId,
    data: serde_json::Value,
    coverage: Coverage,
    consistency: Consistency,
    freshness: Freshness,
    cache_age_ms: Option<u64>,
) -> Envelope {
    let now = chrono::Utc::now();
    Envelope::new(
        format!("obs_{}", ObservationId::new()),
        SourceRef::plugin_live(*session),
        Observation {
            observation_id: ObservationId::new(),
            started_at: now,
            observed_at: now,
            consistency,
            freshness,
            cache_age_ms,
            content_hash: None,
        },
        coverage,
        data,
    )
    .with_content_hash()
}

/// Build and encode a response envelope.
///
/// The returned bytes are what an agent receives, so their length is the
/// length the 64 KiB response budget applies to.
pub fn render_response(
    session: &SessionId,
    data: serde_json::Value,
    coverage: Coverage,
    consistency: Consistency,
    freshness: Freshness,
    cache_age_ms: Option<u64>,
) -> Vec<u8> {
    let envelope = shape_envelope(
        session,
        data,
        coverage,
        consistency,
        freshness,
        cache_age_ms,
    );
    serde_json::to_vec(&envelope).unwrap_or_default()
}

/// Build the deterministic payload stand in of an export.
///
/// The bytes are not an editor render. They are a deterministic byte stream
/// that carries the same information a rendered asset would carry in its
/// header, so the store, the hash, the chunked transfer, and the retrieval
/// path are exercised end to end without an editor.
pub fn synthetic_export_bytes(width: u32, height: u32, node_id: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(EXPORT_PAYLOAD_BYTES + 64);
    bytes.extend_from_slice(b"FIGTETHER-OFFLINE-EXPORT\0");
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(node_id);
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    while bytes.len() < EXPORT_PAYLOAD_BYTES {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        bytes.extend_from_slice(&(z ^ (z >> 31)).to_le_bytes());
    }
    bytes
}
