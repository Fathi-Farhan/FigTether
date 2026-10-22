# Bridge startup review

Review date: 2026-10-21. Baseline: commit `c598d8f` and its preceding manifest/BOM fixes.
This covers source, executable behavior, and a subsequent live Figma CSP diagnosis. A successful
import and authenticated pairing of the final build must be distinguished from unit-test results.

## Verdict

The manifest contract and BOM diagnosis are correct directions. The original implementation
still had gaps that could leave the daemon, generated manifest and compiled plugin disagreeing.
A source installation also required too many commands and a second terminal for routine pairing.

| Finding in the baseline                                                         | Consequence                                                                                   | Implemented correction                                                                                              |
| ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Runtime credentials were written before binding the listener                    | Failed startup could replace a usable descriptor                                              | Bind before publishing the descriptor; hold one OS lock per runtime directory                                       |
| Liveness meant an open TCP port                                                 | A stale file pointing at another service could move automatic manifest generation to its port | Authenticate, request status, compare instance id and port, bound the full probe                                    |
| Discovery missed a local manifest in the plugin folder itself                   | The example port could win over the imported local port                                       | Search `manifest.local.json` in the current folder as well                                                          |
| Parent discovery cannot cross from a home folder to a checkout on another drive | Commands outside the checkout could lose the manifest                                         | Persist the generated manifest's absolute location; read its port rather than store another port copy               |
| Port extraction accepted any colon followed by a number                         | HTTP/remote endpoints or conflicting WebSocket entries could select the wrong port            | Match the same nonzero `ws://localhost:<port>` contract used by the plugin build                                    |
| The build injected the local port but copied the example plugin id              | The built manifest could identify a different plugin                                          | Build from one selected manifest for both port and identity; reject a conflicting environment override              |
| `--out` did not preserve that output's own port and id                          | Regeneration outside the source tree could change the agreement                               | Read the explicit destination before resolving defaults                                                             |
| Doctor did not check the compiled bundle                                        | A correct manifest could conceal a bundle built for an old port                               | Check file presence and build metadata; report known mismatches as failures and old unverifiable builds as warnings |
| Manifest writes directly truncated the destination                              | An interrupted write could leave broken JSON                                                  | Stage a BOM-free file beside the destination and replace it atomically                                              |

The runtime lock uses `fs2` to preserve the declared Rust 1.88 minimum. Standard-library
file locking was added in Rust 1.89. The daemon retains the file handle through cleanup;
the OS releases it after process exit, so a crashed process does not require deleting a lock.
The lock file stays in place to keep all contenders locking the same file. See
[Rust file locking](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock) and
[`fs2` lock rules](https://docs.rs/fs2/0.4.3/fs2/trait.FileExt.html).
Manifest replacement uses [`tempfile::NamedTempFile::persist`](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist)
on the destination filesystem.

## User flow

| Stage                                  | User action                                                 | Automatic behavior                                                                                              |
| -------------------------------------- | ----------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Source install, once                   | Install CLI with Cargo; run plugin `npm ci`                 | Dependencies are installed explicitly                                                                           |
| Figma development identity, once       | Create a development plugin and copy its id                 | Figma supplies the account's plugin id                                                                          |
| Prepare and run                        | `figtether start --plugin-id YOUR_PLUGIN_ID`                | Generate manifest, remember location, build matching plugin, start daemon, show pairing secret in this terminal |
| Figma import, once per path/CSP change | Import the printed `dist/manifest.json` and open the plugin | Figma loads its local bundle                                                                                    |
| Pair and share                         | Paste the secret, select frames, grant access               | The daemon serves only the resulting granted scope                                                              |
| Agent host, once per workspace         | Run the chosen `setup --client ... --apply` and refresh MCP | Install the project's managed MCP entry and design skill                                                        |
| Later sessions                         | `figtether start`, open plugin and pair/share as needed     | Preserve manifest id/port; reuse an active matching daemon or start a fresh foreground daemon                   |

An occupied manifest port fails with recovery instructions. `start --port PORT` explicitly
moves the generated contract and rebuilds the plugin; the user reimports it into Figma.
`manifest --port auto` preserves a verified active daemon or an available manifest port.
The five-minute pairing code can expire during import; `figtether pair` issues a replacement.
The CLI does not infer that a plugin is imported just because a manifest exists on disk.
When reusing an active daemon, `start` checks that the source contract and compiled bundle
already agree before issuing a secret. If they need rebuilding, stop the daemon and repeat
`start`; startup does not replace files underneath a connected plugin.

The `.exe` is currently a CLI for a source installation. It does not embed the plugin, import
it into Figma, or configure every agent host automatically. A future portable distribution
should ship the CLI and matching prebuilt plugin artifacts together and provide an explicit
way to rebuild/reconfigure both for a changed port. Merely distributing today's `.exe` does
not remove the Node/source prerequisite. This change does not publish an installer.

Figma documents development import and domain restrictions in its
[development plugin guide](https://help.figma.com/hc/en-us/articles/360042786733-Create-a-classic-plugin-for-development)
and [manifest reference](https://developers.figma.com/docs/plugins/manifest/).
Keeping one exact loopback WebSocket endpoint is this project's contract; it avoids weakening
the allowlist to make port switching easier. No automatic import API is assumed.

## Validation and limits

### Desktop diagnosis of a plugin stuck connecting

A subsequent live Figma console check showed the plugin trying port `39029` while its loaded
CSP still allowed only `38947`. The daemon and bundle on disk agreed on `39029`.

**SETUP-001: wrong import filename and stale CSP.** Figma Desktop requires the imported file's
basename to be exactly `manifest.json`. Earlier guidance to import `manifest.local.json` was
incorrect. That file remains a generated build input; the canonical import is
`plugins/figma/dist/manifest.json`. The installed desktop application's import validation
explicitly rejects other basenames, and the user's Figma settings still registered the example
manifest. Code/UI records in that same collection are dependency watchers, not extra imports;
doctor ignores them. The CLI, documentation and plugin recovery now
all point at the built manifest. Keep the checked-in example unchanged.

Doctor performs a read-only, best-effort check of relevant Figma Desktop import records. A known
wrong port or invalid filename fails the check; missing/unknown settings produce a warning.
Correct registration with old duplicate entries warns about cleanup. Registration does not
prove the running editor has refreshed its policy; the plugin checks the actual connection.

The plugin tests reachability before asking for a pairing secret, sends no authentication during
that check, and closes the probe socket immediately. A CSP failure stops retries, disables Pair,
and shows the expected port, loaded loopback ports when available, a copyable absolute manifest
path, and close/import/reopen steps. Transient socket failures remain retryable. This avoids
consuming a one-use secret while diagnosing an import problem and removes the contradictory
"Reconnecting" message after a terminal policy block.

This also exposed an unbounded UI state: a WebSocket error without a close event, a silent
dial, or a synchronous policy refusal could leave **Connecting** visible and prevent pairing
again. The bridge now leaves that state on error, bounds a silent dial to five seconds,
cleans up a timed-out handshake, detects policy events for both `localhost` and numeric
loopback, and shows recovery instructions. A failed connection is no longer labeled
**Bridge connected** in the activity log.

The CLI fallback now recommends `start --port PORT` instead of writing a relative manifest
under the caller's home folder. Source discovery requires the plugin build script, and
reusing an active daemon also saves the existing manifest location. Regression tests cover
these paths. The editor must import the built manifest after rebuilding and changing its CSP.

Regression tests exercise competing startups, failed binds preserving descriptors, strict
manifest parsing, custom destination preservation, BOM-free replacement, stale TCP listeners,
authenticated instance checks and remembered locations. A standalone smoke test runs the real
executable and esbuild against a source copy with spaces in its path and an isolated runtime:

```sh
node scripts/validation/startup-smoke.mjs --binary PATH_TO_FIGTETHER
```

It verifies the matching built manifest, one-terminal pairing, doctor discovery from an unrelated
folder, reuse of an active daemon, graceful shutdown, and stable automatic regeneration.
Pairing secrets stay in memory and are not written into test evidence.

These changes improve correctness and reduce the number of manual startup steps. No throughput,
latency or user task-success improvement is claimed without measurements. A wider Rust rewrite
would add risk without addressing the observed setup failures; scheduling and design contracts
are unchanged. Cross-platform CI remains necessary follow-up validation.

The final Windows desktop check imported `plugins/figma/dist/manifest.json`, reopened Bridge,
and authenticated with a fresh secret kept inside the local test process. Figma displayed
**Connected · 0 frames shared**. Daemon status confirmed one plugin connection and one paired
session on port `39029`, with no granted roots and no write audit entries. The recovery panel
was also observed live against the old `38947` CSP before the corrected plugin was reopened.
The duplicate example import was removed using Figma's development-plugin menu, retaining the
built manifest registration and all source files.
Cross-platform behavior remains unverified.
