# ADR-002: Loopback WebSocket transport as the G0 candidate

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Proposed   |
| Created              | 2026-10-20 |
| Related decision IDs | D06        |
| Supersedes           | -          |

## Context

The plugin main sandbox has no network access; the UI iframe does, under manifest-controlled
network domains (`S3`, `S5`). `D06` names the default candidate: a local WebSocket on loopback
where the plugin dials the daemon, finalized only by G0 evidence. `R02` warns that localhost
WebSocket and CSP behavior differ across runtimes, and PRD 8.6.1 requires a proof on real Figma
Desktop before anything else depends on this transport. The bridge protocol in 8.5.4 already
assumes browser-style WebSocket semantics: no custom Authorization header, the 256-bit single-use
pairing secret inside the first auth frame within a 3 second deadline, and no design data, enqueue
operations, or session metadata before auth completes. The default port is `38947`, loopback only;
a collision must fail loudly with `--port` guidance and manifest regeneration, never a silent
rebind (PRD 5.1).

## Decision

Candidate transport: the daemon listens only on the loopback interface at `127.0.0.1:38947`; the
plugin UI iframe opens a browser WebSocket and dials the daemon; after hello/auth, frames follow
8.5.4. G0 must record, per OS (Windows and macOS), on the exact Figma Desktop release under test:

1. Manifest proof: a development manifest with `networkAccess` limited to the required
   loopback/port, `documentAccess=dynamic-page`, `editorType` figma loads and runs (`S5`).
2. CSP and Origin proof: WebSocket construction succeeds under the iframe CSP; the observed
   `Origin` header value (a `null` Origin must be treated as expected for this iframe context) and
   the `Host` expectation are captured; the daemon allowlist, peer and Host validation, and the
   written rule that the token remains the primary authorization boundary (`S` 8.6.1) are
   recorded, with the PRD 8.6.1 rule that the token remains the primary authorization boundary
   regardless of Origin validation.
3. Auth proof: hello/auth completes inside the 3 second auth timeout with the single-use secret;
   request, progress, chunk, result, error, cancel, and heartbeat frames then flow per 8.5.4.
4. Disruption proof: plugin close/reopen and system sleep/wake force re-auth and a new
   `connection_epoch`; old-epoch replies are discarded (`FR16`).
5. Fail-closed binding: port collision aborts with `--port` guidance; IPv6 loopback (for example
   `::1`) is tested where the stack supports it; never bind `0.0.0.0` (8.6.1).
6. Evidence capture per the Appendix C release evidence template: commit, artifact hash, Figma
   version, manifest hash, OS, command, outcome, sanitized logs.

Status remains `Proposed` until G0 passes on both operating systems. Any failure triggers a new
ADR for an alternative local transport with a retest plan; the rejected alternatives below come
back only through that new ADR with fresh evidence. A cloud relay is never a fallback (PRD 4.1 and
8.6.1).

## Rationale

Plugin-dials-daemon keeps the daemon the single authoritative listener and mirrors the only
documented network surface the plugin actually has (the UI iframe), instead of inventing a bridge
the platform never promised. Loopback binding plus token auth is the PRD's primary authorization
boundary, with Origin validation as defense in depth (8.6.1). The loud port failure rule protects
pairing determinism for onboarding (`P09`, `AT03`).

## Consequences

- Positive: one documented mechanism across both OSes; CSP and Origin behavior becomes recorded
  release evidence instead of folklore; the auth-frame pattern matches the browser model.
- Negative: G0 can veto the candidate after work has started; each OS and Figma release
  combination must rerun the matrix before shipping claims.
- Neutral: the port is a release constant; changing it requires explicit user configuration plus
  manifest regeneration, not daemon preference.

## Alternatives considered

- Native messaging (browser extension style host process): rejected; Figma plugins are not browser
  extensions and have no native messaging host, per-OS host installs would break `FR01`
  (prebuilt, no toolchain), and an extra privileged listener is unnecessary attack surface.
- Long-poll HTTP over loopback: rejected for now; it shares the same CSP constraints while adding
  per-request overhead and worse cancel and heartbeat semantics; revisit only through a new ADR
  tied to G0 evidence.
- Cloud relay between daemon and plugin: rejected and explicitly never a fallback; relay is out of
  scope per 4.1, contradicts local-only claims (8.6.2, `AT16`), and would route design data off
  the user's machine.
- Plain HTTP request-response without WebSocket: rejected; it duplicates the 8.5.4 frame types
  poorly and complicates heartbeat, cancel, and progress semantics.

## Verification

Evidence: the completed G0 OS x Figma matrix with all six recorded items per row; failure captures
follow the `R02` diagnosis protocol (Origin, port, CSP, manifest) so a future ADR can act on
concrete data.
AT refs: AT09 (`FR16` disruption series after G0), AT06 (auth, replay, and origin checks after
G0), AT03 (pairing on two OSes at G2).
Gate: G0 only (candidate proof and transport selection for `D06`); later gates reuse the evidence.

## References

- PRD 3 (`S3`, `S5`), 4.1, 4.3 (`FR16`), 5.1 (port rules), 8.4 (state machine, heartbeat, revoke),
  8.5.4, 8.6.1.
- `D06`, `R02`; E01 deliverables (pairing and transport spike) in PRD 4.2.
- ADR-001 (process boundary), `docs/05-security/` (loopback binding and origin validation), source
  register `S3`, `S5`.
