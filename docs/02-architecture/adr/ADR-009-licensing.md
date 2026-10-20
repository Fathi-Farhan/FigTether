# ADR-009: Dual licensing as MIT OR Apache-2.0

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Proposed   |
| Created              | 2026-10-20 |
| Related decision IDs | D10, D11   |
| Supersedes           | -          |

## Context

The project is a fresh Rust implementation with public contracts; `figma-mcp-free` (`S9`) is an
architecture reference, not a runtime dependency. Copying code or fixtures from any reference
requires per-file license checks, attribution, and provenance, and a language rewrite does not
erase those duties (8.11 opening paragraph). The product makes no claim of official Figma
affiliation (8.11), and marketing claims stay measured (`P14`, `AT16`). `D10` proposes the name
FigTether and a dual license but marks both as pending checks: final name and trademark review,
dependency license review, and third-party attribution completeness. Community maintainability is
a PRD goal (`G05`), so the license must be frictionless for contributors, redistributors, and the
plugin ecosystems listed in 8.11.

## Decision

1. All project-owned source (Rust crates, the TypeScript plugin, schemas, fixtures, and the docs
   set) is distributed as `MIT OR Apache-2.0` (SPDX expression). `LICENSE-MIT` and
   `LICENSE-APACHE` ship in the repository and in release bundles; a `NOTICE` accumulates
   required third-party attributions.
2. SPDX headers carry the expression in new source files; CI-level packaging checks record the
   license on `Cargo.toml`, the plugin manifest files, and bundled artifacts, alongside the SBOM
   duties of 8.11.
3. Documentation in `docs/` follows the same expression unless embedded third-party material
   carries its own license; that material is quoted with attribution, excluded, or re-documented
   (8.11 per-file rule).
4. No affiliation or endorsement claims: naming, README, website, and pack text avoid any
   statement of Figma partnership or approval (8.11, `AT16`).
5. The name FigTether stays provisional until the `D10` checks close; a rename releases as a
   coordinated docs and branding change, mechanically cheap while decisions stay pre-release.
6. Material copied from `S9` or any other reference enters only with a per-file license review
   recorded in provenance notes (8.11; Appendix C evidence template).

## Rationale

`MIT OR Apache-2.0` is the standard dual license of the Rust ecosystem: MIT grants the simplest
distribution and embedding terms, while Apache-2.0 adds an express patent grant and defined
contribution terms, which matters for a tool whose integrations touch unrelated vendor APIs.
MIT alone loses the patent grant; Apache-2.0 alone is workable but longer and less familiar to
some plugin and doc contributors. GPL-family terms complicate the packaging channels 8.11
anticipates (bundles, package managers, Community publication), and proprietary or unlicensed
staging contradicts `G05` and the public-contract premise of the whole doc set.

## Consequences

- Positive: frictionless contribution and redistribution; a clear attribution path for any copied
  content; compatibility with the dominant Rust licensing practice and tooling.
- Negative: the name and trademark validation is still open (`D10`), so a rebrand could force a
  coordinated rename across lockfiles, manifests, and documentation.
- Neutral: LICENSE, NOTICE, and SBOM ship in every release bundle; license questions route to the
  release maintainer role defined in 8.11.

## Alternatives considered

- MIT only: rejected; no patent grant and no explicit contribution-term baseline.
- Apache-2.0 only: acceptable fallback if dual tooling creates friction; only via a new ADR plus
  release review, decided no later than G2.
- GPL-family licenses: rejected; embedding and redistribution contexts that 8.11 planning assumes
  (bundled plugin, package channels) become legally awkward for downstream adopters.
- Proprietary or unlicensed source: rejected; contradicts `G05` (community maintainability), the
  public-contract premise of 8.11 versioning, and the evidence-first release culture of this doc
  set.

## Verification

Evidence: G2 release review shows both LICENSE files, NOTICE with third-party attributions, SBOM
with a dependency license audit (`Cargo.lock` and frontend lockfile pins per 8.11), and a passing
claim-language review of README and website (`AT16`). `D10` closure additionally requires: a
recorded trademark search outcome for the final name, sign-off on the dependency license review
(notably the `rmcp` stack per ADR-003, plus tokio, serde, schemars), and attribution completeness
for any copied documentation or harness material (ADR-012).
AT refs: AT16 (claim language), AT03 (packaging experience of the licensed artifacts).
Gate: G2 for the v0.1 artifact set; G4 for final name and Community publication checks (`D11`,
`D10` closure).

## References

- PRD 8.10.2 (license completeness in DoD), 8.11 (open-source strategy and repository hygiene),
  Appendix A (`AT16`), Appendix C (`S9`, evidence template).
- `D10`, `D11`, `G05`; interpretations row "Pricing/claims" in
  `docs/00-overview/01-prd-analysis.md` section 2.
- ADR-003 (dependencies under license review), ADR-012 (third-party attribution of the harness
  pack), `docs/07-ops/` (release process evidence slots).
