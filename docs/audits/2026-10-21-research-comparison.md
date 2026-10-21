# Research comparison and integration

Reviewed 21 October 2026. The two collections are complementary: the user-provided research
adds skill maintenance, reference-based layout analysis, and interaction/resource testing;
Design Evidence supplies an executable, bounded evaluator with explicit uncertainty.
The merge adopts the useful workflow ideas and qualifies unsupported generalizations.

## Scope and method

Read all three supplied Markdown files:

- `00_MASTER_INDEX_AND_CITATION_CROSSCHECK.md`
- `01_AI_AGENT_SKILLS_DESIGN_EMPIRICAL_REFERENCE.md`
- `02_DETERMINISTIC_DETECTOR_RULES_UI_UX_DESIGN.md`

Compared their S1-S12 and U1-U30 proposals with the [skill](../../.agents/skills/design-evidence/SKILL.md),
[research](../../.agents/skills/design-evidence/references/research.md),
[fourteen implemented rules](../../.agents/skills/design-evidence/references/rules.md),
and [browser evidence](../../benchmarks/published/design-evidence-2026-10-21.json).
Input fingerprints and the scoped source-check ledger accompany this report in
[the comparison record](2026-10-21-research-comparison.json).

Checked the identity/abstract of all 22 cited arXiv entries, original VizAssert and ReDeCheck
publication/artifact pages, and the separate Anti-Slop UI manuscript. Examined selected full-text
methods/tables for consequential claims. This is a targeted cross-check, not a replication or
line-by-line certification of every paper or numerical claim. The supplied labels "100% Verified"
are not adopted as verification evidence. An arXiv listing alone does not establish peer review.
The supplied three files are preserved unchanged.

## What each collection contributes

| Area            | Supplied research                                                            | Existing Design Evidence                                          | Integrated decision                                                            |
| --------------- | ---------------------------------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Skill authoring | Twelve proposed rules; modularity, maintenance, verification and regressions | Small entrypoint, references/scripts, explicit user scope         | Add proportionate checks, CLI outcomes/recovery, version and cost records      |
| Geometry        | Collision, containment, alignment, clipping and breakpoint proposals         | Captured overflow and target geometry with exceptions             | Document broader review coverage; do not claim these detectors are implemented |
| Interaction     | Component requirements, routes, state transitions and recovery               | Narrow help-opening browser probe; broader UX explicitly untested | Add requirement-specific interaction contracts and FigTether cases             |
| Aesthetics      | Fixed spacing, typography, palette and gradient rules                        | Opt-in project contracts and review signals                       | Keep values configurable; no universal style blacklist                         |
| Validity        | Strong "executable" and "production verified" language                       | Missing evidence stays unknown/untested; no authorship verdict    | Preserve bounded results and distinguish pseudocode from executable checks     |
| Efficacy        | Published effects from other tasks/models                                    | Local repeatability evidence only                                 | Use matched trials before claiming this skill saves tokens, time or cost       |

## Source corrections that affect adoption

1. **R12, SchemaGUI:** the supplied 820-instance description does not match the inspected v1
   evaluation setup: section 4.2 reports 1,000 instances per scenario/language, six scenarios,
   and 12,000 bilingual instances per evaluated model. Table 1 assigns EN geometry 77.37 to
   Qwen3.5-27B and 79.09 to DeepSeek-R1; the supplied table assigns 79.09 to Qwen3.5-27B.
   Its EN Qwen3-Coder geometry is 72.98, not 68.52. Import the evaluation dimensions, not
   the supplied score table. [SchemaGUI v1, sections 4.2 and 5.1](https://arxiv.org/html/2608.22390v1).
2. **R23, denominator error:** 43 of 49 rental-app severity-zero issues and 52 of 55
   language-app severity-zero issues were attributed to synthetic evaluation. These
   denominators count non-issues from the combined set, not all synthetic findings.
   Consequently 87.8% and 94.5% are not the claimed false-positive rates. The study examines
   two apps and also finds useful issue coverage; it does not prove that adding deterministic
   rules eliminates hallucination. [Study v1, section 4.1.3](https://arxiv.org/html/2507.02306v1).
3. **R2, actionability versus formatting:** reported gains concern added actionable rules,
   often naming previously absent tools/paths. They do not isolate backticks as a causal
   treatment. The +0.41 abstract result concerns 16 open-weight models; correctness +0.10
   concerns three assessed agents. Skill loading overhead and revision overhead are different
   comparisons. [Agent Skill Evolution v1, abstract and sections 5-6](https://arxiv.org/html/2610.04832v1).
4. **R13, mutation versus detection:** UIClip's jitter functions introduce training defects;
   they are not six independently validated deterministic detectors. Its learned evaluation
   does not certify arbitrary 2 px alignment or 4/8 px spacing cutoffs.
   [UIClip v1, section 3.1.2](https://arxiv.org/html/2404.12500v1).
5. **R14, mixed evaluation:** the AutoScore formula is automatic, but the reported 4.422/5
   aggregate incorporates rubric-based judgments. It is not the deterministic AutoScore on
   a five-point scale. [Portal UX Agent v1, sections IV-C, VI-C and VII](https://arxiv.org/html/2511.00843v1).
6. **R21, bibliography:** the five-failure taxonomy and original tool are supported by the
   ReDeCheck repository and ISSTA 2017 work. Do not attribute that artifact simply to
   "ICST 2015" or merge the regression-detection and failure-detection papers into one entry.
   [Author publication page](https://www.gregorykapfhammer.com/research/papers/walsh2017/),
   [original tool](https://github.com/redecheck/redecheck).
7. **R24, conflated sources/versions:** SlopCodeBench v1 reports 11 models and 80% erosion;
   v2 reports 15 coding agents and 77%. These are version-specific experiments, not an
   uncertainty interval. Neither validates a CSS duplication cutoff or color blacklist.
   The separate heuristic manuscript is by Hassan Elkady, not the supplied "M. Elkady";
   its accessed title differs from the supplied title. No independent peer-review status
   was established for it. [SlopCodeBench v1](https://arxiv.org/abs/2603.24755v1),
   [v2](https://arxiv.org/abs/2603.24755v2),
   [Anti-Slop UI manuscript](https://github.com/local-over/Anti-Slop-UI/blob/main/research_paper.md).
8. **U16/U27, standards:** large text includes 18 pt regular or 14 pt bold; meaningful
   non-text contrast is a separate criterion. AA target size is 24 CSS px with spacing and
   other exceptions. The 44 CSS px criterion is AAA, not an automatic mobile AA requirement.
   [Text contrast](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html),
   [AA targets](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html),
   [AAA targets](https://www.w3.org/WAI/WCAG22/Understanding/target-size-enhanced.html).

Other checked sources support useful directions: SkillsBench supports paired evaluation;
SkillTriage warns about excessive procedure; VISTA motivates behavioral checks; MRWeb supports
resource inventories. Their measured improvements/failures belong to their own experimental
conditions and are not FigTether benchmark results.
[SkillsBench v4](https://arxiv.org/abs/2602.12670v4),
[SkillTriage v1](https://arxiv.org/abs/2608.11888v1),
[VISTA v4](https://arxiv.org/abs/2605.26144v4),
[MRWeb v1](https://arxiv.org/abs/2412.15310v1).

## Disposition of all twelve skill proposals

"Adopt" below means guidance incorporated or retained; it does not mean a new automated
skill linter exists. The reference IDs are local to the supplied collection.

| ID  | Decision        | Rationale and concrete treatment                                                                                                     |
| --- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| S1  | Adapt           | Keep valid frontmatter and a discriminating trigger; exact `Use when` prefix and 20-60 words are conventions, not quality guarantees |
| S2  | Adapt           | Use actual environment discovery; this skill is already discoverable. No redundant AGENTS.md registration required                   |
| S3  | Adapt           | Use concrete commands/paths where needed; do not force a CLI token into every design decision                                        |
| S4  | Adapt           | Make true invariants explicit; contextual exceptions are useful and must not be erased by a modal-word blacklist                     |
| S5  | Adapt           | Preserve meaningful workflow order without forcing numbering for every small edit                                                    |
| S6  | Adopt           | Explain CLI exit codes, invalid-input recovery, and missing-observation handling                                                     |
| S7  | Adapt           | Run relevant verification; zero exit does not remove review/unknown findings or certify production readiness                         |
| S8  | Adapt           | Keep progressive disclosure; no universal 300-line/6 KB/three-file cap inferred from aggregate studies                               |
| S9  | Adopt           | Retain explicit user/task precedence and existing authorization boundaries                                                           |
| S10 | Adopt           | Add small-change fast path and avoid repeated passing checks without new evidence                                                    |
| S11 | Adopt principle | Validate local links and runnable bindings with appropriate tools; Python-only regex/compile is unsuitable for this JS skill         |
| S12 | Adapt           | Review executable behavior and permissions; lexical bans alone neither establish nor certify security                                |

## Disposition of all thirty UI proposals

"Workflow" is documented review guidance, not implementation in `evaluate.mjs`.
"Proposed" means additional data, fixtures or code is still required. No new U1-U30
identifiers are emitted by the current fourteen-rule evaluator.

| ID  | Status after merge                | Decision and boundary                                                                                                              |
| --- | --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| U1  | Partial existing                  | Snapshot/profile validation exists; generated UI schema validation is a separate proposed adapter                                  |
| U2  | Proposed                          | Unique snapshot IDs exist; parent/data-binding graph closure and cycle detection are not implemented                               |
| U3  | Workflow                          | Map required components to observed results; arbitrary F1 >= 0.99 can hide a critical missing requirement                          |
| U4  | Workflow                          | Inventory routes/assets and verify expected destination/content; HTTP 200 alone is insufficient                                    |
| U5  | Workflow                          | Require local/offline closure only when specified by the product brief                                                             |
| U6  | Proposed                          | Overlap needs intended-layout applicability; 1% is an unvalidated local tolerance, not a universal defect rule                     |
| U7  | Proposed                          | Parent overflow can be intentional; include clipping/scrolling/positioning semantics before failing                                |
| U8  | Existing subset                   | L001 captures horizontal overflow with declared reflow applicability; limited widths are not exhaustive coverage                   |
| U9  | Workflow; detector proposed       | Review breakpoint boundaries; a 20 px step cannot guarantee finding glitches narrower than 20 px                                   |
| U10 | Proposed                          | Wrapping is a defect only for a declared single-row requirement; 768 px and 2 px are project choices                               |
| U11 | Proposed                          | Leaf scroll metrics alone miss or misclassify inline text, line clamping and intentional clipping                                  |
| U12 | Proposed contract                 | Alignment groups, axis and tolerance must be declared; no universal 2 px standard                                                  |
| U13 | Proposed contract                 | Unequal gaps can encode grouping; require an intended uniform-gap group                                                            |
| U14 | Optional preference               | A project may declare spacing tokens; 13 px/19 px are not inherently defective or AI-generated                                     |
| U15 | Proposed review                   | Reading direction, multicolumn layout and semantic order need context; tau >= 0.90 is not WCAG certification                       |
| U16 | Existing subset                   | A001/A004 retain corrected text sizes, non-text applicability, compositing uncertainty and exceptions                              |
| U17 | Proposed contract                 | Palette matching requires a reference palette; DeltaE <= 5 is a chosen tolerance, not an authorship rule                           |
| U18 | Optional preference               | Semantic heading hierarchy and visual size are distinct; strict heading-size inequalities are not universal                        |
| U19 | Optional preference               | Font counts, line height and measure may be configured; the cited studies do not validate these exact caps                         |
| U20 | Decline universal gate            | Space, alignment and typography can establish grouping without border, shadow or contrasting fill                                  |
| U21 | Workflow                          | Test required outcome with a resettable fixture; any DOM/state/URL change is an inadequate success oracle                          |
| U22 | Workflow                          | Record errors per case; classify expected validation and infrastructure failures rather than suppressing them                      |
| U23 | Existing narrow subset + workflow | Expanded-help capture exists; broader post-action layout checks require explicit cases                                             |
| U24 | Workflow                          | Check promised reversal/reset and derived state; asynchronous completion need not be synchronous                                   |
| U25 | Workflow                          | Declare allowed shared effects; a correct action may update a global status or accessibility announcement                          |
| U26 | Proposed tooling                  | axe-core is not integrated here; severity gating is not complete WCAG conformance and cannot import A11yn's gains                  |
| U27 | Existing subset                   | A003 checks 24 px and spacing/exceptions; 44 px remains a separate AAA or project choice                                           |
| U28 | Workflow                          | Existing help Enter probe is narrow; tab order, focus visibility, modal containment and return require more evidence               |
| U29 | Split                             | Configurable copy/decorative review exists; broken-media detection is proposed, and gradient color is not authorship proof         |
| U30 | Decline universal gate            | Repeated generated styles can be legitimate; unused CSS may target other states/routes. Review multi-state coverage before removal |

## Concrete merge and remaining evidence

Updated the skill entrypoint with proportionate checks, exit-code recovery, interaction
routing, and performance/cost reporting. Added a reusable
[interaction review reference](../../.agents/skills/design-evidence/references/interaction-review.md)
with explicit oracles and five FigTether cases. Extended the
[primary-source review](../../.agents/skills/design-evidence/references/research.md#research-merge-21-october-2026)
with the verified rationale and limitations. Synchronized the installed skill with the repo copy.

The executable evaluator remains ruleset 1.0.0 with fourteen checks. Its earlier eight captures,
800 deterministic repeats, Figma timings, and model-cost omissions remain dated evidence.
They are not reclassified as evidence for all thirty proposals. This merge performs no new
native Figma test or design write, no model efficacy trial, and no full axe/keyboard audit.

For future validation, add failing and intentionally valid fixtures before promoting a
proposal into a blocking rule. Test unknown/incomplete observations and justified exceptions.
Compare skill-guided and baseline runs on matched tasks; preserve failed attempts and record
time, workload, model/harness, tokens and billed cost when observable. Report originality and
task suitability through separate human/task evidence. The result remains a quality-review
workflow; no AI-authorship probability or universal beauty score is supported.
