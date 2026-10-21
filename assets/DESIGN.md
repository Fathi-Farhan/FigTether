# FigTether README visual system

## Context and goals

Present a local design tool as a readable technical publication. Readers should understand
native design creation, authorization, and the scope of recorded measurements before they
inspect implementation details. The redesign follows the
[Editorial skill](https://github.com/bergside/awesome-design-skills/blob/main/skills/editorial/SKILL.md)
and its [design reference](https://github.com/bergside/awesome-design-skills/blob/main/skills/editorial/DESIGN.md),
selected from the user-requested collection. Guidance is applied; third-party artwork is not reused.

## Design tokens and foundations

| Token         | Value                            | Use                                                                  |
| ------------- | -------------------------------- | -------------------------------------------------------------------- |
| Paper         | `#ffffff`                        | Opaque figure background; works in light and dark README themes      |
| Ink           | `#111111`                        | Titles, outlines, primary measurements                               |
| Secondary ink | `#525252`                        | Supporting copy                                                      |
| Rule          | `#d4d4d4`                        | Separators and axes, never the only carrier of meaning               |
| Light fill    | `#f1f1f1`                        | Small illustrated controls                                           |
| Warning       | `#9a4b00`                        | Unmeasured cost and known failures; always paired with explicit text |
| Display type  | Gelasio, Georgia, serif          | Editorial headings, 24/36 px                                         |
| Body type     | Arial, Helvetica, sans-serif     | Labels and explanatory text, 14/16/18 px                             |
| Code type     | Ubuntu Mono, Consolas, monospace | Commands, values, hashes, 13/16/19 px                                |
| Baseline      | 8 px                             | Primary margins and content rhythm                                   |

The SVGs use installed fonts with fallbacks; no external font fetch is needed. The skill's
Gelasio and Ubuntu Mono families are declared, with local substitutes for portable rendering.
The 36 px figure heading is an adaptation for the 1100 px export width.

## Component rules

- Figures must have a 40 px inset, numbered caption, title, and scope sentence.
- Systems must use square outlined nodes, labeled transports, and arrows with an explicit direction.
- Benchmark results must use labeled axes, sample counts, scope, units, and source hashes.
- Independent latency scales must be stated next to the plot; every row starts at zero.
- Percentiles must use distinct bar treatments and adjacent numeric values.
- Missing measurements must say **not measured**. A failed tool must retain its failure label.
- The banner may use its original editorial masthead illustration; it must not contain performance claims.
- The logo must remain readable on a white field in both plugin themes, with adjacent visible product text.

## Accessibility requirements and acceptance criteria

Text must stay legible at a typical README width of 800–1100 px. Supporting text is at least
13 px in the 1100 px source, and the surrounding Markdown repeats substantive benchmark
values and workflow instructions as selectable text. Body and warning text must meet 4.5:1
contrast against paper. Color cannot be the only way to distinguish a measurement or status.
SVGs must provide a title and description; Markdown images must provide meaningful alt text.

The plugin keeps native controls, associated labels, keyboard focus styles, and live status
regions. The setup disclosure must open with the keyboard. Its icon is decorative because the
adjacent text already names the product. The original plugin's compact button sizes are retained;
this documentation redesign does not claim full 44 px touch-target compliance.

## Content and tone

Use precise verbs: **read**, **create**, **edit**, **approve**. Say where an input comes from.
For example: “Run `figtether pair` in a second terminal,” rather than “Enter your secret.”
Explain the required grant separately from the connection and session name.

## Anti-patterns and migration

Replace the previous 3D cable scene, gradients, glowing connectors, mint cards, and faux
terminal scenery. Use flat typography, technical drawing, and directly labeled data instead.
Keep the user's real Bridge screenshot unchanged and identify it as the previous UI.
Historical benchmark data and failure records must not be changed during visual migration.

## QA checklist

- Regenerate all SVGs and pass `node scripts/render-readme-assets.mjs --check`.
- Render and inspect the banner, icon, architecture, desktop chart, offline chart, and guide.
- Confirm units, axes, sample counts, and percentiles agree with the reviewed JSON.
- Confirm text does not overflow at the source dimensions.
- Verify Markdown rendering with HTML disabled and all local links resolved.
- Build the plugin with its icon embedded locally; preserve alpha in the small PNG derivative.
- Run plugin tests and type checks after onboarding/status changes.
