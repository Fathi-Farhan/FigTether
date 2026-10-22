# Before making the repository public

Publishing source is not the same decision as declaring a stable release. FigTether can be
reviewed publicly as an experimental project while desktop compatibility and product-security
work remain open. Do not describe it as production-ready or independently audited.

## What belongs in Git

| Keep public and tracked                                   | Keep local or in a private store                                      |
| --------------------------------------------------------- | --------------------------------------------------------------------- |
| Source code, tests, synthetic fixtures, licenses          | Customer designs, screenshots, exported node data                     |
| `docs/`, architecture, UML, ADRs, security model          | Private incident reports and undisclosed vulnerabilities              |
| CI/release workflow definitions and safe config templates | API keys, `.env`, certificates, signing keys, registry credentials    |
| `Cargo.lock` and npm lockfiles                            | `node_modules/`, `target/`, plugin `dist/`, caches                    |
| Generated schemas and their generator                     | Runtime `daemon.json`, credentials, local manifests, logs             |
| Reviewed benchmark summaries, data, and chart sources     | Raw benchmark logs and PentAGI reports until reviewed/redacted        |
| Deployment/production configuration without secrets       | Live hostnames, database URLs with passwords, account-specific values |

**Do not ignore `docs/` or all production-related files.** Configuration is reviewable source;
its secret values come from a secret store or environment at deployment time. Release binaries,
SBOMs, and checksums belong in release assets rather than normal source commits. A generated
file can still belong in Git when it is a reviewable contract, such as `schemas/*.json`.

`.gitignore` only prevents new untracked files being added. It does not remove files already
tracked or secrets in past commits. Rotate a credential before any cleanup; ask before rewriting
history. Never assume that deleting a token makes it safe again.

## Local checks

```sh
npm ci
npm run public:check
node --test scripts/public-check.test.mjs
node scripts/render-readme-assets.mjs --check
npm run format:check
git diff --check
```

`public:check` checks the proposed public file set for risky filenames, private workstation paths,
placeholder public links, executable SVG content, and broken relative Markdown links. It is a
publication guard, **not a complete secret scanner or an application security audit**.

It also rejects UTF-8 byte order marks in proposed published text files. Generated local
manifests, plugin bundles, raw native designs and validation screenshots stay ignored. Keep
tracked example manifests BOM-free and use the CLI/build to generate machine-specific files.

Before publishing documentation changes, run `npm run agents:check` to confirm that the
shared workflow source, MCP instructions and both FigTether Design skills agree. Public
examples must import `plugins/figma/dist/manifest.json` and distinguish shipped capabilities
from proposed contracts and expected unavailable responses. The
[22 October documentation review](documentation-review-2026-10-22.md) records the current pass.

Run a dedicated scanner on all reachable history and on a temporary copy of the proposed file set:

```sh
gitleaks git --log-opts="--all" --redact --config .gitleaks.toml .
```

Store scanner reports outside the repository. Review findings before adding any exception.
`.gitleaks.toml` exempts only two exact public test values at specific paths, not all tests.
Don't scan your entire machine or publish a scanner report that might contain a credential.

Dependency checks:

```sh
cargo audit
cargo deny check
npm audit
npm --prefix plugins/figma audit
```

These are point-in-time advisory/license checks. New advisories can appear after publication,
and an absence of advisories says nothing about bugs in this project's own code.

## GitHub settings the source tree cannot configure

Before pushing to the new public repository:

- [ ] Confirm ownership and the final repository name. Inspect the license and third-party notices.
- [ ] Enable **private vulnerability reporting** under repository security settings. Verify that
      the report button described in [SECURITY.md](../../SECURITY.md) actually exists.
- [ ] Provide a real private conduct-reporting contact on the maintainer profile; do not invent a mailbox.
- [ ] Enable secret scanning and push protection where available.
- [ ] Protect the default branch: require review and passing CI; restrict force pushes and tag changes.
- [ ] Use read-only default workflow permissions. Keep deployment/signing credentials out of fork PR jobs.
- [ ] Configure environments/reviewers before adding a release-publish step. Current workflows only
      build artifacts; provenance JSON is not a signed SLSA attestation.
- [ ] Run hosted CI on a fresh clone. Local Windows tests do not prove Linux/macOS runner behavior.
- [ ] Verify release contents, not just source: manifests, maps, embedded paths, license files, checksums.

No remote URL is assumed by the docs. Add repository-specific badges and contact links only
after their targets exist. Relative documentation links should keep working in forks.

## Current public-release review

See [the recorded review](publication-review.md) for commands and observations from this cleanup.
Application gaps remain in [implementation status](../10-implementation/01-implementation-status.md),
including Windows permission checks, editor integration, transfer/resource delivery, and pause/reconnect.
Keeping those gaps visible is safer than hiding the documents that describe them.
