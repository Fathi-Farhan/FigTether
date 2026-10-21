# FigTether Source Register

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

This register fixes which external sources FigTether documentation and design may cite. Section 1
reproduces the primary-source register of the PRD Appendix C verbatim (`S1..S11`). Section 2 adds the
upstream Harness2 register (`HS1..HS5`) used by the `08-harness2` docs. Section 3 carries the upstream
citation block. Section 4 states the rules for adding sources here. The register exists so every claim
in this docs set can point at a named source with a retrieval date, and so nothing upstream is silently
restated as a FigTether fact.

## 1. Figma sources (PRD Appendix C, `S1..S11`)

Reproduced from the PRD Appendix C table; register preamble also rendered from the PRD: all sources
were accessed on 20 October 2026. Online documentation can change, so release evidence must record the
dates and versions used and the commit under test. Documentation citations in the PRD are design
inputs, not product benchmark results, and none of these pages promises any FigTether capability.

| ID  | Source                                                                                                     | Purpose                                                            |
| --- | ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| S1  | [Figma MCP - Rate limits & access](https://developers.figma.com/docs/figma-mcp-server/rate-limits-access/) | Official service limits and access rights                          |
| S2  | [Figma - Use plugins in files](https://help.figma.com/hc/en-us/articles/360042532714-Use-plugins-in-files) | Plugin eligibility, lifecycle, and usage limits                    |
| S3  | [Figma - How Plugins Run](https://developers.figma.com/docs/plugins/how-plugins-run/)                      | JavaScript runtime, sandbox, and UI iframe                         |
| S4  | [Figma - exportAsync](https://developers.figma.com/docs/plugins/api/properties/nodes-exportasync/)         | `JSON_REST_V1`, PNG, and SVG                                       |
| S5  | [Figma - Plugin Manifest](https://developers.figma.com/docs/plugins/manifest/)                             | `dynamic-page`, `editorType`, and `networkAccess`                  |
| S6  | [Figma - on events](https://developers.figma.com/docs/plugins/api/properties/figma-on/)                    | Event behavior, global loading, and coverage gaps                  |
| S7  | [Figma - Global API](https://developers.figma.com/docs/plugins/api/figma/)                                 | `fileKey`, page loading, and lifecycle                             |
| S8  | [Official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk)                                  | Rust MCP implementation and compatibility                          |
| S9  | [figma-mcp-free repository](https://github.com/superdoccimo/figma-mcp-free)                                | Reference for the two read paths and the original product's limits |
| S10 | [Figma - Variables API](https://developers.figma.com/docs/plugins/api/figma-variables/)                    | Probe the variable capability; do not assume REST parity           |
| S11 | [Figma - Debugging](https://developers.figma.com/docs/plugins/debugging/)                                  | Performance testing in the normal runtime                          |

Per the PRD, design authority stays with the PRD itself: these pages anchor assumptions and
documentation facts (PRD section 3), and every one of them is re-verified at `G0` on the exact Figma
release under test before it is treated as confirmed platform behavior.

## 2. Harness2 upstream sources (`HS1..HS5`)

Registered to serve `docs/08-harness2/`. All five sources were retrieved on `2026-10-20`.

| ID  | Source                                                                                                        | Purpose                                                                                                                                                                                                                       |
| --- | ------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| HS1 | [Harness2 repository README](https://github.com/google-research/harness2)                                     | Upstream definitions of harness and harnessing, the two recursion levels (task-level and harnessing-level), the eight editable components, install and benchmark entry points, and the upstream citation and license sections |
| HS2 | [BENCHMARKS.md (upstream)](https://github.com/google-research/harness2/blob/main/docs/BENCHMARKS.md)          | Upstream benchmark guides for JobBench, LAB, and WorkBuddy-Bench: data, domains, grading, and integrity-checked downloads                                                                                                     |
| HS3 | [EXPERIMENTS.md (upstream)](https://github.com/google-research/harness2/blob/main/docs/EXPERIMENTS.md)        | Upstream experiment guide: recursion modes and budgets, task selection and cost, reproducibility, web access restrictions, results and resume                                                                                 |
| HS4 | [SETUP.md (upstream)](https://github.com/google-research/harness2/blob/main/docs/SETUP.md)                    | Upstream setup guide: requirements, wheel installations, model configuration and authentication, and the Codex executor                                                                                                       |
| HS5 | [Harness2: Recursive Agent Harnessing for an Open World](https://arxiv.org/abs/2610.14224) (arXiv:2610.14224) | The paper defining Propose-Probe-Compose, task-level and harnessing-level recursion, and the reported gains across the three benchmarks                                                                                       |

Register conditions, all of them binding on the `08-harness2` docs:

1. Retrieval date `2026-10-20`; upstream documentation and code can change at any time, so release
   evidence must record the upstream commit or document version used at retrieval, exactly as the PRD
   Appendix C rule applies to `S1..S11`. A later retrieval that finds different wording must update
   this register and say so; it must not silently reinterpret older FigTether docs.
2. License: the upstream framework is Apache-2.0. Third-party code under upstream `third_party/`, and
   the benchmarks and datasets, retain their upstream licenses per their third-party notices (upstream
   `dependencies/README.md`). FigTether applies the PRD 8.11 rule: third-party material keeps its
   license and attribution, and moving or rewriting files does not remove license obligations; if a
   file is copied, its license is checked and retained first.
3. Upstream disclaimer, quoted: "This is not an officially supported Google product." FigTether
   therefore treats Harness2 as reference material for harness engineering, not as a supported
   component, and never presents an affiliation that does not exist.
4. Upstream capability claims stay upstream facts: the reported gains (up to 11.5 points on
   WorkBuddy-Bench, 4.5 points on JobBench, 11.8 percentage points on LAB's partial pass rate) are
   results of the upstream study on upstream benchmarks, quoted only with `HS` attribution. Capability
   that FigTether has not tested itself stays labeled as hypothesis, not as a FigTether experiment
   result (PRD 8.2 and 2.2).

## 3. Upstream citation block

The BibTeX entry below is copied exactly from the upstream README citation section (`HS1`), authors
and title included as written. The upstream title carries the math markup `Harness$^2$`; this docs set
writes the name as "Harness2" everywhere in prose.

```bibtex
@article{xu2026harness2,
  title   = {Harness$^2$: Recursive Agent Harnessing for an Open World},
  author  = {Xu, Ruiyao and Chen, Yanfei and CuiZhu, Zhongying and Dalvi Mishra, Bhavana and Ming, Yifei and Yu, Han and Han, Rujun and Lee, Chen-Yu and Pfister, Tomas},
  journal = {arXiv preprint arXiv:2610.14224},
  year    = {2026},
  url     = {https://arxiv.org/abs/2610.14224}
}
```

## 4. Rules for adding sources here

1. Every new source gets a new ID in a namespace declared in this register (`S*` for Figma and platform
   sources per the PRD, `HS*` for Harness2, further namespaces declared when first used). IDs are never
   reassigned, and a renamed or moved source keeps its ID.
2. Each row records three things at minimum: retrieval date, purpose (what the source is authority
   for: platform fact, design input, upstream capability claim, or benchmark method), and license or
   attribution status. Release evidence must record the version or commit actually used.
3. Never cite an unverified claim as a FigTether fact. Anything not measured on FigTether is labeled as
   a sourced claim or hypothesis, carries its source ID, and never appears in FigTether benchmark
   tables; benchmark results that FigTether publishes come from `N01..N10` procedures with
   exact commits and separated cached, fresh, snapshot, and export lanes.
4. Prefer primary and official sources (Figma documentation, protocol SDKs, the upstream repo and
   paper). Secondary commentary may inspire design but is not authority for platform behavior.
5. A source that contradicts the PRD cannot resolve the contradiction alone: update the PRD first
   per `docs/README.md` change management, then propagate the change here.
6. Corrections, dead links, and new retrieval dates are appended as register revisions; the register
   never edits a row's past retrieval claim in place.
