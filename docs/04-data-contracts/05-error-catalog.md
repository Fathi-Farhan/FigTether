# Data Contracts: Error Catalog

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose: this document freezes every error code the product may return, the trigger, recovery and
retry rules, the retryable classification, the typical emitter, the unified error object shape,
the distinction between protocol errors and business tool-result errors, batch error semantics,
and sanitization rules for messages. The catalog reproduces the PRD 8.7 error table exactly; the
retryable and emitter columns are commitments made here. Gate G0 has not run, so the catalog stays
`Proposed` until cross-language contract tests pass.

## 1. Error classification

Two error channels exist and must never be mixed (PRD 8.7):

1. **Business tool-result errors.** Expected product outcomes such as `OUT_OF_SCOPE`,
   `PLUGIN_OFFLINE`, `BUSY`, `DOCUMENT_CHANGED`. They are returned as a structured error object
   inside the normal tool result so the agent can read `code`, `retryable`, and `action`. These
   are not protocol failures; the MCP session stays healthy.
2. **Protocol errors.** Malformed MCP requests, wrong envelope shape, unknown schema versions, or
   framing violations travel the JSON-RPC error channel instead of a tool result. The agent can
   fix the request encoding, but no design data or retry advice belongs in this channel.

Rule: any bug that is the daemon's or adapter's fault becomes `INTERNAL_ERROR` as a business error
with a sanitized correlation ID; stack traces never enter either channel (PRD 8.7, FR18).

### 1.1 Code normalization rule

A component that receives an error code it does not recognize (future versions, unknown plugin
builds) normalizes it to `INTERNAL_ERROR` for the caller while logging the raw code locally at
standard diagnostics level. This keeps forward compatibility: a newer daemon talking to an older
MCP process degrades to a generic safe code instead of inventing one (PRD 8.11 versioning rules).

## 2. Canonical catalog from PRD 8.7

All codes are string literals sent verbatim in the `code` field. "Retryable" uses the policy in
section 4. "Emitters" lists the components that raise the code first; the daemon may pass through
plugin errors without change.

| Code                     | Trigger (PRD 8.7)                                              | Recovery / retry (PRD 8.7)                                                      | Retryable   | Typical emitters                          |
| ------------------------ | -------------------------------------------------------------- | ------------------------------------------------------------------------------- | ----------- | ----------------------------------------- |
| `AUTH_REQUIRED`          | Credential missing                                             | Re-pair through CLI or plugin UI; the agent never searches for secrets          | no          | daemon, mcp                               |
| `AUTH_EXPIRED`           | Credential expired                                             | Same pairing recovery as `AUTH_REQUIRED`                                        | no          | daemon, plugin, mcp                       |
| `PERMISSION_DENIED`      | Platform permission blocks the file or action                  | User fixes platform permission                                                  | no          | plugin, daemon                            |
| `OUT_OF_SCOPE`           | Grant does not cover the requested node                        | User widens the grant; agent must not retry                                     | no          | plugin, daemon (double enforcement, FR03) |
| `PLUGIN_OFFLINE`         | Plugin closed, asleep, or unreachable                          | Open and pair a session; use `list_sessions`; no silent fallback                | conditional | daemon                                    |
| `SESSION_NOT_FOUND`      | Stale or unknown session ID                                    | `list_sessions`, then pick a live session                                       | no          | daemon, mcp                               |
| `PAUSED`                 | User paused the session                                        | Resume is a user action only                                                    | conditional | daemon, plugin                            |
| `REVOKED`                | User revoked the grant                                         | New grant is a user action only; cache and assets already purged (FR15)         | no          | daemon, plugin, mcp                       |
| `NODE_NOT_FOUND`         | Node deleted or ID wrong within the session                    | Refresh the tree; per-item errors inside batches                                | no          | plugin, daemon                            |
| `CAPABILITY_UNAVAILABLE` | API, editor, or seat does not support the request              | Show the reason and available alternatives; never infer (FR09)                  | no          | plugin, daemon                            |
| `PAYLOAD_TOO_LARGE`      | Response exceeds the resource limit (PRD 8.5.5)                | Narrow fields or paginate; see `docs/04-data-contracts/06-resource-limits.md`   | conditional | daemon, plugin, mcp                       |
| `SCOPE_TOO_LARGE`        | Traversal or projection exceeds the resource limit (PRD 8.5.5) | Narrow root, fields, or depth; paginate or capture a snapshot                   | conditional | daemon, plugin                            |
| `BUSY`                   | Queue is full (PRD 8.5.5)                                      | Retry after `retry_after_ms` within the deadline; never tight-loop              | yes         | daemon                                    |
| `TIMEOUT`                | Operation slower than its deadline; queue wait included        | Late result is discarded; issue a new fresh request if still needed             | conditional | daemon, plugin                            |
| `CANCELLED`              | Client or user cancelled                                       | No retry of the same job; submit a new request                                  | no          | daemon, mcp                               |
| `DOCUMENT_CHANGED`       | Change detected while reading (PRD 8.3)                        | Retry at most once, or switch to a snapshot                                     | yes (max 1) | plugin, daemon                            |
| `CURSOR_EXPIRED`         | Scope/epoch changed under the cursor                           | Start a new traversal bound to the fresh epoch                                  | conditional | daemon                                    |
| `RESYNC_REQUIRED`        | Watch journal gap or invalid baseline (FR20, P1)               | Re-baseline: fresh read or new snapshot                                         | no          | daemon                                    |
| `UNSUPPORTED_SCHEMA`     | Client, plugin, or daemon schema versions incompatible         | `figtether doctor` lists compatible versions                                    | no          | daemon, plugin, mcp                       |
| `PROTOCOL_MISMATCH`      | Bridge protocol version incompatible (PRD 8.5.4)               | Same recovery path as `UNSUPPORTED_SCHEMA`                                      | no          | daemon, plugin                            |
| `EXPORT_FAILED`          | Node or format unsupported during export                       | Retry only with the fallback format chosen by the caller; never mutate the node | conditional | plugin                                    |
| `FONT_UNAVAILABLE`       | Font missing or unrenderable                                   | Label the text; do not retry the export; do not mutate fonts (PRD 8.7)          | no          | plugin                                    |
| `INTERNAL_ERROR`         | Unexpected bug                                                 | Report the sanitized correlation ID; no automatic retry without classification  | no          | daemon, plugin, mcp                       |

## 2.1 Write path codes from ADR-013

The write path extends the taxonomy with eight codes. They follow the same object shape (section 5),
are business tool-result errors rather than protocol errors, and carry the `action` hint that tells
the agent what it may do next. The codes are proposed: `ADR-013` is pending product approval
(`docs/00-overview/01-prd-analysis.md` section 12). `FONT_UNAVAILABLE` already exists in the PRD
8.7 table above and is reused for a text operation whose font the editor cannot load.

| Code                      | Trigger (write path)                                                                            | Recovery / retry                                                                       | Retryable   | Typical emitters |
| ------------------------- | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- | ----------- | ---------------- |
| `WRITE_NOT_GRANTED`       | The session holds a read grant only, or no grant at all                                         | Ask the user to grant write access in the plugin; the agent cannot widen its own grant | no          | daemon, plugin   |
| `APPROVAL_REQUIRED`       | An apply request arrives without a valid approval token                                         | Ask the user to approve the previewed plan in the plugin; the agent cannot approve it  | no          | daemon           |
| `APPROVAL_EXPIRED`        | The approval expired, or a grant change invalidated it                                          | Preview the plan again and ask for a new approval                                      | no          | daemon           |
| `PLAN_HASH_MISMATCH`      | The submitted plan differs from the plan the user approved                                      | Submit exactly the approved plan, or preview it again for a new approval               | no          | daemon, plugin   |
| `PLAN_INVALID`            | The plan is structurally invalid, or names an operation or property outside the closed contract | Fix the plan and preview it again                                                      | no          | daemon, plugin   |
| `BUDGET_EXCEEDED`         | The plan exceeds an operation, node, text, property, subtree, validity, or rate budget          | Split the plan into smaller plans; the limit is in the message                         | no          | daemon, plugin   |
| `DESTRUCTIVE_NOT_ALLOWED` | A delete or an out-of-scope move without the destructive flag, or without the user toggle       | Ask the user to allow the destructive operation, or drop it from the plan              | no          | daemon, plugin   |
| `MUTATION_REFUSED`        | The editor refused the mutation, for example a reparenting cycle or a readonly parent           | Adjust the plan; the refusal depends on the document, not on a transient condition     | conditional | plugin           |

Notes:

- The daemon is the first emitter for every code except `MUTATION_REFUSED`, which the editor
  raises and the daemon passes through unchanged.
- `BUDGET_EXCEEDED` always names the limit that was reached; the budgets are listed in
  `docs/11-write-path/01-write-path-design.md` section 3 and in `ADR-008` (amendment). The rate
  budget counts the plans a session starts, not the operations inside them.
- `PLAN_INVALID` covers a closed contract violation, for example a property outside the
  allowlist or an unknown operation kind. It is not a best effort path.
- `OUT_OF_SCOPE` (section 2) still answers a write that names a node outside the granted roots;
  the write codes do not replace it.
- The `action` hint of a write refusal is a sentence when the daemon raises the refusal
  (`crates/daemon/src/write_path.rs`) and one of the shared verbs when the plugin raises it
  (`plugins/figma/main/src/write/errors.ts`). The verbs a write refusal can carry are
  `new_grant_required`, `resume_by_user`, `narrow_parameters`, and `use_alternative`. The daemon
  passes a plugin action through unchanged.

## 3. Limit codes from PRD 8.5.5

PRD 8.5.5 additionally names `SESSION_LIMIT` (plugin session count) and `INVALID_LIMIT` (batch
size/depth, export bounds, cache admission). They use the same error object shape as section 5;
they are listed here so the catalog stays complete, and their rows live in
`docs/04-data-contracts/06-resource-limits.md`:

| Code            | Trigger                                                                                                                                    | Recovery / retry                                                 | Retryable   | Typical emitters |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------- | ----------- | ---------------- |
| `SESSION_LIMIT` | More than 4 plugin sessions per daemon (PRD 8.5.5)                                                                                         | User picks or closes a session; agent cannot free one            | no          | daemon           |
| `INVALID_LIMIT` | Batch larger than 50 IDs, depth above 32, export over 16 megapixels or wrong scale, or a cache item above the 16 MiB asset cap (PRD 8.5.5) | Split the batch, lower depth or scale, or choose a smaller frame | conditional | daemon, plugin   |

## 4. Retry policy

The global rule, from FR14 and PRD 8.3 and 8.5.5: retries are bounded, maximum one fresh retry,
only for transient conditions, and only for idempotent read operations. Writes do not exist in
v0.1/v0.2 (D03), so every read qualifies as idempotent with respect to the document (FR24), but
the retry budget still applies only to the codes marked `yes` above: `BUSY`, `DOCUMENT_CHANGED`.
The write path proposed by `ADR-013` does not change the read rule; it adds a stricter rule for
writes, stated below.

- `conditional` means the fixing action is a caller or user decision: retrying verbatim is
  forbidden until the condition changes (parameters narrowed, plugin back online, session resumed,
  retry with the chosen fallback format). Examples: `PAYLOAD_TOO_LARGE` after narrowing fields,
  `TIMEOUT` followed by a new request that is allowed to complete.
- Deadline handling: a request that misses its deadline while waiting in the queue is never
  executed (PRD 8.5.5). `TIMEOUT` therefore implies the work did not start or was discarded late.
- Post-`REVOKED` requests are never answered by replay, and late frames after revoke are dropped
  (FR14, FR15, PRD 8.4 heartbeat rules).

Write path rule: a write is not idempotent, so no write request is ever replayed automatically,
not even once. A refused apply (`WRITE_NOT_GRANTED`, `APPROVAL_REQUIRED`, `APPROVAL_EXPIRED`,
`PLAN_HASH_MISMATCH`, `PLAN_INVALID`, `BUDGET_EXCEEDED`, `DESTRUCTIVE_NOT_ALLOWED`) is never
retried verbatim. The plan has to change and be previewed again, which produces a new plan hash
and needs a new approval. `MUTATION_REFUSED` is conditional because the refusal depends on the
document state rather than on a transient condition, and the corrected plan needs a new approval
as well.

## 5. Error object shape

Every business error, including batch item errors, uses this shape:

```json
{
  "code": "BUSY",
  "message": "Request queue is full for this daemon",
  "retryable": true,
  "action": "wait_then_retry",
  "request_id": "req_01",
  "retry_after_ms": 750
}
```

Field contract:

| Field            | Type              | Rule                                                                                                                                                                                                                                                                                                                                                                        |
| ---------------- | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `code`           | string            | One of the catalog literals; unknown codes normalize to `INTERNAL_ERROR`                                                                                                                                                                                                                                                                                                    |
| `message`        | string            | Human-readable, sanitized per section 8                                                                                                                                                                                                                                                                                                                                     |
| `retryable`      | boolean           | Must match the catalog column; agents may branch on it                                                                                                                                                                                                                                                                                                                      |
| `action`         | string            | Recovery hint: `wait_then_retry`, `repair_pairing`, `narrow_parameters`, `refresh_tree`, `resume_by_user`, `new_grant_required`, `use_snapshot`, `upgrade_component`, `contact_support`; the write path reuses this vocabulary (`new_grant_required`, `resume_by_user`, `narrow_parameters`, `use_alternative`) and the daemon may replace it with a more specific sentence |
| `request_id`     | string            | The correlation ID of the failing request; never a stack trace                                                                                                                                                                                                                                                                                                              |
| `retry_after_ms` | number (optional) | Present only when `code = BUSY`; other codes never carry it                                                                                                                                                                                                                                                                                                                 |

`retry_after_ms` is required on `BUSY` (FR13) and optional nowhere else (PRD 8.7).

## 6. Protocol error shape

Malformed MCP requests use the protocol channel instead of a tool result. Example:

```json
{
  "jsonrpc": "2.0",
  "id": "req_02",
  "error": {
    "code": -32602,
    "message": "Invalid params: session_id must be a string",
    "data": { "schema_version": "1.0" }
  }
}
```

Rules: protocol errors carry no design content, no node IDs derived from user data, and no retry
advice beyond what the JSON-RPC specification allows. `PROTOCOL_MISMATCH` and
`UNSUPPORTED_SCHEMA` are business codes when the mismatch is discovered after negotiation; they
become protocol errors only when the request cannot be routed at all.

## 7. Batch semantics

Batch tools (`get_nodes`, `export_assets`) return an explicit batch status plus per-item status
(PRD 8.7). The batch status is one of:

- `complete`: every item `ok`.
- `partial`: at least one item `error`, and the successful items are returned normally.
- `failed`: the whole batch was rejected before execution (queue, scope, timeout), so no item ran.

Per-item records keep the request order:

```json
{
  "batch_status": "partial",
  "items": [
    { "node_id": "128:11", "status": "ok" },
    {
      "node_id": "9:1",
      "status": "error",
      "error": {
        "code": "NODE_NOT_FOUND",
        "message": "Node not found in this session",
        "retryable": false,
        "action": "refresh_tree",
        "request_id": "req_03"
      }
    }
  ],
  "summary": { "total": 2, "ok": 1, "error": 1 }
}
```

Rules: item errors never abort the undeleted remainder; `summary.ok + summary.error` must equal
`items.length`; a `failed` batch carries the batch-level error object in `error` and an empty
`items` list.

## 8. Sanitization rules

Every message in both channels obeys the diagnostics privacy table (PRD 8.6.2, 8.7, FR18):

1. No stack traces, source paths, or panic dumps in tool results, MCP payloads, or plugin frames.
2. No layer names, layer text, or any design content in `message` or `action`.
3. No tokens, pairing secrets, or credential material in any error field; pairing secrets never
   appear in URLs or tool schemas (FR02).
4. No file IDs or user identity claims; session and grant identifiers are opaque and required, but
   user names and file names are never echoed back as error context.
5. Fixed message templates per code; dynamic parts are limited to numeric counts and opaque IDs
   already known to the caller.
6. Correlation is by `request_id` only; operators join daemon-side logs using the same ID without
   needing content samples.

## 9. Consistency with other contracts

- Queue and limit triggers cross reference `docs/04-data-contracts/06-resource-limits.md`.
- The envelope places `error` next to `data`; see `docs/04-data-contracts/01-mcp-envelope.md`.
- Emission points per tool appear beside each tool row in
  `docs/04-data-contracts/01-mcp-envelope.md`.
- Tests: `AT06`, `AT07`, `AT08`, `AT13`, `AT14` exercise these codes through negative and fuzzing
  paths (PRD Appendix A).
- Write path codes are exercised by `AT21`, `AT23`, `AT24`, `AT25`, `AT26`, and `AT27` in
  `docs/06-testing/02-acceptance-catalog.md`; the write contracts live in
  `docs/11-write-path/01-write-path-design.md` section 2.6.

## 10. Open questions

| ID    | Question                                                                                                                 | Owner / closure                             |
| ----- | ------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------- |
| QEC01 | Whether `SESSION_LIMIT` and `INVALID_LIMIT` should be promoted into the canonical PRD 8.7 table at G0 review             | Contract reviewer                           |
| QEC02 | Exact `action` vocabulary size: keep small set or allow per-tool actions                                                 | Tool surface owner                          |
| QEC03 | Localization of `message` templates; English-only at v0.1 per docs README conventions                                    | Product maintainer                          |
| QEC04 | Whether `TIMEOUT` after a partial transfer that the client already received should return `partial` batch status instead | Scheduler owner, needs AT08 data            |
| QEC05 | Whether the write path keeps its own `action` verbs or reuses the shared vocabulary of section 5                         | Tool surface owner, with the ADR-013 review |
