# README visuals

The current artwork follows the [Editorial design system](DESIGN.md), adapted from the
[user-requested skill](https://github.com/bergside/awesome-design-skills/blob/main/skills/editorial/SKILL.md).
Flat typography, technical drawings, and directly labeled measurements replace the earlier
3D/mint direction. These are original project assets under the repository's MIT OR Apache-2.0
license; third-party logos and artwork are not reused.

| Asset                                                        | Purpose                                             | Source                                                                   |
| ------------------------------------------------------------ | --------------------------------------------------- | ------------------------------------------------------------------------ |
| [`figtether-banner.png`](figtether-banner.png)               | Editorial project masthead, 2172 × 724              | Built-in image generation; exact prompt below                            |
| [`figtether-icon.png`](figtether-icon.png)                   | Black frame-and-tether mark, 1254 × 1254 with alpha | Built-in image generation; exact prompt below                            |
| [Plugin icon](../plugins/figma/ui/assets/figtether-icon.png) | 96 × 96 derivative, displayed at 36 CSS px          | Bicubic resize preserving alpha; embedded into built HTML                |
| [`architecture.svg`](architecture.svg)                       | Local transports and native write protocol          | Deterministic renderer and reviewed Rust/plugin code                     |
| [`benchmark-desktop.svg`](benchmark-desktop.svg)             | Real-editor request latency and measurement limits  | [Versioned JSON](../benchmarks/published/desktop-2026-10-20.json)        |
| [`benchmark-stages.svg`](benchmark-stages.svg)               | Separate offline Rust diagnostic                    | [Offline JSON](../benchmarks/published/offline-stages.json)              |
| [`bridge-setup.svg`](bridge-setup.svg)                       | Terminal-to-plugin pairing and grants               | Deterministic illustrated guide, not a screenshot                        |
| [`bridge-ui-before.png`](bridge-ui-before.png)               | Actual previous Bridge UI                           | User-supplied screenshot, copied unchanged; no pairing secret is visible |

## Regeneration and scope

`bridge-ui-sharing.png` shows the production Bridge UI and controller with synthetic test
ports in Chrome. `bridge-ui-companion.png` shows the browser companion connected to an
isolated native daemon and authenticated synthetic plugin. These are unchanged browser
captures, not generated artwork or native Figma execution evidence. Their hashes, browser
version, and checks are in the [Bridge UX record](../benchmarks/published/bridge-ux-2026-10-21.json).

[`bridge-ui-desktop.png`](bridge-ui-desktop.png) is an unchanged native application capture
of the updated Bridge reopened in Figma Desktop. It shows the unpaired state, not a new
read/write benchmark. The illustrated guide links to it alongside the historical screenshot.

Run `node scripts/render-readme-assets.mjs` to rebuild all four SVGs; add `--check` to detect
drift. They contain no external assets, remote fonts, or executable code. Their title,
description, and surrounding Markdown explain the content without relying on color.
The checked-in PNGs are the selected final generated images and do not regenerate with this script.

The desktop renderer selects the last release build in the reviewed source JSON and checks
required lanes, returned-node counts, successful samples, and ordered percentiles. It refuses
missing or mismatched workloads. To publish a new version, preserve the prior report, select
the new reviewed JSON, update the date and README values, regenerate, and visually inspect.

Each desktop row uses its own zero-based linear scale. Cached-root, fresh 72-node, and PNG
rendering workloads perform different amounts of work. The cache lane includes 20,000
confirmed hits plus 10 fresh fallbacks. Peak daemon memory covers samples across builds.
PNG timing covers rendering/transport/export metadata, not completed client downloads.
Model cost and tokens are unmeasured. Known failures remain visible. See the
[full report](../benchmarks/published/desktop-2026-10-20.md) for hashes, prior builds,
scope mistakes, and limits. Visual redesign does not alter any measurement.

## Originality references

Before logo generation, the [Figma brand page](https://www.figma.com/using-the-figma-brand/),
[MCP's official site](https://modelcontextprotocol.io/), and
[Tether's official assets](https://tether.to/en/media/) were reviewed as marks to avoid.
Neither generation call used a reference image. The result is an original concept based on
native design frames and a connector. This review is not a comprehensive trademark clearance.

The README presentation also draws on [Harness²](https://github.com/google-research/harness2)
and Kiln's illustrated masthead structure. Their artwork and benchmark data are not reused.

## Generation record

Both selected PNGs were generated on 21 October 2026 using the built-in `image_gen` tool,
in generation mode. The banner has an opaque background; the icon preserves alpha.
Exact model revision, seed, and billed cost were not exposed. Generated artwork is illustrative
and is not benchmark evidence. The small plugin derivative is a mechanical resize only.

### Banner prompt

```text
Use case: ads-marketing.
Asset type: 3:1 horizontal editorial masthead banner for FigTether's GitHub README.
Design system: Editorial from bergside/awesome-design-skills. White paper, black ink, sharply typeset serif display, rigorous asymmetric grid. Restrained, credible technical publication rather than a software advertisement.
Primary request: a beautifully simple, completely FLAT graphic banner with a large black serif wordmark "FigTether" on the left. Under it, smaller precise black sans serif text "Read. Create. Edit." On the right, a single original black line drawing: three rectangular native design frames aligned on a document grid, joined by one fine horizontal connector with a small square at its center. One frame has a text line and a square, one has selection corner handles, one contains a tiny regular grid of rectangles. This represents editable design, not screenshots. Minimal black geometry, exact line weights, ample white space. Small muted orange square accents only at one connector point.
Typography: literary editorial serif wordmark, highly legible, intentional kerning, no tech sci-fi font, no inflated sans serif. Text must be spelled exactly.
Composition: 3:1 ultra-wide; everything on a white or very slightly warm white (#fafaf8) background; generous 6% margins, lots of empty space, one fine horizontal black rule near the bottom. No card styling. No rounded tiles. No extra copy. No badges. No logos from Figma, MCP, or Tether.
Avoid: ANY 3D, shadows, gradients, glows, neon, forest green, mint, physical cable, glass, metallic objects, dramatic lighting, clutter, generic AI visual effects, watermarks. This should look like an art-directed print masthead composed from typography and a clean technical drawing.
```

### Icon prompt

```text
Use case: logo-brand.
Asset type: a single original FigTether plugin app mark for a 36px header, in an editorial black-and-white design system.
Primary request: Create a rigorously simple FLAT black geometric mark on transparent background. Two upright rectangular outline frames of identical size stand side by side with a small gap. They are joined at their exact midpoints by one straight horizontal black tether. Each rectangle has a single tiny square selection handle at its upper outer corner, integrated into the line. Use crisp straight lines, sharp corners, uniform moderately thick strokes, ample interior negative space. This is a design-frame connection, not a chain, infinity, letter, or plug.
Composition: center one compact horizontal mark in a square transparent canvas, about 75% canvas width, 55% height. Uniform pure black ink only. Flat clean graphic suitable for deterministic tracing. Strong silhouette at small size. There is no app-tile background.
Constraints: NO lettering, monogram, captions, words, numbers, gradients, shadows, texture, glow, 3D, mockups, rounded-corner squircle background, mint, green, or multicolor. Not Figma's five-part F, not MCP's striped knot, not Tether's T/oval. Invent the mark from the described rectangle and connector geometry, without any reference image. Transparent background. One icon only.
```
