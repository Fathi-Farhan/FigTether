# MCP Adapter Design (figtether mcp)

Status: `Proposed`

This document fixes how the Rust MCP server exposes the FigTether read-only tool surface to agent
clients over stdio: one process per client, the rmcp lifecycle with version pinning (D08), the
twelve tool contract from PRD 8.5.1, consistency semantics with fresh as the default, MCP
cancellation forwarding to the scheduler, opaque grant bound resource URIs, config printing, and
the split between business and protocol error handling. Envelope field rules are anchored in
`docs/04-data-contracts/01-mcp-envelope.md`; the bridge plane is specified in
`docs/02-architecture/02-bridge-design.md`; process topology in
`docs/02-architecture/01-system-design.md`.

## 1. Scope and references

- In scope: the MCP client process, its stdio transport, tool and resource surface.
- Out of scope: plugin internals (`docs/02-architecture/04-plugin-design.md`) and state machines
  (`docs/02-architecture/05-state-machines.md`).
- Normative PRD parts: 8.4 (component table), 8.5.1 (tool surface), 8.5.5 (resource budget), 8.7
  (error table), section 5.1 steps 7 and 8 (CLI and client wiring).
- Code anchors: `crates/mcp` (adapter process), `crates/core` (business rules the process calls),
  `crates/protocol` (schema generation), `crates/cli` (`config`, `doctor` subcommands).

## 2. Process model

| Aspect         | Decision                                                                                                                                                                    |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Instantiation  | One `figtether mcp` process per MCP client (PRD 8.4: one process per client). The client spawns it via its own config; the process never forks agents.                      |
| Isolation      | Each process holds the client credential bound to the current `daemon_instance_id`; queues, limits and grant checks are per client id (PRD 8.5.5 per client queue 16).      |
| Daemon link    | Authenticated loopback channel to the daemon using credentials from the owner only runtime directory (PRD 8.4, 8.6.2). The pairing secret is never present in this process. |
| Daemon absent  | Structured AUTH_REQUIRED with action `start figtether daemon`, then rerun `figtether doctor`; no silent fallback and no retry storm (PRD 8.7).                              |
| Ports          | No listener, socket, shell or filesystem writes beyond sanctioned runtime reads; the process is read-only by construction (D03).                                            |
| Crash behavior | The client restarts the process; a new process revalidates its credential and otherwise answers AUTH_EXPIRED, never reusing state from the dead process.                    |

## 3. rmcp lifecycle and version pinning (D08)

- The `rmcp` crate version, MSRV and enabled features are pinned at G0 and recorded in ADR-003 of
  `docs/02-architecture/adr/`; runtime builds never track a moving SDK branch (PRD 8.4, D08).
- Negotiated capabilities: `initialize` (protocol version exchange), `tools/list`, `tools/call`,
  `resources/list`, `resources/read`, cancellation notifications, and `ping`. Everything beyond
  the tools plus stdio baseline stays disabled until a client matrix entry proves it (R09).
- Client compatibility: at least two real MCP clients prove the handshake in AT14; results feed
  the support matrix (PRD 8.10.2, R09).
- Stdout purity: every byte on stdout is MCP JSON-RPC protocol. Logs, tracing, panics and metrics
  go to stderr in structured form (FR17, AT14). No design text, secret or endpoint value is ever
  written to either stream unredacted (FR18, R13).

## 4. Tool surface (PRD 8.5.1)

Every tool is read-only: `readOnlyHint=true`, `destructiveHint=false` on each definition. Hints
are not authorization and never bypass scope, grant or epoch checks (PRD 8.5.1). All identifier
arguments are strings. Data operations take `session_id` or `snapshot_id`, mutually exclusive.
Secrets and bridge endpoints are never arguments, and tool names freeze after G0.

| Tool                | Phase | Principal inputs                                                                        | Result summary                                                                                         | Hints           |
| ------------------- | ----- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | --------------- |
| `get_bridge_status` | P0    | none                                                                                    | Version, health check; no secret, no design content.                                                   | R true, D false |
| `list_sessions`     | P0    | optional cursor                                                                         | Only sessions granted to this client: scope roots, capabilities (FR04).                                | R true, D false |
| `get_selection`     | P0    | `session_id`                                                                            | In-scope selection summary; selection outside the grant is never leaked or counted for routing (AT05). | R true, D false |
| `list_nodes`        | P0    | source id (`session_id` or `snapshot_id`), `root_id`, cursor, filters, field projection | Child or tree summaries, `next_cursor`, coverage, explicit `truncated` (FR07).                         | R true, D false |
| `get_nodes`         | P0    | source id, `node_ids`, depth, fields, consistency                                       | Node data with per item errors for failed ids.                                                         | R true, D false |
| `inspect_design`    | P0    | source id, `root_id`, detail level, consistency                                         | Implementation context, unsupported feature warnings, asset references (FR08).                         | R true, D false |
| `get_design_tokens` | P0    | source id, `root_id`, requested kinds                                                   | Scoped styles and variables, modes, aliases, coverage; unresolved aliases flagged (FR10).              | R true, D false |
| `export_assets`     | P0    | source id, `node_ids`, PNG or SVG, scale                                                | Per item `asset_id`, hash, MIME, size, resource URI; per item errors (FR11).                           | R true, D false |
| `get_snapshot_info` | P0    | `snapshot_id`                                                                           | Capture time, content hash, coverage, expiry.                                                          | R true, D false |
| `watch_scope`       | P1    | `session_id`, scoped `root_id`                                                          | `watch_id` and cursor; polling documented when the client has no event support.                        | R true, D false |
| `get_changes`       | P1    | `session_id`, `watch_id`, cursor                                                        | Changed and deleted ids or RESYNC_REQUIRED (FR20).                                                     | R true, D false |
| `get_job_status`    | P1    | `job_id`                                                                                | Status, progress and result for long operations; not a transaction API (PRD 8.5.1).                    | R true, D false |

Notes: P1 rows ship disabled and hidden until G3; `get_changes` is the P1 counterpart of watch with
cursor based resync. Grant creation, scope expansion, pairing, revoke and writing assets to the
filesystem are user actions in the plugin UI or CLI, never reachable through read tools (PRD
8.5.1).

## 5. Response envelope

Every tool result is the envelope documented in `docs/04-data-contracts/01-mcp-envelope.md`:
`schema_version`, `request_id`, `source` (`kind`: `plugin_live`, `plugin_snapshot` or
`offline_bundle`, plus `session_id`, `grant_epoch`, `connection_epoch`), `observation`
(`observation_id`, `started_at`, `observed_at`, `consistency`, `freshness`, `cache_age_ms`,
`content_hash`), `coverage` (root ids, returned nodes, `truncated`, `next_cursor`,
`unavailable_fields`), `data` and `warnings`. Cached responses carry index age on `freshness`, and
null is only valid where availability annotations declare it (PRD 8.5.2).

## 6. Consistency semantics (PRD 8.3)

| Requested                  | Rule                                                                                                                                                                                                 |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fresh` (live default)     | The read always reaches the plugin; an old cache never satisfies it. Identical concurrent fresh requests may coalesce into one observation window (PRD 8.3).                                         |
| `cached` with `max_age_ms` | Only on explicit request; the result carries `observed_at` and `cache_age_ms`; an age breach forces a fresh read or a structured failure, never a silent serve.                                      |
| snapshot source            | The result always refers to the same capture; no live freshness claim; captures stay labeled `consistency=best_effort` (PRD 8.3, analysis commitment 3).                                             |
| plugin offline             | A fresh read fails with PLUGIN_OFFLINE; snapshot data is delivered only when the request explicitly targets a snapshot. Snapshots are never substituted for live reads (fail closed interpretation). |
| change during traversal    | `best_effort` result with start and end timestamps; DOCUMENT_CHANGED plus at most one retry, only for transient and idempotent reads (PRD 8.3, 8.5.5).                                               |
| live pagination            | Cursors bind to observation, scope and epoch; change or expiry yields CURSOR_EXPIRED and a snapshot is suggested for stable traversal (PRD 8.3).                                                     |

Freshness is decided by `freshness` in the envelope, never by adding a new source kind; cache
status is not a source (PRD 8.5.2).

## 7. Cancellation

- MCP cancellation notifications map to scheduler cancellation for the matching `request_id` (PRD
  8.5.1). The scheduler rejects an undispatched job (CANCELLED), or forwards a `cancel` frame over
  the bridge (`docs/02-architecture/02-bridge-design.md`, section 6).
- Any result or chunk arriving after cancellation is discarded and never delivered (FR14, N07
  wording: logical cancel within 1 s).
- Cancellation of an editor render already started can be asynchronous; the residual limit is
  documented in the PRD 8.6.1 malformed payload row and iteration continues.
- The P1 `get_job_status` tool reports progress and terminal states; it does not turn Figma
  operations into atomic transactions (PRD 8.5.1).

## 8. Asset resources and opaque URIs

- Assets surface as resources with an opaque, grant-bound URI scheme, for example
  `figtether://asset/{opaque_token}`. The token never encodes node ids, filesystem paths, secrets
  or grant scope, and it is minted only inside `export_assets` results (FR11).
- Resource URIs bind to the producing grant epoch; after revoke or epoch change, resource reads
  return REVOKED or AUTH_EXPIRED, never a silent refetch or revalidation by the agent (PRD 8.6.1
  scope row, FR15).
- Metadata travels with each asset: MIME type, byte size, pixel dimensions and SHA-256 content
  hash (PRD 8.5.1 export row, AT11).
- Inline image budget per PRD 8.5.5: raw 1 MiB, at most 2 MiB encoded per response; larger images
  stay behind the resource URI rather than being downscaled automatically. The MCP response JSON
  cap stays 64 KiB UTF-8, with projection and pagination for everything else (PRD 8.5.5).

## 9. Error handling: business versus protocol

| Lane           | Trigger                                             | Shape                                                                                                                      | Examples                                                                                                                                                                                                                                                                      |
| -------------- | --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Business error | Valid MCP request the product must refuse or report | Structured tool result error: `code`, `message`, `retryable`, `action`, `request_id`, optional `retry_after_ms` (PRD 8.7). | PERMISSION_DENIED, OUT_OF_SCOPE, PLUGIN_OFFLINE, SESSION_NOT_FOUND, PAUSED, REVOKED, NODE_NOT_FOUND, CAPABILITY_UNAVAILABLE, PAYLOAD_TOO_LARGE, SCOPE_TOO_LARGE, BUSY, TIMEOUT, CANCELLED, DOCUMENT_CHANGED, CURSOR_EXPIRED, EXPORT_FAILED, FONT_UNAVAILABLE, INTERNAL_ERROR. |
| Protocol error | Malformed MCP framing or unknown method             | Standard MCP JSON-RPC error on the protocol lane.                                                                          | Parse error, invalid request, method not found.                                                                                                                                                                                                                               |

Rules from PRD 8.7: every error carries `retryable` and a user action; `retry_after_ms` accompanies
BUSY; batch results carry per item status plus explicit complete, partial or failed classification;
private stack traces stay out of agent results; retry behavior for INTERNAL_ERROR is manual
classification, not automatic.

## 10. UNSUPPORTED_SCHEMA and PROTOCOL_MISMATCH behavior

- At startup the process checks its schema and protocol version against the daemon; a major
  mismatch or a pinned incompatibility refuses to start with one clear stderr line pointing to
  `figtether doctor`, which lists compatible versions (PRD 8.7).
- A version conflict detected after startup surfaces UNSUPPORTED_SCHEMA or PROTOCOL_MISMATCH as a
  structured business error per call, with recovery action `update or rerun figtether doctor`.
- Unknown optional fields are tolerated; unknown required fields are protocol mismatch. Schema
  additions remain minor until a frozen contract in `docs/04-data-contracts/` requires a major
  (PRD 8.11).

## 11. Client wiring: config print

- `figtether config print --client generic` prints a generic MCP entry: binary path plus the
  arguments `mcp --stdio`, with no tokens in the output (PRD 5.1 step 7).
- The CLI never writes into other applications config files silently; the user copies the entry
  into their client (PRD 5.1 step 7).
- Pairing stays on the plugin plus CLI path, so the model never sees a credential; example config
  contains no token field (PRD 8.6.1 token and prompt row).

## 12. Traceability

| Anchor                              | Covered by                       |
| ----------------------------------- | -------------------------------- |
| FR04 restricted discovery           | Section 4 `list_sessions`; AT05. |
| FR06 and FR12 fresh and envelope    | Sections 5 and 6; AT04, AT10.    |
| FR07 pagination and truncation      | Section 4 `list_nodes`; AT07.    |
| FR11 asset export and hash          | Sections 4 and 8; AT11.          |
| FR13 BUSY backpressure              | Section 9; AT08.                 |
| FR14 cancellation and bounded retry | Section 7; AT08, AT15.           |
| FR17 stdio purity and two clients   | Section 3; AT14.                 |
| FR18 sanitized diagnostics          | Sections 3 and 9; review at G1.  |

## 13. Open questions

1. Exact rmcp version, MSRV and feature flags: resolved at G0 with two client handshakes (AT14,
   D08) and recorded in ADR-003.
2. Resource list pagination and subscription support per target client; disabled until measured.
3. Whether inline image content blocks are reliable enough to use at all, or whether resources
   remain the only asset lane (client matrix entry per R09).
4. Do P1 watch tools need an explicit client capability flag, or hide behind list only; decided
   with the client matrix at G3.
