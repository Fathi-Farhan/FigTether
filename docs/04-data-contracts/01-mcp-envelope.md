# MCP Response Envelope

Status: `Proposed`

Purpose paragraph: This document defines the common response envelope that every successful FigTether MCP tool result carries: provenance (`source`), the observation window (`observation`), completeness (`coverage`), payload (`data`), and annotations (`warnings`). It makes labeled evidence inseparable from data, so an agent never decides on unlabeled content (`FR12`, `G03`, `P03`). Normative base: PRD 8.5.2, PRD 8.3, PRD 8.5.5. All timestamps are RFC 3339 UTC strings like `2026-10-20T08:00:00Z`. All names here are `Proposal contract` names frozen only after Gate `G0` sign-off; before that they change through an ADR in `docs/02-architecture/adr/`.

## 1. Scope and normative sources

Sources in force: PRD 8.5.2 (shape, source kinds, hash and null rules), PRD 8.3 (consistency semantics, stale-cache and cursor rules), `FR12` plus `AT04`, `AT10` (freshness evidence contract), `FR09` plus `P08`, `AT12` (null vs unavailable and capability answers), and PRD 8.5.5 (response caps). Error results follow `docs/04-data-contracts/05-error-catalog.md`; they are referenced, not restated.

## 2. Canonical envelope example (PRD 8.5.2, verbatim)

The block reproduces the PRD example exactly; `req_01`, `session_01`, `obs_42`, and `sha256:...` are illustrative placeholders from the PRD.

```json
{
  "schema_version": "1.0",
  "request_id": "req_01",
  "source": {
    "kind": "plugin_live",
    "session_id": "session_01",
    "grant_epoch": 3,
    "connection_epoch": 2
  },
  "observation": {
    "observation_id": "obs_42",
    "started_at": "2026-10-20T08:00:00Z",
    "observed_at": "2026-10-20T08:00:00.150Z",
    "consistency": "best_effort",
    "freshness": "fresh_read",
    "cache_age_ms": 0,
    "content_hash": "sha256:..."
  },
  "coverage": {
    "root_ids": ["1:2"],
    "returned_nodes": 24,
    "truncated": false,
    "next_cursor": null,
    "unavailable_fields": []
  },
  "data": {},
  "warnings": []
}
```

The sample combination `consistency=best_effort` with `freshness=fresh_read` is the PRD mid-traversal-change case: the read reached the plugin, but an edit inside the observation window broke the single-window guarantee, so the result is best_effort (PRD 8.3).

## 3. Top-level fields

| Name             | Type             | Required | Notes                                                                                                                                                              |
| ---------------- | ---------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `schema_version` | string           | yes      | Contract version of the envelope and model. First release: `"1.0"`. Unsupported major versions reject with `UNSUPPORTED_SCHEMA` (PRD 8.7, PRD 8.11 versioning row) |
| `request_id`     | string           | yes      | Identity of the answered tool call; matches the MCP request identifier so late results can be discarded (`FR14`, PRD 8.4 heartbeat)                                |
| `source`         | object           | yes      | Provenance block, section 4                                                                                                                                        |
| `observation`    | object           | yes      | Observation window block, section 5                                                                                                                                |
| `coverage`       | object           | yes      | Coverage block, section 6                                                                                                                                          |
| `data`           | object           | yes      | Tool specific payload; shape fixed per tool in `docs/04-data-contracts/02-tool-catalog.md`. Whole response capped at 64 KiB encoded UTF-8 (PRD 8.5.5)              |
| `warnings`       | array of objects | yes      | Non-fatal notices; may be empty. Shape in section 11                                                                                                               |

## 4. `source` block

| Name               | Type        | Required                                | Notes                                                                                                                                                                                |
| ------------------ | ----------- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `kind`             | string enum | yes                                     | One of `plugin_live`, `plugin_snapshot`, `offline_bundle` (PRD 8.5.2); semantics in section 7.1                                                                                      |
| `session_id`       | string      | yes for `plugin_live`                   | UUID issued per plugin run and tab; never derived from file name or `fileKey` (PRD 8.4 entities, `FR04`, `D05`)                                                                      |
| `grant_epoch`      | integer     | yes                                     | Version of the grant authorizing the read; increments on scope change or revoke (PRD 8.4 entities)                                                                                   |
| `connection_epoch` | integer     | yes                                     | Version of the connection; increments on each reconnect, stale answers discarded (PRD 8.4 entities, `FR16`)                                                                          |
| `snapshot_id`      | string      | Proposal: yes for `plugin_snapshot`     | UUID of the immutable capture, bound to its origin session and grant (PRD `FR05`, PRD 8.4). The PRD envelope omits this field; held as a proposal until the gap is closed after `G0` |
| `bundle_id`        | string      | Proposal: yes for `offline_bundle` (P1) | Identity of the imported versioned bundle (`FR21`, `AT20`); shape decided with the P1 work at `G3`                                                                                   |

Snapshot and offline reads replace `session_id` with the capture or bundle identity; grants are inherited from the origin session recorded at capture time (`FR05`, `FR21`).

## 5. `observation` block

| Name             | Type                  | Required | Notes                                                                                                                            |
| ---------------- | --------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `observation_id` | string                | yes      | Identity of one observation result; not an official Figma version number (PRD 8.4 entities, glossary)                            |
| `started_at`     | string (RFC 3339 UTC) | yes      | Start of the observation window (`FR06`)                                                                                         |
| `observed_at`    | string (RFC 3339 UTC) | yes      | End of the observation window; start and end are both mandatory (`FR06`, PRD 8.3)                                                |
| `consistency`    | string enum           | yes      | `fresh`, `cached`, or `best_effort` (PRD 8.3); semantics in section 7.2                                                          |
| `freshness`      | string enum           | yes      | `fresh_read` or `cache_hit` (PRD 8.5.2); cache status lives here, never as a new source kind. Semantics in section 7.3           |
| `cache_age_ms`   | integer               | yes      | `0` on a fresh read; on `cache_hit`, milliseconds between `observed_at` and the serve moment, which stays internal to the daemon |
| `content_hash`   | string                | yes      | Hash over the canonical payload per section 9 (PRD 8.5.2)                                                                        |

## 6. `coverage` block

| Name                 | Type             | Required | Notes                                                                                                 |
| -------------------- | ---------------- | -------- | ----------------------------------------------------------------------------------------------------- |
| `root_ids`           | array of strings | yes      | Granted root node IDs that bound this read; union of subtrees (PRD 8.4 scope roots)                   |
| `returned_nodes`     | integer          | yes      | Count of node objects in `data` for this response                                                     |
| `truncated`          | boolean          | yes      | `true` when a limit stopped the traversal; truncation is never hidden (`FR07`). Policy in section 13  |
| `next_cursor`        | string or null   | yes      | `null` when complete or not resumable; otherwise an opaque cursor (`FR07`, PRD 8.3 pagination row)    |
| `unavailable_fields` | array of strings | yes      | Fields the platform, seat, or capability could not fill, with reason where applicable (`FR09`, `P08`) |

## 7. Enumerations

### 7.1 `source.kind`

| Value                 | Rule                                                                                                               |
| --------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `plugin_live`         | Data produced by the Plugin API on a live, authorized session this request; requires valid `session_id` and epochs |
| `plugin_snapshot`     | Data read from one immutable capture; always refers to the same capture and one `snapshot_id` (PRD 8.3)            |
| `offline_bundle` (P1) | Data imported from an exported versioned bundle; the import cannot pose as live (`FR21`, `AT20`)                   |

### 7.2 `consistency`

| Value         | Rule (PRD 8.3)                                                                                                                                                                                                                              |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fresh`       | Default live mode in P0; queries the Plugin API again. An old cache entry never satisfies the request. Identical truly concurrent requests may share one observation window (coalescing)                                                    |
| `cached`      | Only when explicitly requested with `max_age_ms`. Result carries `observed_at` and `cache_age_ms`; when age exceeds `max_age_ms`, perform a fresh read or fail. Default live cache TTL is 2 seconds and only for explicitly cached requests |
| `best_effort` | No single intact window: the document changed mid-traversal, or the data is a capture written while the user edited. Snapshot capture is always `best_effort` (committed interpretation 3 in `docs/00-overview/01-prd-analysis.md`)         |

### 7.3 `freshness`

| Value        | Rule                                                                                                                                                        |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fresh_read` | Served from a read that reached the plugin within this request (PRD 8.3 fresh rule, glossary "Fresh read")                                                  |
| `cache_hit`  | Served from a stored read: live cache, snapshot store, or bundle import; `cache_age_ms` and the original `observation` timestamps describe that stored read |

Snapshot and offline reads are served from storage, so they use `cache_hit`; they carry no freshness obligation toward the live document (PRD 8.3), and their `observation` timestamps describe the capture window.

## 8. Consistency and freshness rules (PRD 8.3 walk-through)

- Fresh must reach the plugin: a `fresh` request that cannot reach a live session fails with `PLUGIN_OFFLINE`; it never falls back to snapshot or cache silently (`FR06`, `N02`, PRD 5.2).
- Cached requests carry `observed_at` plus `cache_age_ms`; when age exceeds `max_age_ms`, the daemon performs a fresh read or fails instead of returning stale data (PRD 8.3).
- Snapshot reads are bound to one capture (`FR05`); they need no freshness comparison against the live document and stay labeled snapshot, not live (PRD 8.4).
- Plugin offline: fresh fails and snapshots are available only through an explicit snapshot source (PRD 8.3, PRD 8.7).
- Mid-traversal change: the result becomes `best_effort` with true start and end times; when a change is detected the response carries a warning or `DOCUMENT_CHANGED`, and at most one retry is allowed (PRD 8.3, `FR14`).
- Pagination is bound live: cursors are tied to the observation, scope, and epochs; detected change or expiry yields `CURSOR_EXPIRED`; snapshots are recommended for stable traversal (PRD 8.3).
- Pause blocks new live and snapshot reads until resume; revoke drops both from the daemon, surfacing `PAUSED` or `REVOKED` errors instead of silent denial (PRD 8.3, `FR15`, PRD 8.7).

## 9. Canonical `content_hash` rule (PRD 8.5.2)

- `content_hash` is a SHA-256 over the canonical payload of this response: UTF-8 bytes, no insignificant whitespace.
- Key order and number formats are determined by the schema, not by serialization accident: the versioned model in `schemas/` fixes field order per object, integers serialize plainly, and floats use the precision their schema declares.
- The hash is not evidence that a Figma server stored that version remotely; `observation_id` is likewise not an official Figma version (PRD 8.5.2, PRD 8.4).
- Snapshot identity (`snapshot_id`) and its content hash are stable once captured; re-reading a snapshot reproduces identical strings (`FR05`).
- Canonicalization details (float precision, optional-field omission) are pinned with the `AT13` fuzz and property corpus once the model lands in `crates/model`.

## 10. `null` vs unavailable (PRD 8.5.2, `FR09`, `AT10`)

- `null` means a value is genuinely empty or none, and only when the field availability metadata of that record says the field was `available` in the session context.
- A value unknown because the API, seat, or capability could not produce it is never `null` posing as empty: the field is omitted and reported with an unavailable reason in `coverage.unavailable_fields` or as a warning (`P08`, `FR09`).
- Unknown node types are preserved as `unsupported`, never silently dropped (PRD 8.5.3, `AT10`).
- This separation is what lets capability discovery answer honestly through runtime probes instead of inferring plan or seat from names (`FR09`, `P13`, `AT12`).

## 11. `warnings` array

Each warning is a proposal-shape object: `code` (string, vocabulary aligned with `docs/04-data-contracts/05-error-catalog.md`), `message` (human readable, no layer names or design text), plus optional `node_id` and `field`. Warnings annotate; they never invalidate data on their own. Typical codes: `DOCUMENT_CHANGED` (PRD 8.3), `CAPABILITY_UNAVAILABLE` (PRD 8.7), unsupported paint or font property (PRD 8.5.3).

## 12. Error results

Business errors are structured tool result errors, not payloads inside this envelope: they follow `docs/04-data-contracts/05-error-catalog.md`, implementing the PRD 8.7 table (`code`, `message`, `retryable`, `action`, `request_id`, optional `retry_after_ms`) with batch `complete`, `partial`, `failed` status. Malformed MCP requests use protocol-level errors instead (PRD 8.7). In error results `request_id` is the correlation key for late-result discards and sanitized diagnostics (`FR14`, `FR18`).

## 13. Truncation policy

- `truncated=false`, `next_cursor=null`: complete result.
- `truncated=true`, `next_cursor` non-null: resumable partial; the caller fetches the next page (`FR07`, `AT07`).
- `truncated=true`, `next_cursor=null`: partial and not resumable; the caller narrows root, fields, or depth; `SCOPE_TOO_LARGE` may be raised instead (PRD 8.5.5 traversal row).

Every truncation is visible in `coverage`; the daemon must not end a page early without either flag (`FR07`).

## 14. Worked examples

### 14.1 Cached hit with serve-time age

The observed window belongs to the original read; `cache_age_ms` measures serve-time age.

```json
{
  "schema_version": "1.0",
  "request_id": "req_02",
  "source": {
    "kind": "plugin_live",
    "session_id": "session_01",
    "grant_epoch": 3,
    "connection_epoch": 2
  },
  "observation": {
    "observation_id": "obs_43",
    "started_at": "2026-10-20T08:00:04.800Z",
    "observed_at": "2026-10-20T08:00:05.000Z",
    "consistency": "cached",
    "freshness": "cache_hit",
    "cache_age_ms": 5000,
    "content_hash": "sha256:9f2b0e1c6d4a3f58b7e2c1d0f9a8b7c6d5e4f302918763ba5c49d1f0e2b3a4c5"
  },
  "coverage": {
    "root_ids": ["1:2"],
    "returned_nodes": 24,
    "truncated": false,
    "next_cursor": null,
    "unavailable_fields": []
  },
  "data": {},
  "warnings": []
}
```

### 14.2 Snapshot read

`source.snapshot_id` is the section 4 proposal extension; capture data always reads `best_effort`.

```json
{
  "schema_version": "1.0",
  "request_id": "req_03",
  "source": {
    "kind": "plugin_snapshot",
    "snapshot_id": "snap_a01",
    "grant_epoch": 3,
    "connection_epoch": 2
  },
  "observation": {
    "observation_id": "obs_44",
    "started_at": "2026-10-20T07:55:12.010Z",
    "observed_at": "2026-10-20T07:55:12.640Z",
    "consistency": "best_effort",
    "freshness": "cache_hit",
    "cache_age_ms": 0,
    "content_hash": "sha256:0a1b2c3d4e5f60718293a4b5c6d7e8f900112233445566778899aabbccddeeff"
  },
  "coverage": {
    "root_ids": ["1:2"],
    "returned_nodes": 24,
    "truncated": false,
    "next_cursor": null,
    "unavailable_fields": []
  },
  "data": {},
  "warnings": []
}
```

`snap_a01`, `obs_43` to `obs_44`, and the hash strings are illustrative samples, not fixtures.

## 15. Related documents

- `docs/04-data-contracts/02-tool-catalog.md`: tools, inputs, and caps producing this envelope.
- `docs/04-data-contracts/05-error-catalog.md`: error and batch status shapes (PRD 8.7).
- `docs/04-data-contracts/06-resource-limits.md`: limit matrix behind truncation and payload rules.
- `docs/04-data-contracts/03-bridge-protocol.md`: transport carrying the daemon side; architecture: `docs/02-architecture/01-system-design.md`; implementation homes per PRD 8.4: `crates/mcp`, `crates/core`, `crates/model`.
