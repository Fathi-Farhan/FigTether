# Plugin Design (Figma Adapter)

Status: `Proposed`

This document fixes the design of the TypeScript plugin inside Figma Design: the division of
labor between the main sandbox and the UI iframe, the manifest strategy for development and
production (D11), the per session capability probe flow, the scoped and sliced traversal engine,
the normalization pipeline with field availability annotations, serial asset export, snapshot
capture orchestration from the UI, the grant UI flow, accessibility requirements, and credential
hygiene. Normative sources: PRD 8.5.3, 8.5.5, 8.6.1 and 8.6.2, section 5.3 (UI minimum), section
5.1 (installation steps). Plugin runtime facts re-verify at G0 per assumptions S2, S3, S5, S6, S7.

## 1. Scope and references

- In scope: `plugins/figma/main` (main sandbox) and `plugins/figma/ui` (UI iframe), their
  manifest, and their bridge client behavior.
- Out of scope: daemon protocol rules (`docs/02-architecture/02-bridge-design.md`), MCP surface
  (`docs/02-architecture/03-mcp-design.md`), state machines
  (`docs/02-architecture/05-state-machines.md`), data contracts in `docs/04-data-contracts/`.
- Key PRD locks: FR03 double enforcement, FR09 capability probes with explicit unavailable
  answers, FR11 asset export, FR24 no document mutation, S5 manifest rules for dynamic-page and
  network access.

## 2. Runtime division: main sandbox versus UI iframe

| Concern         | Main sandbox                                                                                                                                                    | UI iframe                                                                                                       |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Environment     | Figma plugin sandbox with documented Plugin API access; no network primitives, no reliable general storage.                                                     | HTML, CSS, TypeScript runtime with fetch and WebSocket, subject to the manifest and app CSP.                    |
| Responsibility  | Read handlers, capability probes, traversal, normalization inputs, export calls, snapshot build. Never opens sockets.                                           | Network dialer to the daemon, pairing and grant UI, status display, credential memory, chunk transfer plumbing. |
| Trust boundary  | Treats bridge payloads as untrusted data; validates every incoming postMessage against the schema and drops unknown shapes (PRD 8.4 validated postMessage row). | Never runs plugin API calls for reads; it only relays validated requests and never stores design data.          |
| Message channel | Typed postMessage to and from the UI iframe only; payloads mirror the bridge schema subset in `crates/protocol`.                                                | Opens and owns the WebSocket to `ws://127.0.0.1:38947` and reassembles chunks before handoff (PRD 8.4 diagram). |
| Failure mode    | A failed handler returns a structured error; it never leaves traversal state behind.                                                                            | A failed socket moves the session to OFFLINE and stops all reads; it never fakes results.                       |

The sandbox never performs network I/O and never stores secrets; the iframe never touches the
document. This split is tested on both sides (`S3` runtime facts, G0 criterion C1 in
`docs/02-architecture/02-bridge-design.md`).

## 3. Manifest strategy

- Development manifest, generated or imported by `figtether manifest` (PRD 5.1 step 4): plugin id
  belongs to the user account, `documentAccess=dynamic-page` (never load all pages by default, S5),
  `editorType` set to Figma Design, and `networkAccess` limited to the required loopback entries
  for `127.0.0.1` on port 38947. No wildcard domains, ever (PRD 3 assumptions, S5, 8.6.1).
- The manifest is regenerated only through an explicit CLI action; port changes always rerun
  manifest generation plus doctor, never a silent port shift (PRD 5.1, port collision row).
- `devAllowedDomains` is never treated as production configuration; production publication needs a
  separate reviewed manifest with justified network access reasons (PRD 8.6.1, D11 out of v0.1).
- Doctor checks the committed manifests: loopback only domains, editor type, dynamic-page, and
  that the development bundle is not shipped with production claims (PRD 8.7 doctor row).

## 4. Capability probe flow (FR09, D09)

1. When the plugin attaches to a session, the main sandbox runs a bounded probe set before the
   user grant: document and page access, node enumeration under a granted root, text and font
   loading, paint and effect field exposure, component references, variables API presence (D09),
   and supported export formats and scales.
2. Each probe outcome is one of: available, unavailable with reason, or unknown with timestamp.
   Unavailable and unknown answers are valid results, never errors (PRD 8.7 CAPABILITY_UNAVAILABLE
   row, AT12).
3. The fingerprint is sent once as the bridge `capabilities` frame and recomputed per session;
   capabilities are never inferred from account names or seat labels (FR09).
4. Request time capability checks gate handler dispatch; a missing capability yields
   CAPABILITY_UNAVAILABLE with the recorded reason, no silent substitute (fail closed).
5. Probe evidence feeds the support matrix (S2, D09); the plugin never rewrites a capability
   based on user identity strings.

## 5. Traversal engine (PRD 8.5.5)

- Scoped: the engine walks only under granted roots and re checks membership at slice boundaries.
  Nodes that leave the subtree stop resolving; sibling or ancestor context outside the scope is
  never included (PRD 8.4, FR03).
- Sliced: each batch targets 8 ms of work, then yields to the editor runtime (the availability and
  precision of yield timers inside the sandbox is an open question, section 12). Batches shrink
  adaptively when a slice overruns, and each boundary emits a `progress` frame (PRD 8.5.5).
- Budgeted: at most 10,000 visited nodes per operation; batch size 50 node ids and depth 32 are
  hard limits answered with INVALID_LIMIT; a resumable overrun returns a partial result with a
  cursor, otherwise SCOPE_TOO_LARGE (PRD 8.5.5, FR07).
- Cancel aware: cancellation and revoke checks run at every slice boundary; a cancelled traversal
  reports partial coverage with a reason and frees all intermediates (FR14).
- Mutation free: traversal never changes selection, current page or visibility (FR24, AT18).

## 6. Normalization pipeline (PRD 8.5.3)

- Primary input: documented Plugin API properties read per node (typed values: geometry, layout
  fields, text runs, fills, strokes, effects, component references, variables bindings).
- Optional adapter input: subtree `exportAsync` with `JSON_REST_V1` fills gaps where the typed API
  does not expose a field; it is never the only model and never triggers REST calls (FR08, FR19,
  S4).
- Availability annotations: every field carries available, unavailable with reason, or unknown;
  `null` is only a value where the annotation says available (PRD 8.5.2, 8.5.3).
- Unknown node types are preserved as unsupported entries with their raw type string; they are
  never dropped or guessed into a known shape (PRD 8.5.3 identity row, AT10).
- Mixed typography becomes styled runs; mixed values are reported per run, fonts that fail to load
  are flagged and produce FONT_UNAVAILABLE on export paths (PRD 8.5.3 typography row).
- Geometry states the coordinate system explicitly; absolute transforms are never repackaged as
  relative CSS positions (PRD 8.5.3 geometry row).
- Unresolved style and variable aliases stay unresolved IDs with mode ambiguity reported; the
  plugin never resolves external libraries outside the grant (FR10).
- Visual fallback: where paint or effect data cannot represent the render (image fills, complex
  effects), the node payload references a PNG fallback produced by the export pipeline (PRD 8.5.3
  paint row).
- The plugin produces measured values and annotations; canonicalization, hashing and coverage
  labeling live in `crates/core` so rules stay out of the adapter (PRD 8.4 decision row).

## 7. Export pipeline (FR11, PRD 8.5.5)

- Formats: PNG and SVG for fixture supported shapes; scale restricted to 0.25 through 2; a request
  beyond 16 megapixels is answered INVALID_LIMIT with guidance to pick a smaller frame or scale.
- Serial: one global export lane; the scheduler runs at most one editor export at a time across
  all sessions, requests queue behind it (PRD 8.5.5, AT11).
- Failure mapping: EXPORT_FAILED for unsupported node shapes or render failures, FONT_UNAVAILABLE
  when a text node cannot render its fonts; the response names a fallback format the user chose,
  never a silent format switch (PRD 8.7).
- SVG outputs are flagged as data: the UI sanitizes previews, SVG is never executed as HTML, and
  raw bytes are delivered with an explicit MIME type (PRD 8.6.1 SVG row).
- Asset outputs above the 1 MiB frame travel as chunked transfers with a 32 MiB transfer ceiling;
  a single asset above 16 MiB is refused at the daemon (PRD 8.5.5).
- Export cost and render time are reported on a metrics lane for N03 evidence; export queue wait
  is included in the 60 s deadline (PRD 2.1, 8.5.5).

## 8. Snapshot capture (FR05, PRD 8.3)

- Capture is always a user action from the UI (Capture snapshot button); agents never trigger it
  (PRD 8.5.1 grant and capture ownership row).
- The UI orchestrates: it asks the sandbox to build a capture, the sandbox walks the granted
  roots, produces the normalized payload and a content hash, and streams the result as a chunked
  transfer with `started_at` and `observed_at` window metadata (PRD 8.3 best_effort rule).
- A capture during user edits stays valid as `consistency=best_effort`; the plugin does not
  silently retry and the UI shows the capture window rather than claiming transactionality (PRD
  8.3, 8.5.2).
- The daemon mints the immutable `snapshot_id` and binds it to the originating session, grant
  epoch and content hash; edits never flow back into a stored snapshot (FR05).
- Capture obeys the same 10,000 node traversal budget and slicing rules; oversized captures
  return partial coverage with a cursor, never a truncated silent capture (PRD 8.5.5).

## 9. Grant UI flow (PRD 5.1 steps 5 to 8, 5.3)

1. Pair: the user enters the pairing secret from the CLI; the iframe authenticates inside the 3 s
   window and displays the session name and fingerprint for confirmation (PRD 5.1 step 5).
2. Status: the UI shows connection state, shared root, mode, last read time, queue depth and
   available capabilities (PRD 5.3).
3. Select root: the user picks the node or subtree roots to share; the selection is checked to be
   in the current document and the grant binds those root ids; later selection changes never widen
   the grant (PRD 5.1 step 6).
4. Allow session read: the user presses allow; the plugin registers the grant with the daemon,
   which assigns `grant_id` and `grant_epoch` (PRD 8.4 grant row).
5. Operate: Pause, Resume, Capture snapshot and Revoke buttons cover the session lifecycle (PRD
   5.1 step 8). New operations inside the granted scope never reconfirm; scope expansion always
   requires explicit user action (PRD 5.3).
6. Revoke: stops further sends, drops the queue, purges cache and assets, and answers later
   agent reads with REVOKED; already delivered data is outside daemon reach and the UI says so
   (PRD 5.3, 8.4).
7. CLI revoke remains available when the plugin itself is unresponsive (PRD 8.5.1).

## 10. Accessibility (PRD 5.3)

- Keyboard: every action is reachable and operable by keyboard without pointer capture; focus
  order follows the visual order of the panel.
- Focus visibility: a visible focus indicator on all interactive elements, never suppressed.
- Labels: controls carry screen-reader labels; status messages are announced politely.
- Contrast: WCAG AA contrast as a target for text and controls; icons and text, never color alone,
  express status (PRD 5.3 status row).
- Error wording: error messages name the recovery action, for example re-pair, regrant or restart
  the daemon (PRD 5.3).

## 11. Credential and data hygiene (PRD 8.5.4, 8.6.2)

- The pairing secret and the session credential live only in memory inside the UI iframe. They are
  never written to `clientStorage`, `localStorage` or any persistent store; they are erased on
  plugin close, revoke or expiry (PRD 8.6.2 plugin credential row).
- Secrets never appear in URLs, query strings or logged fragments (FR02); the UI never logs raw
  postMessage payloads that could contain them.
- The plugin never persists design data, snapshots or assets; all retained design data lives in
  the bounded daemon memory and is purged on revoke, TTL, eviction or daemon exit (PRD 8.6.2
  storage table).
- Stage reads and writes to postMessage stay within the schema subsets documented in
  `docs/04-data-contracts/`; both sides drop unknown or oversized messages.

## 12. Traceability

| Anchor                                     | Covered by                                    |
| ------------------------------------------ | --------------------------------------------- |
| FR03 double enforcement                    | Sections 5 and 9; AT05, AT06, AT17.           |
| FR09 capability probes                     | Section 4; AT12.                              |
| FR08 and FR10 normalization                | Section 6; AT10.                              |
| FR11 asset export                          | Section 7; AT11.                              |
| FR13 and FR14 queue and deadline awareness | Sections 5 and 7; AT08.                       |
| FR15 grant UI and revoke                   | Section 9; AT06, AT15.                        |
| FR24 no document mutation                  | Sections 5 and 8; AT18.                       |
| PRD 5.3 accessibility                      | Section 10, verified in the acceptance suite. |

## 13. Open questions

1. Timer and yield availability inside the sandbox for the 8 ms slicing loop (`setTimeout`
   accuracy); captured in the G0 spike and drives adaptive batch defaults (analysis gap 5).
2. Whether `networkAccess` supports port scoped loopback entries, or whether port binding
   validation must live fully server side (criterion C3 of the bridge design).
3. Image fill URL origin and lifetime from the editor; decides what the adapter may pass through
   under FR23 and the asset hygiene rules (analysis gap 6).
4. Variables and style API coverage under Starter seats (S2, D09): probe matrix at G0 decides
   which fields are available versus unavailable.
5. Whether the UI iframe Origin observed on each OS is a stable app origin or `null`, which sets
   the allowlist rule on the daemon (bridge criterion C2).
