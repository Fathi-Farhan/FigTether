# Publication review: 20 October 2026

This is a source-publication review, not a production approval or an external penetration test.
It covers the working tree based on `e9e23b0` and its two reachable commits. No GitHub remote was
configured during the review, so repository settings and hosted CI could not be verified.

## Changes made

- Replaced the root overview and contributor guide with source-build instructions, recovery
  steps, concrete developer checks, and visible limitations.
- Added original architecture/chart SVGs with a local generator. The chart uses a real offline
  run at the source commit recorded in [its data](../../benchmarks/published/offline-stages.json).
- Separated practical guides from proposed design references. Kept architecture, UML, ADRs,
  licenses, workflow definitions, and safe manifests public.
- Removed nonexistent repository/contact URLs and unpublished release links. The security policy
  describes GitHub's private-reporting UI without inventing an email address.
- Expanded ignore rules for runtime credentials, local manifests, signing material, raw benchmark
  output, and assessment evidence. Safe `.env.example` templates remain tracked.
- Added a publication file/link/SVG check and regression tests.
- Pinned workflow actions to full upstream commit IDs, disabled persisted checkout credentials,
  used Node 22, added plugin dependency auditing, and passed release input through a validated
  environment variable rather than interpolating it into shell source.

## Secret review

Gitleaks **8.24.3** scanned both reachable commits with its default rules. Three initial findings
were inspected in source:

| Finding                                       | Assessment                                                       |
| --------------------------------------------- | ---------------------------------------------------------------- |
| `Sec-WebSocket-Key` in two e2e files          | The RFC 6455 public example nonce, not an application credential |
| `session_credential` in the UI redaction test | A fixed, nonfunctional test string used to verify redaction      |

The checked-in configuration adds exact-value, exact-path exceptions to the generic API-key
rule for those fixtures. A rescan found no remaining detections. Tests are not excluded wholesale.
Scanner reports remain outside the repository because reports can contain credentials.

The proposed public file set is scanned separately; `.gitignore` is not treated as a history
cleanup tool. No credential rotation or history rewrite was indicated by the reviewed findings,
and neither operation was performed. Pattern scans cannot prove the absence of every secret.

## Dependency review

- `cargo-audit 0.22.2`: 221 lockfile dependencies, zero known vulnerabilities or warnings.
  RustSec database commit: `999ec89d5d3f10f20be14750ec8c2d8b44e0ac9b` (database updated 17 October 2026).
- `npm audit`: zero reported vulnerabilities in the root lockfile.
- `npm --prefix plugins/figma audit`: zero reported vulnerabilities, including development tooling.

These observations are point-in-time advisory results, not evidence about undisclosed bugs.
Run the commands again before tagging a release. License policy is configured in `deny.toml`;
the legal right to publish still needs the owner's review of provenance and notices.

## Checks and their limits

Run `npm run public:test`, `npm run public:check`, `npm run format:check`, the Rust workspace
checks, and the plugin checks in [CONTRIBUTING.md](../../CONTRIBUTING.md). The chart generator's
`--check` mode compares the checked-in SVGs byte-for-byte with their source data.

The reviewed tree contains no actual maintainer workstation path in Markdown. Generic example
paths in protocol/config tests are intentional. Runtime and artifact directories are ignored;
reviewed chart aggregates and generated contract schemas are deliberately tracked.

This review does not establish real Figma Desktop support. Existing automated suites include a
reference daemon and fake editor input; one real-editor acceptance scenario is intentionally
ignored. See [implementation status](../10-implementation/01-implementation-status.md) for runtime
gaps, including permission checks, pause/reconnect, transfers, and asset-resource delivery.

## Owner actions before pushing

1. Confirm the destination repository, copyright ownership, and license/notices.
2. Enable private vulnerability reporting and verify the button exists. Provide a real private
   conduct-reporting contact; these cannot be configured by committing Markdown.
3. Enable secret scanning/push protection and protect the default branch/tags.
4. Run hosted CI from a fresh clone. Workflow changes were reviewed locally, not run on GitHub.
5. Publish as **experimental source**, not as a secure production release. Do not ship credentials,
   customer designs, local runtime files, or unreviewed assessment evidence.

No files were removed from existing history and no remote push was performed by this review.
