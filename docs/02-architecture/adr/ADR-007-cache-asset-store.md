# ADR-007: LRU design cache and memory-only asset store

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Accepted   |
| Created              | 2026-10-20 |
| Related decision IDs | D04, D07   |
| Supersedes           | -          |

## Context

Repeating identical fresh workloads would waste editor time and keep the plugin busy, but any
cache risks serving design data that no longer matches the document (`P03`, `R03`). PRD 8.3 fixes
the posture: cache keys include session (or snapshot), `grant_epoch`, node IDs, field projection,
export settings, and the model schema version; entries are removed on revoke or expiry;
invalidation of parents, descendants, styles, and component dependencies is conservative; the
default live cache TTL is 2 seconds and serves only explicitly cached requests; the snapshot TTL
is 30 minutes idle and user-boundable; pause rejects new live and snapshot reads until resume;
revoke clears both stores from the daemon. `FR15` requires revoke to drop the grant and clean
cache and assets; `FR14` forbids revived grants; `D04` keeps fresh reads cache-independent; the
budget table fixes 128 MiB total with LRU and a 16 MiB single-asset cap (8.5.5).

## Decision

1. One bounded in-memory budget of 128 MiB shared by the normalized model cache and the asset
   store, evicted LRU overall; a single asset larger than 16 MiB is refused, not truncated
   (8.5.5). Storage is memory-only; nothing design-related persists to disk (8.6.2).
2. Cache key is canonical and complete: `source.kind`, `session_id` or `snapshot_id`,
   `grant_epoch`, the ordered set of `node_ids`, the projection fingerprint, the export settings
   fingerprint, and the model `schema_version`. Two sessions holding the same node IDs never hit
   the same entry (session isolation, `AT05`).
3. Live entries: TTL 2 seconds from write and usable only by `consistency=cached` requests
   carrying `max_age_ms` (ADR-005); `fresh` never consults the cache by design (`FR06`, `D04`).
4. Snapshot entries (capture payloads and their decorations): idle TTL 30 minutes; configuration
   may shorten, and the user may bound it further; nothing on the cache side extends default TTLs
   (8.3).
5. Conservative invalidation: any detected change (diffs observed on read paths, warnings during
   windows, and in v0.2 watch-driven invalidations per `D07`) drops the subtree of the changed
   node: the parent chain, descendants, and every entry whose key or payload references its
   styles or component dependencies. In doubt, drop (8.3). Grant epoch bumps and scope changes
   invalidate all bound entries and cursors immediately.
6. Revoke, expiry, and pause: revoke clears cache and the asset store immediately, including
   asset URIs and cursors bound to the epoch, and refuses late frames (`FR14`, `FR15`, 8.4 revoke
   paragraph, `N07`). Expiry evicts silently as an LRU event. Pause rejects new live and snapshot
   reads but does not clear (8.3).
7. Asset store: memory-only, keyed by opaque `asset_id` bound to session and grant, storing
   bytes, MIME type, dimensions, hash, and export settings; served only through grant-protected
   resource URIs (`FR11`, 8.6.2). Over-limit assets never enter the store; oversized transfers use
   the bridge chunk budget with bounded reassembly or fail `PAYLOAD_TOO_LARGE` (8.5.4, 8.5.5).

## Rationale

Keying by session, epoch, projection, settings, and schema version makes every cache hit provably
bound to the authorization and shape it came from, which is what revoke semantics need to stay
trustworthy (`FR14`, `FR15`, `N07`). A 2 second live TTL plus conservative invalidation keeps
cached answers inside a window that is honest about stale risk, matching the `N01` cached
benchmark contract without turning the cache into a freshness authority, which ADR-005 forbids.

## Consequences

- Positive: cached reads meet `N01`; revoke is provably clean in evidence (`AT15`, `N07`); memory
  caps keep `N04` within reach; assets never outlive their grants.
- Negative: conservative invalidation over-drops under busy designs; key fingerprints slightly
  grow envelopes.
- Neutral: eviction metrics are diagnostics-only and never contain design content (`FR18`); the
  cache never influences consistency labels (ADR-005 owns that contract).

## Alternatives considered

- No cache at all: rejected; it loses the explicitly cached tier and its benchmark contract
  (`N01`) and burdens the plugin with repeated identical reads.
- A TTL-carrying cache with a fresh overlap window: rejected; contradicts `D04`, `FR06`, and
  committed interpretation 1 in `docs/00-overview/01-prd-analysis.md`.
- Per-page or per-frame cache keys: rejected; layer-only keys ignore style and component
  dependencies, so stale styled reads survive subtree edits against the 8.3 invalidation rule.
- Disk-backed asset store: rejected; conflicts with the memory-only default and the deletion
  rules of 8.6.2 and adds uninstall cleanup obligations.

## Verification

Evidence: property tests over key and invalidation logic including style and component dependency
chains; a revoke-clears-now test with subsequent asset URI rejection; LRU behavior under the
128 MiB cap with the 16 MiB per-asset rejection; soak confirms memory and error budgets.
AT refs: AT04 (cache age truth), AT05 (no cross-session hits), AT11 (asset handling), AT15
(revoke clears cache and assets and blocks retry), AT08 (limits under soak).
Gate: G1 (cache fixtures and AT series), G2 (soak and release evidence with `N01`).

## References

- PRD 4.3 (`FR06`, `FR11`, `FR12`, `FR14`, `FR15`), 8.3 (cache paragraph and pause/revoke), 8.4,
  8.5.4, 8.5.5, 8.6.2.
- `D04`, `D07`; committed interpretations 1 and 5 in `docs/00-overview/01-prd-analysis.md`.
- ADR-004 (epoch binding), ADR-005 (consistency labels), ADR-006 (cursor epochs), ADR-008 (the
  numeric budget authority), ADR-011 (`PAYLOAD_TOO_LARGE`, `REVOKED`).
