# Connect your agent to FigTether

Pairing connects the Figma plugin to the daemon. Your agent application also needs its own
MCP connection. Complete both steps so the agent can work directly on the shared canvas.
Use a current source build of both daemon and plugin; see the
[installation tutorial](../README.md#build-and-connect).

## 1. Prepare Figma

Start `figtether` in one terminal and leave it running. In a second terminal, run
`figtether pair`. Paste that secret into FigTether Bridge, share the intended frame, and
choose **Allow editing** if the agent should design. The secret stays local; it does not
belong in an agent prompt or MCP configuration.

For multiple agents, share their intended frames together and give each a clear name.
Names coordinate work; they do not enforce per-agent isolation within one shared grant.

## 2. Choose your host and design workspace

The workspace can be an empty folder for design work. It does not need the FigTether source
code. Install the chosen agent application separately. These setup profiles target
**Codex CLI/local IDE**, **Gemini CLI**, and **Claude Code**; desktop/cloud applications with
similar names can use different connection mechanisms.

From your design folder, preview one setup:

```sh
figtether setup --client codex
figtether setup --client gemini
figtether setup --client claude
```

Run only the profile for your host, or install several if you use several hosts. From another
directory, add `--workspace "C:/projects/my-design"`. The directory must already exist.
The preview shows FigTether's new entry and file locations, without printing unrelated settings.

| Host        | Project connection file | Installed design skill                     |
| ----------- | ----------------------- | ------------------------------------------ |
| Codex       | `.codex/config.toml`    | `.agents/skills/figtether-design/SKILL.md` |
| Gemini CLI  | `.gemini/settings.json` | `.gemini/skills/figtether-design/SKILL.md` |
| Claude Code | `.mcp.json`             | `.claude/skills/figtether-design/SKILL.md` |

When the preview points at the intended executable and workspace, install it:

```sh
figtether setup --client codex --apply
```

Substitute `gemini` or `claude` as appropriate. This writes project-local settings and the
skill. It preserves unrelated configuration, refuses to overwrite an unmanaged/edited
FigTether entry or skill, and refuses linked/reparse paths. Setup is serialized with a local
lock. It does not change global prompts, workspace trust, or host approval settings.
Management records live in `.figtether/`; keep this local directory out of version control.
Generated configuration contains your absolute executable path, so review it before sharing.

Refresh/restart the FigTether MCP entry in the host. Accept any workspace/MCP trust requests
your host requires. Permission to call a host tool and permission to apply a Figma plan are
separate controls.

## 3. Check the connection

```sh
figtether setup --client codex --check --json
```

This validates the managed entry, launches its registered executable, initializes the real
MCP adapter, checks the new tools, discovers paired/shared sessions, and performs a bounded
scoped read. It reports each stage separately. It never mutates a Figma design.

**A successful CLI check does not prove the host exposes the tools.** Inside the agent
application, send:

```text
Use FigTether open_design for the shared frame named "Website Test".
Inspect its current design and tell me whether editing is available.
```

Use your actual frame name. The host should call `open_design` and receive the frame context
and image. If several frames match, it should ask which one. Then give it the design brief:

```text
Create a landing page for my neighborhood bicycle repair shop in Website Test.
Use editable Figma layers. Preview the changes, apply the approved plan,
and verify the result by reading the changed nodes and inspecting a new render.
```

Manual review remains the default. The user may enable Auto mode for eligible new plans;
agents must never enable it or approve through the browser controls. See the
[Bridge tutorial](bridge-ui-guide.md) for review rules.

## How much guidance is automatic?

MCP initialization supplies the canvas-first workflow. Project setup also installs a native
skill where the selected host can discover it. Both are generated from
[one workflow source](../integrations/figtether/workflow.json). A host may need to activate
or authorize the skill, and its higher-priority policies still apply. Connecting MCP alone
does not install skills into every application or make a text-only model understand images.

Normal design work starts with the brief and shared canvas. Required host instructions,
user-provided assets, implementation work, and bridge debugging can still require files.
The workflow does not impose a general ban on reading source code.

## Other MCP clients and manual configuration

```sh
figtether config print --client codex
figtether config print --client gemini
figtether config print --client claude
figtether config print --client generic
```

Codex receives TOML; the other profiles receive `mcpServers` JSON. For clients with an Add MCP
server form, select **stdio**, use the absolute executable path, and add `mcp` and `--stdio`
as separate arguments. These print commands do not write settings or install a skill.
Local stdio requires a host that can start local processes; a cloud-only chat cannot be
assumed to reach your computer's loopback daemon.

## Update, remove, and recover

After an update, repeat `setup --client YOUR_HOST --apply` in each workspace to refresh
the managed entry and skill. Wait for other agents to finish before restarting the shared
daemon/plugin. Refresh the host's MCP connection and run its connection check again.

To remove only the unchanged files/entry installed by setup:

```sh
figtether setup --client codex --remove
```

Unrelated settings remain. If you edited the managed entry or skill, setup refuses removal
until you reconcile those edits. It leaves parent directories and empty server maps in place.

| Symptom                                         | Action                                                                                      |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Command not found                               | Use the [PATH recovery steps](quickstart.md#figtether-is-not-recognized).                   |
| Paired in Figma, tools absent in agent          | Install/refresh the host MCP entry; check workspace trust and tool permissions.             |
| CLI check passes, agent still cannot call tools | Check the host's own MCP catalog; the CLI cannot certify host visibility.                   |
| Skill-file read rejected                        | Restore the host's authorized workspace read access. Do not bypass its policy.              |
| Agent asks to study the whole bridge project    | Start in your design workspace, use the installed workflow, and name the shared frame.      |
| Image exceeds the inline cap                    | Export a smaller section or lower scale, then call `get_asset`.                             |
| Image unavailable after access changed          | Export again under the current grant. Asset access expires after five minutes.              |
| Host/model cannot display images                | Report visual verification as unavailable; node metadata alone cannot establish appearance. |
| Setup refuses an existing entry                 | Preserve/reconcile the existing entry instead of overwriting it.                            |
| Setup lock remains after interruption           | Verify no setup is running before removing `.figtether/setup.lock`.                         |

### Windows error 10013: reserved port

Windows reserves a range of TCP ports for the hypervisor (Hyper-V, WSL, Docker Desktop) and
reserves it again at every boot. A port inside such a range cannot be bound even when no process
listens on it, so the daemon reports error 10013 for the default port `38947`.

Ask the doctor for the diagnosis first. The port check names the reserved range and the free port
to use instead:

```powershell
figtether doctor
```

Let the CLI pick that free port and write it into the plugin manifest:

```powershell
figtether manifest --plugin-id YOUR_EXISTING_PLUGIN_ID --port auto --out plugins/figma/manifest.local.json
```

The command prints the daemon command that uses the same port, because the plugin manifest and
the daemon have to agree on it:

```powershell
figtether daemon --port 39029
```

Rebuild the plugin for that port, because the build reads the port from the manifest:

```powershell
npm --prefix plugins/figma run build
```

Import the updated local manifest and reopen the plugin. Pair again with `figtether pair`;
pairing and MCP use the port in the daemon runtime automatically. The port is never changed
silently: the daemon keeps the port it was given and reports why it cannot bind it.

The manual route explains why the port is refused. The exclusion table of the operating system
lists the reserved ranges:

```powershell
netsh interface ipv4 show excludedportrange protocol=tcp
```

Choose a free port outside those ranges. For example, if 39147 is available:

```powershell
figtether manifest --plugin-id YOUR_EXISTING_PLUGIN_ID --port 39147 --out plugins/figma/manifest.local.json
npm --prefix plugins/figma run build
figtether daemon --port 39147
```

The build reads the port from `plugins/figma/manifest.local.json`. It uses the same port for
the socket, manifest allowlist, and private browser companion validation. Alternatively set
`$env:FIGTETHER_PORT = '39147'` for a build; the imported manifest must still allow that port.
Do not delete Windows port reservations or disable a firewall to work around this error.

The [agent workflow](agent-workflow.md) describes the tools. The
[evaluation guide](../benchmarks/agent-integration.md) explains repeatable tests and the
limits of current host/model evidence.
