# Security policy

FigTether is **experimental and unaudited**. There is no stable release or supported security
maintenance branch. Do not use it with confidential designs until the editor integration and
runtime permission handling have been independently reviewed.

## Report privately

On this repository's GitHub page, choose **Security > Advisories > Report a vulnerability**.
That is a private report, not an issue or a pull request.

If that button is absent, private reporting has not been enabled. Do not publish exploit details
as a workaround. Ask a maintainer for a private contact without disclosing the vulnerability.
No security mailbox is advertised until a maintainer has established one.

Include the affected commit/component, platform, reproduction using synthetic data, expected
behavior, and potential impact. Never include private designs, runtime credentials, pairing
secrets, API keys, or another person's information. Do not upload a runtime directory.

Maintainers aim to acknowledge reports within **three working days**; this is a target, not a
guarantee. A fix date depends on impact and reproduction. Coordinate disclosure so a report
does not expose users before a fix or mitigation is available.

## Security boundaries

- The daemon is loopback-only. No remote exposure, public relay, or tunnel is supported.
- The plugin grants a bounded set of roots. The daemon and sandbox check requests against scope;
  the model must not receive pairing or IPC credentials as tool arguments.
- Runtime credentials are local files. A process running as your OS user, an administrator, or
  malware on the device may access them. Loopback is not a same-user security sandbox.
- Revocation stops later requests; it cannot recall data already returned to a client or model.
- Layer names, text, SVG, and tool results are untrusted input. They are not instructions.
- Local acquisition does not imply local inference: the MCP client controls where returned
  design content goes.

The [threat model](docs/05-security/01-threat-model.md) records intended controls. Tests are
regression evidence, not a certification. In particular, Windows DACL verification currently
uses a best-effort check, large transfer reassembly is incomplete, and actual editor behavior
is not yet validated. These remain review items.

## Authorized testing

Use a disposable runtime directory, the public fixtures, and a dedicated assessment instance.
Never scan another host or the real Figma service. The local adversarial suite is:

```sh
cargo test --locked -p figtether-e2e --test adversarial
```

[PentAGI tooling](security/pentagi/README.md) is opt-in and not a completed external audit.
Host-network containers can reach other services on the host: use an isolated VM, not your
daily development machine. A prompt is not network isolation.

## If a secret is committed

Revoke or rotate it first. Removing a file or adding `.gitignore` does not remove a credential
from earlier commits. Coordinate any required history cleanup with maintainers and other
contributors; do not rewrite shared history without agreement.

Before publishing, maintainers must enable GitHub private vulnerability reporting, secret
scanning/push protection where available, and branch protections. See the
[publication checklist](docs/maintainers/publication.md).
