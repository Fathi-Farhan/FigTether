# End User Installation Guide

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

> **Planned distribution guide.** There are no public binaries or Community plugin releases yet.
> For installation and first-use commands that work with the current source checkout, use the
> [README tutorial](../../README.md#build-and-connect) or [quickstart](../quickstart.md).
> Cargo can install a locally built `figtether.exe`; npm builds the Figma plugin. There is no
> published FigTether CLI package for npm, npx, or pnpm.
> Archive names and release instructions below describe a future distribution, not a download available today.

This document is the end-user quickstart for FigTether v0.1, following PRD section 5.1 step by
step, then the post-install controls, an error cheat sheet, upgrade and rollback pointers, and the
privacy wording. The release sequence below remains a historical proposal. The CLI now exists
and has limited desktop evidence, but public distribution remains planned for `G2` (FR01).
This document is the proposed release quickstart deliverable of the v0.1 Definition of Done
(PRD 8.10.2); it is not the current source-install tutorial.

## 1. Quickstart (PRD 5.1, exactly eight steps)

### Step 1: download binary, plugin bundle, and checksum

Take the archive for your OS and architecture plus the plugin bundle and its checksum file from the
release page. Planned targets per PRD 8.11: Windows x64, macOS arm64 and x64 where runner and editor
testing exist; Linux receives core CLI and fixtures only until an editor environment is tested and
supported (see `docs/07-ops/04-release-process.md`).

### Step 2: verify the checksum

```powershell
certutil -hashfile .\figtether-v0.1.0-windows-x64.zip SHA256
```

Compare the output with the `.sha256.txt` published next to the archive. On macOS you can verify
directly:

```sh
shasum -a 256 --check figtether-v0.1.0-macos.zip.sha256.txt
```

Checksums are the floor of the supply chain story (PRD 8.11); signing is added per platform
availability and does not replace the published checksum. Archive names are illustrative until the
naming convention is fixed at `G2`.

### Step 3: run the doctor

```sh
figtether doctor
```

Doctor checks versions, the runtime directory and its ACL, the expected port, the manifest, and
pairing state. It does **not** request a Figma PAT (FR01/FR02 path needs no token, `AT02`; there is
no REST client in the product). Output is sanitized.

### Step 4: start the daemon

```sh
figtether daemon
```

Default port `38947`, project-reserved, loopback only. If the port is taken, the daemon **fails
with a hint to pass `--port` and to regenerate the manifest**; it never silently changes the port.

```sh
figtether daemon --port 38948
```

After any port override, regenerate the manifest (Step 5) so the plugin targets the same port.

On Windows the port can also be unusable without any process on it: the operating system reserves
port ranges for the hypervisor (Hyper-V, WSL, Docker Desktop) and re-reserves them at every boot.
The daemon then reports error 10013. The doctor names the reserved range and the free port to use:

```sh
figtether doctor
```

Let the CLI pick that port and write it into the manifest (Step 5):

```sh
figtether manifest generate --plugin-id "<your-plugin-id>" --port auto
```

The command prints the matching `figtether daemon --port <port>` command, because the plugin
manifest and the daemon have to agree on the port. Do not delete Windows port reservations or
disable a firewall to work around the error.

### Step 5: generate or import the development manifest

```sh
figtether manifest generate --plugin-id "<your-plugin-id>"
```

Use your own plugin id. The release bundle already contains the prebuilt UI and adapter binary; you
point a development manifest at them. On a port change, rerun this command with the new
`--port`, or with `--port auto` to let the CLI pick a free port.

### Step 6: open the plugin and pair

Open the plugin in Figma Design on an eligible seat. Enter the temporary pairing secret displayed
by the local CLI pairing action (proposed shape `figtether pair`); the secret is single use, 5
minute TTL, 256 bits of randomness, never logged, never in a URL, never shown to the agent. The
plugin confirms the session name and the fingerprint; check the fingerprint before continuing.

### Step 7: share a root and allow the read session

Select the root to share and press **Allow read session**. The grant binds root ids. Later
selection changes never widen the grant; widening scope is always an explicit user action on a new
grant version (PRD 5.3).

### Step 8: connect your MCP client

Print the generic client configuration and paste the executable plus `mcp --stdio` entry into your
MCP client. The CLI never writes another application's configuration silently (PRD 5.1 step 7).

```sh
figtether config print --client generic
```

The agent side of the loop: it calls `list_sessions` first, then reads a summary before requesting
detail (`get_selection`, then `inspect_design` or `get_nodes` with field projection). You keep the
edge controls of the next section.

## 2. Post-install controls

| Control          | Where                                                                                         | Effect                                                                                                                                           |
| ---------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| Pause            | Plugin UI button (PRD 5.3)                                                                    | Gates new live and snapshot reads until Resume; in-flight results already produced are not retroactively deleted                                 |
| Resume           | Plugin UI button                                                                              | Restores the session to READY only after the state machine revalidates pairing and grant                                                         |
| Capture snapshot | Plugin UI button                                                                              | Immutable capture with a stable `snapshot_id` and content hash; labeled `snapshot`, never presented as live; readable until TTL or user deletion |
| Revoke           | Plugin UI button, or CLI via proposed `figtether revoke` even when the plugin is unresponsive | Cancels queued jobs, drops unsent responses, invalidates cache, cursors, and asset URIs, and makes further reads return REVOKED                  |

Revoke stops future sends but cannot recall data an agent already received; the UI says this
plainly, per PRD 5.3. Snapshot and live data descriptions follow the three mode definitions of PRD
5.2.

## 3. Error cheat sheet

Top ten error families from the PRD 8.7 table, mapped to user actions. Agent-facing code fields are
specified in `docs/04-data-contracts/05-error-catalog.md`.

| Code                                    | What it means                                        | User action                                                                                   |
| --------------------------------------- | ---------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `AUTH_REQUIRED` / `AUTH_EXPIRED`        | Credential missing or expired                        | Re-pair from the local CLI; the agent must not search for secrets                             |
| `PERMISSION_DENIED` / `OUT_OF_SCOPE`    | Platform permission or grant does not cover the node | Fix platform permission or make a wider grant yourself; the agent cannot widen scope          |
| `PLUGIN_OFFLINE` / `SESSION_NOT_FOUND`  | Plugin closed or session stale                       | Reopen and pair the plugin; the agent re-runs `list_sessions`; no silent fallback             |
| `PAUSED` / `REVOKED`                    | Your deliberate decision                             | Press Resume, or issue a new grant yourself                                                   |
| `NODE_NOT_FOUND`                        | Node deleted or id wrong within the session          | Refresh the tree; batch requests report per-item status                                       |
| `CAPABILITY_UNAVAILABLE`                | API, editor, or seat does not support it             | Read the reported reason and available alternatives; probe results are final for that session |
| `PAYLOAD_TOO_LARGE` / `SCOPE_TOO_LARGE` | Resource limit exceeded                              | Narrow root, fields, or depth; paginate with a cursor                                         |
| `BUSY`                                  | Queue full                                           | Retry after `retry_after_ms`; no tight loops                                                  |
| `TIMEOUT` / `CANCELLED`                 | Slow or cancelled operation                          | Late results are discarded; issue a fresh request if still needed                             |
| `DOCUMENT_CHANGED`                      | Design changed during the read                       | One retry maximum, or capture and read a snapshot instead                                     |

Remaining codes (`CURSOR_EXPIRED`, `RESYNC_REQUIRED` which is G3, `UNSUPPORTED_SCHEMA`,
`PROTOCOL_MISMATCH`, `EXPORT_FAILED`, `FONT_UNAVAILABLE`, `INTERNAL_ERROR`) resolve through doctor
(version alignment) or are documented in `docs/04-data-contracts/05-error-catalog.md`. Every structured
error carries `code`, `message`, `retryable`, `action`, `request_id`, and optional
`retry_after_ms`.

## 4. Upgrade and rollback

- Plugin and daemon are checked for compatibility at startup; an incompatible major version fails
  loudly, never half-works (PRD 8.7).
- Keep the previous binary: the upgrade procedure does not migrate private data by default; there is
  nothing durable to migrate by design (PRD 8.6.2 storage table).
- Run `figtether doctor` after upgrading to surface version mismatches.
- The detailed operator procedure lives in `docs/07-ops/03-runbook.md` (upgrade and rollback
  section); the release-facing policy, including deprecation windows, lives in
  `docs/07-ops/04-release-process.md`.

## 5. Privacy wording

> Local-only means FigTether processes do not send designs to any FigTether project cloud; the
> Figma editor and the AI client or model each have their own network paths.

Supporting facts for user-facing copy (PRD 8.6.2, FR18):

1. No cloud telemetry by default; crash reports are strictly opt-in and reviewed and sanitized
   before sending.
2. Diagnostic logs stay on the device, contain no design content, layer names, file ids, or tokens,
   and rotate locally at maximum 7 days or 10 MiB.
3. Pairing secrets live in memory only, shown solely by an explicit local CLI action.
4. The UI must also state that whatever the client reads can be forwarded to an AI model per the
   client's own configuration, even though the bridge is local (PRD 5.3).
5. Revoke cannot claw back data already delivered to an agent (PRD 5.3).

Any README or website copy must pass `AT16`: no unlimited claims, measured limits included, and the
core-path claim restricted to "does not consume official MCP/REST quota".

## 6. Open questions

1. Whether `figtether pair` becomes part of `figtether daemon` startup output or a separate
   interactive subcommand; the proposed surface lists both `pair` and `daemon` (PRD 8.4).
2. Exact checksum file naming and whether multiple checksum algorithms are published; floor is
   SHA-256 per PRD 8.11.
3. Availability of per-platform signing at v0.1; this affects whether Step 2 instructions also
   explain verification of signatures where they exist.
