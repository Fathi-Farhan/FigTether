# End User Installation Guide

Status: current experimental source installation, reviewed 22 October 2026.

Use the [README tutorial](../../README.md#build-and-connect) for prerequisites and the
[quickstart](../quickstart.md) for executable paths and detailed recovery. Public binary
archives and a Community plugin are not released distribution methods. The
[release process](04-release-process.md) describes planned distribution gates.

## Install and connect

1. Install the Rust CLI and plugin dependencies as described in the README. Keep the plugin
   source folder in place. The executable alone does not embed or install the plugin.
2. Create a development plugin in Figma Design and copy its Figma-generated id. Keep its
   starter files separate from this checkout.
3. From the source checkout, prepare the matching bundle and start the bridge:

```sh
figtether start --plugin-id YOUR_PLUGIN_ID
```

4. Import the printed **plugins/figma/dist/manifest.json** through Figma's development menu
   and run the imported FigTether plugin. Figma requires the basename manifest.json.
   manifest.local.json is generated build input, not the import target.
5. Paste the one-use secret into the Bridge. It expires after five minutes. Keep it local;
   it never belongs in prompts or MCP configuration. For a replacement, run figtether pair.
6. Select the intended frame, choose **Update selection**, then **Share selected frames**.
   Choose **Allow editing** for native design changes. Later selection changes do not expand access.
7. Configure the chosen agent with the [agent setup tutorial](../agent-setup.md), refresh its
   MCP entry and call open_design for the intended shared frame.

For later starts, run figtether start. It preserves the manifest id and port, reuses a compatible
running daemon or starts one, and prints a fresh secret in that terminal. Bare figtether and
figtether daemon start only the daemon; pairing separately is the manual alternative.

## Port and manifest contract

The imported manifest fixes Figma's permitted development endpoint, and the build injects
that same port. Port 38947 is a fallback when no agreement exists. The daemon never silently
moves a manifest-named port. On collision or a Windows reserved range, run doctor and follow
its instructions. To explicitly choose another port during source startup:

```sh
figtether start --port 39147
```

When the daemon is already active, finish its work and stop it before changing or rebuilding
the contract. Reimport the rebuilt dist/manifest.json, reopen the plugin, pair and restore access.
The lower-level figtether manifest --port auto preserves a verified running agreement or a
bindable existing manifest. Generating a manifest alone does not rebuild its JavaScript bundle.

The CLI generates BOM-free JSON. Do not hand-edit manifest encodings. For parse errors, stale
CSP, or a plugin stuck connecting, use the [startup review](../bridge-startup-review.md).

## Editing and session controls

Manual review is the default. An agent previews a plan, obtains its exact approval, applies it
and verifies the changed nodes and PNG. The user may enable **Auto mode** for eligible new
warning-free plans. Agents must not enable it or approve for the user. Warnings, partial writes
and errors stop automation. Reloads, reconnects and access changes can turn Auto off.

The [Bridge guide](../bridge-ui-guide.md) explains Pause, Resume, editing, approval, the browser
companion and Stop sharing. Snapshot storage/capture and watch/change-journal workflows are
unavailable in the shipped pipeline. See the [feature matrix](../06-testing/end-user-feature-review.md).

To stop the daemon gracefully:

```sh
figtether stop
```

It cancels queued work, revokes credentials, closes sockets, removes the runtime descriptor
and clears retained design memory. A new session requires pairing and a user grant again.
Revoke stops future delivery; it cannot recall content already received by an agent.

## Diagnose and upgrade

Run figtether doctor and figtether status. For machine-readable failures, commands that
support --json emit structured code/message/action/retryable diagnostics on stderr; successful
results use stdout. Follow the recovery action and inspect the canvas before repeating a write.
The [support guide](../../SUPPORT.md) covers common errors and safe bug reports.

Before upgrading, finish shared work. Update the CLI and matching plugin together, refresh the
host's managed workflow with setup --client YOUR_HOST --apply, then reload and pair as needed.
Keep a previous matching source/binary build for rollback. Version/contract checks do not prove
all editor or host combinations work.

## Privacy and support boundaries

The bridge runs locally; Figma and the AI client/model have their own network paths. Content
read by the agent may be forwarded to its model provider according to that client's settings.
Do not publish pairing secrets, companion URLs, runtime descriptors, exported designs or raw
logs. Use a public synthetic fixture for reports. Windows has limited native evidence; macOS,
other seats, extended soak tests, manual undo restoration and version checkpoints remain
unverified. This guide does not declare a stable release or a complete Figma API implementation.
