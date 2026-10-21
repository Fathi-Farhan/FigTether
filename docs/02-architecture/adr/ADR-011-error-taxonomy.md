# ADR-011: Structured error taxonomy over MCP tool results

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Accepted   |
| Created              | 2026-10-20 |
| Related decision IDs | D03, D04   |
| Supersedes           | -          |

## Context

Agents misbehave with free-text errors: they retry blindly, invent missing facts (`P08`), or pull
stack traces into prompts, hurting both prompts hygiene and privacy (`R13`). PRD 8.7 fixes
delivery: business errors travel as structured tool result errors; malformed MCP requests use
protocol-level errors; every error carries `code`, sanitized `message`, `retryable`, `action`,
`request_id`, and optional `retry_after_ms`; stack traces stay private (`FR18`); batches carry
per-item status plus an explicit overall state. The budget table contributes `SESSION_LIMIT` and
`INVALID_LIMIT` (8.5.5); the bridge contributes `PAYLOAD_TOO_LARGE` and frame-level error frames
(8.5.4). Fail-closed is committed interpretation 2 in `docs/00-overview/01-prd-analysis.md`:
uncertainty denies with a structured reason.

## Decision

1. The full code list is verbatim from PRD 8.7 plus the two codes marked there as 8.5.5 rows:

| Code                   | Trigger                              | Retryable                        | Action for the agent                   |
| ---------------------- | ------------------------------------ | -------------------------------- | -------------------------------------- |
| AUTH_REQUIRED          | credential missing                   | no                               | ask the user to re-pair                |
| AUTH_EXPIRED           | credential expired                   | no                               | ask the user to re-pair                |
| PERMISSION_DENIED      | platform permission or grant missing | no                               | user fixes rights or extends grant     |
| OUT_OF_SCOPE           | node outside grant scope             | no                               | user fixes rights or extends grant     |
| PLUGIN_OFFLINE         | plugin closed or asleep              | after recovery only              | re-check with `list_sessions`          |
| SESSION_NOT_FOUND      | session closed or stale              | no                               | list sessions and stop guessing        |
| PAUSED                 | user paused the session              | no                               | ask the user to resume                 |
| REVOKED                | user revoked the grant               | no                               | ask for a new grant, never reuse       |
| NODE_NOT_FOUND         | node deleted or ID wrong             | no                               | refresh the tree; per item in batches  |
| CAPABILITY_UNAVAILABLE | API, editor, or seat lacks support   | no                               | report reason and alternatives         |
| PAYLOAD_TOO_LARGE      | transfer exceeds budget              | no                               | narrow the request per chunk rules     |
| SCOPE_TOO_LARGE        | traversal resource limit             | no                               | narrow root, fields, depth; paginate   |
| SESSION_LIMIT          | sessions above 4 per daemon          | no                               | user closes or selects a session       |
| INVALID_LIMIT          | batch, depth, or byte parameters     | no                               | adjust parameters and repaginate       |
| BUSY                   | queue full                           | yes                              | wait `retry_after_ms`; no tight loops  |
| TIMEOUT                | 20 s read or 60 s export deadline    | one retry, idempotent reads only | retry once or issue a new fresh read   |
| CANCELLED              | request cancelled                    | no                               | repeat only as a new fresh read        |
| DOCUMENT_CHANGED       | change detected during read          | one retry, idempotent            | retry once or take a snapshot          |
| CURSOR_EXPIRED         | scope or epoch change, or expiry     | no                               | start a new listing or take a snapshot |
| RESYNC_REQUIRED        | watch journal gap (P1)               | no                               | resync baseline (`FR20`)               |
| UNSUPPORTED_SCHEMA     | incompatible versions                | no                               | align versions via `doctor`            |
| PROTOCOL_MISMATCH      | incompatible protocol version        | no                               | align versions via `doctor`            |
| EXPORT_FAILED          | node, format, or render unsupported  | no                               | use the chosen fallback format         |
| FONT_UNAVAILABLE       | font missing or unsupported          | no                               | report; never substitute silently      |
| INTERNAL_ERROR         | unexpected defect                    | no auto-retry                    | report sanitized correlation ID        |

2. The `message` field is sanitized: no design text, layer names, tokens, `fileKey`, or stack
   traces (`FR18`, `R13`). Private stack details go to local diagnostics keyed by a correlation ID
   that the error result carries for support correlation.
3. Delivery split: business and operational outcomes use the structured tool result with `code`;
   only MCP framing failures (unknown tool, invalid parameters, unparseable payload) produce
   protocol-level errors at the client boundary.
4. Batch responses: `get_nodes` and `export_assets` carry per-item success or error and state the
   overall result explicitly as complete, partial, or failed (8.5.1 and 8.7 factual wording).
5. Retry discipline: at most one fresh read retry, only for transient, idempotent cases, mirroring
   `FR14` and 8.5.5; whenever the answer is time-based, `retry_after_ms` is present in the result.

## Rationale

A finite, machine-readable vocabulary gives agents deterministic behavior, and the action column
is the anti-invention guard for `P08`. Stack traces stay private for privacy (`R13`) and prompt
hygiene. Per-item batching prevents one bad node ID from erasing otherwise valid results, which
the per-item error wording of 8.5.1 already assumes.

## Consequences

- Positive: a testable error matrix; support flows map directly from codes; diagnostics correlate
  through `request_id` plus correlation ID.
- Negative: new failure kinds require taxonomy additions rather than ad hoc strings; results
  become slightly larger.
- Neutral: codes freeze after G0 with the tool names (8.5.1); future codes append rows with the
  same columns and appear first as experimental diagnostics.

## Alternatives considered

- Human-string errors only: rejected; agents guess retry logic from prose, which is the `P08`
  behavior the PRD aims to prevent.
- Codes without action hints: rejected; half the value is telling the agent the next concrete
  step, the 8.7 recovery column.
- Protocol errors for product failures: rejected; it collapses MCP framing errors into business
  semantics and hides per-item outcomes for batch tools.
- Stack traces in development builds of tool results: rejected; even development logs can reach
  public issues (`R13`); traces live in local diagnostics only.

## Verification

Evidence: an error matrix suite that drives each code intentionally (wrong token, stale session,
queue exhaustion, oversized export, mid-read change, malformed frame), plus prompt and log
scanning confirming no design content or stack traces, plus batch partial states asserted.
AT refs: AT06 (`AUTH_*`, permissions, revoke), AT08 (`BUSY`, `TIMEOUT`, cancel), AT15 (post-revoke
refusals), AT10 (`CAPABILITY_UNAVAILABLE`, unavailable fields), AT07 (`SCOPE_TOO_LARGE`,
`INVALID_LIMIT`), AT04 (`DOCUMENT_CHANGED`).
Gate: G1 (matrix fixtures) and G2 (release evidence including the prompt scan).

## References

- PRD 4.3 (`FR13`, `FR14`, `FR18`), 8.3, 8.5.1 (surface and per-item errors), 8.5.4, 8.5.5,
  8.7 (error table), Appendix A.
- `D03`, `D04`; committed interpretation 2 in `docs/00-overview/01-prd-analysis.md` section 10.
- ADR-005 (`DOCUMENT_CHANGED` source contract), ADR-006 (`CURSOR_EXPIRED`, `INVALID_LIMIT`),
  ADR-008 (`SESSION_LIMIT`, timeout rows), `docs/04-data-contracts/` (error envelope schema).
