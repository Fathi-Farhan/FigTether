# FigTether Figma plugin: main sandbox

The main sandbox is the half of the Figma plugin that talks to the document. It runs inside the
Figma plugin runtime, it has the Plugin API and no network, and it answers the read operations
that the daemon asks for.

The split is deliberate (PRD 8.4, `docs/02-architecture/04-plugin-design.md` section 2):

| Concern   | Main sandbox (`main/`)                       | UI iframe (`ui/`)                                   |
| --------- | -------------------------------------------- | --------------------------------------------------- |
| Network   | never opens a socket                         | owns the WebSocket to `ws://127.0.0.1:38947/bridge` |
| Document  | reads the document, never writes to it       | never touches the document                          |
| Secrets   | never sees a pairing secret or a credential  | keeps both in memory only                           |
| Transport | typed `postMessage` channel to the UI iframe | relays the bridge frames over that channel          |

## Rules the sandbox keeps

- **Read only.** No handler creates, modifies, deletes, moves or resizes anything. No handler
  changes the current selection or the current page as a side effect of a read (FR24, AT18).
- **Grant enforced on this side too.** Every node that is read has to be inside the granted roots.
  A node outside the grant is refused with `OUT_OF_SCOPE` before its content is read (FR03).
- **Honest coverage.** A field the platform did not expose is reported in `unavailable_fields` with
  a reason, never as an empty value. An unknown node type keeps its verbatim type string.
- **Bounded work.** At most 10,000 visited nodes per operation, at most 50 node ids per batch, at
  most depth 32, at most 16 megapixels per export, scale between 0.25 and 2.

## Layout

```text
plugins/figma/main/
  src/
    main.ts          entry point, wires the API, the transport and the runtime
    runtime.ts       grant state, request dispatch, the answers to the UI
    bridge.ts        the postMessage envelope: validation, counters, chunking
    transport.ts     the Figma UI channel and the window channel
    handlers.ts      the operation handlers and the registry
    traversal.ts     the sliced traversal engine
    scope.ts         grant membership, the scope guard
    normalize.ts     Figma node to plugin node
    tokens.ts        variable and style resolution
    capabilities.ts  the runtime capability probes
    projection.ts    field projection
    cursor.ts        the pagination cursors
    canonical.ts     canonical JSON and the content hash
    encoding.ts      UTF-8 and base64 helpers
    bytes.ts         the byte and pixel budgets
    errors.ts        the error taxonomy and the error payload
    types.ts         re-exports of the shared contract plus the availability vocabulary
    figma-api.ts     the injectable view of the Plugin API
  test/              the unit tests, one file per topic
```

The wire contract itself is not duplicated here. `src/types.ts` re-exports `@figtether/shared`,
which mirrors `crates/protocol`. The shared package owns the frame types, the plugin payload types
and the canonical encoding helpers, so the daemon, the UI and the sandbox cannot drift apart.

## Operations

Every operation is registered in the handler registry of `src/handlers.ts`. The operation names
are the wire names of `crates/protocol/src/lib.rs`.

| Operation           | Parameters                                                         | Result                                                                                                                                          |
| ------------------- | ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `list_nodes`        | `root_id`, `cursor?`, `filters?` (`types`, `max_depth`), `fields?` | the nodes under the root, `next_cursor`, `truncated`, `coverage`                                                                                |
| `read_nodes`        | `node_ids` (max 50), `depth?` (max 32), `fields?`                  | `batch_status`, one item per requested id with a per item error, the normalized nodes                                                           |
| `inspect_design`    | `root_id`, `detail?` (`summary`, `layout`, `full`), `depth?`       | the implementation context: geometry, layout, typography with runs, paints, effects, component and token references, plus the unsupported types |
| `get_design_tokens` | `root_id`, `requested_kinds?`                                      | the variables and styles in scope, the collections and modes, and the references that did not resolve                                           |
| `export_assets`     | `node_ids`, `format` (`png`, `svg`), `scale?`                      | `batch_status` and one item per node with the exported bytes, the MIME type and the content hash                                                |
| `get_selection`     | none                                                               | the selected nodes that are inside the grant                                                                                                    |
| `capture_snapshot`  | none                                                               | the normalized document, the content hash, the capture window and `consistency: best_effort`                                                    |
| `list_sessions`     | none                                                               | the sessions this plugin runtime serves                                                                                                         |
| `watch_scope`       | `root_id`                                                          | `CAPABILITY_UNAVAILABLE`, the watch is a phase P1 capability                                                                                    |
| `get_changes`       | `watch_id`, `cursor?`                                              | `CAPABILITY_UNAVAILABLE`, the change journal is a phase P1 capability                                                                           |

The two phase P1 operations answer with the honest capability error. They never fake a watch id or
an empty event list.

## The channel to the UI iframe

The envelope of the channel is the one the UI parses:

```jsonc
{
  "channel": "figtether.bridge",
  "kind": "frame", // the discriminator, `type` carries the same value
  "request_id": "req_01",
  "payload": {},
}
```

The channel marker, the envelope and the vocabulary of the answers live in the shared package, in
`plugins/figma/shared/src/bridge.ts`. The sandbox and the user interface import them from there,
so the two sides of the channel cannot drift apart.

### Inbound kinds the sandbox accepts

| Kind                | Payload                            | Effect                                               |
| ------------------- | ---------------------------------- | ---------------------------------------------------- |
| `bridge_frame`      | a `BridgeFrame` of the daemon      | dispatched by `message_type`                         |
| `frame`             | a `BridgeFrame` of the daemon      | the same, under the name of the protocol view        |
| `grant_update`      | `{ root_ids, grant_epoch, mode? }` | installs the grant and recomputes the capabilities   |
| `grant_request`     | `{ root_ids }`                     | verifies the roots and answers with `grant_verified` |
| `cancel`            | `{ request_id }`                   | cancels the request at the next slice boundary       |
| `revoke`            | `{ reason }`                       | drops the grant and cancels the running requests     |
| `capabilities`      | `{}`                               | answers with the capability report                   |
| `selection_request` | `{}`                               | answers with `selection`                             |
| `status_request`    | `{}`                               | answers with `status`                                |
| `capture_request`   | `{}`                               | builds the capture and streams it as `capture_chunk` |
| `ui_ready`          | `{}`                               | answers with the status                              |

Everything else is rejected and counted. A rejected message never reaches a handler.

### Outbound kinds the sandbox sends

| Kind             | Payload                                                             |
| ---------------- | ------------------------------------------------------------------- |
| `result`         | `{ request_id, ok: true, data }`                                    |
| `error`          | `{ code, message, retryable, action, request_id, retry_after_ms? }` |
| `progress`       | `{ request_id, visited, elapsed_ms }`                               |
| `chunk`          | the chunk contract, used when a payload does not fit into one frame |
| `capabilities`   | `{ capabilities, notes }`                                           |
| `selection`      | `{ roots: [{ node_id, node_type, child_count }] }`                  |
| `grant_verified` | `{ ok, root_ids, error? }`                                          |
| `status`         | `{ queue_depth, last_read_at, mode, capabilities }`                 |
| `capture_chunk`  | `{ transfer_id, sequence, bytes, total_bytes, final, checksum }`    |

The first six kinds are the kinds of the daemon contract, so the user interface forwards every
answer to the daemon without a rename. The last three are local affordances of the grant flow and
of the capture flow, they never travel to the daemon. The list lives in the shared package, in
`plugins/figma/shared/src/bridge.ts`, and the sandbox derives it from there.

### The Figma channel

The sandbox listens on `figma.ui.onmessage` and posts with `figma.ui.postMessage`. The runtime
delivers the payload of a `pluginMessage` envelope, and the sandbox accepts both shapes: a message
that carries a `pluginMessage` member is unwrapped, a message that does not is taken as it is. The
window channel is registered as well, so a UI that uses `window.parent.postMessage` is served too.

The user interface has to unwrap `pluginMessage` on its side: `figma.ui.postMessage` arrives in
the iframe as `event.data.pluginMessage`, and the envelope of the channel is the member, not the
whole event data.

## Grant enforcement

The daemon checks the grant epoch and the node membership. The sandbox checks both again:

1. A request frame carries a `grant_epoch`. A frame that was issued under an older epoch is
   refused with `OUT_OF_SCOPE`.
2. Every node is resolved by id and its ancestry is compared with the granted roots. The ancestry
   is read as identity only, so the content of a node outside the grant is never touched.
3. The traversal re-checks the membership at every slice boundary, so a grant that shrinks in the
   middle of a walk stops the walk instead of leaking.

A grant update with the mode `paused` stops the reads with `PAUSED` until the user resumes.

## The traversal engine

The walk works in slices of about eight milliseconds and yields to the editor between the slices
with `await new Promise((resolve) => setTimeout(resolve, 0))`. Every slice boundary emits a
`progress` message and re-checks the cancellation and the grant.

- The budget is 10,000 visited nodes. A walk that hits the budget returns a partial result with a
  cursor and `truncated: true`. It never truncates silently.
- The cursor is an opaque token that carries the grant epoch, so a cursor of an older grant is
  refused with `CURSOR_EXPIRED`.
- A cancelled walk reports the partial coverage it reached.

## Availability annotations

`unavailable_fields` is a list of strings, exactly like the Rust contract. The path of the field
and the reason travel together, separated by a colon:

```text
geometry.bounding_box:capability_unavailable
text.runs[0].font_family:capability_unavailable
fills:projection_omitted
```

The reasons are the closed vocabulary of `docs/04-data-contracts/04-normalized-model.md` section 4:
`capability_unavailable`, `projection_omitted`, `not_applicable`, `visual_gap`, `not_in_grant`.

## Capability probes

The probes run at startup and again after every grant update. They never infer a capability from
an account name, a seat label or a plan.

| Capability            | How it is decided                                                   |
| --------------------- | ------------------------------------------------------------------- |
| `document_access`     | the document root and the current page are reachable                |
| `variables`           | the variables API of the runtime exposes the local variables        |
| `component_libraries` | the team library API is present                                     |
| `local_styles`        | at least one of the local style functions exists                    |
| `font_listing`        | the font listing API exists                                         |
| `export_png`          | a probe export of a node of the current page succeeds               |
| `export_svg`          | a probe export of a node of the current page succeeds               |
| `image_fills`         | the API exposes the image paint type, the probe reports what it saw |

An unavailable capability is a valid answer, never an error. A request that asks only for an
unavailable capability is answered with `CAPABILITY_UNAVAILABLE`.

## Decisions that the contract leaves open

- **Transforms.** The Figma transform is a 2x3 matrix. The wire format carries four numbers, so
  the plugin sends the linear part `[a, b, c, d]` and the translation travels in the bounding box.
  The rotation has its own field.
- **Sizing.** The Plugin API exposes `FIXED` and `AUTO` per axis, not `HUG` and `FILL`. The
  plugin reports `HUG` when one of the axes is automatic and `FIXED` otherwise. It never claims
  `FILL`, because that depends on the parent and cannot be proven from the node.
- **Automatic line height.** The API does not expose the value of an automatic line height. The
  plugin reports 1.2 times the font size, which is the editor default for single spacing.
- **Font weight.** The API exposes the style name, never a numeric weight. The plugin maps the
  well known style names to a weight and uses 400 for a name it does not know.
- **Font availability.** The plugin compares the runs of a text node with the font list of the
  editor and marks a run that cannot be rendered.
- **Styles and variables of a node.** The Rust contract has no field for the property a variable
  is bound to, so the plugin reports the unique references of a node without the binding property.

## Tests

The tests live in `test/` and run with Vitest, without an editor. The Plugin API sits behind the
injectable interface of `src/figma-api.ts`, and `test/fake-figma.ts` builds a small document tree
for the tests.

```sh
cd plugins/figma
npx vitest run main
```

The suite covers the scope membership, the pagination cursor, the field projection, the byte and
pixel budgets, the canonical hash, the validation of the message envelope, the sliced traversal
and the dispatch of the runtime.

## Boundaries

- The sandbox never opens a socket and never stores a credential.
- The sandbox never writes to the document and never changes the selection or the current page.
- The sandbox answers only the allowlisted operations. There is no `eval` and no dynamic code
  anywhere in the chain.
