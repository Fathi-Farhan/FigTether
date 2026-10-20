# Support

Status: `Proposed`. The support channels below are the intended ones for the project.

FigTether is a local tool. Most problems are local problems: a permission the editor denied, a
plugin that is not running, a daemon that cannot bind its port, a seat that does not support
the capability you need. The issue template is built around that.

## Where to ask

| Topic                                | Channel                               |
| ------------------------------------ | ------------------------------------- |
| A defect or a crash                  | The bug report issue template         |
| A missing capability                 | The feature request issue template    |
| A question about how to use the tool | The question issue template           |
| A possible vulnerability             | `SECURITY.md`, never a public issue   |
| A change to a published contract     | An ADR in `docs/02-architecture/adr/` |

Before opening an issue, run `figtether doctor`. The output is sanitized and it already
answers most configuration questions: versions, runtime directory, bound port, manifest state,
pairing state, and detected capabilities.

## What to put in a report

Every report carries these fields. The template asks for them so that a report can be
triaged without a back and forth.

| Field                        | Why it matters                                                                                                                                                                                                   |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| FigTether version            | The version or commit the behavior belongs to. Use the version that `figtether doctor` prints, not the one you think you have.                                                                                   |
| Operating system and version | The support matrix is per operating system and per architecture. Windows 11 and Windows 10 behave differently around the runtime directory permissions.                                                          |
| CPU architecture             | `x86_64` and `arm64` are separate builds.                                                                                                                                                                        |
| Figma Desktop version        | The editor version decides which capabilities exist. Report the build number, not only the marketing version.                                                                                                    |
| Capability report            | The sanitized capability output of the session. It names the capabilities and their state, and never contains design content. It is the fastest way to show that a capability is unavailable rather than broken. |
| Error id                     | The stable error code, for example `PLUGIN_OFFLINE`, `OUT_OF_SCOPE`, or `CAPABILITY_UNAVAILABLE`, plus the correlation id from the diagnostic output.                                                            |
| Public fixture               | The fixture from `fixtures/`, or a synthetic fixture you created for the report, with the steps to reproduce it.                                                                                                 |
| What you expected            | The expected behavior.                                                                                                                                                                                           |
| What happened                | The observed behavior, with the exact error text.                                                                                                                                                                |
| Logs                         | The sanitized diagnostic output, with the design content removed.                                                                                                                                                |

## What never goes into a report

- Private designs, screenshots of private designs, exported node data, or file contents.
- Tokens, pairing secrets, session credentials, or the contents of the runtime directory.
- Personal data of anyone else.

If a defect cannot be shown without a real design, build a synthetic fixture that reproduces
the shape of the problem. A maintainer can help with that: open a public issue with the
sanitized details and ask for help in building a fixture. Never attach a private design to a
public issue to make a report easier to understand, and never ask a maintainer for one either.

The same rule applies to the maintainers: a report never needs a private design, and the
project never asks for one.

## Support matrix

The support matrix lists the operating systems, the architecture, and the last tested Figma
Desktop version with the date of the test. It is published with the release notes and it
reflects real desktop tests only. A platform that has not been tested is listed as untested,
not as supported.

## Response expectation

- A well formed report with the fields above is triaged without a clarification round.
- A report that is missing the capability report or the error id is asked for once, with the
  exact command to produce it.
- A defect that blocks a release is treated as a release blocker, following the incident
  procedure in `docs/07-ops/03-runbook.md`.
- A security report never goes through this path. Use `SECURITY.md`.

## Contributing

If you want to fix the problem yourself, `CONTRIBUTING.md` describes the process, the ADR rule
for contract changes, the traceability requirement, and the evidence rules. A small pull
request with a regression test is the fastest route to a fix.
