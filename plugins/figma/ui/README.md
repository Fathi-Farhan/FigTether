# FigTether plugin UI (iframe)

This directory holds the user interface of the Figma plugin. It runs inside the plugin iframe and
owns the only network path of the plugin: the local WebSocket to the daemon.

Normative sources: `docs/02-architecture/04-plugin-design.md` (division of labor, grant flow,
accessibility, credential hygiene), `docs/05-security/02-access-control.md` (pairing, grant
lifecycle, credential storage policy), `docs/00-overview/03-personas-journeys.md` (journey C, the
designer control loop) and PRD section 5.3 (UI minimum).

## What the UI owns

- The WebSocket to `ws://localhost:38947/bridge`, the first frame handshake and the heartbeat.
- The pairing secret and the session credential. Both live in closure variables, never in
  `localStorage`, `sessionStorage`, a cookie or a URL, and both are dropped as soon as they are
  used or the session ends.
- The pairing panel, the grant panel, the write approval panel, the read session status strip, the
  session controls and the sanitized activity log.
- The strict postMessage channel to the main sandbox.
- The decision of the user about a write plan. The decision travels to the daemon as a frame of the
  type `approval`; the plugin never applies a plan on its own.

## What the main sandbox owns

- Every read against the document, every capability probe and the build of a capture.
- The membership check of the granted roots. The UI never grants a root that the sandbox did not
  verify.

## Build integration

| Item                | Value                                |
| ------------------- | ------------------------------------ |
| Entry module        | `plugins/figma/ui/src/ui.ts`         |
| Markup              | `plugins/figma/ui/index.html`        |
| Stylesheet source   | `plugins/figma/ui/styles.css`        |
| Bundled script path | `dist/ui.js` next to `dist/ui.html`  |
| Bundled style path  | `dist/ui.css` next to `dist/ui.html` |

The bundle of `plugins/figma/build.mjs` picks `ui/src/ui.ts` as the entry of the user interface and
writes `dist/ui.js`. The entry imports the stylesheet, so the bundler writes `dist/ui.css` next to
the bundle and the markup links it with `./ui.css`. There is no external asset and no remote font.

The wire types come from the shared package `@figtether/shared`, which mirrors `crates/protocol`.
The compiler, the bundler and the tests all resolve that package through the same path mapping of
the root solution, so there is exactly one contract on both sides of the bridge.

## Flows

### Pair

1. The user pastes the one time secret that `figtether pair` prints and presses **Pair**.
2. The input field is emptied in the same event handler, so the document holds no copy.
3. The UI dials the daemon. The first frame is `hello_auth` with
   `{secret, client_id, protocol_version}` and it is sent inside the three second window that the
   daemon allows. The secret is cleared from memory right after the frame is written.
4. The daemon answers with a `result` frame whose request id is `auth` and whose payload is the
   accepted handshake: `{session_credential, session_id, connection_epoch, grant_epoch,
capabilities}`. The session credential replaces the secret in memory.
5. The panel shows the session name and a short fingerprint of the session id. The fingerprint is a
   hash, never the identifier itself.

A refused handshake names the recovery action, for example `AUTH_EXPIRED. Run 'figtether pair' for a
new secret, then pair again.`

### Grant

1. The user presses **Refresh candidates** and the sandbox reports the candidate roots of the
   current selection.
2. The user checks the roots and presses **Allow read session**.
3. The UI sends a `grant_update` to the sandbox with `{root_ids, grant_epoch}` so that the sandbox
   can enforce the membership, and a `grant_update` frame with the same roots to the daemon.
4. The daemon registers the grant and assigns the epoch. The status strip shows the granted roots.

A later selection change never widens the grant. The grant panel always states this, and the
controller keeps the granted roots separate from the candidate list.

### Write grant

The write grant is a second, separate action next to the read grant. The read grant never implies
it: a read grant always sends the write mode `read`, and only the **Allow write session** button
sends the write mode `write`. The button uses the roots of the read grant, raises the grant epoch
and states that every plan still needs an approval. The button turns into **Restrict the session to
reading**, so the user can take the write grant back.

### Write approval

The agent initiates design creation and edits through declarative plans. Preview resolution
does not mutate the document; after user approval, `apply_write` reaches the native mutation
executor in the plugin sandbox. The UI iframe relays requests and never accesses document nodes.

1. The daemon sends a request frame with the operation `preview_write`. The request carries the plan
   and the approval id the daemon issued for it.
2. The sandbox resolves the plan and answers with a `preview` message. The interface shows the plan
   in the approval panel: the nodes to create, to change, to move and to remove, the intent of the
   agent, the warnings of the plugin and the reasons of a blocked plan. A blocked plan offers no
   approve action.
3. The user presses **Approve the plan** or **Reject the plan**. The interface sends an `approval`
   frame to the daemon with `{approval_id, plan_hash, approved, reason?}`. The reason is optional
   and it never reaches the log.
4. The daemon applies the approved plan and the plugin answers with a `write_result` message. The
   status area reports the counts of the nodes, the undo commit and the version id.

The interface never moves the focus to the approve button, the panel holds no form and the approve
button is a plain button, so a key press that belongs to another control cannot approve a plan.

### Write result

The status area reports how many nodes were created, changed, moved and removed, whether the change
was committed to the undo history of Figma as one step and the id of the version checkpoint when one
was written. The wording never promises a revert that FigTether does not provide: the recovery path
is the undo command of the editor.

### Read session status

The status strip shows the connection state, the number of granted roots, the mode (live, snapshot
or offline bundle), the time of the last read, the queue depth and the available capabilities. The
capability list names every probe outcome, including unavailable with a reason.

### Controls

| Control          | Effect                                                                                 |
| ---------------- | -------------------------------------------------------------------------------------- |
| Pause            | Sends `grant_update` with mode `paused`. New reads are refused, the grant is intact.   |
| Resume           | Sends `grant_update` with mode `live`. The epoch moves, cursors and assets expire.     |
| Capture snapshot | Asks the sandbox for a capture, reassembles the chunks and streams them to the daemon. |
| Revoke           | Asks for a confirmation, sends `revoke`, drops the credential and clears the state.    |

Every button reports a success or an error state in a live region. Revoke needs a confirmation and
states the honest boundary of the action: data the client already received cannot be recalled.

## Sandbox channel contract

Every message on the channel is a strict envelope:

```json
{
  "channel": "figtether.bridge",
  "kind": "grant_update",
  "type": "grant_update",
  "payload": {}
}
```

The channel marker, the envelope and the vocabulary of the answers live in the shared package, in
`plugins/figma/shared/src/bridge.ts`. The user interface and the main sandbox import them from
there, so the two sides of the channel cannot drift apart. The discriminator travels in `kind` and
in `type`, and the sandbox accepts either of them.

The UI accepts a message only when the channel matches, the payload is a typed object, the type is
known and the message stays inside the size cap of 1 MiB. A message that fails a check is dropped
without a reply.

### From the UI to the sandbox

| Type                | Payload                               | Meaning                                                    |
| ------------------- | ------------------------------------- | ---------------------------------------------------------- |
| `frame`             | a bridge frame                        | A frame of the daemon that the sandbox has to execute.     |
| `grant_update`      | `{root_ids, grant_epoch, write_mode}` | The roots that the user granted and the mode of the grant. |
| `cancel`            | `{request_id}`                        | The user cancelled an operation.                           |
| `revoke`            | `{reason}`                            | The user revoked the grant.                                |
| `capabilities`      | `{}`                                  | Ask the sandbox for its capability report.                 |
| `selection_request` | `{}`                                  | Ask the sandbox for the candidate roots of the selection.  |

### From the sandbox to the UI

| Type           | Payload                                                 |
| -------------- | ------------------------------------------------------- |
| `result`       | `{data}`                                                |
| `error`        | the structured error object                             |
| `progress`     | `{request_id, visited, elapsed_ms}`                     |
| `chunk`        | `{transfer_id, sequence, bytes, total_bytes, checksum}` |
| `capabilities` | `{capabilities, notes}`                                 |
| `selection`    | `{roots: [{node_id, node_type, child_count}]}`          |
| `preview`      | the preview of a write plan                             |
| `write_result` | the result of an applied write plan                     |

The UI forwards every frame that the daemon sent to the sandbox with the type `frame`, and it
answers the daemon with the result of the sandbox on the request id of the daemon. The preview and
the write result travel to the daemon the same way, wrapped under `preview` and under `result`.

### From the UI to the daemon

| Type       | Payload                                       | Meaning                                |
| ---------- | --------------------------------------------- | -------------------------------------- |
| `approval` | `{approval_id, plan_hash, approved, reason?}` | The decision of the user about a plan. |

The approval decision goes straight to the daemon over the socket. The sandbox never sees it.

## Integration notes

The list below records the original integration gaps. Selection messages, request-ID propagation,
and daemon grant-epoch acknowledgements have since been implemented and regression-tested.
Items 1 to 3 are historical, not current blockers. Capture, reconnect, mode handling, and editor
loading still need end-to-end validation; see the
[current implementation status](../../../docs/10-implementation/01-implementation-status.md).

1. **The candidate roots of the selection.** The sandbox has no message that reports the selection
   before a grant exists: the handler `get_selection` calls `requireGrant` and therefore refuses
   before the first grant. The UI sends `selection_request` and expects `selection`. The sandbox
   needs two entries in its allowlist and a handler that reads `currentPage.selection` without a
   grant.
2. **The request id of a result.** The result envelope of the sandbox carries `{data}` without the
   request id. The UI answers the daemon in the order in which it forwarded the requests, and it
   reads the request id out of the transfer id of a chunk transfer (`tr_<request id>_<counter>`).
   The sandbox should carry the request id in the result payload.
3. **The grant epoch.** The daemon registers a grant and assigns its own epoch, but it does not
   answer the plugin with that epoch. The UI keeps a counter that starts at 1, which matches the
   first epoch of the daemon, and it raises the counter on every scope change. The daemon should
   answer a `grant_update` with the assigned epoch so that the UI can hand the same epoch to the
   sandbox.
4. **The reconnect.** The daemon accepts a pairing secret in the `hello_auth` frame and consumes
   it. A reconnect with the in memory session credential is refused with `AUTH_REQUIRED` until the
   daemon accepts the session credential in the same field.
5. **The user capture.** The daemon has no entry point for a capture that the user starts in the
   plugin. The UI asks the sandbox for the capture and sends the result with a request id that the
   plugin minted. The daemon needs a job that waits for that request id.
6. **The queue depth and the last read time.** The daemon reports neither to the plugin. The status
   strip shows "Not reported by the daemon" until the daemon sends them with the status of the
   session.
7. **The mode of a grant.** The frozen contract carries the root ids and the epoch. The mode is the
   extension of `docs/02-architecture/02-bridge-design.md` section 6, which describes a grant
   update that carries the live or the paused mode. The field is ignored by a daemon that does not
   know it yet.

## Accessibility

- Semantic structure: `header`, `main`, `section` with `aria-labelledby`, `form`, `fieldset` with a
  `legend`, `dl` for the status facts and `ol role="log"` for the activity log.
- Every control is a native element with an associated label. The markup test checks that every
  element that the interface renders into exists and that every label points at an existing control.
- The focus outline is never removed. `:focus-visible` draws a 3 px ring with an offset.
- Status is never carried by color alone. Every state has a text label and a text marker, and the
  activity log and the status strip are plain text.
- Status updates are announced with `role="status"` and `aria-live="polite"`. Errors use
  `role="alert"`.
- The tab order follows the visual order. A skip link leads to the main content.
- The approval panel is reachable and operable with the keyboard alone. The approve button is never
  the default focus target: the panel holds no form, the button is a plain button, no element of the
  markup carries `autofocus` and the interface never moves the focus to it.
- The palette is declared twice, for light and for dark, and the contrast test computes the WCAG
  contrast ratio of every text pair and of every control pair against the AA target.

## Robustness

- Reconnect uses a bounded exponential backoff with jitter: 500 ms base, factor 2, cap of 15 s and
  at most 6 attempts. After the last attempt the interface asks the user to pair again. There is no
  retry loop that can run forever.
- The heartbeat runs every 5 s. Three missed heartbeats mark the session offline.
- `PLUGIN_OFFLINE` and `BUSY` are reported with their recovery action. `BUSY` shows the
  `retry_after_ms` of the daemon.
- When the editor blocks the socket by its content security policy, the interface names the fix:
  regenerate the manifest with `figtether manifest` so that `networkAccess` allows
  `ws://localhost:38947`, then reopen the plugin.
- A frame that exceeds the 1 MiB frame cap is refused with `PAYLOAD_TOO_LARGE` and never parsed.
- A frame of an older connection epoch is discarded before its payload is read.

## Log hygiene

A log line carries the operation, the count, the duration and the error code, nothing else. The
operation name and the error code pass an allowlist, so a layer name, a text, a file id or a
credential cannot reach a log line even by accident. The write path adds the kinds of the operations
of a plan and their counts, for example `op=create_node count=2`. The plan itself, the intent of the
plan and the text of the document never reach a log line.

## Tests

```bash
cd plugins/figma/ui
npm install
npm test          # vitest run
npm run typecheck # tsc for the sources and for the tests
```

The suite runs without a browser. The DOM is reachable only through the `UiPort` interface, the
socket through the `SocketLike` interface and the clock through the `TimerApi` interface, so the
whole controller is testable with plain fakes. The same tests also run with the test command of
the plugin package (`npm test` in `plugins/figma`).

| File                      | Covers                                                                     |
| ------------------------- | -------------------------------------------------------------------------- |
| `test/envelope.test.ts`   | Channel check, JSON guard, size cap, type allowlist, payload shape.        |
| `test/secret.test.ts`     | Credential memory, redaction of known secrets and of credential shapes.    |
| `test/backoff.test.ts`    | The reconnect schedule, its cap, its jitter and its attempt limit.         |
| `test/status.test.ts`     | The connection state machine, the session phase and the fingerprint.       |
| `test/log.test.ts`        | The log format and the allowlists of operation and error code.             |
| `test/bridge.test.ts`     | The handshake, the auth window, the heartbeat, the reconnect and the caps. |
| `test/controller.test.ts` | The pair, grant, pause, resume, revoke, relay and error flows.             |
| `test/transfer.test.ts`   | The bounded reassembly and the chunk split of a capture.                   |
| `test/contrast.test.ts`   | The contrast of both palettes and the absence of external assets.          |
| `test/markup.test.ts`     | The element identifiers, the labels and the live regions of the markup.    |
| `test/hygiene.test.ts`    | No persistent store, no eval, no markup injection, loopback only.          |
| `test/write.test.ts`      | The preview, the approval, the write grant, the result and the log.        |

## Layout

```
plugins/figma/ui
  index.html          the markup of the interface
  styles.css          the single stylesheet, light and dark palette
  src
    ui.ts             the entry point of the bundle
    dom.ts            the DOM implementation of the interface port
    controller.ts     the flows, without any DOM access
    bridge.ts         the socket to the daemon
    sandbox.ts        the postMessage channel to the main sandbox
    envelope.ts       the strict envelope validation
    protocol.ts       the shared contract and the channel constants
    write.ts          the write path: the preview, the counts and the decision
    secret.ts         the credential memory and the redaction
    status.ts         the connection state machine
    log.ts            the sanitized activity log
    backoff.ts        the bounded reconnect schedule
    transfer.ts       the chunked transfer handling
    random.ts         the random identifiers
  test                the unit tests
  types               the ambient declarations of the compiler
```
