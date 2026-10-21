# FigTether Privacy and Data Handling

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

This document states how every class of data that FigTether touches is stored, protected, and
deleted; how diagnostics are sanitized; what the local-only claim does and does not mean; and how
exports to disk and SVG content are handled safely. The data handling matrix is transferred from
PRD section 8.6.2 (`FGT-PRD-001`) and is normative for release wording as well as implementation. It
complements `docs/05-security/01-threat-model.md` (assets and residuals) and
`docs/05-security/02-access-control.md` (credential and grant mechanics). Nothing here describes
running code; each row is an obligation on the G0 to G3 gates and is traceable in
`docs/06-testing/03-traceability-matrix.md`.

## 1. Principles

These are the committed interpretations from `docs/00-overview/01-prd-analysis.md` Section 10 that
govern privacy behavior:

1. **Memory only by default.** Design data, credentials, and snapshots stay in bounded memory; only
   owner-only runtime support files touch disk (PRD 8.6.2).
2. **Fail closed.** Any storage permission uncertainty denies the operation (PRD 8.6.2 credential row).
3. **Bounded everything.** Every cache, queue, and transfer has an explicit cap (PRD 8.5.5).
4. **No silent substitution.** Snapshot never poses as live data, and offline imports never pose as
   session reads (`AT20`).
5. **Sanitized diagnostics.** Logs and doctor output never carry design content (PRD 8.6.2,
   8.7, `FR18`).

## 2. Data handling matrix

Copied from PRD 8.6.2, with evidence anchors added.

| Data class                     | Default state and location                                                             | Deletion or rotation                                                            | Evidence               |
| ------------------------------ | -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ---------------------- |
| Pairing secret                 | Memory only; displayed only through an explicit local CLI action                       | Consumed on use, after 5 minutes, or at daemon stop                             | `FR02`, `AT06`         |
| IPC client credential          | Owner only runtime directory; Windows DACL and POSIX permissions verified              | Rotation or daemon restart; fail closed if directory permissions are inadequate | `FR02`, `FR16`, `AT09` |
| Plugin credential              | Memory in the plugin UI only                                                           | Closed, revoked, or expired; never `clientStorage`, never `localStorage`        | `FR02`, `AT06`         |
| Design data, snapshots, assets | Bounded memory only                                                                    | Revoke, TTL, eviction, or daemon exit                                           | `FR15`, `AT15`         |
| Diagnostic logs                | Errors, latency, sizes, versions only; no layer names, no text, no file IDs, no tokens | Local rotation at most 7 days or 10 MiB                                         | `FR18`                 |
| Offline bundle (P1)            | Only explicit export to user chosen paths                                              | Deleted by the user; excluded from daemon purge claims                          | `FR21`, `AT20`         |

Two non-storage clarifications follow from this matrix:

- Secret handling in memory: the plugin UI keeps the session credential only while the plugin run is
  paired; it is deleted on plugin close, on revoke, or at expiry, and a fresh pairing is required
  afterward (PRD 8.4 state machine, `AT09`).
- Snapshot reading after the plugin closes stays possible only through the explicit snapshot source
  with the snapshot label; it never impersonates a live session (PRD Section 5.2 mode table, `AT04`).

## 3. Caches, snapshots, and materialized copies

| Copy                              | Lifetime                                                                                               | Deletion trigger                                                                            | Evidence               |
| --------------------------------- | ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------- | ---------------------- |
| Design cache entries (live reads) | Keyed by session, `grant_epoch`, node IDs, field projection, export settings, schema version (PRD 8.3) | Revoke, expiry, LRU eviction, or exit; default TTL 2 seconds for explicit cached reads only | `FR12`, `AT04`, `AT15` |
| Snapshot captures                 | UUID plus content hash, immutable (`FR05`)                                                             | TTL after idle (default 30 minutes, user bounded) or revoke (PRD 8.3)                       | `AT04`, `AT15`         |
| Asset store entries               | Grant bound opaque `asset_id` (PRD 8.5.1)                                                              | Revoke or eviction; single asset cap 16 MiB, cache cap 128 MiB, LRU (PRD 8.5.5)             | `AT11`, `AT15`         |
| Undelivered responses in queues   | Until send or deadline                                                                                 | Cancel, revoke, or deadline; deadlined requests are never executed (PRD 8.4, 8.5.5)         | `FR14`, `AT15`         |
| Watch journal (P1)                | At most 10,000 events or 5 minutes (PRD 8.3)                                                           | Overflow or resync; tombstones for deleted IDs                                              | `FR20`, `AT19`         |
| Runtime directory files           | While the daemon runs                                                                                  | Deleted at shutdown; stale files after a crash are detected and not trusted (PRD 8.7)       | `AT09`                 |

Volume and allocation rules (PRD 8.5.5) back these lifetimes: bounded queue of 64 requests total and
16 per client, frame cap 1 MiB, transfer cap 32 MiB, and bounded reassembly with a transfer deadline.
Cache caps are not an RSS guarantee (PRD 8.5.5 final paragraph), which is why eviction plus revoke
deletion, not absence of growth, is the privacy property.

## 4. Diagnostic sanitization

Allowed fields (allowlist; anything not listed requires a security review before it ships):

- Error codes and structured error names (`AUTH_EXPIRED`, `OUT_OF_SCOPE`, `REVOKED`, and the rest of
  the PRD 8.7 error table), each with `retryable`, `action`, and `request_id`.
- Protocol and schema versions, daemon and plugin build versions, OS version.
- Counters and measurements: requests per operation and source, cache hits, fresh plugin reads, queue
  wait and depth, p50 and p95 latency, byte counts, cancel and revoke counts, event gaps, RSS (PRD 8.7).
- Local correlation IDs that are meaningless outside the machine.

Never logged, in any component (`FR18`, PRD 8.6.2, `R13`):

- Layer names and node display names; text content and text runs; document labels and file
  identifiers including `fileKey` and file URLs; variable, style, or design token values; PNG and
  SVG bytes or hashes that could reidentify content; the pairing secret and any credential.

Additional rules:

- `figtether doctor` prints versions, manifest port, bind address, runtime directory ACL result,
  pairing and session state, and capabilities, and nothing else (PRD 8.7 doctor row). It never
  prints a secret; the pairing secret is displayed only by the explicit CLI pair action (PRD 8.6.2).
- Doctor output is the intended paste surface for support requests, which is why the allowlist is
  enforced in the doctor code path itself, not by user caution.
- Log files live in the owner only runtime directory, rotate locally at most 7 days or 10 MiB, and
  are never uploaded by FigTether (PRD 8.6.2 rotation row).
- Fuzzing and soak evidence must be attached as sanitized logs only; raw design content never enters
  the evidence pack (Appendix A evidence template, `R13`).

## 5. Telemetry and crash reports

- There is no cloud telemetry in any default configuration. FigTether makes no network calls beyond
  its loopback paths; metrics stay on the machine (PRD 8.6.2, `FR19`, `N09`).
- Crash report sending is explicit: an opt-in user action, shown for review before sending, and
  sanitized of design content, design identifiers, and credentials (PRD 8.6.2 final paragraph).
- The sanitizer for crash reports strips stack frames down to FigTether component symbols plus
  versions and error codes; editor memory dumps are never collected by FigTether.
- Security reports go through a private channel; public issues must not be asked to contain designs
  or tokens (PRD 8.11 security reporting row).
- The support issue template requests only version, OS, capability context, error ID, and a public
  fixture (PRD 8.7 support row).

## 6. Local-only claim wording

The exact committed claim (PRD 8.6.2, rendered for user facing docs):

> FigTether processes do not send design data to a project cloud service. The Figma editor and the
> AI client, including the model behind it, keep their own network paths. FigTether does not claim
> protection against same user malware, OS administrators, or a compromised device.

Interpretation obligations:

- The claim is scoped to FigTether processes; it is not a claim about the whole Figma application.
  Packet capture cannot conclude that the total application makes no internet calls; evidence must
  attribute connections to FigTether processes and match them against adapter code at the exact
  commit (Appendix A note, `N09`, `AT01`, `AT02`).
- The claim never becomes "no internet access", "air gapped", or "unlimited local reads" (PRD 8.11
  claims restrictions, `AT16`).
- Same-user malware, OS administrators, and compromised devices are named as out of scope wherever
  the local-only wording appears (residual risks RH01 to RH03 in `01-threat-model.md`).

## 7. SVG and filesystem export hygiene

- Exported SVG is data, not code. It carries a MIME label and is never interpreted as HTML or shown
  as an active page; previews are rendered in a sanitized, non-executing context (PRD 8.6.1 SVG row,
  `AT11` active SVG case).
- Assets are identified by an opaque `asset_id` bound to the grant, with MIME, byte size, and hash
  recorded; asset URIs become invalid on revoke (PRD 8.5.1, `FR03`, `AT15`).
- Filesystem exports happen only to user chosen directories. The agent cannot pass an arbitrary
  path; export endpoints write nowhere else (PRD 8.5.1, PRD 8.6.1).
- Path traversal defense: the user chosen directory is the only writable root, opaque IDs prevent
  design derived path components, and parent traversal components are rejected before use
  (threat TH08 in `01-threat-model.md`).
- Ephemeral image fill URLs from the editor are never promoted to permanent asset references
  (PRD 8.5.3 assets row, `FR23`).
- The P1 offline bundle is written only to a user chosen path, carries a manifest hash and schema
  version, enforces decompression bounds (zip bomb handling is fuzz scoped in `AT13`), and an
  imported bundle can never pose as a live session (`FR21`, `AT20`).

## 8. Agent prompt context hygiene

- Responses are bounded and staged: MCP responses up to 64 KiB, inline images up to 1 MiB raw and
  2 MiB encoded per response, otherwise resource references with projection and pagination
  (PRD 8.5.5, `FR07`).
- The pairing secret and the bridge endpoint are never tool parameters, so the model cannot echo a
  credential into a prompt (PRD 8.5.1).
- All design content, including layer names and text, is untrusted data; FigTether structures never
  embed instructions in that content (`D03`, TH07).
- Warnings, coverage, and availability fields describe missing data as unavailable; nothing is
  invented to fill gaps (`FR12`, `AT10`).
- What the AI client retains after receiving results is outside FigTether control (PRD 8.6.1
  residual for the token into prompt row).

## 9. Deletion verification hooks

| Guarantee                                                             | Where verified                                    |
| --------------------------------------------------------------------- | ------------------------------------------------- |
| Revoke drops grant, deletes cache and assets, blocks retry, no replay | `AT15`                                            |
| Daemon restart invalidates old credentials and stale files            | `AT09`                                            |
| Local diagnostics rotate and carry no content                         | Code review at G1 plus log inspection on fixtures |
| No mutation of the document through any read or export operation      | `AT18`                                            |
| No official MCP or REST egress, loopback only                         | `AT01`, `AT02`, `N09`                             |
| Bundle integrity and no implicit live routing (P1)                    | `AT20`                                            |

## 10. Cross references

- Threats, boundaries, and residual risks: `docs/05-security/01-threat-model.md`.
- Credential, pairing, and grant lifecycle: `docs/05-security/02-access-control.md`.
- Limits backing the bounded-memory property: `docs/04-data-contracts/06-resource-limits.md` and
  `docs/04-data-contracts/01-mcp-envelope.md`.
- Acceptance scenarios and traceability: `docs/06-testing/02-acceptance-catalog.md`,
  `docs/06-testing/03-traceability-matrix.md`.
- Release claims and evidence wording: `docs/07-ops/04-release-process.md` (claims reviewed per
  `AT16`).
- Planned implementation paths per PRD 8.4: `crates/daemon` (log rotation, shutdown deletion),
  `crates/core` (cache and journal lifetimes), `crates/assets` (asset store eviction),
  `crates/cli` (doctor and pair output sanitizer), `plugins/figma/ui` (credential memory only).
