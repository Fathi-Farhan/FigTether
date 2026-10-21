# Research: deterministic review of AI-generated UI and UX

Research cutoff: 21 October 2026. This is a focused primary-source literature review and
implementation proposal, not a systematic review or a validated authorship classifier.

## Finding

Deterministic rules can identify specified accessibility failures, layout defects, design-token
drift, and recurring visual patterns. The reviewed evidence does **not** establish that these
patterns determine whether a UI was made by AI. Human and AI work can share the same pixels,
DOM, and usability defects. A function of identical observations cannot distinguish their
creation histories; this is a scope argument, not an empirical detection-accuracy result.

The practical deliverable is a reproducible **quality and pattern detector**. It measures
observable properties, separates standards from taste, and abstains when evidence is missing.
An authorship detector would require a separate labeled study and a defensible threat model.

## Search and selection

Searches covered AI-generated interface evaluation, front-end generation benchmarks,
UI authorship detection, accessibility test formats, provenance, and detector dataset bias.
Primary papers, W3C specifications, original research summaries, and official technical
specifications were prioritized. The full-text methods and limitations of Design2Code,
DesignBench, WebGen-Bench, and the UX prototype study were inspected. The search did not
produce a validated universal deterministic UI-authorship rule set. That search result is
not proof that no relevant detector exists.

The sources below address different constructs. Image-forensics or text-detection evidence
must not be presented as direct validation of a UI classifier. Likewise, automated WCAG
checks do not measure visual originality or task suitability.

## What the evidence supports

### Visual fidelity has several dimensions

Design2Code uses reference webpages, element-level matching, and human comparisons. Its
analysis finds discrepancies between automatic metrics and human judgments and recommends
considering both high-level appearance and detailed elements. This supports separate checks
for content, geometry, and rendering rather than treating a screenshot similarity number as
complete design quality. It evaluates reproduction, not AI-authorship detection.
[Design2Code, v3](https://arxiv.org/html/2403.03163v3).

DesignBench evaluates generation, editing, and repair across frameworks. Its visual measures
include CLIP and SSIM, while code measures include compilation and modification similarity;
some evaluation uses model judgments checked against human annotation. These measures are
task-specific. Learned embeddings and model judgments are not deterministic design rules
merely because their final output is numerical. Pinning a runtime does not validate what a
metric measures. [DesignBench, v3](https://arxiv.org/html/2506.06251v3).

### Appearance and successful interaction are separate

WebGen-Bench constructs requirement-linked interaction tests and evaluates website appearance
separately. Its operation-and-expected-result approach motivates concrete task checks: can a
user obtain a pairing secret, understand the current state, grant the intended frame, review a
plan, and recover from rejection? Static CSS or screenshots cannot establish those outcomes.
We borrow this separation, not its published scores or model-based evaluator.
[WebGen-Bench, v2](https://arxiv.org/html/2505.03733v2).

The 2026 study _Usable but Conventional_ used 92 participants and ten prototypes, with
authorship hidden during evaluation. Both human and generated prototypes appeared among
stronger and weaker evaluations. Pragmatic usefulness and hedonic qualities such as originality
did not move together. Its small prototype set and survey setting limit generalization; it
does not justify labeling a whole style as AI-generated. The useful inference is to evaluate
organization, task flow, and differentiation separately.
[Study and discussion](https://arxiv.org/html/2605.15124v1).

### Usability principles need task evidence

Nielsen Norman Group's heuristics motivate checks for visible system status, consistency,
error recovery, and recognition. A deterministic checker can verify that a captured error
state has certain controls or labels, but whether those labels help a user recover depends
on the task and requires interaction review. Its aesthetic-usability discussion also explains
why visual polish should not be treated as evidence of successful operation.
[Usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/),
[aesthetic-usability effect](https://www.nngroup.com/articles/aesthetic-usability-effect/).

### Standards provide bounded, testable thresholds

WCAG 2.2 defines text contrast of at least 4.5:1 for ordinary text and 3:1 for qualifying large
text, with exceptions. Large text means at least 18 pt, or 14 pt bold: 24 CSS px and approximately
18.667 CSS px bold. A 14 px bold label does not qualify. Do not round a measured ratio upward
before testing the threshold. Solid-color measurements do not resolve text over images or
complex compositing. [SC 1.4.3 explanation](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).

Meaningful non-text visual information has a 3:1 contrast requirement under SC 1.4.11, with
defined exceptions. The checker requires an explicit meaningful-graphic observation and
adjacent colors; it does not assume every border or decorative rule is a required cue.
[SC 1.4.11 explanation](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html).

SC 2.5.8 uses a 24 by 24 CSS-pixel minimum with spacing, equivalent-control, inline,
user-agent, and essential exceptions. A blanket 44 px requirement would confuse this AA
criterion with a stronger target or project preference. Small controls are not automatically
failures: spacing and applicability matter. The implementation checks captured rectangles
and requires explicit evidence for non-geometric exceptions.
[SC 2.5.8 explanation](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html).

SC 1.4.10 addresses reflow at a width equivalent to 320 CSS px, with exceptions for content
whose meaning requires two dimensions. Horizontal overflow observed at 420 px is a useful
layout defect signal, but is not alone a complete WCAG reflow determination. Exception and
viewport coverage must be recorded.
[SC 1.4.10 explanation](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html).

Name/role/value and visible keyboard focus are separate obligations. A Figma layer name is
not a computed accessibility name. A CSS outline declaration does not prove that keyboard
users can reach a control, see it, or operate it. The evaluator checks supplied browser AX
names and leaves keyboard operation and focus appearance to an interaction review.
[SC 4.1.2](https://www.w3.org/WAI/WCAG22/Understanding/name-role-value.html),
[SC 2.4.7](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html).

The normative authority is [WCAG 2.2](https://www.w3.org/TR/WCAG22/); the Understanding pages
explain it. This tool implements limited standard-derived checks, not a conformance audit.
W3C explicitly states that tools alone cannot determine accessibility; knowledgeable human
evaluation remains necessary. [WAI evaluation overview](https://www.w3.org/WAI/test-evaluate/).

ACT Rules Format 1.1 distinguishes applicability, expectations, assumptions, and test cases,
with passed, failed, inapplicable, cantTell, and untested outcomes. These distinctions inform
the report format. This implementation is not represented as an approved ACT rule or an
ACT-consistent implementation. Its additional **review** status identifies local heuristics.
[ACT Rules Format 1.1](https://www.w3.org/TR/act-rules-format-1.1/).

### Provenance and forensic detection answer different questions

C2PA describes content provenance, signed assertions, and bindings to assets. Verified
credentials can support a statement about a recorded process under a trust model; absence
of credentials does not establish human creation. A CSS pattern is not a substitute for that
record. The current evaluator neither validates C2PA manifests nor authenticates authorship.
[C2PA 2.2 specification](https://spec.c2pa.org/specifications/specifications/2.2/specs/C2PA_Specification.html).

_Fake or JPEG?_ shows that image detectors can learn compression and size biases from a
dataset. This is evidence about generated-image detection, not UI classification. Its relevant
methodological lesson is to control for browser, screenshot size, compression, template,
framework, and domain before interpreting a supposed UI detector's accuracy.
[Paper, v2](https://arxiv.org/abs/2403.17608).

## Rule architecture

1. **Standard-derived observations:** contrast, supplied computed names, target geometry,
   and overflow under explicitly declared applicability. Each has an observation boundary.
2. **Project contracts:** declared type sizes, radii, and permitted decorative treatments.
   These are configurable requirements, not universal aesthetics.
3. **Review signals:** repeated same-sized rounded surfaces, pill-heavy controls, and a
   configurable vocabulary of generic marketing phrases. Counts are deterministic; their
   interpretation is contextual. A human-designed dashboard may trigger them legitimately.
4. **Unmeasured UX:** task completion, error recovery, information hierarchy, trust, originality,
   focus visibility, keyboard operation, and responsive states not captured in the input.

No category produces an AI probability. A geometric or contrast failure remains the same
failure when the input's self-reported author changes. No-finding output does not become
“human-made,” “beautiful,” or “accessible.”

## Determinism contract

Within a fixed Node/V8 runtime, the evaluator is a pure function of the versioned snapshot,
profile, and rule implementation.
It makes no model or network calls and adds no current timestamp. It canonicalizes object
keys and sorts node/result identifiers using code-point order, not locale collation. The report
records SHA-256 digests of the input, profile, and rule code. Collection metadata is input
evidence; collection itself may vary with fonts, browser versions, theme, viewport, and state.

Malformed numbers, duplicate node IDs, unsupported schema versions, and invalid profiles
are rejected rather than converted into passes. Unknown colors, uncomputed names, missing
measurements, incomplete captures, and unsupported effects produce explicit uncertainty.

## Validation design and limits

Byte-identical repeatability is checked within the recorded runtime. Cross-runtime floating-point
equivalence is not certified. The browser run manifest records the Node and browser versions;
standalone CLI users should retain `node --version` alongside their report.

The included tests cover threshold boundaries, large-text classification, small-target
spacing, missing data, partial capture, profile opt-in, stable ordering, byte-identical repeated
runs, and author-label invariance. These establish selected implementation invariants; they
do not establish general usability or an AI-detector accuracy rate.

A future authorship study would need consented, provenance-labeled human, AI, and mixed
examples; matched tasks and tools; template- and generator-disjoint held-out sets; screenshot
normalization; false-positive and false-negative rates with uncertainty; and adversarial
edits. User studies would separately measure task success, recovery, time, and perceived
originality. Until such work exists, no sensitivity, specificity, or AI-detection accuracy is
claimed for this tool.

For practical use, run several widths, both supported themes, and critical workflow states.
Keep raw evidence local when it includes design content. Publish aggregate findings with
artifact hashes and explicit omissions. The initial FigTether exercise is an engineering
smoke test of this workflow, not validation on a representative UI corpus.

## Research merge: 21 October 2026

The user supplied a three-document research collection covering skill engineering and thirty
proposed UI checks. The following additions were checked against original sources. These are
workflow extensions; the fourteen-rule static evaluator and its published results are unchanged.
The repository's documentation review contains the full rule-by-rule comparison.

### Skills need local evaluation and proportionate procedures

SkillsBench v4 reports matched with/without-skill evaluations over 87 tasks and 18 configurations,
with average pass rates of 33.9% and 50.5%. This supports paired evaluation, not a guaranteed
benefit for this skill, nor a universal maximum of three supporting files.
[SkillsBench, v4](https://arxiv.org/abs/2602.12670v4).

Agent Skill Evolution separates rule revisions from loading the whole skill body. Its reported
compliance gains concern added actionable rules; the study does not isolate Markdown backticks
as the causal mechanism. Avoid treating every prose step as defective without a backticked
command. [Agent Skill Evolution, v1](https://arxiv.org/html/2610.04832v1).

SkillTriage attributes functional and efficiency regressions using paired runs. Its finding
that excessive verification and heavy pipelines contribute to regressions supports a fast path
for small changes and checks proportional to the affected behavior. These findings do not
establish that the revised Design Evidence skill is faster; that remains unmeasured.
[SkillTriage, v1](https://arxiv.org/abs/2608.11888v1).

From Registry to Repository documents one-time copying and limited subsequent maintenance.
Retain source/version hashes and check local bindings when copying a skill; a hash match
establishes synchronization, not effectiveness or security.
[Registry study, v2](https://arxiv.org/abs/2607.00911v2).

### Expected behavior is a separate test layer

VISTA links design requirements to component localization and behavioral probes. Its 90.1%
figure concerns audited Web deliveries containing unresponsive controls, not the proportion
of all controls that were unresponsive. Borrow requirement-linked probes without claiming
the same failure prevalence or improvement on FigTether.
[VISTA, v4](https://arxiv.org/abs/2605.26144v4).

Interaction2Code studies dynamic interactions; FrontendBench uses prompt/test-case pairs;
MRWeb tracks resources and navigation. Together they motivate explicit initial states,
actions, expected results, route/resource inventories, and post-action checks. A DOM-change
predicate is too weak to certify a specific requirement. The practical contracts are in
[interaction review](interaction-review.md).
[Interaction2Code, v3](https://arxiv.org/abs/2411.03292v3),
[FrontendBench, v2](https://arxiv.org/abs/2506.13832v2),
[MRWeb, v1](https://arxiv.org/abs/2412.15310v1).

ReDeCheck's original implementation detects five classes of responsive layout failures.
Its context-sensitive layout comparisons motivate checking breakpoint neighborhoods and
retaining screenshots. A few sampled browser widths do not reproduce its full method or
VizAssert's formal verification. Intentional overlaps and scroll regions need applicability
decisions. [ReDeCheck implementation](https://github.com/redecheck/redecheck).

### Keep training mutations, scores, and standards distinct

UIClip uses synthetic jitter transformations for training a learned model. A transformation
that deliberately degrades an example is not an independently validated inverse detector.
It does not establish universal spacing lattices, palette tolerances, or a requirement that
every panel have a border, shadow, or contrasting background.
[UIClip, section 3.1.2](https://arxiv.org/html/2404.12500v1#S3.SS1.SSS2).

SchemaGUI uses controlled schemas and reference geometry, rather than a universal quality
oracle. Portal UX Agent combines automatic checks with model judgments; its aggregate rating
must not be called a deterministic score. Adopt bounded contracts where useful without
importing either benchmark's scores as a FigTether result.
[SchemaGUI, v1](https://arxiv.org/html/2608.22390v1),
[Portal UX Agent, v1](https://arxiv.org/html/2511.00843v1).

Synthetic Heuristic Evaluation reports both useful issue discovery and false alarms. Its
43/49 and 52/55 fractions describe which severity-zero issues came from synthetic evaluation;
they are not false-positive fractions among all synthetic findings. The study does not
compare a deterministic-rule intervention against a no-rule baseline.
[Synthetic Heuristic Evaluation, section 4.1.3](https://arxiv.org/html/2507.02306v1).

SlopCodeBench measures iterative code degradation, not UI color or CSS-style authorship.
Anti-Slop UI is a separate author-published heuristic framework; mixing it into the same
citation does not transfer empirical validation to aesthetic blacklists.
[SlopCodeBench, v2](https://arxiv.org/abs/2603.24755v2),
[Anti-Slop UI author manuscript](https://github.com/local-over/Anti-Slop-UI/blob/main/research_paper.md).

Retain WCAG applicability and exceptions. The 44 by 44 CSS-pixel target criterion is SC 2.5.5
at AAA, not a mobile-only extension of AA. A project can choose 44 px targets explicitly.
[SC 2.5.5 explanation](https://www.w3.org/WAI/WCAG22/Understanding/target-size-enhanced.html).
