---
name: figtether-design
description: Create or revise editable Figma designs through FigTether MCP. Start with the shared canvas and render. Use source code for implementation or bridge debugging when needed.
---

# Design through FigTether

FigTether reads and edits the native Figma frames the user shares. Start design work with the brief and the canvas; repository exploration is not a prerequisite. Discover the shipped tool contract rather than assuming access to every Figma feature.

1. Call open_design with the user's frame_name or known session_id/frame_id. Resolve returned choices with the user instead of choosing another agent's frame. If tools are missing, connect or refresh the host's FigTether MCP entry; pairing the plugin alone does not register a host.

2. Inspect the actual PNG and bounded tree. For missing access, use get_bridge_status and list_sessions to discover granted_root_ids, grant_mode, capabilities and write budgets. Read a small list_nodes subtree and explicit get_nodes fields as needed. Use returned IDs, including created IDs; never invent IDs or plugin-only aliases. Truncation and unavailable_fields are limits on the evidence, not complete coverage.

3. Follow the user's audience, content, and visual brief. Do not insert bridge specifications, benchmark tables, or audit findings into the design unless requested.

4. Compose a coherent preview_write plan within the reported budgets and property allowlist. create_node accepts initial fills; TEXT accepts text plus font_family and font_style together. Use FIXED/HUG/FILL for Auto Layout sizing. Supported paint writes include solid and four gradient kinds. Use FRAME containers instead of GROUP: the current MCP shape cannot carry grouping child IDs. SECTION has native parent restrictions; an empty VECTOR does not provide path editing. Instances, variants, prototypes, image import, and variable/style writes are not exposed. Do not substitute arbitrary code execution for a missing operation.

5. Check get_write_approval for the exact preview. A user-enabled Auto mode may approve an eligible new warning-free plan; do not ask again when that approval is already returned. Otherwise request review in the Bridge once and continue independent work while pending. Never enable Auto, approve for the user, or expand a grant. Apply only the unchanged approved plan with its token. Warnings, destructive actions, changed grants, expired approvals and partial writes require the corresponding review; never retry an uncertain write blindly. Keep secrets and approval tokens out of chat, logs and evidence.

6. Read back changed nodes and inspect a fresh PNG. Verify consequential properties such as text, dimensions and Auto Layout child sizing; an applied receipt alone does not prove the requested appearance. export_assets returns metadata and get_asset delivers bounded PNG image blocks within the active grant. Inline SVG bytes are unavailable. For PAYLOAD_TOO_LARGE, narrow the section, fields or scale. A tiny or blank image can result from clipping or hidden ancestors; inspect the affected scope before claiming visual verification.

7. Use error code, action and retryability to recover. A discovery timeout does not establish missing sharing. Retry a smaller read at most once when appropriate; do not repeatedly request unavailable capabilities. Snapshot storage, watch_scope and get_changes are unavailable in the shipped pipeline; use fresh reads and PNG export. Request missing sharing/editing or plan review once, explaining the concrete Bridge step, rather than repeating permission questions for already authorized work.

8. Keep shared sessions running for other agents. Changing grants invalidates approvals. Rebuilds, plugin reloads and daemon restarts can clear pairing, sharing and Auto; finish active design work before maintenance and explain which user controls must be restored. For source setup use figtether start; the canonical Figma import is the generated dist/manifest.json, not manifest.local.json. The manifest fixes the plugin port and the bundle must match it. Use figtether doctor for observed setup failures. Required host instructions, user briefs and implementation files remain valid inputs.

Report the edited frame, verified behavior and unfinished work. Distinguish native Figma execution from synthetic tests, expected capability refusals and untested features. Keep test findings out of the requested design. Record timings, failures, versions and available token/cost data separately; missing cost is unmeasured. Static checks do not establish design quality or full Figma feature support.
