# Release Process

Status: `Proposed`

This document defines the FigTether release machinery: the gate flow `G0` to `G4` with exit
criteria and fail decisions from PRD 8.10, the release candidate checklist enforcing the v0.1
Definition of Done, the artifact set per release, supply chain and versioning policy, post-release
verification duties, and explicit deferrals. Estimates are ordered assumptions for two engineers
plus part-time QA, not schedule commitments (PRD 8.10).

## 1. Gate flow

Cut only when the previous gate is closed and documented in the approval record
`docs/99-appendices/03-approval-record.md`. No later-phase implementation starts before core
`G0` risks close (PRD critical path note).

| Gate and phase         | Estimate                | Deliverables                                                                                                             | Exit criteria                                                                                                                                                                                                   | Decision if the gate fails                                                                                                                                              |
| ---------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `G0` Feasibility spike | 1-2 weeks               | Pairing over local WebSocket, Starter-seat read, JSON and PNG export, permission probes, Rust MCP handshake              | `AT01`, `AT02`, `AT12`, `AT18` pass; desktop proof on Windows and macOS; `ADR-002` and `ADR-003` pinned from evidence                                                                                           | If one OS fails, restrict the support matrix to proven OS/seat combos; if the eligible-Starter path fails, reposition the product before MVP; stop-and-record otherwise |
| `G1` Alpha             | 3-4 weeks               | Scoped live and snapshot mode, normalized model, pagination, cache, cancellation, local diagnostics                      | All P0 functional requirements demonstrably working; integration and negative tests pass; diagnostics hygiene verified                                                                                          | Remaining work stays internal; ship a developers-only alpha with documented limitations and no public binaries                                                          |
| `G2` Public v0.1       | 2-3 weeks after `G1`    | Prebuilt binary and plugin bundle, doctor UX, user docs, security and resource tests, benchmark report, pilot with users | All P0 `AT01`-`AT18` targets pass; handshakes verified on two MCP clients; NFR `N01`-`N10` evidence recorded; zero critical data leak, crash, or misrouting bugs; artifacts built from the exact release commit | Delay public publication; keep the alpha channel and address blockers with a dated re-review                                                                            |
| `G3` v0.2              | 3-4 weeks after `G2`    | Watch with resync (FR20), versioned offline bundle (FR21), multi-client scheduling (FR22)                                | `AT19`, `AT20` pass; watch coverage honesty validated; schema migration executed cleanly                                                                                                                        | Features are not published as stable; they land only after the gate passes, or are withdrawn from the release notes                                                     |
| `G4` v0.3+             | After demand validation | Starter templates, optional package managers, Community plugin submission                                                | Supporting ADR plus explicit user evidence; Community acceptance is an outcome, never an assumption (D11)                                                                                                       | Continue development-distribution mode (development manifests with user plugin ids) until evidence lands                                                                |

Definition of Ready (PRD 8.10.1) applies at every gate entry: owner, acceptance test, scope, and
curated fixture per requirement; schema, grant, credential, and resource limit decisions approved
through an ADR; SDK dependencies locked to tested versions; unproven features labeled
experimental or unsupported, never stated as fact.

## 2. Release candidate checklist

A release candidate is publishable only when every item is checked against the **exact release
candidate commit** (PRD 8.10.2, Definition of Done v0.1):

- [ ] All FR P0 and all P0 acceptance tests pass at the exact commit; evidence records follow the
      Appendix C release-evidence template (commit, artifact sha256, schema and protocol version,
      OS/CPU/RAM, Figma version, plugin manifest hash, account plan and seat context without
      personal identity, fixture hash, test id, commands, expected and actual, duration, latency,
      RSS, network path evidence, known limitations, reviewer, tested_at).
- [ ] `G0` evidence stands: the eligible-Starter path works, no PAT anywhere in the product, no
      official MCP or REST calls in the core (`AT01`, `AT02`, `N09` review at the release commit).
- [ ] Benchmarks are split by workload class, cached, fresh, and export reported separately, each
      with configuration and raw results, published in `docs/06-testing/04-benchmark-plan.md`
      evidence slots; no cross-class averaging anywhere.
- [ ] Revoke, scope isolation, stale epoch, parser limits, and platform permission checks have
      runtime evidence, not source review alone.
- [ ] Full build pipeline green: Rust and TypeScript build, format, lint, tests, dependency audit,
      SBOM, checksums, and provenance record completed for the RC commit.
- [ ] Chosen OS binaries and the plugin bundle are exercised from the distributed artifacts, not
      only from the source tree (`AT03` on two OSes, `AT14` on two clients).
- [ ] Documentation complete: README, quickstart, troubleshooting, support matrix with last-tested
      Figma version and date, privacy notice, security reporting, changelog, and licenses; the
      support matrix reflects real desktop tests.
- [ ] Known limitations recorded bluntly (including untested-platform deferrals, `AT16` claims
      review, CLI surface listed as proposed tooling where it is still in scope only for the
      bundled release channel), zero P0 blocker issues open, and rollback to the previous release
      available.

## 2.1 Pre-release freeze

The checklist above implies a freeze discipline; this section states it in operational terms.

1. A candidate commit is named in the evidence bundle; from that point, only P0 blocker fixes may
   merge, and every fix forces a re-run of the acceptance tests that touched the change at minimum,
   plus a rebuild of all artifacts (artifacts are never patched in place, only rebuilt from a new
   exact commit).
2. The RC commit hash, artifact hashes, and CI run id are recorded together; a mismatch between the
   announced hash and the produced hashes aborts the release rather than guessing which one shipped
   (tag points to the tested commit, PRD 8.11).
3. Documentation-only changes during the freeze are allowed as long as they do not alter claims or
   support matrix rows; claim-bearing edits re-open the `AT16` review.
4. Freeze ends only when the gate decision is written into the approval record and the channel move
   (alpha, beta, stable per section 3.1) is published with the final evidence recomputed.

## 3. Artifact set per release

| Artifact          | Content                                                                                                       | Notes                                                                                          |
| ----------------- | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| OS binaries       | `Windows x64`, `macOS arm64`, `macOS x64` when runner plus editor testing exists; Linux core CLI and fixtures | No installer requiring admin rights as a default (PRD 8.11)                                    |
| Plugin bundle     | Prebuilt main and UI, versioned together with the binary                                                      | Development manifests with the user's own plugin id are the distribution path until D11 closes |
| Checksums         | Minimum SHA-256 per released file                                                                             | Floor of verification for every user (see `docs/07-ops/02-end-user-install.md`)                |
| SBOM              | Dependency inventory of core and plugin                                                                       | Produced in CI at the RC commit                                                                |
| Provenance record | Build provenance linking artifacts to the commit and CI run                                                   | Required even where cryptographic signing is not available                                     |
| Release notes     | Measured claims only, per-limit tables, known limitations                                                     | Must survive the `AT16` claims review: no unlimited claims                                     |
| Evidence bundle   | Release evidence records per gate and per test                                                                | Stored under the future `docs/release-evidence` mirror                                         |

Per PRD 8.11: release channels alpha, beta, and stable; archives are immutable per version (a
published file is never silently replaced); the release tag points to the exact tested commit.
Builds come from lockfiles (`Cargo.lock` plus a frontend lockfile), pinned release actions,
least-privilege CI, dependency audits, and artifact checks; signing is added per platform
availability.

### 3.1 Release channel ladder

| Channel  | Audience                                 | Entry criteria                                                                                                                                              | Distribution rules                                                                                                                                    |
| -------- | ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `alpha`  | Maintainers and contributors only        | `G1` exit evidence reached; current known-limitations list attached to every build                                                                          | Source-built or internal binaries; no public encouragement to adopt; docs label the CLI as proposed where it is                                       |
| `beta`   | Invited pilot users (the `G2` pilot run) | P0 acceptance set passes on the exact RC commit; quickstart validated with at least one eligible-Starter seat                                               | Prebuilt binaries and plugin bundle may distribute; feedback flows only through issue templates; nothing is called stable                             |
| `stable` | Public                                   | `G2` gate decision recorded as passed; complete artifact set including SBOM, checksums, provenance; support matrix published with last-tested Figma version | Immutable archive per version; tag points to the tested commit; withdrawal happens only through the incident procedure of `docs/07-ops/03-runbook.md` |

A version moves one channel at a time; skipping beta for a stable publication requires an explicit
maintainer decision recorded in the approval record, and never applies when a `G2` exit item is
unresolved.

### 3.2 Roles and approvals at release

Release duties are split along the roles used by the PRD approval record
(`docs/99-appendices/03-approval-record.md`; all roles are pending assignment until kickoff).

| Role               | Release duties                                                                                                                 |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| Product maintainer | Claims review against `AT16`; changelog accuracy; decides if the gate decision is recorded as pass or hold                     |
| Rust/core owner    | Confirms MSRV and rmcp pin match `ADR-003` evidence (`D08`); signs off core-path egress review at the release commit           |
| Plugin owner       | Re-verifies eligibility and API coverage on the current Figma build; owns the plugin manifest hash in the evidence bundle      |
| Security reviewer  | Inspects sanitized diagnostics in the evidence; triggers the incident procedure if any exposure appears                        |
| QA/release owner   | Builds artifacts, checksums, SBOM, and provenance; keeps the tag-to-commit discipline; runs the post-release verification pass |

If a duty cannot be performed (for example, macOS artifacts blocked by runners), the gap is stated
in the release notes and the support matrix is narrowed accordingly; absence of a role never
becomes a silent skip.

## 4. Versioning policy

1. Core and CLI follow SemVer. Breaking changes go through a major version bump, and unsupported
   major versions are rejected at runtime, never half-negotiated.
2. Protocol and schema versions are separate version streams (`schema_version` and
   `protocol_version`, documented in `docs/04-data-contracts/01-mcp-envelope.md` and
   `docs/04-data-contracts/03-bridge-protocol.md`);
   a core patch bump alone does not move them, and unknown optional fields must be tolerated by
   every consumer rather than aborting the handshake.
3. Deprecation notice is a minimum of one minor release or 90 days, whichever is longer;
   documented exceptions exist only for security patches.
4. Compatibility policy: the last-tested Figma Desktop version and its date are recorded in the
   support matrix; user-side editor runtime is never assumed to be pinnable, so the product probes
   capabilities at runtime (FR09) instead of trusting version numbers.

## 5. Post-release verification duties

Per DoD v0.1 and `AT03`, the shipped artifacts, not the source tree, are the reference:

1. Rerun the P0 acceptance set against the distributed binary and plugin bundle (two OSes, two MCP
   clients), and record fresh evidence in the release evidence bundle.
2. Verify published checksums against the artifacts actually downloaded via the release channel;
   a checksum mismatch freezes distribution instantly per section 3 immutability rules.
3. Monitor support intake (issue templates per `docs/07-ops/03-runbook.md`) for error id clusters
   that hint at platform drift (`R01`) and route any scope or leak report through the incident
   response procedure immediately.
4. Update the support matrix and the approval record with the outcome; a silent divergence between
   docs and shipped behavior is a blocker for the next gate.

## 6. Explicit deferrals

- Community plugin publication (`D11`) is out of v0.1; it is evaluated in the `G4` window with a
  production manifest review and a submission dossier; until then, distribution is development
  manifests only (`R06`).
- v0.2 P1 features (watch FR20, offline bundle FR21, multi-client fairness FR22) are gated by `G3`
  evidence; they are never promised in v0.1 channels, and watch is invalidation-only, never a truth
  source (`D07`).
- Performance beyond the initial measured profile is not promised (`D12`); improvement claims
  require reproducible benchmark publication.
- Write-back and code execution are permanently out of scope for the documented v0.1 to v0.3 line
  unless a separate RFC with mutation testing and permissions lands (`D03`, FR24, PRD 7.1).

## 7. Open questions

1. Cryptographic signing coverage at v0.1 (macOS notarization, Windows Authenticode) depends on
   available certificates and runner budget; the artifact set section stays checksum-and-provenance
   only until that is decided.
2. Whether the evidence bundle is published in-repo, as release assets, or both; interacts with the
   immutable-archive rule and repo size limits.
3. Naming convention for archives and checksum files to be frozen at `G2` (referenced as
   illustrative in `docs/07-ops/02-end-user-install.md`).
