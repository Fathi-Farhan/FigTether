# Security Policy

Status: `Proposed`. The reporting path below is the intended one. The concrete repository
address and the contact mailbox are filled in by the maintainer before the first public
release, and the placeholders are marked.

## Reporting a vulnerability

Report a suspected vulnerability privately. Do not open a public issue, a public discussion, a
pull request, or a social media post about it.

1. Preferred: use the private vulnerability reporting of the repository. Open the `Security`
   tab of the repository and choose `Report a vulnerability`. This creates a private thread
   that only the maintainers can read.
2. Alternative: send an email to the security reviewer role. The mailbox is recorded in the
   approval record at `docs/99-appendices/03-approval-record.md` and in the repository
   `SECURITY.md` metadata of the published repository.

<!-- maintainer: replace the placeholders below with the real repository address and mailbox before the first public release. -->

- Repository: `https://github.com/<owner>/<repo>/security/advisories/new`
- Mailbox: `security@<domain>`

### Never report through a public channel

A public issue, a comment, a pull request, a gist, a paste site, or a chat message is a public
channel. The rule is absolute:

- Never ask for designs, screenshots of designs, exported node data, tokens, pairing secrets,
  session credentials, or any other credential in a public issue, a public discussion, or a
  pull request. Not even as an example of a format.
- Never paste such material into a public issue to make a report easier to understand.
- Never ask a reporter to publish their design or their token so that the problem can be
  reproduced.
- If a reporter has already posted private material in public, ask them to remove it, treat
  it as compromised, and rotate any credential that was exposed.

Reproductions use a public fixture from `fixtures/` or a synthetic fixture that the reporter
creates for the case. The daemon never needs a real design to show a credential, scope, or
isolation defect.

## What to include

A report is easier to act on when it contains:

- The affected component: the daemon, the MCP adapter, the CLI, the plugin adapter, the plugin
  UI, the build and release pipeline, or the documentation.
- The version or commit: the exact tag, the binary version reported by `figtether doctor`, or
  the commit hash.
- The platform: the operating system and its version, and the CPU architecture.
- The editor version: the Figma Desktop build, when the defect needs the editor.
- The capability report: the sanitized capability output of the affected session. It contains
  capability names and states, never design content.
- The error id and the correlation id from the diagnostic output, with the message text
  removed if it contains anything from a design.
- The reproduction steps, using a public or synthetic fixture.
- The expected and the observed behavior.
- The impact: what an attacker or an accidental user can reach.

Please do not include:

- Private designs, screenshots of private designs, or exported node data.
- Tokens, pairing secrets, session credentials, or runtime directory contents.
- Personal data of any other person.

If a field is unknown, say so. A missing field never delays the first response.

## Response process

1. The maintainers acknowledge the report.
2. The report is triaged: severity, affected versions, and whether the issue is already public.
3. A fix is prepared on a private branch, and a regression test is added.
4. The fix is released, and the advisory is published with the affected versions and the
   upgrade path.
5. If the report turns out not to be a vulnerability, the reporter is told why, and the
   discussion moves to a public issue only with the reporter's agreement.

## Acknowledgement service level

- Every report is acknowledged within **3 working days**.
- The acknowledgement states that the report was received, names the person or role that owns
  it, and gives the next step.
- A report that needs more time still gets the acknowledgement on time, with an expected date
  for the next update.
- A report that arrives through a public channel by mistake is acknowledged in public, and the
  channel is moved to the private path without repeating any private detail.

The acknowledgement service level is the proposed target of PRD 8.11. It is a commitment about
the first response, not about the fix date.

## Scope

In scope:

- The local daemon, the MCP adapter, the CLI, the plugin adapter, and the plugin UI.
- The pairing, the grant, the scope enforcement, and the credential handling.
- The isolation between sessions, between clients, and between users of the same machine.
- The local trust model: the loopback binding, the `Host` and `Origin` validation, the
  pairing timeout, the revocation path.
- The build and release pipeline: the dependency lockfiles, the pinned actions, the SBOM, the
  checksums, and the provenance record.
- The resource limits: the queue bounds, the payload byte caps, the transfer caps, and the
  depth limits.

Out of scope:

- The Figma editor itself and the Figma plugin API. Report those to Figma.
- The AI client and the model that receives the data. Those paths belong to the user and to
  the client vendor.
- Vulnerabilities that need an administrator account, a malicious local administrator, or a
  modified build of the operating system. The local trust model is documented in
  `docs/05-security/01-threat-model.md`, and it states where the boundary is.
- Denial of service from a user who already has local code execution.

## Security properties the project holds itself to

- The core path never calls the official Figma MCP or REST endpoints. The core does not carry
  a personal access token, and it never asks for one.
- Reads are read only. A read never mutates a document, a selection, or a page.
- Scope is enforced twice, in the plugin and in the daemon, and a result that no longer
  matches the current grant epoch is discarded.
- Revoke stops further delivery, drops the cache entries and the assets of the grant, and
  blocks a retry of a revoked job.
- Diagnostics are sanitized: no layer names, no text content, no file ids, no tokens.
- Release artifacts carry checksums, an SBOM, and a provenance record that names the exact
  commit they were built from.
