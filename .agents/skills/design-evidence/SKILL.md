---
name: design-evidence
description: Design or review UI and documentation visuals against a concrete brief, using deterministic measurements and sourced quality rules. Use for requests to reduce generic AI-looking patterns, validate a design system, or build repeatable UI quality checks; do not infer AI authorship from appearance.
---

# Design with evidence

Produce a useful design with a clear reason for its structure, then verify observable claims.
A deterministic quality check is not an AI-authorship detector.

## Design or revise

Use the user's task, audience, content, target surface, and existing design as the brief.
Resolve routine choices from context; ask only when a missing requirement materially changes the result.
Keep the existing product and authorized scope. Do not substitute a generic landing-page template.

Choose a visual direction that supports the task. Record the intended hierarchy, token values,
and relevant states in a small profile or project design note. Reuse established components
where they fit. A card, gradient, serif, or rounded corner is acceptable when the brief supports it.

Use native editable graphics for diagrams and charts, and generated raster artwork when that
is the desired medium. Preserve real benchmark data and provenance. Do not use rendered
artwork as proof that an interaction or benchmark passed.

## Review

For a small revision, inspect the affected states and run the relevant checks. Use a broader
viewport/state sweep when changing shared layout, components, or a multi-step flow. Do not
rebuild unrelated environments or repeat passing checks without a new reason.

Inspect the actual render before describing its visual quality. Capture the viewport, theme,
state, browser/editor version, and artifact hash when available. Screenshots can establish
appearance; use DOM/AX data or scoped Figma nodes for measurable semantics and geometry.

Read [the rules](references/rules.md) before running the checks. Read
[the research](references/research.md) when discussing evidence, thresholds, or detector validity.

Use the local evaluator:

```sh
node scripts/evaluate.mjs snapshot.json --profile profile.json --out report.json
node --test scripts/evaluate.test.mjs
```

Paths above are relative to this skill directory. The evaluator uses Node.js 22+, no model,
network service, or paid API. Its script records findings but never edits a design.
Exit 0 means no failed observations, 1 means an observed failure, and 2 means invalid input
or invocation. Fix the input or collector for exit 2; do not weaken a threshold to obtain exit 0.
Review and unknown outcomes can remain at exit 0. If a required tool or observation is
unavailable, record the affected check as untested or uncertain and continue independent work.

For a browser capture, evaluate the exported function in
[scripts/collect-browser.mjs](scripts/collect-browser.mjs). Pass computed accessible names
from the browser accessibility tree when available; otherwise naming checks remain uncertain.
Do not read password/input values, local credentials, or hidden application state into a snapshot.
For Figma, adapt observed node data to the same schema and leave unavailable DOM/AX facts unknown.
For interaction, route, or resource review, read
[interaction contracts](references/interaction-review.md). Use requirement-specific expected
outcomes and resettable fixtures; a click followed by any DOM mutation does not prove success.

## Interpret results

Keep three result classes separate: standard-derived checks, project contract violations,
and patterns requiring review. A configured zero-gradient policy is a project preference,
not a scientific definition of good design or AI generation.

Use passed, failed, inapplicable, cantTell, untested, and review as documented in the rules.
Missing data never counts as a pass. No findings is not proof of accessibility or usability.
Do not output an AI probability, authorship verdict, universal beauty score, or unsupported
claim that a style change improves users' task success.

Fix evidenced defects first, inspect the result, and rerun affected checks. Keep justified
exceptions explicit. For task suitability, originality, trust, focus behavior, and recovery,
record human or interaction evidence separately from static measurements.

## Report

State what changed, what was measured, remaining uncertainty, and where evidence is saved.
Retain failed attempts and version hashes. Record latency, workload/sample count, and available
token/billing data separately; missing cost data is unmeasured. Claim a skill improves performance
only after matched with/without-skill runs on the same tasks and environment.
Default documentation and examples to English unless the user's brief specifies another language. Preserve the user's brief rather than imposing
this project's editorial style on unrelated products. Preserve existing permission boundaries;
an approval-gated Figma write still requires the exact plan's approval.
