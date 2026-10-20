# ADR-012: Harness2 methodology adoption for the agent-facing harness

| Field                | Value         |
| -------------------- | ------------- |
| Status               | Proposed      |
| Created              | 2026-10-20    |
| Related decision IDs | D10, D12, D03 |
| Supersedes           | -             |

## Context

FigTether's product surface is agent-facing: MCP tools, resources, prompts, and skill packs that
model clients consume. The PRD demands honest, measured claims (`P14`, `AT16`) and a maintainable
path for tooling packs (`G05`, 8.11 repository hygiene and release channels). The Harness2
methodology (Google Research, 2026) is adopted here as the working frame for the agent harness
evolution: it structures harness packs (skills, manifests, reports), keeps harness evaluation
reproducible, and stays provider-neutral. The detailed mapping lives in `docs/08-harness2/*`
(design docs authored separately in this doc set); this ADR records the adoption decision and its
boundaries. `D03` fixes the safety frame: read-only, no eval, no shell, no arbitrary code
execution anywhere in the tool chain.

## Decision

1. Methodology-only adoption: FigTether follows Harness2 practices to design, review, and evolve
   the agent-facing harness (tool descriptions, prompt and skill packs, usage runbooks,
   evaluation notes). There is no runtime dependency on the harness2 Python repository: nothing
   in the daemon, MCP adapter, CLI, or plugin imports it, shells out to it, or auto-updates from
   it (`D03` frame applied to dependencies).
2. The FigTether harness pack ships as documentation artifacts under `benchmarks/harness-pack/`: Markdown
   skill files plus a JSON manifest describing the tools, resources, and guardrails an agent can
   rely on. Skills are plain text that any MCP client can read; the pack is versioned and
   checksummed like the rest of the release artifacts (8.11 supply chain, Appendix C template).
3. Attribution: Harness2 sources are Apache-2.0. Adopted structure or text from Harness2 receives
   a `NOTICE` entry, and any copied file gets a per-file license check recorded in provenance
   notes (8.11 per-file rule; rewriting does not remove attribution duties).
4. Claims discipline: methodology adoption creates no performance or quality claims. Agent-side
   improvement is evidenced only through reproducible benchmark and evaluation artifacts that
   `D12` allows (PRD 2.2 comparison protocol; publications in `06-testing/04-benchmark-plan.md`),
   plus the two-client interoperability evidence (`AT14`, ADR-003).
5. Ownership: `docs/08-harness2/*` is authoritative for harness content and evaluation cadence;
   this ADR owns the decision to use the methodology and the no-runtime-dependency boundary.
   Shared skills for coding agents remain internal assistance, never a product claim.

## Rationale

A documented methodology beats improvised prompt-wrangling for a product whose core value is
agent behavior: stale-risk handling, projection discipline, and error responses with concrete
actions (ADR-011) only pay off when the harness teaches agents to use them. Keeping the harness
as data (Markdown plus a manifest) makes packs diffable, reviewable, and versionable without
adding a runtime surface to secure. Because Harness2 contributes methodology and formats rather
than code, FigTether keeps single-language ownership of the product (`D01`), keeps `FR17` intact,
and adds no supply chain surface beyond documents.

## Consequences

- Positive: agent guidance travels with the product and stays human-readable; evaluation is
  reproducible and claim-safe; no runtime footprint and no new update channel.
- Negative: pack content can drift from actual tool behavior unless contract docs change in the
  same PR (README change management applies to pack updates).
- Neutral: the methodology may evolve; updates arrive as documentation changes with attribution
  intact; harness work never gates core gates G0 through G2.

## Alternatives considered

- A runtime dependency on the harness2 Python repository: rejected; it adds a Python toolchain to
  releases and an update surface that conflicts with the fail-closed, no-eval, no-shell stance
  (`D03`, `FR01` prebuilt simplicity).
- No methodology, ad hoc skills: rejected; unreviewable claims and inconsistent agent behavior
  across releases, which the PRD already bans (`P14`, `R07`).
- Vendor-specific prompt packs tied to one client: rejected; `FR17` requires at least two clients,
  and the harness must not encode client-specific hacks (`R09`).
- Shipping the harness as compiled tool artifacts: rejected; the value is inspectable text and
  structure; compiled packs lose reviewability and diffability.

## Verification

Evidence: `docs/08-harness2/*` present with the methodology mapping, an attribution file, and the
pack manifest; pack files hashed in release artifacts; evaluation notes use reproducible
procedures (PRD 2.2 comparison protocol) instead of anecdotes; the claim-language review covers
pack text (`AT16`).
AT refs: AT14 (harness-neutral interoperability across two clients), AT16 (measured claims in
pack and docs).
Gate: G2 (pack ships with the v0.1 documentation set), then G3 and G4 for evaluation-driven
updates; the attribution review feeds the `D10` license checklist closure.

## References

- PRD 2.2, 4.3 (`FR17`, `FR23`), 8.5.1 (tool surface the pack describes), 8.10.2, 8.11, Appendix A
  (`AT14`, `AT16`).
- `D10` (attribution review), `D12` (claims only via reproducible benchmark), `D03` (no eval or
  shell anywhere).
- `docs/08-harness2/` (harness design and evaluation), `benchmarks/harness-pack/` (pack), ADR-003 (SDK and
  two-client evidence), ADR-009 (license files and NOTICE), ADR-011 (error actions the pack
  explains).
