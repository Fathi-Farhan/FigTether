# FigTether Access Control and Credential Model

Status: `Proposed`

This document defines who and what can act on FigTether, how the plugin and MCP client are
authenticated (pairing), how permission is expressed and changed over time (grant lifecycle), where
enforcement actually happens, how credentials are stored per platform, and which guarantees are
explicitly not claimed. It is normative for crates `daemon`, `mcp`, `cli`, and `core` and for the
plugin. Threat framing and asset definitions live in `docs/05-security/01-threat-model.md`; storage
and deletion policy details live in `docs/05-security/03-privacy-data-handling.md`. Security
defaults follow the fail closed principle in `docs/00-overview/01-prd-analysis.md` Section 10.2:
uncertainty in auth, scope, version, or capability denies or reports `unsupported`. Nothing here
describes running code; all items are obligations on the G0, G1, and G2 gates.

## 1. Identity model

FigTether has no user accounts and no human identity provider. Identity is local and technical:

| Principal                 | Identity presented                                                                   | Trust level                              | Can do                                                                           | Cannot do                                                                             |
| ------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| Agent / AI client         | The MCP client process; identified technically by its local installation `client_id` | Untrusted caller                         | Call the frozen read tools of PRD 8.5.1 with `session_id` or `snapshot_id`       | Pair, extend scope, revoke, capture snapshots, or write to the filesystem (PRD 8.5.1) |
| `figtether mcp` (Rust)    | Loops credentials from the owner only runtime directory                              | Trusted core component                   | Validate tool parameters, check session and epoch before dispatch, forward reads | Serve stale epoch answers (`FR16`)                                                    |
| User via CLI              | The operator of the machine                                                          | Authoritative human                      | Pair, revoke, run `figtether doctor`, print config                               | No role beyond local operation                                                        |
| `figtether daemon` (Rust) | `daemon_instance_id`, random UUID per start (PRD 8.4)                                | Trusted core component                   | Hold secrets and grants, schedule, cache, revoke                                 | Claim recall of delivered data                                                        |
| Plugin UI iframe          | Temporary session credential after pairing, held in memory only                      | Semi trusted (needs daemon verification) | Perform pairing, show grant UI, transport frames                                 | Enforce scope alone; the daemon rechecks everything (`FR03`)                          |
| Plugin main sandbox       | Reachable only through validated postMessage (T4)                                    | Semi trusted                             | Execute allowlisted read handlers after membership checks                        | Mutate the document (`FR24`, `D03`)                                                   |

Fixed interpretation rules (PRD 8.4 entity table):

- `client_id` marks an installation of an MCP client on this machine; it is never evidence of a
  specific human and never a permission source by itself (PRD Section 5.4, `FR04`).
- `session_id` is a UUID per plugin run and tab. It is never derived from a file name or `fileKey`
  because `fileKey` is not generally available and two tabs can show similarly named files (`D05`,
  `AT05`).
- Document label is display only. It is never a routing key or authorization identity (PRD 8.4).

## 2. Pairing flow

Purpose: convert a human present at the machine into a temporary, single use, daemon bound plugin
session credential, without any secret reaching the model, URL, disk, or logs (`FR02`, PRD 8.5.4).

1. The user starts the daemon on the default port 38947, bound to the loopback literal only
   (PRD Section 5.1, `FR19`, `AT02`).
2. The user triggers pairing from the CLI; the daemon generates a 256 bit random secret. It is
   displayed locally through that explicit CLI action only (PRD 8.6.2 row for the pairing secret).
3. The user types the secret into the plugin UI. The plugin UI opens the WebSocket to the loopback
   endpoint.
4. The first frame after the connection opens is the authenticating hello and auth frame. If
   authentication is not completed within 3 seconds, the server closes the connection (PRD 8.5.4).
5. Before authentication completes, no design data, no enqueued operation, and no session metadata
   may traverse the connection (PRD 8.5.4).
6. The daemon compares the presented secret using constant time comparison (PRD 8.6.1 credential row).
7. The secret is single use. A successful pairing consumes it; every later replay is rejected
   (`FR02`, `AT06` replay case).
8. On success the secret is swapped for a temporary session credential kept in plugin UI memory
   only. It is never written to `clientStorage` or `localStorage`, never placed in a URL, fragment,
   query string, or log record (PRD 8.5.4, PRD 8.6.2).
9. The agent later receives `session_id`, scope summary, and capabilities without ever seeing a
   secret (PRD Section 5.4 step 2, PRD 8.6.1 token into prompt row).

Failure handling (`AT06` cases): wrong secret, expired secret (5 minute TTL), replayed secret, the
3 second auth window exceeded, and wrong Host or Origin all fail closed with `AUTH_REQUIRED` or
`AUTH_EXPIRED`. Each denied attempt is logged as a decision code with no content (`FR18`). A failed
pairing never degrades into an unauthenticated session; the daemon does not silently extend the
window or accept a second use.

The pairing secret is not a universal key: other endpoints, including the client CLI entry point,
use separate local credentials and must not accept the pairing credential as CLI authorization
(PRD 8.5.4 final paragraph).

## 3. Grant lifecycle

State machine (PRD 8.4): `UNPAIRED` then `PAIRED` then `AWAITING_GRANT` then `READY`, with
`READY` able to enter `PAUSED` and back, `REVOKED` as terminal, and `OFFLINE` reachable from
`PAIRED`, `READY`, and `PAUSED`. Reconnect requires reauthentication and grant revalidation.

Grant creation is a human action in the plugin UI:

- The user selects the roots to share and presses **Allow read session**; the grant binds
  `session_id`, the root set (each root plus its descendants, deduplicated), and `grant_epoch`
  (PRD Section 5.1 step 6, PRD 8.4 entity rows).
- Subsequent selection changes never widen the grant (PRD Section 5.1, committed interpretation 4).
- The agent cannot create, extend, or renew a grant; scope expansion is always a UI action
  (PRD 8.5.1).

| Aspect             | Pause                            | Revoke                                                               |
| ------------------ | -------------------------------- | -------------------------------------------------------------------- |
| New live reads     | Rejected with `PAUSED` (PRD 8.3) | Rejected with `REVOKED` (`FR15`)                                     |
| New snapshot reads | Also rejected while paused       | Rejected; snapshot eviction from the daemon (`FR15`)                 |
| Queued work        | Retained, resumes on resume      | Cancelled; undelivered responses dropped (PRD 8.4)                   |
| Cache and assets   | Kept but not served              | Deleted (`FR15`, `AT15`)                                             |
| Credentials        | Unchanged                        | Revoked session stops receiving; plugin drops credential (PRD 8.6.2) |
| Recovery           | User presses Resume (`FR15`)     | New grant required from the user                                     |
| Late frames        | Held by epoch rules              | Refused; late results discarded (`FR14`, `FR16`)                     |

Epoch rules (`FR03`, `FR16`, threat rows TH05 and TH11 in `01-threat-model.md`):

- `grant_epoch` increments whenever scope changes or the grant is revoked. Handlers, cursors, cache
  entries, and asset URIs bound to the old epoch stop resolving (`FR03`).
- `connection_epoch` increments on every reconnect. Answers carrying an old epoch are discarded,
  and long dead work is never revived (`FR16`, `AT09`).
- After any epoch bump, a fresh membership check is required before data flows again (`AT17`).
  Out of subtree nodes stop resolving; sibling and ancestor information outside the scope is not
  included (PRD 8.4).
- CLI revoke remains available when the plugin is unresponsive (PRD 8.5.1, `AT15`).

## 4. Enforcement points

Double enforcement exists because either side can be buggy or fed manipulated frames (PRD 8.4
decision: enforcement on both sides against bridge input and bugs).

| #   | Enforcement point      | Planned component              | Check performed                                                                                                                                        | Failure result                                    | Evidence               |
| --- | ---------------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------- | ---------------------- |
| E1  | MCP tool invocation    | `crates/mcp`                   | Identifiers are opaque strings; `session_id` and `snapshot_id` are exclusive and mandatory for data operations; secret and endpoint are not parameters | Structured tool result error                      | PRD 8.5.1, `AT14`      |
| E2  | Daemon pre dispatch    | `crates/daemon`                | Session valid, grant active, `grant_epoch` current, `connection_epoch` current                                                                         | `AUTH_EXPIRED`, `REVOKED`, or `SESSION_NOT_FOUND` | `FR03`, `AT06`, `AT15` |
| E3  | Daemon membership      | `crates/core`                  | Requested node IDs belong to a granted root subtree, reverified on every fresh read                                                                    | `OUT_OF_SCOPE` per item                           | `FR03`, `AT17`         |
| E4  | Plugin handler         | `plugins/figma/main`           | Allowlisted handler rechecks membership before touching the scene graph                                                                                | Handler refused                                   | `FR03`, `AT17`         |
| E5  | Cursor and asset URIs  | `crates/core`, `crates/assets` | Bound to the grant epoch of issuance                                                                                                                   | `CURSOR_EXPIRED`                                  | `FR03`, `FR16`         |
| E6  | Cache key construction | `crates/core`                  | Key includes session or snapshot, `grant_epoch`, node IDs, field projection, export settings, schema version (PRD 8.3)                                 | Stale entry never served                          | `FR12`, `AT04`         |
| E7  | Result surface         | `crates/core`                  | No sibling, ancestor, or cross session data beyond the scope; only in scope style references, others as unresolved IDs                                 | Warning and coverage fields                       | `FR04`, `FR10`, `AT05` |

## 5. Credential storage policy

| Data                      | Location                                                                                                    | Protection and verification                                                                                 | Rotation or deletion                                      |
| ------------------------- | ----------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| IPC client credential     | Owner only runtime directory: Windows, DACL on a per user directory; macOS and Linux, POSIX 0700 equivalent | Ownership verified at daemon start and before each credential write; doctor reports the ACL check (PRD 8.7) | Rotated on daemon restart (`FR16`)                        |
| Plugin session credential | Memory of the plugin UI iframe only                                                                         | Never `clientStorage`, never `localStorage`, never persisted in any form (PRD 8.6.2)                        | Dropped on close, revoke, or expiry                       |
| Pairing secret            | Daemon memory only                                                                                          | Displayed via explicit local CLI action only (PRD 8.6.2)                                                    | Consumed on use, at 5 minutes, or at daemon stop          |
| Runtime directory files   | Per user runtime path                                                                                       | Stale files after a crash are detected and not trusted (PRD 8.7)                                            | Deleted at shutdown together with design memory (PRD 8.7) |

Fail closed rule (PRD 8.6.2 credential row, committed interpretation 2): if the ownership check of
the runtime directory cannot prove that it is owner only, the daemon refuses to write credentials
there, refuses to start the affected listener, and reports the failure through `figtether doctor`
and startup output. A downgrade to a shared directory is never accepted silently.

Restart hygiene: a daemon restart produces a new `daemon_instance_id` and new credentials; a plugin
from the previous run cannot authenticate with its old credential and must re-pair or, when the
credential is still valid in memory within a short reconnect, reauthenticate and raise the
connection epoch (PRD 8.4 state machine, `AT09`).

## 6. Audit points

- Constant time comparison is used for the pairing secret and for credential verification; the
  implementation is reviewed at the G1 security review (PRD 8.10.2, threat TH03).
- Authorization decisions are logged as codes and correlation IDs without design content. Log
  records never contain layer names, text, file IDs, or tokens (`FR18`, PRD 8.7).
- Credential rotation on daemon restart is recorded as an event with error codes only (`FR16`).
- `figtether doctor` output is sanitized before a user pastes it into an issue (PRD 8.7, `R13`).
- Support requests follow the issue template: version, OS, capability context, error ID, and a
  public fixture; private files are never a prerequisite (PRD 8.7).
- Revocation and scope change decisions are reconstructible from diagnostics plus user action
  history; daemon side telemetry contains no design content (PRD 8.6.2).

## 7. Explicit non claims

- Same-user malware and OS administrators are not defended. A process with the same user rights, or
  an administrator, can read memory and the runtime directory. This is residual risk RH01 and RH02
  in `01-threat-model.md` (PRD 8.6.2, `R10`).
- Revoke cannot recall data already delivered to the agent. It stops later transfers and deletes
  daemon side state; bytes already received remain with the client (PRD Section 5.3, PRD 8.4).
- The agent cannot elevate privilege through read tools, and `readOnlyHint=true` on tools is not an
  authorization mechanism (PRD 8.5.1).
- Hints, tool names, and document labels are advisory; authorization rests solely on session,
  grant, epoch, and membership checks (`FR03`).
- Localhost reachability is not identity: Host and Origin checks reduce spoofing risk, but the
  token remains the primary authorization boundary for the null origin case (PRD 8.6.1 TH01).

## 8. Cross references

- Threats and residuals: `docs/05-security/01-threat-model.md`.
- Storage, deletion, logging, and export hygiene: `docs/05-security/03-privacy-data-handling.md`.
- Tool surface, envelope, and epoch fields: `docs/04-data-contracts/02-tool-catalog.md` and
  `docs/04-data-contracts/01-mcp-envelope.md` (PRD 8.5.1 and 8.5.2).
- Resource limits referenced by E1 to E7: `docs/04-data-contracts/06-resource-limits.md`.
- Acceptance scenarios: `docs/06-testing/02-acceptance-catalog.md`; traceability:
  `docs/06-testing/03-traceability-matrix.md`.
- Transport decision pending G0: `docs/02-architecture/adr/ADR-002` (`D06`, `R02`).
- Planned implementation paths per PRD 8.4: `crates/daemon`, `crates/core`, `crates/mcp`,
  `crates/cli`, `crates/protocol`, `plugins/figma/ui`, `plugins/figma/main`; these paths are created
  at implementation time.
