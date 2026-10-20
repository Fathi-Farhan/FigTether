# Try FigTether from source

FigTether is an experimental bridge, not a released desktop integration. The Rust services and
simulated plugin tests run locally; the actual Figma Desktop workflow below still needs validation.
Use a throwaway file with synthetic content. No prebuilt download or Community plugin is available.

## Prerequisites

- Rust installed through [rustup](https://rustup.rs/). The checkout selects its pinned compiler.
- Node.js 22.14 or newer and npm.
- Figma Desktop, a Figma Design file, and permission to create/run a development plugin.
- An MCP client that can launch a local stdio server.

Windows source builds also need the MSVC C++ build tools. Linux can run the Rust services and
tests; it is not a supported Figma Desktop target. Windows/macOS editor support is unverified.

## 1. Build the binary and plugin

From the repository root:

```sh
cargo build --locked --release -p figtether-cli
npm --prefix plugins/figma ci
npm --prefix plugins/figma run build
```

Add `target/release` to your `PATH` for this terminal, or use the binary's full path.

```powershell
# Windows PowerShell, from the checkout
$env:PATH = "$(Get-Location)\target\release;$env:PATH"
figtether --help
```

```sh
# macOS or a POSIX shell, from the checkout
export PATH="$PWD/target/release:$PATH"
figtether --help
```

## 2. Create a local development manifest

Use Figma's development-plugin creation workflow to obtain your own plugin ID. It is an identifier,
not a credential. Replace `YOUR_PLUGIN_ID` below with that value:

```sh
figtether manifest --plugin-id YOUR_PLUGIN_ID --out plugins/figma/manifest.local.json
```

The local manifest is ignored by Git. It references the built files in `plugins/figma/dist`.
The checked-in `manifest.json` is an example, not a registered Community plugin.

**Keep the default port, `38947`.** The UI currently hardcodes that address. Changing the daemon's
port and regenerating a manifest alone does not change the UI endpoint; custom-port support needs
additional work. Do not expose the daemon on a public interface to work around a connection failure.

## 3. Start the daemon

```sh
figtether daemon
```

Leave this terminal open. In a second terminal, with the same `PATH`:

```sh
figtether status
figtether pair
```

The pair command prints a one-use secret that expires after five minutes. Don't paste it into a
chat, issue, model prompt, or MCP configuration.

## 4. Pair and grant a frame

1. Open a synthetic Figma Design file in Figma Desktop.
2. In the development plugin menu, import `plugins/figma/manifest.local.json` and run FigTether.
3. Paste the secret into the plugin's pairing field.
4. Select a small frame, refresh candidates, and choose **Allow read session**.

Keep the plugin open while reading. Changing your selection does not expand the grant.
Revoke the session when finished. Pause/resume and snapshot controls exist in the UI but do not
yet establish complete daemon-side workflows; don't rely on them as validated security controls.

## 5. Configure the MCP client

```sh
figtether config print --client generic
```

Copy the output to your client's MCP configuration and restart that server entry. A generic entry
looks like this, with an absolute path to your own binary:

```json
{
  "mcpServers": {
    "figtether": {
      "command": "/absolute/path/to/figtether",
      "args": ["mcp", "--stdio"]
    }
  }
}
```

Windows paths need JSON escaping, for example `C:\\tools\\figtether.exe`. The CLI generates that
escaping for you. No token belongs in this file: the adapter reads its local runtime credential.

Start with `get_bridge_status`, `list_sessions`, and `get_selection`. Request a small subtree
with `list_nodes` before asking for individual descendants. The daemon only accepts descendants
whose relationship to a granted root it has learned.

## If something fails

```sh
figtether doctor --manifest plugins/figma/manifest.local.json
```

| Symptom                          | What to do                                                                                                            |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Port already in use              | Stop the other local daemon/process; do not use a tunnel or change to a public bind.                                  |
| Plugin cannot connect            | Check daemon status and the local manifest. Report the Figma build and CSP/network error without a design attachment. |
| `AUTH_REQUIRED` / `AUTH_EXPIRED` | Run `figtether pair` again and enter the new secret only in the plugin.                                               |
| `OUT_OF_SCOPE`                   | Start at a granted root and list its children, or grant a new root yourself.                                          |
| `BUSY`                           | Respect `retry_after_ms`; narrow the request rather than retrying in a loop.                                          |
| `REVOKED`                        | Only a new user-granted session can restore access.                                                                   |
| Snapshot/watch unavailable       | Those workflows are not implemented in the shipped daemon.                                                            |

The bundle's editor loading, mixed text/layout fidelity, large exports, reconnect, and per-seat
capabilities still need real-editor testing. See [implementation status](10-implementation/01-implementation-status.md).
For a reproducible bug, use [SUPPORT.md](../SUPPORT.md). For a security issue, use
[SECURITY.md](../SECURITY.md).

## Shut down

Revoke the session with `figtether revoke --all`, then press Ctrl+C in the daemon terminal.
Do not attach the runtime directory or `daemon.json` to an issue: it contains credentials.
