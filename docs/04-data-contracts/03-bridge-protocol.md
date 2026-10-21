# Bridge Protocol (Daemon to Plugin)

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose paragraph: This document defines the authenticated transport between the FigTether daemon and
the Figma plugin: frame envelope, message types, auth exchange, chunk transfer, heartbeat and epoch
handling, and error mapping. Normative base: PRD 8.5.4 (bridge protocol), PRD 8.3 (change rules),
PRD 8.5.5 (frame and transfer budgets), PRD 8.6.1 (credential rules), PRD 8.7 (error codes). Field
and message names are `Proposal contract` names frozen only after Gate `G0`; changes before that go
through `docs/02-architecture/adr/`.

## 1. Transport and versioning

- Default transport candidate is a local WebSocket on loopback; the plugin dials the daemon on the
  default port `38947` (PRD 8.4 architecture, PRD 5.1, `D06`). The transport is proven on Figma
  Desktop before other work depends on it (PRD 8.6.1 manifest paragraph, `R02`); if the runtime
  cannot support it, an alternative local transport is evaluated through ADR-002, never a public
  relay fallback (PRD 8.6.1).
- WebSocket browsers cannot set custom `Authorization` headers, so auth is carried inside the first
  protocol frame instead (PRD 8.5.4).
- Every frame carries `protocol_version`. Initial `hello` negotiates it: an incompatible major
  version is refused with `PROTOCOL_MISMATCH`, and `UNSUPPORTED_SCHEMA` covers model schema
  versions the peer cannot parse (PRD 8.7 `UNSUPPORTED_SCHEMA / PROTOCOL_MISMATCH` row; PRD 8.11
  versioning: SemVer for the core, separate protocol and schema versions, unsupported major
  rejected, unknown optional fields tolerated).
- JSON Schema is the single contract source for Rust and TypeScript and is tested in both directions
  (PRD 8.5.4 closing sentence; fixtures live next to `plugins/figma/main` and `crates/protocol`).

## 2. Frame envelope

Every frame on the socket is one JSON envelope with a typed payload (PRD 8.5.4):

| Name               | Type           | Required | Notes                                                                                                                      |
| ------------------ | -------------- | -------- | -------------------------------------------------------------------------------------------------------------------------- |
| `protocol_version` | string         | yes      | Negotiated at hello; mismatch is fatal (section 1)                                                                         |
| `message_type`     | string enum    | yes      | One value from the message table in section 3                                                                              |
| `request_id`       | string or null | yes      | Correlates request, progress, chunk, result, error, cancel; `null` on frames unrelated to a request                        |
| `session_id`       | string or null | yes      | Session the frame belongs to; accepted only after auth                                                                     |
| `connection_epoch` | integer        | yes      | Raised on each reconnect; stale answers discarded (PRD 8.4)                                                                |
| `grant_epoch`      | integer        | yes      | Grant version the frame is authorized against (PRD 8.4)                                                                    |
| `deadline_ms`      | integer        | yes      | Remaining budget for the operation, read 20 s and export 60 s including queue wait (PRD 8.5.5)                             |
| `payload`          | object         | yes      | Sender and message-type specific typed payload; schema validated before large allocation (PRD 8.6.1 malformed payload row) |

Example request frame:

```json
{
  "protocol_version": "1.0",
  "message_type": "request",
  "request_id": "req_01",
  "session_id": "session_01",
  "connection_epoch": 2,
  "grant_epoch": 3,
  "deadline_ms": 20000,
  "payload": {
    "op": "get_nodes",
    "node_ids": ["1:2", "1:5"],
    "depth": 8,
    "fields": ["id", "type", "name"]
  }
}
```

## 3. Message types (PRD 8.5.4 allowlist)

| message_type     | Direction        | Purpose                                                                                                                                            |
| ---------------- | ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `hello` / `auth` | plugin -> daemon | First frame after the socket opens: protocol negotiation plus pairing secret; nothing flows before it (PRD 8.5.4)                                  |
| `capabilities`   | both             | Negotiated feature set: supported operations (allowlisted), compression off in P0, watch availability; feeds runtime capability discovery (`FR09`) |
| `grant_update`   | plugin -> daemon | User changes grant roots through the plugin UI; raises `grant_epoch` after double verification (`FR03`, PRD 8.4 grant entity)                      |
| `request`        | daemon -> plugin | One bounded, allowlisted editor operation with its deadline (PRD 8.5.4; no eval or shell in handlers, `D03`)                                       |
| `progress`       | plugin -> daemon | Sliced traversal progress so the plugin can yield around the 8 ms slice target (PRD 8.5.5 traversal slice row)                                     |
| `chunk`          | plugin -> daemon | One transfer frame of a larger payload, section 5                                                                                                  |
| `result`         | plugin -> daemon | Final payload for a `request_id`                                                                                                                   |
| `error`          | both             | Structured error with PRD 8.7 fields, section 6                                                                                                    |
| `cancel`         | daemon -> plugin | Cancellation forwarded from MCP into the scheduler (PRD 8.5.1 closing paragraph)                                                                   |
| `heartbeat`      | both             | Liveness every 5 s (PRD 8.4 heartbeat paragraph)                                                                                                   |
| `revoke`         | both             | User revoke or pause state change; stops further sends, PRD 8.4 heartbeat paragraph                                                                |

Handler operations are allowlisted; there is no eval and no shell execution anywhere in the chain
(PRD 8.5.4, `D03`). The exact wire enum spellings (for example `hello_auth` versus separate `hello`
and `auth` values) are part of the proposal contract and are frozen at `G0`.

## 4. Auth exchange sequence

1. The daemon listens on loopback only and validates Host, Origin, and endpoint allowlist; DNS
   rebinding and remote exposure are rejected by binding and validation, not by web filters
   (PRD 8.6.1 first two rows, PRD 5.1).
2. Within 3 seconds of connection open, the plugin must send the first `auth` frame containing the
   pairing secret; on timeout the daemon closes the connection (PRD 8.5.4).
3. Before auth completes: no design data, no enqueueing of operations, and no session metadata may
   flow in either direction (PRD 8.5.4).
4. The daemon compares the secret with a constant-time comparison against the pending pairing secret
   (PRD 8.6.1 credential leak row, `FR02`).
5. The secret is single use: after successful comparison it is exchanged for a temporary session
   credential held only in plugin memory. It is never written to query strings, logged URL
   fragments, logs, or persistent plugin storage, and never sits in `clientStorage` or
   `localStorage` (PRD 8.5.4, PRD 8.6.2 plugin credential row).
6. The pairing secret itself lives only in daemon memory, is displayed only through an explicit
   local CLI action, and is destroyed when used, after 5 minutes, or at daemon stop (PRD 8.6.2
   pairing row, `FR02`).
7. A short reconnect within the same plugin run may reuse the still-valid in-memory temporary
   credential, but it must raise `connection_epoch` and the daemon cancels the work of the old
   connection (PRD 8.4 state machine).
8. Separate local CLI endpoints use their own local credentials; the pairing credential is never a
   universal pass for other endpoints (PRD 8.5.4 closing sentence).

## 5. Chunk transfer

Large payloads (for example exported PNG or SVG bytes) travel as chunk frames (PRD 8.5.4):

| Payload field | Type           | Required | Notes                                                                                                                                                                 |
| ------------- | -------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `transfer_id` | string         | yes      | Identity of one logical transfer; the daemon accepts only transfers it requested (PRD 8.5.4)                                                                          |
| `sequence`    | integer        | yes      | Position of this frame within the transfer                                                                                                                            |
| `byte_count`  | integer        | yes      | Bytes carried by this frame; each frame counts against the 1 MiB bridge frame cap (PRD 8.5.5)                                                                         |
| `total_bytes` | integer        | yes      | Declared full transfer size; used for bounded reassembly and validated against the 32 MiB transfer cap before buffers are allocated (PRD 8.5.4, PRD 8.5.5, PRD 8.6.1) |
| `final`       | boolean        | yes      | Marks the closing frame                                                                                                                                               |
| `checksum`    | string or null | yes      | Final checksum value on the `final` frame; `null` on earlier frames (PRD 8.5.4 "final checksum")                                                                      |

Rules:

- The daemon performs bounded reassembly: buffer space is committed against the declared
  `total_bytes` first, so a lying or malformed declaration fails allocation checks instead of
  exhausting memory (PRD 8.6.1 malformed payload row, `AT13`).
- A transfer whose frames do not complete before the request deadline is discarded; late frames are
  rejected (PRD 8.5.4, `FR14`).
- Size limits are enforced after decoding and decompression as well; compression is disabled in P0
  (PRD 8.5.4 closing sentence).
- Exceeding 1 MiB per frame or 32 MiB per transfer yields `PAYLOAD_TOO_LARGE` plus cleanup, not a
  best-effort truncation (PRD 8.5.5 bridge row).
- Chunking is a transport concern and is not semantic pagination: the MCP response built from the
  transfer still has to fit the 64 KiB envelope cap, or the tool paginates for real (PRD 8.5.4,
  PRD 8.5.5 response row).

Example chunk frame:

```json
{
  "protocol_version": "1.0",
  "message_type": "chunk",
  "request_id": "req_07",
  "session_id": "session_01",
  "connection_epoch": 2,
  "grant_epoch": 3,
  "deadline_ms": 60000,
  "payload": {
    "transfer_id": "tr_01",
    "sequence": 2,
    "byte_count": 524288,
    "total_bytes": 1048576,
    "final": false,
    "checksum": null
  }
}
```

## 6. Error mapping on this transport (PRD 8.7)

Error frames carry `code`, `message`, `retryable`, `action`, `request_id`, and optional
`retry_after_ms`; private stack traces never reach the agent.

| Transport condition                         | Code                                              | Behavior                                                                                                                       |
| ------------------------------------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| No valid `auth` frame within the 3 s window | connection close; `AUTH_REQUIRED` on next attempt | Re-pair through UI or CLI (PRD 8.5.4, PRD 8.7)                                                                                 |
| Secret expired, replayed, or wrong          | `AUTH_REQUIRED` / `AUTH_EXPIRED`                  | The agent never hunts for the secret itself (PRD 8.7, `FR02`, `AT06`)                                                          |
| Incompatible major `protocol_version`       | `PROTOCOL_MISMATCH`                               | Connection refused; doctor reports compatible versions (PRD 8.7)                                                               |
| Schema version the model layer cannot parse | `UNSUPPORTED_SCHEMA`                              | Handler refuses, no partial parse (PRD 8.7)                                                                                    |
| Frame over 1 MiB or transfer over 32 MiB    | `PAYLOAD_TOO_LARGE`                               | Cleanup, reassembly aborted (PRD 8.5.5)                                                                                        |
| Deadline exceeded                           | `TIMEOUT`                                         | Late results discarded even if they arrive (`FR14`, PRD 8.5.5)                                                                 |
| `cancel` accepted                           | `CANCELLED`                                       | Reported to the waiting client (PRD 8.7)                                                                                       |
| Grant or session revoked mid-transfer       | `REVOKED`                                         | Job cancelled, unsent responses dropped (PRD 8.4, section 8)                                                                   |
| Frame with a stale `connection_epoch`       | none (silently discarded)                         | Old-connection work is already cancelled; no replay (PRD 8.4, `AT09`)                                                          |
| Chunk sequence gaps or checksum mismatch    | transfer discarded                                | Structured `INTERNAL_ERROR`-class failure surfaced as a per-item batch error, never fallback (PRD 8.5.4; fuzz coverage `AT13`) |
| Socket dead between heartbeats              | `PLUGIN_OFFLINE`                                  | MCP reads fail with a structured error; no silent fallback to snapshots (PRD 8.3, PRD 8.7)                                     |
| Scheduler queue full at call time           | `BUSY`                                            | Carries `retry_after_ms` (PRD 8.5.5, `FR13`)                                                                                   |

## 7. Heartbeat

- Both sides send heartbeat frames every 5 seconds (PRD 8.4 heartbeat paragraph).
- After 3 consecutive missed heartbeats the peer is marked offline, which corresponds to the
  `N08` target of detecting writer disappearance within 15 s (PRD section 2.1, `FR16`, `AT09`).
- Request deadlines and the revoke action are applied early when disconnect evidence is available,
  so a dead socket does not leave an operation parked for the full 20 s or 60 s budget (PRD 8.4).
- Reconnect after any offline period requires the auth handshake again and raises
  `connection_epoch`; jobs from the old connection are not replayed blindly (PRD 8.4 state machine,
  `FR16`).

## 8. Epoch enforcement and revoke propagation

- `connection_epoch` increments on every reconnect; answers that carry a stale epoch are discarded
  by the receiver before any parsing of their payload (PRD 8.4 connection epoch entity, `AT09`).
- `grant_epoch` increments when the user changes scope or revokes; scope stays monotonic within a
  grant until the epoch changes, so a moved selection never widens access (committed interpretation
  4 in `docs/00-overview/01-prd-analysis.md`, `FR03`).
- Membership double enforcement: both the plugin handler and the daemon re-verify session, grant
  epoch, and node membership per request, so a bridge bug or forged frame cannot widen scope
  (`FR03`, `AT06`, `AT17`).
- Revoke propagation: revoke cancels any job in flight, removes responses that were not sent yet,
  invalidates asset URIs and cursors bound to the grant, and rejects late frames (PRD 8.4 heartbeat
  paragraph, `AT15`).
- Logical propagation is expected within 1 s (`N07`); caching and queued work is dropped on revoke
  (PRD 8.3 cache paragraph, PRD 5.4 step 6).
- Data an agent has already received cannot be retracted by the daemon (PRD 8.4 closing note,
  PRD 5.3).
- CLI revoke stays available while the plugin is unresponsive (PRD 8.5.1 closing paragraph).

## 9. Related documents

- MCP envelope that carries same `request_id`, epochs, and observation data to the agent:
  `docs/04-data-contracts/01-mcp-envelope.md`
- Tool surface that the scheduler issues over this bridge: `docs/04-data-contracts/02-tool-catalog.md`
- Error catalog: `docs/04-data-contracts/05-error-catalog.md`;
  full transport and memory budgets: `docs/04-data-contracts/06-resource-limits.md`
- Threat model for the credential and loopback rules: `docs/05-security/` folder
- Architecture and state machine: `docs/02-architecture/01-system-design.md`; state machine spec in
  PRD 8.4; implementation homes: `crates/protocol`, `crates/daemon`, `plugins/figma/ui`
