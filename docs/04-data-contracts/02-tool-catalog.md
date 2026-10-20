# MCP Tool Catalog

Status: `Proposed`

Purpose paragraph: This document defines the twelve MCP read tools FigTether exposes to agents with purpose, inputs (types in parentheses), outputs, caps, and one workload example for the tools named required by the contract set. Results use the common envelope of `docs/04-data-contracts/01-mcp-envelope.md`. Normative base: PRD 8.5.1 (tool surface), PRD 8.3 (consistency semantics), PRD 8.5.5 (resource budget). Tool and field names are `Proposal contract` names frozen only after Gate `G0`; before that they change through an ADR in `docs/02-architecture/adr/`.

## 1. Tool surface (PRD 8.5.1)

| Tool                | Main inputs                                             | Result                                                                  | Phase |
| ------------------- | ------------------------------------------------------- | ----------------------------------------------------------------------- | ----- |
| `get_bridge_status` | none                                                    | Version, health; no secret or design content                            | P0    |
| `list_sessions`     | optional cursor                                         | Sessions granted to this client, scope, capabilities                    | P0    |
| `get_selection`     | `session_id`                                            | Summary of selection inside scope; out-of-scope selection is not leaked | P0    |
| `list_nodes`        | source ID, `root_id`, cursor, filters, field projection | Child or tree summary, `next_cursor`, coverage                          | P0    |
| `get_nodes`         | source ID, `node_ids`, depth, fields, consistency       | Node data with per-item error                                           | P0    |
| `inspect_design`    | source ID, `root_id`, detail, consistency               | Implementation context, unsupported features, asset refs                | P0    |
| `get_design_tokens` | source ID, `root_id`, requested kinds                   | Styles and variables in scope, modes, aliases, coverage                 | P0    |
| `export_assets`     | source ID, `node_ids`, PNG or SVG, scale                | `asset_id`, hash, MIME, size, resource URI; per-item error              | P0    |
| `get_snapshot_info` | `snapshot_id`                                           | Capture time, hash, coverage, expiry                                    | P0    |
| `watch_scope`       | `session_id`, scoped `root_id`                          | `watch_id` and cursor; polling when the client lacks events             | P1    |
| `get_changes`       | `session_id`, `watch_id`, cursor                        | Changed and deleted IDs, or `RESYNC_REQUIRED`                           | P1    |
| `get_job_status`    | `job_id`                                                | Status, progress, result of long operations                             | P1    |

Creating grants, widening scope, pairing, revoke, and export to the filesystem stay under user control through UI or CLI; an agent cannot raise its own permissions through these read tools, and snapshot capture is created through the plugin UI in P0 (PRD 8.5.1 closing paragraph, `FR15`).

## 2. Cross-cutting rules

- Every read tool declares `readOnlyHint=true` and `destructiveHint=false`; hints describe behavior and are not an authorization mechanism. The real gates are grants, epochs, and per-node membership checks on both daemon and plugin (PRD 8.5.1, `FR03`).
- Identifier parameters are strings; `node_id` has meaning only together with its session or snapshot and is never parsed into a number (PRD 8.4 entity table).
- Data operations (`get_selection`, `list_nodes`, `get_nodes`, `inspect_design`, `get_design_tokens`, `export_assets`) require exactly one of `session_id` or `snapshot_id`; the two are mutually exclusive (`FR04`, `AT05`).
- The pairing secret, daemon port, and bridge endpoints never appear as tool parameters or any model-visible field (`FR02`, `AT06`, PRD 8.6.1 token prompt row).
- MCP cancellation is forwarded into the scheduler and late results are discarded (`FR14`, PRD 8.5.1 closing paragraph). Timeouts: 20 s read, 60 s export, queue wait included (PRD 8.5.5).
- Long operations in P1 report through `get_job_status`; the job API does not make Figma operations transactional (PRD 8.5.1 closing paragraph).
- Caching is opt-in: `consistency=fresh` is the live default; `cached` requires explicit `consistency=cached` plus `max_age_ms` (`D04`, `AT04`, PRD 8.3). Fresh-read retries are at most 1, transient and idempotent only (PRD 8.5.5, `FR14`).

## 3. Tool subsections

### 3.1 `get_bridge_status` (P0)

Purpose: verify that a daemon is reachable and which contract versions are spoken, before spending calls on data.
Inputs: none at all.
Outputs: daemon and plugin versions, `schema_version`, `protocol_version`, health state; never secrets, grant roots, or design content (PRD 8.5.1 row).
Caps: extra small response; works while unpaired.

### 3.2 `list_sessions` (P0)

Purpose: session discovery restricted to what this MCP client may see.
Inputs: `cursor` (string or null, optional): opaque continuation cursor.
Outputs: sessions granted to this client with `session_id`, display document label (never a routing key), grant roots, and runtime capabilities (`FR04`); paused and revoked appear as status (`PAUSED`, `REVOKED`, PRD 8.7).
Caps: paginated under the 64 KiB response cap (PRD 8.5.5).

### 3.3 `get_selection` (P0)

Purpose: show which nodes the user currently selects, so the agent can pick correct roots.
Inputs: `session_id` (string, required; live sessions only).
Outputs: summary of the selection parts inside the grant; selection outside the grant is not leaked, not even by count (PRD 8.5.1 row, PRD 8.4 scope note).
Caps: cheap live read; moving the selection never widens the grant (committed interpretation 4 in `docs/00-overview/01-prd-analysis.md`).

### 3.4 `list_nodes` (P0)

Purpose: walk or page the layer tree inside a granted root with projected fields, keeping responses small.
Inputs:

- `session_id` / `snapshot_id` (string, exactly one required): source of the read (`FR04`).
- `root_id` (string, required): must be inside the grant (`FR03`, `AT17`).
- `cursor` (string or null, optional): continuation bound to observation, scope, and epochs (PRD 8.3).
- `filters` (object, optional): node types, depth bound, name-less structural filters.
- `fields` (array of strings, optional): field projection; default summary set (`P04`, `AT07`).

Outputs: node summaries with children, `next_cursor`, and full `coverage`; `truncated` is never hidden (`FR07`).

```json
{
  "session_id": "session_01",
  "root_id": "1:2",
  "cursor": null,
  "filters": {
    "types": ["FRAME", "TEXT"],
    "max_depth": 4
  },
  "fields": ["id", "type", "name", "children", "visible"]
}
```

Caps: at most 10,000 nodes visited per operation; resumable traversal yields partial plus cursor, otherwise `SCOPE_TOO_LARGE`; responses fit 64 KiB; out-of-scope siblings and ancestors never appear (PRD 8.5.5, `AT07`).

### 3.5 `get_nodes` (P0)

Purpose: fetch normalized full detail for a bounded batch of nodes with an explicit consistency mode.
Inputs:

- `session_id` / `snapshot_id` (string, exactly one required).
- `node_ids` (array of strings, required): at most 50 IDs (PRD 8.5.5 batch row).
- `depth` (integer, optional): at most 32 (PRD 8.5.5 batch row).
- `fields` (array of strings, optional): projection as in `list_nodes`.
- `consistency` (string, optional): `fresh` is the live default; `cached` also needs `max_age_ms`; `best_effort` is server-assigned on mid-read change (PRD 8.3).

Outputs: one record per requested ID plus per-item errors (`NODE_NOT_FOUND`, `OUT_OF_SCOPE`) and batch status `complete`, `partial`, or `failed` (PRD 8.7 batch rule).
Caps: batch and depth limits above; exceeding either is `INVALID_LIMIT` or a required client-side split into batches (PRD 8.5.5).

```json
{
  "session_id": "session_01",
  "node_ids": ["1:2", "1:5", "1:9"],
  "depth": 8,
  "fields": ["id", "type", "name", "geometry", "fills"],
  "consistency": "fresh"
}
```

### 3.6 `inspect_design` (P0)

Purpose: return the implementation-context view of a subtree: layout, typography, paints, effects, component references, plus the unsupported-feature and asset-reference lists an agent must not invent (`P08`, `AT10`).
Inputs:

- `session_id` / `snapshot_id` (string, exactly one required).
- `root_id` (string, required): inside grant.
- `detail` (string, optional): such as `summary`, `layout`, `full`; default summary (PRD 8.5.1 row).
- `consistency` (string, optional): as in `get_nodes`.

Outputs: normalized implementation context per PRD 8.5.3 domains, `unsupported` entries preserving unknown types, and opaque asset references.
Caps: response capped at 64 KiB; larger content moves to staged resources; absolute coordinates never double as relative positions (PRD 8.5.3 geometry row, PRD section 2.1 read-path split).

```json
{
  "session_id": "session_01",
  "root_id": "1:2",
  "detail": "layout",
  "consistency": "fresh"
}
```

### 3.7 `get_design_tokens` (P0)

Purpose: read styles and variables actually in scope or inside approved dependencies with honest coverage (`FR10`).
Inputs:

- `session_id` / `snapshot_id` (string, exactly one required).
- `root_id` (string, required): binds resolution to the subtree.
- `requested_kinds` (array of strings, optional): such as `color_styles`, `text_styles`, `effect_styles`, `variables`.

Outputs: matched styles and variables with collection and mode identifiers, values, aliases, and resolved references; unresolved, cyclic, or mode-ambiguous aliases are reported, never guessed (PRD 8.5.3 variables row, `AT10`).
Caps: capability-gated; on seats where the Variables API cannot answer the result is `CAPABILITY_UNAVAILABLE` or `unavailable` with reason, a valid answer, not a tool failure (`D09`, `FR09`); library content outside the grant is never pulled silently (PRD 8.5.3 components row).

```json
{
  "session_id": "session_01",
  "root_id": "1:2",
  "requested_kinds": ["color_styles", "text_styles", "variables"]
}
```

### 3.8 `export_assets` (P0)

Purpose: export nodes as PNG or SVG through the plugin renderer and return grant-bound assets, never ephemeral URLs (`FR11`, `AT11`).
Inputs:

- `session_id` / `snapshot_id` (string, exactly one required; live export needs a live session).
- `node_ids` (array of strings, required): nodes inside grant.
- `format` (string, required): `png` or `svg` (PRD 8.5.1 row).
- `scale` (number, optional): between 0.25 and 2 (PRD 8.5.5 export row).

Outputs: per item `asset_id`, content hash, MIME type, byte size, and a resource URI bound to the grant; per-item errors such as `EXPORT_FAILED`, `FONT_UNAVAILABLE`, `PAYLOAD_TOO_LARGE`; explicit batch status (PRD 8.5.1 row, PRD 8.7).
Caps: export timeout 60 s; images of at most 16 megapixels; scale outside 0.25 to 2 is `INVALID_LIMIT`; a single asset of at most 16 MiB; one global export at a time on the separated queue (PRD 8.5.5, `N03`); active SVG is delivered with MIME, never rendered as a page (PRD 8.6.1).

```json
{
  "session_id": "session_01",
  "node_ids": ["1:2", "1:9"],
  "format": "png",
  "scale": 1.0
}
```

### 3.9 `get_snapshot_info` (P0)

Purpose: describe a stored capture without touching the editor.
Inputs: `snapshot_id` (string, required; immutable capture identity, `FR05`).
Outputs: capture window, stable content hash, origin session and grant, coverage summary, and idle TTL expiry state; snapshot default TTL is 30 minutes idle, user-bounded (PRD 8.3 cache paragraph).
Caps: works while the plugin is offline or paused; the result is never relabeled live (PRD 8.4 state machine note).

### 3.10 `watch_scope` (P1)

Purpose: subscribe to change invalidation for one scoped root; events are an invalidation source, never a truth source (`D07`, `FR20`).
Inputs:

- `session_id` (string, required): live only; no snapshot watches.
- `root_id` (string, required): scoped to a granted root; watch capability may be reported unavailable without breaking live reads (PRD 8.3 watch paragraph).

Outputs: `watch_id` plus initial cursor; when the MCP client cannot receive event resources the agent falls back to polling via `get_changes` (PRD 8.5.1 row).
Caps and P1 semantics: debounce 250 ms; at most 10 batches per second per session; the journal holds at most 10,000 events or 5 minutes; overflow or reconnect marks a gap and forces resync (PRD 8.3 watch paragraph, `AT19`); cursor obeys the read pagination epoch binding; `nodechange` behavior must be verified on the release editor before freeze (`S6`).

```json
{
  "session_id": "session_01",
  "root_id": "1:2"
}
```

### 3.11 `get_changes` (P1)

Purpose: drain a watch journal: changed IDs, deletion tombstones, or `RESYNC_REQUIRED` when the journal cannot serve the requested range.
Inputs:

- `session_id` (string, required): same session as the watch.
- `watch_id` (string, required): issued by `watch_scope`.
- `cursor` (string, required): watch cursor; gapped or expired cursors answer `RESYNC_REQUIRED` or `CURSOR_EXPIRED` (PRD 8.3, `FR20`).

Outputs: changed and deleted node IDs with tombstones; the journal never carries full payload JSON (PRD 8.3 watch paragraph).
Caps: journal bounds above; unobserved dependency events are handled by fresh reads and no claim of perfect synchronization is made (PRD 8.3, `R14`).

### 3.12 `get_job_status` (P1)

Purpose: let a client that cancelled or lost a response observe what happened to a long operation.
Inputs: `job_id` (string, required; issued for long operations on the P1 job path).
Outputs: `queued`, `running` with progress, `succeeded` with a result reference, or a terminal error code from `docs/04-data-contracts/05-error-catalog.md`.
Caps: job records clear with their session; cancelled jobs report `CANCELLED`; a request whose deadline expired while queued was never executed at all (PRD 8.5.5 timeout row). This is observability only; it never turns a Figma operation into an atomically committed transaction (PRD 8.5.1 closing paragraph).

## 4. Related documents

- Envelope semantics for all results: `docs/04-data-contracts/01-mcp-envelope.md`.
- Error codes, retry flags, and batch status: `docs/04-data-contracts/05-error-catalog.md` (PRD 8.7).
- Limit matrix: `docs/04-data-contracts/06-resource-limits.md`; transport: `docs/04-data-contracts/03-bridge-protocol.md`.
- Scheduler and cancellation behavior: `docs/02-architecture/01-system-design.md`; implementation homes per PRD 8.4: `crates/mcp`, `crates/core`.
- Tool traceability to `AT04`, `AT05`, `AT07`, `AT10`, `AT11`, `AT12`, `AT14`, `AT18`, `AT19`: `docs/06-testing/03-traceability-matrix.md`.
