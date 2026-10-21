# Bridge Transport and Protocol Design (Daemon and Plugin)

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

This document fixes the engineering design of the authenticated loopback bridge that connects the
FigTether daemon (Rust) to the FigTether plugin (TypeScript) inside Figma Design: topology and
port, the G0 criteria that finalize transport choice (D06), first-frame authentication, the frame
envelope, the message type allowlist, chunking and transfer limits, heartbeat cadence, and the
mapping of scheduler backpressure to BUSY responses. Normative sources: PRD 8.4, 8.5.4, 8.5.5 and
8.6.1 (FGT-PRD-001 v1.1). Every number here is a target until evidence exists (PRD 2.1).

## 1. Scope and references

- Scope: the daemon-to-plugin plane only. The MCP client plane is specified in
  `docs/02-architecture/03-mcp-design.md`; session, connection and job state machines are in
  `docs/02-architecture/05-state-machines.md`.
- Wire-level frozen values (limits, error codes, canonical JSON and hashing) are recorded after G0
  in `docs/04-data-contracts/`; MCP envelope rules live in
  `docs/04-data-contracts/01-mcp-envelope.md`.
- Code anchors, per the planned workspace layout (PRD 8.4): `crates/daemon` (listener, session
  registry, scheduler), `crates/protocol` (envelope and frame types, schema generation),
  `plugins/figma/ui` (dialer, credential memory), `plugins/figma/main` (allowlisted read
  handlers), `schemas/` (cross-language contract).
- Transport decision record: `docs/02-architecture/adr/` (ADR-002), resolved only by G0 evidence
  for D06; risk register entry R02 covers per-OS behavior differences.

## 2. Topology and endpoints

| Aspect             | Decision                                                                                                                                                                           |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Dial direction     | The plugin UI iframe dials the daemon (outbound). The daemon never dials the plugin, and the main sandbox performs no network I/O (PRD 8.4: the UI iframe is the network adapter). |
| Default endpoint   | `ws://127.0.0.1:38947`. Port 38947 is project owned, loopback only (PRD 5.1 step 3).                                                                                               |
| Port collision     | Fails loudly with `--port` guidance and a regenerated manifest; the daemon never rebinds silently (PRD 5.1, 8.7).                                                                  |
| Listener identity  | One daemon instance per user and port; `daemon_instance_id` differs after each restart (PRD 8.4).                                                                                  |
| Companion endpoint | A separate authenticated loopback path serves CLI and MCP local clients with runtime directory credentials; the pairing secret is never a universal authorization (PRD 8.5.4).     |
| Access checks      | Host header, origin allowlist, method and path allowlist are resolved before upgrade; no wildcard origin is accepted (PRD 8.6.1).                                                  |
| Frame budget       | 1 MiB maximum per frame, enforced before allocation. No permessage-deflate: compression is off in P0 (PRD 8.5.4).                                                                  |

## 3. Transport finalization criteria (D06, R02, G0)

The transport is a candidate, not a fact (D06). It is finalized only when every criterion below
has G0 evidence captured on Windows and macOS Figma Desktop with the exact development manifest.
Evidence is recorded per the PRD Appendix C release evidence template and lands in the evidence
slots of `docs/07-ops/04-release-process.md`; the decision record is `ADR-002` in
`docs/02-architecture/adr/`.

| ID  | Finalization criterion                                                                                                                                                                                    | Evidence recorded at G0                                                |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| C1  | The plugin UI iframe can open `ws://127.0.0.1:38947` on both target OS, the CSP connect-src rules of the Figma app permitting the manifest-declared loopback address.                                     | Connection, failure and close logs per OS.                             |
| C2  | Observed Origin values are deterministic enough to implement an allowlist (a known app origin or `null` admitted, with the session token remaining the primary authorization boundary, PRD 8.6.1).        | Origin capture per OS, plus negative tests with wrong Host and Origin. |
| C3  | `networkAccess` in the manifest, limited to the required loopback entries, validates and does not block the connection; no wildcard domain is ever shipped (PRD 3, S5).                                   | Manifest hash plus plugin console capture.                             |
| C4  | Authentication in the first frame completes inside the 3 s window on cold start, with no custom Authorization header (browser WebSocket constraint, PRD 8.5.4).                                           | Auth timing samples on both OS.                                        |
| C5  | Close and reopen of the plugin reconnects cleanly and bumps `connection_epoch` with no replay of old requests. Sleep, wake and kill detection are benchmarked at G1 and G2 (AT09, N08), not claimed here. | Reconnect log with epoch values before and after.                      |
| C6  | No core path traffic to official MCP or REST endpoints while the bridge is active (FR19, AT01, AT02, method of N09).                                                                                      | Egress capture attributed to the FigTether process.                    |

If any of C1 to C4 fails on one target OS, the PRD G0 exit rule applies: restrict support on that
OS (PRD 8.10), then evaluate an alternative local transport through a new ADR and retest. A silent
fallback and a public relay are both forbidden (PRD 8.6.1).

## 4. First-frame authentication (PRD 8.5.4)

1. The daemon starts one 3 s auth window when the socket opens. The first frame received must be
   the auth frame (`message_type: auth`), which doubles as the hello: its typed payload carries
   `protocol_version`, an opaque plugin build identifier and the pairing secret. Individual hello
   and auth frames would put the secret outside the first frame, so the PRD family `hello/auth`
   is implemented as this single frame.
2. Pre-auth reads are byte bounded (1 MiB frame cap) and schema shallow: anything beyond the auth
   payload closes the socket. No work, metadata and capability disclosure happens before auth.
3. Validation is strictly ordered and fail closed: protocol major version match, secret length
   and encoding check, the 5 min TTL not expired, the secret not consumed yet, then a constant
   time comparison that never takes an early exit branch on mismatch (PRD 8.6.1).
4. Success: atomically consume the secret (single use, FR02), create the session instance with a
   fresh `session_id` and `connection_epoch` (PRD 8.4), and answer with an `auth_result` variant
   carrying the ephemeral session credential, session mirror identifiers and the protocol limits.
   Secret bytes are zeroized immediately.
5. Failure: close the socket with the mapped auth close reason. A failed attempt never consumes
   the secret; it stays valid until TTL or daemon stop. Only error categories are metricized;
   secret bytes are never logged, and no URL, query or fragment transport exists at all (FR02).
6. Prohibited before `auth_result` (PRD 8.5.4): design data of any kind, operation enqueue,
   session metadata beyond non-correlatable error categories, and capability advertisements.
7. The CLI displays the secret through an explicit local pair action; agents never see it, and it
   is absent from tool schemas (PRD 8.5.1, 8.6.1 token and prompt row).

## 5. Frame envelope

| Field              | Shape                                | Direction        | Meaning                                                                                                                    |
| ------------------ | ------------------------------------ | ---------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `protocol_version` | semver string, for example `1.0`     | both             | Major must match; minor and accepted diffs are defined in `docs/04-data-contracts/`.                                       |
| `message_type`     | enum from the allowlist in section 6 | both             | Unknown pre-auth closes; unknown post-auth yields PROTOCOL_MISMATCH then close.                                            |
| `request_id`       | string                               | both             | Correlates request, progress, chunk, result, error and cancel; null on heartbeat.                                          |
| `session_id`       | string or absent                     | both             | Absent until auth succeeds and until a grant binds the session.                                                            |
| `connection_epoch` | unsigned integer                     | both             | Bumped by the daemon on every successful reconnect; frames with a stale value are dropped without side effects (PRD 8.4).  |
| `grant_epoch`      | unsigned integer or absent           | both             | Bumped on scope change or revoke (PRD 8.4); absent until a grant exists.                                                   |
| `deadline_ms`      | unsigned integer or absent           | daemon to plugin | Remaining wall clock budget for the correlated request: 20,000 ms read, 60,000 ms export, queue wait included (PRD 8.5.5). |
| `payload`          | one typed object per `message_type`  | both             | Rust serde types in `crates/protocol`; no free form JSON blobs.                                                            |

Epoch rules: the plugin echoes the epochs it received; the daemon compares them before executing a
request and before accepting a result or chunk. Late frames carrying a previous epoch are counted
as a metric and otherwise ignored (FR14, FR16, AT09).

## 6. Message type allowlist

The PRD names the families (8.5.4): hello/auth, capabilities, grant update, request, progress,
chunk, result, error, cancel, heartbeat, revoke. Wire names below map one to one; `auth_result` is
the named response variant of the auth family.

| Type           | Direction        | Purpose                                                                                                         | Notes                                                                                                                       |
| -------------- | ---------------- | --------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `auth`         | plugin to daemon | First frame: hello fields plus the pairing secret.                                                              | Mandatory first frame inside 3 s (PRD 8.5.4).                                                                               |
| `auth_result`  | daemon to plugin | Session credential, session mirror, limits.                                                                     | Replaces no other type.                                                                                                     |
| `capabilities` | both             | Plugin sends the capability fingerprint per session; daemon sends granted roots, supported operations and mode. | After auth only.                                                                                                            |
| `grant_update` | daemon to plugin | Current roots, `grant_epoch`, live or paused mode.                                                              | On allow, pause, resume and any scope change (FR03, FR15).                                                                  |
| `request`      | daemon to plugin | One allowlisted operation with parameters and `deadline_ms`.                                                    | Dispatch table in section 10.                                                                                               |
| `progress`     | plugin to daemon | `request_id`, visited or produced counts, stage label.                                                          | Emitted at slice boundaries (PRD 8.5.5).                                                                                    |
| `chunk`        | plugin to daemon | `transfer_id`, `sequence`, `bytes`, declared limit, payload bytes.                                              | Daemon to plugin is reserved and unused in P0.                                                                              |
| `result`       | plugin to daemon | Final payload, or transfer completion with checksum.                                                            | Single frame below 1 MiB else chunks.                                                                                       |
| `error`        | both             | `code`, `message`, `retryable`, `action`, `request_id`.                                                         | Codes from the PRD 8.7 table.                                                                                               |
| `cancel`       | daemon to plugin | `request_id` to stop.                                                                                           | Plugin confirms by dropping the work; plugin side cancel is reported via `error` or `result`, never by steering the daemon. |
| `heartbeat`    | both             | Minimal tick payload without session metadata.                                                                  | Cadence defined in section 8.                                                                                               |
| `revoke`       | both             | Plugin to daemon when the user revokes in the UI; daemon to plugin to command purge and stop sending.           | Purge rules in section 3 of `docs/02-architecture/05-state-machines.md`.                                                    |

Any other type: pre-auth closes the socket; post-auth sends PROTOCOL_MISMATCH error then closes
(PRD 8.7).

## 7. Chunking and transfer rules (PRD 8.5.4)

- F1: A transfer exists only under a request the daemon issued. Unsolicited `transfer_id` values
  are rejected and counted.
- F2: Chunk fields per PRD 8.5.4: `transfer_id` (UUID), `sequence` (0 based, monotonic per
  transfer), `bytes` (count of this chunk payload), the declared total byte limit (32 MiB
  maximum) and the payload bytes; a chunk fits the 1 MiB frame.
- F3: The final chunk carries a checksum over the reassembled byte stream (SHA-256, canonical
  rules per `docs/04-data-contracts/01-mcp-envelope.md`).
- F4: Reassembly is bounded: the buffer is sized from the declared totals only after the transfer
  is matched to an inflight request. A chunk beyond the declared limit aborts the transfer,
  reports PAYLOAD_TOO_LARGE and frees the buffer (PRD 8.5.5).
- F5: Transfers may interleave, but each transfer keeps monotonic, gapless sequencing; gaps wait
  no longer than the correlated deadline.
- F6: A transfer incomplete when its deadline passes is discarded, and later chunks for it are
  rejected (PRD 8.5.4). Downstream the request fails as TIMEOUT; late arrival never revives it.
- F7: A checksum mismatch aborts the transfer; no partial result is ever published.
- F8: Chunking is required only when a payload exceeds the 1 MiB frame; otherwise the whole result
  travels in one `result` frame.
- F9: Chunk counts are never exposed to MCP clients: transport chunking is not semantic pagination
  (PRD 8.5.4), and MCP JSON stays inside the 64 KiB response budget (PRD 8.5.5).
- F10: Cancel or revoke during an open transfer stops further chunk processing, frees the buffer
  immediately and never reads back from it (FR14, FR15).
- F11: Compression stays off in P0; enabling permessage-deflate is a `docs/04-data-contracts/`
  change with re-measured limits.
- F12: Auth material is never transported in chunk or transfer payloads.

## 8. Heartbeat and offline detection (PRD 8.4)

| Parameter          | Value                                                      | Note                                                                  |
| ------------------ | ---------------------------------------------------------- | --------------------------------------------------------------------- |
| Cadence            | Every 5 s after auth, from the plugin.                     | The daemon echoes inside its own tick; both directions exist.         |
| Miss tolerance     | 3 consecutive missed heartbeats.                           | Third miss declares OFFLINE on both sides.                            |
| Detection target   | Offline recognized within 15 s.                            | Matches N08 wording; a target until AT09 evidence at G1 and G2.       |
| While OFFLINE      | No new operations, no chunk acceptance, no grant mutation. | Recovery path defined in `docs/02-architecture/05-state-machines.md`. |
| Deadline interplay | Request deadlines may fire early on proven disconnect.     | Late plugin results for such requests are discarded (PRD 8.4).        |

Reconnect after OFFLINE reuses the in memory session credential of the same plugin run, bumps
`connection_epoch`, and requires grant revalidation; a restart of the plugin or daemon always
produces a new session and a new grant (PRD 8.4, 8.6.1 restart rows).

## 9. Backpressure and BUSY mapping (PRD 8.5.5)

- The scheduler enforces 64 queued requests in total and 16 per client. An enqueue that would
  exceed a cap is refused with BUSY carrying `retry_after_ms`; the plugin and MCP layers honor the
  delay and never retry in a tight loop.
- One active editor operation per session and one global asset export at a time; queue wait counts
  inside the 20 s read and 60 s export deadlines (PRD 8.5.5).
- Priority: revoke, cancel and heartbeat first, then small reads, then exports, with aging so
  large jobs are not starved (PRD 8.5.5).
- A request that exhausts its deadline while queued is never executed (PRD 8.5.5).
- The bridge never truncates silently: an oversized result is projected or chunked at the
  scheduler; truncation surfaces only through explicit coverage fields (FR07, FR12).

## 10. Handler allowlist and code safety (PRD 8.5.4, D03)

- `plugins/figma/main` keeps a static registry of named handlers; each entry maps to one MCP tool
  operation documented in `docs/02-architecture/03-mcp-design.md`. Unknown or unprobed operations
  answer CAPABILITY_UNAVAILABLE with an explicit reason (PRD 8.7).
- No `eval`, no `Function` constructor, no dynamic script loading, no shell or process surfaces
  from anywhere in the plugin; the read only property is enforced by handler audit, not by hint
  flags (D03, FR24).
- The handler revalidates every request independently of the daemon: session and grant binding,
  scope membership, batch size 50 and depth 32 limits, and the consistency contract (FR03, FR24).
- Handlers never write to the document, selection or pages as a side effect of reads.

## 11. Schemas as the cross-language contract

- `schemas/` JSON Schema files are generated from Rust types in `crates/protocol` (serde plus
  schemars). The TypeScript types in `plugins/figma` are generated or validated against the same
  files, so the schemas are the single source of truth for both sides (PRD 8.5.4).
- Contract tests run both ways in CI: Rust parses TypeScript authored fixtures, TypeScript
  validates Rust authored payloads; a mismatch fails the build (PRD 8.5.4, E02).
- `protocol_version` is negotiated in the auth frame; an exact major match is required and a minor
  mismatch is tolerated only where `docs/04-data-contracts/` defines the diff. Unknown optional
  fields are tolerated; required field removal is a major change (PRD 8.11 versioning row).

## 12. Failure matrix

| Condition                                          | Bridge behavior                                           | Client visible error                                                       |
| -------------------------------------------------- | --------------------------------------------------------- | -------------------------------------------------------------------------- |
| Wrong, expired, reused secret or 3 s window breach | Close before any data exchange.                           | AUTH_REQUIRED or AUTH_EXPIRED; user pairs again.                           |
| Wrong Host or Origin, disallowed path              | Reject before upgrade; metric only.                       | Connection failure; no design data existed to leak.                        |
| Oversized frame or transfer                        | Close pre-auth; abort request and free buffers post-auth. | PAYLOAD_TOO_LARGE.                                                         |
| Stale `connection_epoch` or `grant_epoch`          | Drop the frame without side effects, count it.            | TIMEOUT or REFRESH path per state machine.                                 |
| Unknown message type or payload schema             | PROTOCOL_MISMATCH error then close (post-auth).           | PROTOCOL_MISMATCH; `figtether doctor` lists compatible versions (PRD 8.7). |
| Bad checksum or non monotonic chunks               | Abort transfer, free buffers.                             | Failed result for that request.                                            |
| 3 missed heartbeats                                | Both sides declare OFFLINE.                               | PLUGIN_OFFLINE; snapshots only via explicit snapshot source (PRD 8.3).     |
| Deadline breach (read or export)                   | Stop or discard the result after `deadline_ms`.           | TIMEOUT; late frames are rejected.                                         |
| Revoke mid transfer                                | Purge, stop sends, reject late frames.                    | REVOKED (PRD 8.4).                                                         |

## 13. Traceability

| Anchor                                         | Covered by                                              |
| ---------------------------------------------- | ------------------------------------------------------- |
| FR02 pairing, single use, 5 min TTL            | Sections 4 and 11; AT06.                                |
| FR03 double enforcement and epochs             | Sections 4, 5, 10; AT05, AT06, AT17.                    |
| FR13 bounded queue, BUSY with `retry_after_ms` | Section 9; AT08.                                        |
| FR14 deadline, cancellation, bounded retry     | Sections 5, 7, 8; AT08, AT15.                           |
| FR16 reconnect with new epoch, no blind replay | Sections 3, 5, 8; AT09.                                 |
| AT06 negative auth and host or origin tests    | Section 3 criteria C2, C4 and section 12.               |
| AT09 close, reopen, sleep, wake, restart       | Section 3 criteria C5, then G1 and G2 evidence per N08. |

## 14. Open questions

1. Exact close codes and error class names are frozen with `docs/04-data-contracts/` after G0.
2. Whether the UI iframe Origin is a stable app origin or `null` per OS, decided by C2 captures.
3. IPv6 loopback (`::1`) support when Figma resolves `localhost` to it; PRD 8.6.1 says test if
   supported. Affects the host allowlist and manifest entries.
4. Whether the Figma manifest supports port scoped loopback entries in `networkAccess`; if not,
   the host plus port validation stays entirely server side. Criterion C3 decides.
5. Progress frame frequency: per slice boundary of the 8 ms traversal target, or a fixed interval;
   measured during G1 profiling (N06).
