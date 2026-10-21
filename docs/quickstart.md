# Install and use FigTether

FigTether is an experimental bridge, not a released desktop integration. The Rust services and
simulated plugin tests run locally. A limited Windows/Figma Desktop 126.10.10 website test now
has [recorded evidence](../benchmarks/published/desktop-2026-10-20.md); other platforms and seats
still need validation.
Use a throwaway file with synthetic content. No prebuilt download or Community plugin is available.

The [README tutorial](../README.md#build-and-connect) walks through installation, plugin import,
pairing, and a first approved design. This page adds executable paths and recovery commands.
There is no published npm CLI package: `npm install figtether`, `npx figtether`, and
`pnpm dlx figtether` are not supported installation methods for this project. npm builds the
local Figma plugin; Cargo installs the Rust executable.

## Prerequisites

- Rust installed through [rustup](https://rustup.rs/). The checkout selects its pinned compiler.
- [Node.js](https://nodejs.org/en/download) 22.14 or newer and npm.
- [Figma Desktop](https://www.figma.com/downloads/), a Figma Design file, and permission to create/run a development plugin.
- An MCP client that can launch a local stdio server.

Windows source builds also need the **Desktop development with C++** workload from
[Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/).
Reopen the terminal after installing prerequisites, then check `cargo --version`, `node --version`,
and `npm --version`. Linux can run the Rust services and
tests; it is not a supported Figma Desktop target. Windows has one synthetic-file test; macOS
editor support is unverified.

## 1. Install the binary and build the plugin

Extract this repository's source ZIP or clone it using the URL in its GitHub **Code** menu.
Open a terminal in the extracted/cloned repository root, beside `Cargo.toml`, then run:

```sh
cargo install --locked --path crates/cli
npm --prefix plugins/figma ci
npm --prefix plugins/figma run build
figtether --help
```

Cargo installs the CLI in its installation `bin` directory. With default Rust settings this
is `%USERPROFILE%\.cargo\bin` on Windows or `~/.cargo/bin` on macOS. A custom Cargo home or
installation root can change that directory. Cargo prints the installed executable path.
See [Cargo's installation reference](https://doc.rust-lang.org/cargo/commands/cargo-install.html).

### figtether is not recognized

Open a new terminal first. If the command still cannot be found, use the installed executable
directly. With default Cargo settings on Windows:

```powershell
# Windows PowerShell: verify the installed executable
& "$env:USERPROFILE\.cargo\bin\figtether.exe" --help

# Make it available by name in this terminal
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
figtether --help
```

```sh
# macOS or a POSIX shell, with default Cargo settings
"$HOME/.cargo/bin/figtether" --help
export PATH="$HOME/.cargo/bin:$PATH"
figtether --help
```

For permanent Windows setup, add the actual Cargo `bin` directory to your **user Path** in
Windows Environment Variables, then reopen your terminal and agent application. Every terminal
has its own environment; a temporary `$env:PATH` change in Terminal A does not update Terminal B.
The MCP configuration generated later uses an absolute path to avoid this dependency.

If you prefer to keep the executable in the build directory, use
`cargo build --locked --release -p figtether-cli` instead of `cargo install`. Its default output
is `target/release/figtether.exe` on Windows or `target/release/figtether` on macOS, relative to
the checkout. `CARGO_TARGET_DIR` overrides this build location. In PowerShell, run a local
executable with `& "C:\actual\path\figtether.exe" daemon`; replace the example path.

#### Why the path matters, and what has nothing to do with it

`figtether` without a path is a name that a shell resolves through `PATH`. That is the whole
reason. It is not a developer mode, and it has nothing to do with Figma's developer mode or with
Windows developer mode.

- **By path** works everywhere, in every script, in CI, and in an agent configuration:
  `& "D:\rust\kiln-target\debug\figtether.exe" doctor`. Use this form when the path is long, when
  several builds exist, or when a script should not depend on the environment.
- **By name** works when the directory that holds the executable is on `PATH`. Add it for the
  current shell only, or permanently in the user environment variables.
- A **custom `CARGO_TARGET_DIR`** moves the build output, so the executable is not in the
  checkout's `target` directory. Ask Cargo where it went instead of guessing:

  ```powershell
  # Windows PowerShell
  cargo build --locked -p figtether-cli
  (Get-ChildItem -Recurse -Filter figtether.exe -Path $env:CARGO_TARGET_DIR, "$PWD\target" -ErrorAction SilentlyContinue |
      Select-Object -First 1).FullName
  ```

  ```sh
  # macOS or a POSIX shell
  cargo build --locked -p figtether-cli
  find "${CARGO_TARGET_DIR:-target}" -name figtether -type f -perm -u+x | head -n 1
  ```

- The **MCP configuration** never depends on `PATH`: `figtether config print --client generic`
  writes the absolute path of the executable it is running as, so the agent starts the same
  binary in every terminal.

**Checkpoint:** the CLI help opens, and both `plugins/figma/dist/main.js` and
`plugins/figma/dist/ui.html` exist. A Windows `.exe` built this way is a CLI, not a setup wizard.

## 2. Create a local development manifest

1. Open a Figma Design file in Figma Desktop.
2. Choose **Plugins → Development → New plugin**, select **Figma design**, name it
   `FigTether Local`, choose **Custom UI**, and save its starter files in a separate folder.
3. Open the starter `manifest.json` in a text editor. Copy the value of its `id` field.
   This is the Figma-generated plugin identifier, not a credential or a pairing secret.

See [Figma's plugin creation guide](https://developers.figma.com/docs/plugins/plugin-quickstart-guide/).
Return to the terminal in this repository's root and replace `YOUR_PLUGIN_ID` with that value:

```sh
figtether manifest --plugin-id YOUR_PLUGIN_ID --out plugins/figma/manifest.local.json
```

Inside a checkout the command writes `plugins/figma/manifest.local.json` and keeps the plugin
id of the manifest that already exists, so `--plugin-id` can be left out. Without a checkout it
writes `manifest.json` in the current directory and uses the id `figtether-local`.

The local manifest is ignored by Git. It references the built files in `plugins/figma/dist`.
The checked-in `manifest.json` is an example, not a registered Community plugin.

#### Which manifest file is which

Three manifest files exist in the plugin folder, and they are not interchangeable:

| File                                | What it is                                                                                                        | Who reads it                                                                                  |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `plugins/figma/manifest.local.json` | Your manifest, with your own plugin id and the port you chose. Generated by `figtether manifest`, ignored by Git. | You import this one into Figma. The plugin build reads it to inject the port into the bundle. |
| `plugins/figma/manifest.json`       | The checked-in example, with the placeholder id `figtether-dev`.                                                  | Tracked in Git as a reference. Figma reads it only if you import it.                          |
| `plugins/figma/dist/manifest.json`  | The copy the build writes next to the bundles, with the paths rewritten so they resolve inside `dist`.            | The built plugin bundle. Not imported by hand.                                                |

Figma remembers the manifest path you imported, and the plugin id in it. Change the port, rebuild,
and reopen the plugin; the manifest Figma holds is the one you imported.

#### When Figma shows a manifest error

Figma parses a development manifest with a strict JSON reader. A file that starts with a UTF-8
byte order mark fails that reader, and the plugin entry shows **View manifest error** instead
of a runnable plugin. The same happens when the file is not JSON at all.

`figtether doctor` names the cause. It reports `the manifest ... starts with a byte order mark`
for the mark, and `the manifest ... is not valid JSON` for broken JSON. The doctor looks for the
manifest in the current directory and in every parent directory, so it finds the file from any
directory inside the checkout; `--manifest <path>` names it explicitly.

The command writes the manifest without a byte order mark. When an editor adds one anyway,
write the file again with the command instead of editing it by hand:

```sh
figtether manifest --plugin-id YOUR_PLUGIN_ID --port auto --out plugins/figma/manifest.local.json
```

After the file changes, import it again with **Plugins → Development → Import plugin from
manifest** and select the same file. Figma keeps the cached error until the manifest is
imported again or Figma restarts.

In Figma, choose **Plugins → Development → Import plugin from manifest** and select
`plugins/figma/manifest.local.json` from this repository. Run the imported **FigTether** entry.
Do not select the generated starter plugin's manifest. Keep the FigTether plugin folder in
place; Figma loads its code from disk. Rebuilding requires closing and reopening the plugin
to load the updated bundle.

The default port is `38947`. If the daemon reports error 10013, Windows has reserved that port
range for the hypervisor and nothing listens on it. Run `figtether doctor` for the diagnosis; the
port check names the reserved range and the free port to use. Then let the CLI write the manifest
for that port:

```sh
figtether manifest --plugin-id YOUR_PLUGIN_ID --port auto --out plugins/figma/manifest.local.json
```

`--port auto` keeps the port that the machine and the plugin already agree on, and picks a free
port only when there is no agreement. That is the port of the running daemon, or the port of the
manifest that already exists when it can still be bound. The command prints the matching
`figtether daemon --port <port>` command. Rebuild the plugin and start the daemon with that port,
because the plugin manifest and the daemon have to agree on it. The port is never changed
silently. See [Windows reserved-port recovery](agent-setup.md#windows-error-10013-reserved-port).

## 3. Start the daemon

```sh
figtether
```

With no subcommand, `figtether` starts the daemon in this terminal. `figtether daemon`
still works; use that form for daemon flags such as `--runtime-dir`. Starting the daemon
does not generate a pairing secret or grant access to any Figma frame.

#### Which port the daemon binds

One command has to be enough, so the port is resolved instead of asked for:

1. A port that you name with `--port` wins.
2. The port of the local plugin manifest wins over the default, because the plugin reads
   its port from that manifest and the two have to agree.
3. The default bridge port `38947` is used when nothing names another one.
4. When the default port cannot be bound and no manifest names a port, a free port is
   picked. The daemon says so, and prints the command that writes the manifest for the
   same port. The change is announced, never silent.

The daemon prints the port and its source on startup:

```text
FigTether daemon 0.1.0
  bridge address           127.0.0.1:39029
  port source              plugins/figma/manifest.local.json
```

`figtether doctor` shows the same port, so the daemon and the doctor never disagree about
where the bridge is.

Leave this terminal open. In a second terminal, with the same `PATH`:

```sh
figtether status
figtether pair
```

The pair command prints a one-use secret that expires after five minutes. Don't paste it into a
chat, issue, model prompt, or MCP configuration.
Local setup scripts can use `figtether pair --json`; the output contains only the secret and
its expiry. Keep that output local too.

### Stopping the daemon

The daemon runs in the foreground of its own terminal, so `Ctrl+C` stops it. A daemon that was
started in the background, by a script, or by an agent needs a command:

```sh
figtether stop
```

`figtether terminate` is the same command under a different name. It asks the daemon to stop
through its own control endpoint and waits for the port to be released, so the graceful cleanup
happens: queued work is cancelled, credentials are revoked, sockets are closed, the design memory
is wiped, and the runtime file is removed. A daemon killed with a signal skips all of that and
leaves a stale credential behind.

```sh
figtether stop --json          # machine readable outcome
figtether stop --port 39029    # a daemon on a non default port
figtether stop --timeout 30    # wait longer for a busy daemon
```

The command reports one of three outcomes: `stopped`, `not_running` (nothing to stop, which is
not an error), or `still_running` (the daemon did not release its port in time).

## 4. Pair and grant a frame

The [illustrated Bridge UI guide](bridge-ui-guide.md) explains where the secret comes from,
what each status means, and how pairing differs from granting design access.

1. Open a synthetic Figma Design file in Figma Desktop.
2. Run the FigTether plugin you imported in step 2.
3. Paste the secret into the plugin's pairing field.
4. Create/select a small frame, click **Update selection**, check it, and choose **Share selected frames**.
5. For native design creation or editing, also choose **Allow editing**.

Keep the plugin open while reading. Changing your selection does not expand the grant.
Pairing alone shares no frame; a write grant still requires approval for each proposed plan.
The [README's first-design prompt](../README.md#6-make-your-first-design) provides a complete example.
Pause/resume and snapshot controls exist in the UI but do not
yet establish complete daemon-side workflows; don't rely on them as validated security controls.

## 5. Configure the MCP client

```sh
figtether config print --client generic
```

Copy only the printed JSON object to your client's MCP configuration and restart that server entry.
Merge it with existing servers. In a client with a server form, choose stdio, the printed
absolute executable path, and the arguments `mcp` and `--stdio`. The exact settings location
and supported configuration format depend on the client. A generic entry
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

For supported CLI hosts, use the [project setup tutorial](agent-setup.md) to install both the
connection and design skill. Start design work with `open_design` and your shared frame name.
For manual discovery, use `get_bridge_status` and `list_sessions`. Use the session's `granted_root_ids` and
`grant_mode` to discover shared frames and whether editing is allowed. For creation, enable
**Allow editing**, then let the agent preview a plan for your approval. The
[canvas-first agent workflow](agent-workflow.md) explains this loop, inline node styling,
connection recovery, and which guidance is supplied automatically.

For movable controls, click **Open outside Figma** and drag the browser tab into its own
window. Keep the Figma plugin running. Manual review is the default; you can explicitly
enable **Auto mode** for new plans without warnings, blocks, or deletions. See the
[Bridge tutorial](bridge-ui-guide.md) for screenshots, review rules, and recovery steps.

The desktop test found a `get_selection`
timeout and an `inspect_design` contract mismatch; use `list_nodes`/`get_nodes` for the tested
reading path while those issues remain open. Request a small subtree
with `list_nodes` before asking for individual descendants. The daemon only accepts descendants
whose relationship to a granted root it has learned.

## If something fails

```sh
figtether doctor --manifest plugins/figma/manifest.local.json
```

| Symptom                          | What to do                                                                                                                                                                 |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Port already in use              | Check `figtether status` first and reuse an existing FigTether daemon. Do not stop another agent's shared daemon. Identify a conflicting process before changing anything. |
| Plugin cannot connect            | Check daemon status and the local manifest. Report the Figma build and CSP/network error without a design attachment.                                                      |
| `AUTH_REQUIRED` / `AUTH_EXPIRED` | Run `figtether pair` again and enter the new secret only in the plugin.                                                                                                    |
| `OUT_OF_SCOPE`                   | Start at a granted root and list its children, or grant a new root yourself.                                                                                               |
| `BUSY`                           | Respect `retry_after_ms`; narrow the request rather than retrying in a loop.                                                                                               |
| `REVOKED`                        | Only a new user-granted session can restore access.                                                                                                                        |
| Snapshot/watch unavailable       | Those workflows are not implemented in the shipped daemon.                                                                                                                 |

The bundle's editor loading, mixed text/layout fidelity, large exports, reconnect, and per-seat
capabilities still need real-editor testing. See [implementation status](10-implementation/01-implementation-status.md).
For a reproducible bug, use [SUPPORT.md](../SUPPORT.md). For a security issue, use
[SECURITY.md](../SECURITY.md).

## Update an installation

```sh
figtether update --dry-run
figtether update
```

The updater uses the original source checkout, requires a clean Git tree with an upstream,
pulls with `--ff-only`, stages a rebuilt CLI, rebuilds the plugin, and replaces the executable
you invoked. It requires the source-build tools and does not download a published release.
It refuses dirty checkouts instead of discarding local edits. Concurrent updates of the same
executable are blocked by an update lock.

Use `figtether update --source "C:/projects/figtether"` if the checkout moved. For a ZIP or
intentional local changes, use `--local` to rebuild existing source without pulling; it does
not obtain a newer source version. Dependency downloads may still occur. See the
[README update guide](../README.md#update-figtether) for moving the plugin manifest and the
one-time upgrade from a CLI that does not yet have `update`.

Wait until all shared agents are finished, restart the daemon and MCP entry, and close/reopen
the Figma plugin after updating. The updater does not restart them or revoke their grants.
If an update is interrupted, verify no updater is running before removing
`.figtether-update.lock` beside the executable. Retry the command; build failures preserve
the CLI but can leave the source/dependencies/plugin output partially updated.

## Shut down

Close your own MCP connection when finished. If another agent is still using the same local
installation, leave the shared plugin/session and daemon running. Separate frame names are
a working convention, not enforced access isolation between clients.

When everyone using the installation is finished, revoke access in the plugin or run
`figtether revoke --all`, then press Ctrl+C in the daemon terminal. This revokes all shared
sessions; do not use it as routine cleanup for only one agent.
Do not attach the runtime directory or `daemon.json` to an issue: it contains credentials.
