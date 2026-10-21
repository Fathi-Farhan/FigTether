# Design evidence and repeatable review

The repository includes the [Design Evidence skill](../.agents/skills/design-evidence/SKILL.md)
and a [primary-source research review](../.agents/skills/design-evidence/references/research.md).
The goal is to catch observable UI defects and inconsistent design decisions, including
patterns people describe as generic or AI-looking. Appearance alone does not establish authorship.

The [research comparison](audits/2026-10-21-research-comparison.md) maps the user-provided
12 skill rules and 30 UI rules to this implementation, with source corrections and adoption
decisions. The merged [interaction review workflow](../.agents/skills/design-evidence/references/interaction-review.md)
adds requirement-specific outcomes, recovery, resources, and evidence/cost records. Those
instructions do not add executable rules to the static detector.

## What the detector does

Fourteen versioned checks cover scoped capture completeness, explicit manual-review gaps,
solid-color text contrast, supplied browser accessibility names, target size/spacing,
meaningful graphic contrast, captured horizontal overflow, declared type/radius tokens,
decoration budgets, repeated surfaces, pill-heavy controls, and configurable copy phrases.

[Rule definitions and exceptions](../.agents/skills/design-evidence/references/rules.md)
separate standard-derived checks from project contracts and review-only patterns. A gradient
is not automatically a defect. Its use can violate a project profile without proving AI generation.
Missing data produces uncertainty, not a pass. The summary never claims complete accessibility,
usability, or authorship certification.

## Run the evaluator

From the repository root, with Node.js 22 or later:

```sh
node .agents/skills/design-evidence/scripts/evaluate.mjs snapshot.json --profile .agents/skills/design-evidence/references/figtether-bridge-profile.json --out report.json
node --test .agents/skills/design-evidence/scripts/evaluate.test.mjs
```

The [input contract](../.agents/skills/design-evidence/references/rules.md#input) explains the
snapshot format. `snapshot.json` is observed input supplied by the reviewer, not a file already
included in the checkout. The evaluator makes no model or network calls. Identical normalized
input, profile, and rule code produce byte-identical reports within a fixed Node/V8 runtime,
with input/profile/code hashes. Retain the runtime version; cross-runtime bit equivalence is
not certified.

The [DOM collector](../.agents/skills/design-evidence/scripts/collect-browser.mjs) captures
rendered text, geometry, and style observations without reading input values. Naming checks
need computed browser AX evidence; heuristic labels alone do not establish accessibility.
Figma node data can be adapted to the contract, but a layer name is not an accessible name,
and a design frame is not a browser reflow test.

## Browser exercise

After building the plugin, the optional local harness renders `plugins/figma/dist/ui.html`
in an isolated headless Chrome profile, enriches observations with browser AX names, and
checks widths of 320, 420, and 960 CSS pixels in light and dark themes. At 420 px it also
opens the focused help disclosure using Enter. It enters no secret and performs no design write.

```sh
npm --prefix plugins/figma run build
node scripts/validation/design-evidence-browser.mjs
```

The harness defaults to the usual Windows Chrome installation path. Set
`DESIGN_REVIEW_BROWSER` to another local Chromium executable when needed. It saves raw
snapshots, screenshots, reports, and isolated browser profiles under ignored benchmark results.
Each default run gets a new timestamped directory and `run-summary.json`; existing evidence
is never overwritten. Optional positional arguments select a new results directory and a new
output file. Publish an explicitly reviewed summary under a new versioned filename.
The versioned [browser review record](../benchmarks/published/design-evidence-2026-10-21.json)
contains aggregate findings and local evaluator timings. Those timings are not Figma request
latency, model inference time, or user task-completion time.

The initial exercise found browser-default password-field type sizing outside the declared
profile; the field now inherits the body font. Draft runs also exposed a keyboard harness
configuration issue and closed-disclosure collection error. Their raw evidence is retained
and identified as superseded, rather than reported as verified product failures.

## Read the findings

`passed` applies only to a specific observed target and rule. `failed` records an observed
violation. `inapplicable`, `cantTell`, `untested`, and `review` preserve exceptions, unknowns,
missing tests, and contextual judgment. Exit code zero means no failed check was found in
that run; review and uncertainty may remain.

Task completion, error recovery, trust, originality, information hierarchy, keyboard/focus
behavior beyond the narrow disclosure exercise, and unobserved states still need interaction
or human review. The initial exercise is not a representative human/AI benchmark or a
validated classifier. The research review explains what would be needed to make such claims.
