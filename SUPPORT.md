# Getting help

FigTether is experimental. For installation and the known limits, start with the
[quickstart](docs/quickstart.md). For suspected vulnerabilities, use [SECURITY.md](SECURITY.md),
not a public issue.

## Before opening an issue

Run `figtether status` and `figtether doctor --manifest plugins/figma/manifest.local.json`.
The doctor can fail before the daemon is started or a session is paired; include which step you
reached, not just its exit code.

Use the repository's bug-report, question, or capability-request form. Include:

- FigTether commit or version, OS, and architecture;
- Figma Desktop build if the issue involves the plugin;
- steps on a synthetic design or [public fixture](fixtures/README.md);
- expected behavior, actual behavior, and the error code;
- relevant diagnostic lines, after reviewing them for private content.

Do not attach designs, screenshots of client work, exported node data, pairing secrets, tokens,
or `daemon.json`. Redact your username, file paths, and document names from diagnostics. If you
cannot reproduce without private data, describe the structure rather than uploading the file.

## Common cases

| Error                            | Recovery                                                                       |
| -------------------------------- | ------------------------------------------------------------------------------ |
| `PLUGIN_OFFLINE`                 | Open Figma and restart/pair the plugin. No snapshot fallback is implied.       |
| `AUTH_REQUIRED` / `AUTH_EXPIRED` | Generate a new secret locally with `figtether pair`.                           |
| `OUT_OF_SCOPE`                   | Traverse from a granted root, or change the grant in the plugin.               |
| `BUSY`                           | Wait for the suggested retry delay and reduce concurrent requests.             |
| `CAPABILITY_UNAVAILABLE`         | Check whether the workflow is implemented; some advertised tools are reserved. |
| Port collision                   | Stop the conflicting local process. The UI currently uses port 38947.          |

## Support status

Windows and macOS are intended desktop targets, not a verified support matrix. Linux runs the
core/tests only. Reports from real-editor sessions are welcome; please distinguish an automated
mock test from a desktop result. There is no guaranteed support response time.

To work on a fix, see [CONTRIBUTING.md](CONTRIBUTING.md).
