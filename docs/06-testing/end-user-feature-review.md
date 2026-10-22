# End-user failures and Figma feature coverage

Assessment: 21–22 October 2026. Scope: FigTether CLI, local daemon, MCP, Bridge UI,
and the explicitly shared native Figma test frame. No grant or Auto setting is enabled by
the test runner. User data outside the granted frame is excluded.

## Method and provenance

The requested PentAGI reference is used as a testing methodology, following the user's
clarification that it should be applied like a skill. This is not an execution of the PentAGI
service or a claim of independent autonomous-agent coverage. The reviewed upstream revision is
[`843dfeb4bfc809c09d1bc5022e567c602d8fc9dd`](https://github.com/vxcontrol/pentagi/tree/843dfeb4bfc809c09d1bc5022e567c602d8fc9dd),
specifically the [execution manager](https://github.com/vxcontrol/pentagi/blob/843dfeb4bfc809c09d1bc5022e567c602d8fc9dd/backend/pkg/templates/prompts/primary_agent.tmpl)
and [report evaluator](https://github.com/vxcontrol/pentagi/blob/843dfeb4bfc809c09d1bc5022e567c602d8fc9dd/backend/pkg/templates/prompts/reporter.tmpl).
The applied sequence is: inventory the surface, state an expected outcome, reproduce one
failure, isolate its trigger, fix it, retain regression evidence, and check the result against
the actual requirement. A refused unsupported feature is not counted as a working feature.

Tests use isolated runtime directories for malformed input, stale state, queue pressure,
authentication, replay, cancellation, and permission failures. Native Figma checks use only
the user's shared frame, exact approved plans, readback, and actual PNG exports. Pairing and
approval tokens remain in the local test process and are redacted from evidence.

## Reproduced findings

| ID       | Reproduction and consequence                                                                                                                                                                                                         | Correction                                                                                                                                                                                     | Verification                                                                                                                                   |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| UX-001   | Native `list_nodes` with names/text/geometry succeeds, but adding `fills` times out after 20 seconds. Solid paints have RGB colors; reading an assumed alpha inserts an undefined array item, which canonical serialization refuses. | Mirror native RGB typing and use opaque alpha when normalizing solid colors. Recognize native gradient paint kinds.                                                                            | Native field-isolation logs retained; RGB regression test passes. Rich native readback now succeeds on the refreshed session.                  |
| UX-002   | Open outside Figma sends a request, but the socket silently drops incoming `ui_control`. Controller-only tests missed this boundary.                                                                                                 | Relay `ui_control`; test incoming message kinds at the socket boundary. Add launch status, five-second timeout, browser-launch acknowledgement, and a private clickable fallback link.         | Native launch opened FigTether Bridge in Microsoft Edge. Browser companion authentication/state/error checks also run with an isolated daemon. |
| UX-003   | A result that cannot be serialized/sent returns `false`; the controller previously ignored it, leaving the agent waiting for a generic timeout.                                                                                      | Complete the same request with a bounded structured error, show visible recovery, pause Auto, and retain the failure in activity. A failed preview delivery cannot trigger automatic approval. | A failed delivery is asserted to emit an error for the original request and allow the next queued request.                                     |
| UX-004   | `start --plugin-dir` on an existing unrelated folder prints only an OS missing-file message. Missing-daemon commands do not explain what to run next.                                                                                | Preserve error causes, add source-package context, and emit code/message/action/retryable diagnostics. `--json` failures use JSON on stderr, leaving stdout for results and MCP.               | Real-binary failure matrix covers 22 command cases, with expected exit codes and recovery output.                                              |
| FEAT-001 | Auto-layout sizing accepts `AUTO`, while the native API accepts `FIXED`, `HUG`, and `FILL`.                                                                                                                                          | Validate the native enum before applying.                                                                                                                                                      | Native HUG parent verified; final FILL width assertion follows the additional resize-order fix.                                                |
| FEAT-002 | Linear gradient writes omit the required transform; solid alpha is placed inside native RGB and can be lost.                                                                                                                         | Supply a validated 2×3 gradient transform, support four native gradient kinds, and combine solid color alpha with paint opacity.                                                               | Paint/alpha/transform regression tests; native render fixture records actual editor results separately.                                        |
| FEAT-003 | Snapshot/watch/change-journal operations are advertised as available despite missing shipped storage/journal behavior.                                                                                                               | Advertise them as unavailable with a fresh-read/export alternative. Publish allowed write operations, node types, and properties for agent discovery.                                          | Daemon capability regression and real MCP discovery test.                                                                                      |

Additional findings from the live session on 22 October:

- **UX-005 - large export blocks later commands.** The sandbox sent a result larger
  than the UI's 1 MiB envelope limit. The UI discarded it without completing the
  active request. Subsequent native reads timed out behind that request. The runtime
  now checks the complete response envelope and returns `PAYLOAD_TOO_LARGE` with a
  smaller-scope recovery message. Cancelling a read releases the UI queue; queued
  cancelled operations are removed. A cancelled write stays serialized until its
  outcome is known. Regression tests cover oversize output, the next read, queued
  cancellation, and write serialization.
- **FEAT-004 - native inspection contract drift.** The real plugin returns a bounded
  `nodes` subtree, while the daemon expected a single `node`. The resulting
  `PROTOCOL_MISMATCH` incorrectly suggested updating compatible software. The daemon
  now normalizes the shipped subtree, preserves truncation, and accepts the legacy
  singular form. Inspection keeps the root alias for existing consumers. A regression
  uses the actual editor response shape, including a truncated subtree.
- **FEAT-005 - incorrect transform translation.** The plugin sent four matrix
  coefficients where the Rust wire contract expects translation and scale. An
  identity transform consequently appeared at x=1 with scale_x=0. The adapter now
  extracts translation from Figma's 2×3 matrix and computes each basis-vector length.
  Rotation remains the separate contract field. Native rotated/translated matrices
  are covered by a regression. The contract does not preserve affine shear.
- **UX-006 - discovery timeout suggests changing access.** When a root read failed,
  `open_design` returned `needs_access_or_target` even though the grant was ready.
  It now preserves the original structured failure when no candidate can be read.
  A synthetic editor regression injects `TIMEOUT` and checks that the tool returns
  that error instead of asking for a new grant.
- **FEAT-006 - FILL silently becomes fixed.** Canonical property ordering applied
  `layoutSizingHorizontal` before `width`. Figma's subsequent resize restored fixed
  sizing. Native render showed a 160px child inside a 600px padded parent. The apply
  path now loads required fonts first, resizes once, and then applies sizing and
  other properties. A regression emulates Figma's resize side effect. The final
  native check requires 568px width, accounting for 16px padding on both sides.

The earlier manifest filename/CSP issue is documented as SETUP-001 in
[the startup review](../bridge-startup-review.md).

## Feature boundaries

FigTether does not currently expose the entire Figma Plugin API. A successful tool handshake
does not prove full Figma feature support. The following inventory distinguishes the shipped
contract from native execution evidence.

| Feature family                 | Current exposed contract                                                                | Evidence boundary                                                                                                                                                   |
| ------------------------------ | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Canvas discovery and rendering | `open_design`, `list_sessions`, scoped PNG delivery                                     | Real native render and reads are recorded; the original rich-fill read failure is retained.                                                                         |
| Selection, tree and inspection | `get_selection`, `list_nodes`, `get_nodes`, `inspect_design`                            | Bounded native reads plus parser/scope/projection tests. Truncation is explicit.                                                                                    |
| Text                           | Create and replace text, explicit font family/style, font size                          | Native text creation, replacement, font selection and readback pass. Rich per-range formatting is not exposed.                                                      |
| Geometry and appearance        | Position, dimensions, rotation, opacity, visibility, locks, rounded corners, blend mode | Strict property allowlist; native feature fixture reads back concrete mutations.                                                                                    |
| Auto Layout                    | Direction, alignment, spacing, padding, FIXED/HUG/FILL sizing                           | Corrected enum; native fixture covers parent/child behavior. Grid layout and all child sizing options are not exposed.                                              |
| Paints                         | Solid and four gradients; alpha, stops, validated transform                             | Native RGB failure reproduced; expanded write types have regression checks and a native render fixture.                                                             |
| Primitive creation             | FRAME, TEXT, RECTANGLE, COMPONENT, VECTOR, SECTION, GROUP, LINE, ELLIPSE                | Creation support does not imply every editing feature of the node. Native parenting rules still apply.                                                              |
| Components                     | Create component nodes; inspect existing component metadata                             | Creating instances, swapping components, variants, component sets and properties are not implemented write operations.                                              |
| Vector editing                 | Create VECTOR nodes                                                                     | Vector paths/networks, boolean operations and pen editing are not exposed. An empty vector is not evidence of path editing.                                         |
| Structure changes              | Insert, move and delete through destructive plans                                       | Parser/apply/scope tests. Destructive changes require explicit user controls; Auto does not bypass them.                                                            |
| Tokens and libraries           | Read scoped variables/styles and available metadata                                     | Availability depends on the editor/permissions. Creating/binding variables, publishing styles, and remote-library writes are not exposed.                           |
| Assets                         | PNG/SVG export; inline retrieval of bounded PNGs                                        | Actual native PNG export and retrieval verified. SVG export metadata and PNG-only inline delivery must be distinguished. Image import/fill creation is not exposed. |
| History and safety             | Undo commit and optional version request on approved writes                             | Write tests cover partial outcomes and readback requirements. This is not arbitrary history editing.                                                                |
| Snapshots and change watching  | Reserved tool contracts                                                                 | Unavailable in the shipped pipeline; use fresh reads and exports. Refusal is recorded as unavailable, not success.                                                  |
| Prototyping and collaboration  | No exposed write contract                                                               | Interactions, transitions, comments, multiplayer state, pages/slides and publishing are not claimed as supported.                                                   |

## Recorded validation state

The latest plugin suite passes 540 tests across 41 files, plus type checking, lint,
and formatting. The real CLI failure matrix passes 22 cases. The synthetic MCP suite
passes 42 cases, including UX-006. The browser companion passes 23 cases and the UI
render fixture passes 10. The Rust workspace passes 710 tests with one ignored test;
the later adapter-only change also passes its 69 tests. Clippy with warnings denied,
public-file checks and documentation
audit pass. These counts describe separate suites and must not be summed as native
Figma feature coverage.

All 17 MCP tools have recorded calls against a real paired Figma session. This includes
explicit refusal of unavailable snapshot/watch/change tools and an unknown-job error;
these are negative checks, not implemented features. Native writes create FRAME, TEXT,
RECTANGLE, ELLIPSE, LINE, VECTOR and COMPONENT nodes, replace text, set properties and
fills, and exercise four gradient kinds. The vector is empty; path editing is not exposed.
Native readback verifies the text, 8-degree rotation, and a HUG parent height of 96px.
The initial QA export was 1×1 because its fixture was outside a clipping ancestor. It
does not establish visual verification. Moving the fixture within the shared root,
exporting it and then hiding it provided the actual 840×434 render evidence.

After loading FEAT-004 and UX-005, real inspection succeeds, a large SVG returns
`PAYLOAD_TOO_LARGE` in about 11.5 seconds, and the next PNG export succeeds in under
one second. The failure no longer jams the queue. The original failures remain retained.

The Studio Notes design is complete in native frame `46:2`, below the original page.
It contains 17 editable child layers. Native geometry puts every child within its
1296×850 section. The scoped design evaluator records 27 passed observations, no failed
observations, six inapplicable and eight untested; it does not establish full accessibility
conformance. Actual section and full-page PNGs were inspected. The old test rectangle no
longer covers the CTA. The native QA fixture is hidden after use.

Loading FEAT-006 resets the plugin session. Pairing has been restored; the final FILL
readback remains pending the user's restored sharing/editing/Auto. The runner cannot
restore that authority itself. No claim is made that the pending assertion already passed.

Two additional contract limits remain explicit: GROUP is in the creation allowlist, but
the Rust MCP operation shape cannot carry the sandbox's required `children_ids`; use a
FRAME container. Completing grouping needs a coordinated write-contract and destructive
policy change. An unknown job currently uses `SESSION_NOT_FOUND` with a session-oriented
default recovery hint even though its message says the job is absent. Neither condition
is counted as successful native feature support.

## Repeatable checks

```powershell
node scripts/validation/end-user-errors.mjs --binary PATH_TO_FIGTETHER
node scripts/validation/startup-smoke.mjs --binary PATH_TO_FIGTETHER
node scripts/validation/agent-integration.mjs --binary PATH_TO_FIGTETHER --out NEW_EVIDENCE_DIRECTORY
$env:FIGTETHER_TEST_BINARY = 'PATH_TO_FIGTETHER'
node scripts/validation/bridge-ux/companion.mjs
node scripts/validation/bridge-ux/render.mjs
```

`native-features.mjs` requires a real, explicit session and granted root. Its `read` phase
records supported responses and structured unavailability for reserved tools. The `features`
phase creates a scoped disposable fixture, tests native text, geometry, component/primitive
creation, gradients and Auto Layout, renders it, and hides that fixture after verification.
The `design` phase turns the previously approved test frame into editable Studio Notes below
the existing design. It stops on warnings, pending approvals, and partial writes. It never
turns on Auto, changes a grant, or clicks approval controls.

```powershell
node scripts/validation/native-features.mjs --session SESSION_ID --root FRAME_ID --phase read
node scripts/validation/native-features.mjs --session SESSION_ID --root FRAME_ID --phase features
node scripts/validation/native-features.mjs --session SESSION_ID --root FRAME_ID --phase design --fixture APPROVED_TEST_FRAME_ID
```

Local raw evidence is under ignored `artifacts/end-user-auto/`. The report preserves expected
negative results separately from product defects and infrastructure/harness errors. Full
feature coverage, every possible failure, cross-platform behavior, and model task success are
not established by this finite assessment. Native execution after a hot reload requires the
user to restore sharing/editing/Auto; a test runner cannot restore that authority itself.
