# Verification: AI agents can create and edit designs

Reviewed on 21 October 2026 against this working tree. **FigTether implements native Figma
design creation and editing, as well as reads.** Its write path is reachable through the
Rust MCP adapter and daemon; mutations execute in the Figma plugin sandbox.

## What the code implements

| Layer                | Source                                                                                     | Verified behavior                                                                                                                                                   |
| -------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| MCP tool surface     | [`tools_write.rs`](../crates/mcp/src/tools_write.rs)                                       | Registers `preview_write`, `get_write_approval`, and `apply_write`; sends typed requests to the daemon. `apply_write` is annotated as a mutating operation.         |
| Rust contracts       | [`write.rs`](../crates/model/src/write.rs)                                                 | Defines write grants, plans, operations, allowed properties, budgets, and canonical plan hashes.                                                                    |
| Daemon routing       | [`client_api.rs`](../crates/daemon/src/client_api.rs)                                      | Parses plans, requires write grants and scope, queues previews, checks approval tokens before apply, forwards mutations, and records the result in its audit trail. |
| Approval enforcement | [`write_path.rs`](../crates/daemon/src/write_path.rs)                                      | Binds approval to the plan hash, session, grant epoch, expiry, and requesting client. Rejects missing or mismatched authorization.                                  |
| Plugin execution     | [`apply.ts`](../plugins/figma/main/src/write/apply.ts)                                     | Calls native editor creation methods, appends/reparents children, sets supported properties, text and fills, moves nodes, and removes nodes.                        |
| Agent transport      | [`daemon.rs`](../crates/mcp/src/daemon.rs), [`endpoint.rs`](../crates/mcp/src/endpoint.rs) | Uses an authenticated loopback WebSocket connection to the local daemon.                                                                                            |

The supported plan operations are `create_node`, `insert_child`, `set_properties`,
`set_text`, `set_fills`, `move_node`, and `delete_node`. Creation covers `FRAME`, `TEXT`,
`RECTANGLE`, `COMPONENT`, `VECTOR`, `SECTION`, `LINE`, `ELLIPSE`, and `GROUP`, subject to
available editor APIs. A group requires children; it is not an empty-frame substitute.
Vector node creation does not imply unrestricted access to every vector property.
The property allowlist and editor capability checks constrain what a plan can do.

For example, an agent can create a landing-page design inside a granted empty frame,
add rectangles and text, set their positions and dimensions, apply supported layout and
style properties, and update the text. The result is editable native nodes, rather than
only a reference screenshot. FigTether supplies the tools; the AI model supplies the design
decisions and plan contents.

## The write flow

1. The user selects and grants a root, then enables the separate write grant in the plugin.
2. The agent sends a plan through `preview_write`.
3. Rust validates the plan; the plugin resolves it against the document without applying it.
4. The user reviews the exact preview and approves or rejects it in the plugin.
5. The agent calls `get_write_approval` to obtain the approved plan's authorization token.
6. The agent calls `apply_write` with the same plan. Rust checks authorization before dispatch.
7. The plugin applies native operations and returns created/changed node IDs and results.

An initial empty root is still required: an agent cannot grant itself arbitrary document
access. A read grant cannot authorize an edit. Undo commit reporting exists; manual undo
restoration and version checkpoints remain unverified in the desktop evaluation.

## Evidence and limits

The [desktop evaluation](../benchmarks/published/desktop-2026-10-20.md) created a website with
71 editable children under one granted frame, giving a 72-node scene. It exercised actual
preview controls, user approval, writes, text readback, and native PNG export. It also retained
failures and artifact hashes. This is real-editor evidence for that limited workflow.

The Rust [daemon tests](../crates/daemon/tests/write_path.rs) cover successful approved apply,
refusal without a write grant or token, scope checks, preview metadata, session modes, and
audit access. The [end-to-end tests](../crates/e2e/tests/write_path.rs) additionally cover
unchanged documents during preview, wrong-plan tokens, changed grants, budget and destructive
policy refusals, and isolation between sessions and clients. These automated tests use a
simulated editor; their passing result does not establish every real-editor capability.

Re-run the focused Rust suites:

```sh
cargo test --locked -p figtether-daemon -p figtether-e2e --test write_path
```

The [21 October verification record](../benchmarks/published/readme-ui-2026-10-21.json)
retains command durations, 20 passing Rust write-path tests, 467 passing plugin tests,
the initial stale-copy assertion failure, type checks, artifact hashes, native UI evidence,
and unmeasured cost status. It is separate from the earlier desktop latency benchmark.

The source build remains experimental. `get_selection` and `inspect_design` still failed in
the final desktop controls. The historical **Proposed** label in `ADR-013` records the
original release decision; it does not mean the write code or observed design creation is
absent. This verification does not change the ADR's formal release approval status.

## Local MCP, official MCP, and cost

The normal agent integration **does use MCP: FigTether's own local stdio server**. It then
speaks the project's local daemon protocol over WebSocket. The native read/write path uses
the Figma Plugin API, without calling Figma's official MCP service or REST API. It does not
need a Figma personal access token or credentials for the official MCP service.

Consequently, this path does not consume official Figma MCP tool calls. Calling the workflow
“without MCP” would obscure the local server it actually uses. Figma editor access and plugin
permissions still apply. AI model/subscription costs remain separate; the recorded session
did not expose model billing or token telemetry, so cost is **not measured**, not zero.
