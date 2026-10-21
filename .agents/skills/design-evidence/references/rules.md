# Deterministic rule contract

Ruleset: `1.0.0`. Node.js 22 or later. No model, paid API, or network calls are made by the evaluator.

## Input

Supply JSON with `schema_version: 1`, `source.kind` (`dom`, `figma`, or `manual`),
`capture.complete` (boolean), and `nodes` (at most 10,000). Node IDs must be unique.
Each node requires an `id`, `role` (`text`, `control`, `graphic`, or `surface`), and explicit
`visible` boolean. An observation may be missing or null when unavailable. Do not fabricate
CSS or AX observations from a screenshot or layer name.

Optional observations include `text`, `disabled`, `foreground`, `background`, `font_size`,
`font_weight`, `border_radius`, `background_image`, `box_shadow`, `name`, `name_source`,
`contrast_role`, `adjacent_color`, and `rect` (`x`, `y`, `width`, `height`). Geometry uses CSS
pixels for DOM input; color strings support opaque hex and rgb/rgba sRGB. Alpha, unsupported
color spaces, unknown compositing, and missing measurements are not guessed.

A browser `viewport` contains positive `width`, `height`, and `scroll_width`; optional
`reflow_exception` explicitly declares whether two-dimensional layout is essential.
A Figma frame does not supply browser reflow or accessible-name evidence.

Example observed text, without provenance claims:

```json
{
  "schema_version": 1,
  "source": { "kind": "dom" },
  "capture": { "complete": true },
  "nodes": [
    {
      "id": "heading",
      "role": "text",
      "visible": true,
      "text": "Connect your local daemon",
      "foreground": "#111111",
      "background": "#ffffff",
      "font_size": 24,
      "font_weight": 400
    }
  ]
}
```

`complete` describes the collector's declared scope in this state, not the entire product.
An incomplete capture cannot establish a spacing or decoration-budget pass that depends
on unobserved targets. Keep collection limitations alongside the input.

## Rules

| ID   | Category         | Applicability and expectation                                                                                                   | Limits / exceptions                                                                                                                   |
| ---- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Q001 | Coverage         | Report whether the scoped capture is complete.                                                                                  | A declaration is not independent proof of complete product coverage.                                                                  |
| Q002 | Manual review    | Leave task success, recovery, hierarchy, originality, keyboard/focus, and uncaptured states untested.                           | Separate interaction/user evidence is required.                                                                                       |
| A001 | Standard-derived | Visible text with resolved opaque colors and font metrics: contrast ≥4.5, or ≥3 for text ≥24 px or bold ≥18⅔ px at weight ≥700. | Inactive controls and explicitly marked incidental/logotype text are excluded. No rounding upward. Unknown compositing abstains.      |
| A002 | Standard-derived | Enabled visible controls with `name_source: browser-ax` or `verified-manual` require a nonempty supplied name.                  | A heuristic label or Figma layer name produces cantTell. This does not check all role/state semantics.                                |
| A003 | Standard-derived | Captured target is ≥24 ×24 px, or its required spacing does not intersect another supplied target.                              | Explicit inline, equivalent, user-agent, or essential exceptions need an evidence string. Small size alone is not a failure.          |
| A004 | Standard-derived | A graphic explicitly marked `contrast_role: essential` needs contrast ≥3 against supplied adjacent color.                       | Decorative/logotype exceptions are explicit. Unclassified images and unknown colors abstain.                                          |
| L001 | Layout           | Compare document scroll width with captured viewport width.                                                                     | 1 px tolerance for integer measurement; overflow with unknown essential-layout applicability is review. Not a full WCAG reflow audit. |
| D001 | Project contract | Text/control font size belongs to the configured `tokens.font_sizes` list.                                                      | Opt-in; 0.01 px numeric tolerance; derived type sizes must be explicitly declared.                                                    |
| D002 | Project contract | Radius belongs to `tokens.radii`.                                                                                               | Opt-in; same tolerance; unknown radii abstain.                                                                                        |
| D003 | Project contract | Number of supplied gradient backgrounds ≤`decoration.max_gradients`.                                                            | Opt-in; zero gradients is a local preference, never an AI test. Missing observations abstain.                                         |
| D004 | Project contract | Number of supplied non-none box shadows ≤`decoration.max_shadows`.                                                              | Opt-in; legitimate shadows may be allowed. Missing observations abstain.                                                              |
| P001 | Pattern review   | At least 6 rounded surfaces (radius ≥8 px), with ≥60% in one width/height/radius group quantized to 4 px.                       | Minimum and fraction are configurable exploratory defaults, not research-validated thresholds. Dashboards may intentionally repeat.   |
| P002 | Pattern review   | At least 5 enabled controls, ≥75% at width ≥2×height and radius ≥half height.                                                   | Configurable exploratory defaults. Tags or deliberate pill controls can justify it.                                                   |
| P003 | Pattern review   | Literal, case-insensitive word/phrase matches against a configured local vocabulary.                                            | The vocabulary is a copy-review preference. Phrases do not establish quality or authorship.                                           |

For A003, place a circle of radius 12 px at each undersized target's rectangle center. Against
another undersized target, compare center distance with 24 px; against a full-size target,
compare the distance to its nearest rectangle point with 12 px. Boundary contact is accepted.
Use actual clickable target geometry, including a correctly measured associated label if
relevant; do not invent an expanded rectangle by joining disconnected areas. Overlays,
transforms, and obscured or irregular hit areas may require manual verification.

Sources and the distinction between normative WCAG and these partial checks are explained in
[the research](research.md). The custom rules are not approved ACT rules or an accessibility certification.

## Profiles

A profile has `schema_version: 1` and a stable `id`. Omit a contract to leave it untested.
Optional `tokens` contains `font_sizes` and `radii`; optional `decoration` contains
`max_gradients` and `max_shadows`. Optional `review` enables pattern review and may override
`repeated_surface_min`, `repeated_surface_ratio`, `pill_min`, `pill_ratio`, and `generic_phrases`.
The shipped Bridge profile records its compact existing UI, including derived code type
sizes. It does not impose the README's editorial typography on the plugin or other products.

## Outcomes and determinism

**Passed** means this observed target met this rule. **Failed** means the supplied evidence
violated it. **Inapplicable** means no supplied target matched, or an explicit exception applied.
**CantTell** means evidence/applicability was insufficient. **Untested** means the check or
configuration was not evaluated. **Review** means a captured pattern needs contextual judgment.

The report contains rule version, input/profile/code hashes, scoped coverage, category,
node ID, outcome, and evidence. Its summary always leaves authorship and full WCAG conformance
unassessed. No AI probability or combined aesthetic score is calculated.

Within a pinned Node/V8 runtime, the evaluator sorts nodes and results and canonicalizes object keys; it introduces no clock,
random seed, locale-dependent ordering, network, or model calls. Identical normalized inputs,
profile, and code produce identical bytes. Changing metadata changes its recorded input hash,
but changing a self-reported author label must not change quality findings.

CLI exit codes: `0` means evaluation completed without a failed check, even if review or
unknowns remain; `1` means at least one check failed; `2` means malformed input or another
execution error. Consumers must inspect coverage and outcomes rather than treating exit `0`
as an accessibility or usability approval.
