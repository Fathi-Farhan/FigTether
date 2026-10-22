# Design directly in Figma

FigTether connects an agent's MCP tools to the editable frames you share in Figma. For normal
design work, the agent starts with your brief and the canvas. Reading the FigTether repository,
research collection, or benchmark history is not a setup requirement.

## Connect once, then work on the shared canvas

Build and connect using the [quickstart](quickstart.md). Keep the local daemon and Bridge plugin
open. Pairing establishes the connection; sharing a frame establishes its scope. Enable
**Allow editing** to let the agent propose designs inside those frames.

The agent then follows this loop:

1. Call `open_design` with the intended `frame_name` or known session/frame IDs. It returns
   choices for ambiguous targets, or a bounded tree, PNG image, editing access, and budgets.
   Use `get_bridge_status` and `list_sessions` to diagnose missing pairing/sharing.
2. Inspect the returned image and brief. Read a bounded subtree with `list_nodes` and request
   specific `get_nodes` fields when needed. `export_assets` returns metadata; `get_asset`
   delivers its PNG under the current grant. Inline PNGs are capped at 1 MiB with a 2 MiB
   encoded-response budget by default; asset access expires after five minutes. For a large
   design, export smaller sections or lower the scale.
3. Compose a coherent edit and call `preview_write`. You see its exact proposal in the plugin.
4. Manual review is the default. The agent checks `get_write_approval`; a user-enabled Auto
   mode may already approve a new warning-free plan without deletions. Otherwise approve
   or reject it yourself. The agent calls `apply_write` only with the unchanged approved plan.
5. Read back the changes and inspect a new render. Check the result against the website brief.

Discover capabilities and the write allowlist before planning an unfamiliar operation.
Use `FIXED`, `HUG` and `FILL` for Auto Layout sizing and check resulting parent/child dimensions.
The exposed creation allowlist includes GROUP, but the current MCP shape cannot supply its
required child IDs; use FRAME containers. Vector node creation does not expose vector paths.
Instances/variants, prototyping, image import and variable/style writes are not available.
Snapshot storage, watch and change-journal tools return explicit unavailability. See the
[feature and error review](06-testing/end-user-feature-review.md) for the evidence boundary.

Your current selection never silently expands a grant. A read-only session cannot edit. Each
plan still needs exact-plan approval, obtained manually or through your explicit Auto-mode
choice. Agents must not enable Auto mode or drive companion controls to approve for you.
A successful static check does not establish that a
website communicates clearly or looks good.

## What the agent receives automatically

The local MCP server supplies canvas-first workflow instructions in its initialization response.
They explain discovery, bounded reads, inline node styling, native approval, and visual verification.
An agent does not need to open this document to receive that guidance. Whether it follows the
instructions depends on the MCP host and model; the bridge cannot guarantee design quality.

The repository also includes a focused [FigTether Design skill](../.agents/skills/figtether-design/SKILL.md).
Clients that discover project skills can load it for Figma design tasks. Other clients need their
own skill installation or instruction mechanism. `figtether setup` installs the workflow for the
selected supported host and workspace; see the [setup tutorial](agent-setup.md). Connecting MCP does **not** automatically install
the full skill into every client. The optional [Design Evidence skill](design-review.md) supports
measured review; it does not replace the design brief or visual inspection.

The [cross-host integration analysis](audits/2026-10-21-agent-integration.md) distinguishes
Codex, Gemini CLI, Claude Code, and other clients. It records current configuration and image
delivery gaps before this implementation. Current setup, `open_design`, and `get_asset` are
described above and in the setup tutorial. The earlier analysis retains its original evidence.

## Create a styled node in one operation

`create_node` accepts initial `fills`. A `TEXT` node can also include `text`, `font_family`,
and `font_style`. Supply both font fields together; omitted text fonts use Inter Regular.
The plugin loads the font before creating the text node, then applies initial styling.

This example is the `plan` value inside a `preview_write` call. Replace the session and parent
with IDs returned by your connection; the example does not grant access or approve a write.

```json
{
  "plan_id": "website-heading-1",
  "session_id": "SESSION_ID_FROM_LIST_SESSIONS",
  "intent": "Add the website headline inside the shared frame",
  "destructive": false,
  "operations": [
    {
      "kind": "create_node",
      "parent_id": "GRANTED_FRAME_ID",
      "node_type": "TEXT",
      "properties": {
        "name": "Hero headline",
        "x": 64,
        "y": 120,
        "fontSize": 48
      },
      "text": "A clearer place to start.",
      "font_family": "Inter",
      "font_style": "Bold",
      "fills": [{ "type": "SOLID", "color": "#182A24" }]
    }
  ]
}
```

This is one operation. Its four ordinary properties, fills, text and two font fields count as
eight property changes. Its text counts toward the Unicode character budget. The default
limits remain 50 operations, 100 created nodes, 200 property changes, and 20,000 text characters
per plan, plus 10 applied plans per minute per session. Combining fields does not bypass budgets.

A complete successful apply receipt lets the daemon learn the new IDs under their already-known
parents. The next edit can use those IDs without an extra traversal. Partial or inconsistent
receipts are not trusted for automatic learning; inspect the document before continuing. Use
real IDs returned by the bridge, not invented IDs or plugin-only aliases.

## Recover a connection

When two agents use the same local installation and plugin, they share its grant. Use separate
working frames and share the intended roots together. Changing the grant invalidates pending
approvals. This coordination does not enforce per-agent isolation: both authorized local clients
can access the shared roots. Closing one MCP connection does not require revocation.

An agent should not run `figtether revoke` or stop the daemon as routine cleanup while others
are working. Revocation ends shared access for every client using that session. A new pairing
starts a new session and waits for you to grant frames again; it never restores the revoked grant.
After a local or daemon-triggered revoke, the Pair action resets the plugin transport before
authenticating the fresh secret, so the sharing buttons work again. Old socket callbacks are
discarded and daemon revocation also clears the sandbox's scope and pending work.

After three missed heartbeats, the daemon marks the session offline and closes the stale socket.
The UI can reconnect with its rotating, in-memory session credential. Authentication advances
the connection epoch; the same plugin session resends only its previously shared roots and
read/write mode. A paused session stays paused. Pending previews and approvals are discarded.

A new pairing creates a new session that needs a new user grant. Revocation invalidates reconnect
credentials. Restarting the daemon or closing the plugin loses its in-memory credentials, so
pair again. The UI uses bounded retries; check the daemon and use a new pairing secret if recovery
fails. Credential-based recovery is covered by simulated-plugin socket tests; the updated build
still needs a fresh Figma Desktop validation after reload.

A plugin build can trigger a Figma hot reload. Finish active work before rebuilding/restarting;
pairing, sharing, editing and Auto may need deliberate restoration by the user. Do not treat a
timeout as proof of missing access. Follow its error/action and try one smaller read when
appropriate. An uncertain or partial write requires inspection before another plan.

For local setup automation, `figtether pair --json` prints only `secret` and
`expires_in_seconds`. Keep this output local, enter the secret in the Bridge UI, and never put
it in chat, model prompts, MCP configuration, or benchmark artifacts.

## Measure the workflow separately from the design

Retain versions, source and artifact hashes, request timings, failures, sample sizes, and what
was actually verified. Keep technical audit tables and benchmark results out of the website
unless they belong to its requested content. Report unavailable model tokens and billing as
unmeasured, not zero. See the [bridge integration record](../benchmarks/published/bridge-integration-2026-10-21.md)
for the current verification and desktop limitations.

For a repeatable local connection check after a build:

```sh
cargo build --locked -p figtether-cli
npm run bridge:smoke -- --binary target/debug/figtether
```

On Windows select `target/debug/figtether.exe`; use your actual binary path if Cargo has a
custom target directory. This check starts an isolated daemon on a free loopback port,
validates local pairing JSON, authenticates a simulated plugin, and checks MCP instructions,
tool discovery, and granted-root discovery. It never opens or edits Figma. Each run saves
sanitized responses and timings under a new ignored results directory. Do not publish its
runtime subdirectory, which contains local credentials.
