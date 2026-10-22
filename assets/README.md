# FigTether visuals

The current [Canvas Playground visual system](DESIGN.md) follows the user's selected banner:
large friendly sans-serif type, cream and forest green, a small lime accent, and a terminal
connected to an editable frame. It replaces the earlier serif/editorial documentation style.
These are original project assets under MIT OR Apache-2.0; third-party artwork is not reused.

| Asset                                                        | Purpose                                                      | Source                                                           |
| ------------------------------------------------------------ | ------------------------------------------------------------ | ---------------------------------------------------------------- |
| [Banner](figtether-banner.png)                               | Selected Canvas Playground masthead, 2172 × 724              | Built-in Image Gen; exact prompt below                           |
| [Icon](figtether-icon.png)                                   | Existing black frame-and-tether mark, 1254 × 1254 with alpha | Built-in Image Gen, 21 October 2026                              |
| [Plugin icon](../plugins/figma/ui/assets/figtether-icon.png) | 96 × 96 derivative on a cream tile                           | Mechanical resize preserving alpha                               |
| [Architecture](architecture.svg)                             | Local transports and native write protocol                   | Deterministic renderer and source contract                       |
| [Setup guide](bridge-setup.svg)                              | Start, import, pair and share                                | Current CLI and Bridge labels; illustrated, not a screenshot     |
| [Desktop benchmark](benchmark-desktop.svg)                   | Release-build latency and measurement limits                 | [Recorded JSON](../benchmarks/published/desktop-2026-10-20.json) |
| [Offline benchmark](benchmark-stages.svg)                    | Separate synthetic Rust diagnostic                           | [Recorded JSON](../benchmarks/published/offline-stages.json)     |
| [Website diagnostic](benchmark-website-2026-10-21.svg)       | Debug-build native website diagnostic                        | [Recorded JSON](../benchmarks/published/website-2026-10-21.json) |
| [Previous Bridge UI](bridge-ui-before.png)                   | Historical UI evidence                                       | Unchanged user-supplied screenshot                               |
| [Browser sharing UI](bridge-ui-sharing.png)                  | Historical controlled-browser capture                        | Synthetic ports, not native execution evidence                   |
| [Browser companion](bridge-ui-companion.png)                 | Historical companion capture                                 | Isolated daemon and synthetic plugin                             |
| [Native Bridge UI](bridge-ui-desktop.png)                    | Historical native unpaired capture                           | Unchanged Figma Desktop screenshot                               |

## Regeneration and evidence

Run `node scripts/render-readme-assets.mjs` to rebuild all five SVGs; add `--check` to detect
drift. The renderer reads reviewed benchmark JSON, validates workloads and percentile ordering,
and writes self-contained figures with titles/descriptions, without remote fonts or scripts.
PNG artwork is selected separately and does not regenerate with this script.

Each desktop workload has its own zero-based linear scale. The offline stages share a scale.
Cached-root, fresh-read and PNG workloads perform different work; release and debug runs
remain separate. Historical failures are retained, model billing is unmeasured, and PNG export
metadata timing does not establish completed client downloads. See the [desktop report](../benchmarks/published/desktop-2026-10-20.md),
[website report](../benchmarks/published/website-2026-10-21.md) and [offline notes](../benchmarks/published/README.md).
This visual refresh changes presentation, never recorded measurement values.

The Bridge CSS shares this palette in light/dark themes. Existing screenshots remain dated
records rather than being recolored to resemble a newly tested state. Browser fixture details
are recorded in the [Bridge UX data](../benchmarks/published/bridge-ux-2026-10-21.json).

## Banner generation record

Generated on **22 October 2026**, using the built-in `image_gen` tool in edit/redesign mode.
The user selected the first displayed independent result, named **Canvas Playground**. The
actual prior banner was attached as a context reference; its serif style was explicitly replaced.
The final opaque PNG is copied unchanged into this repository. SHA-256:
`bbc1148d7055a907691b5dd9b97d8e5aadfd2d0d63218fdf5bf484a049658774`.

Three independent banner concepts were generated. Exact model revision, seed and billed cost
were not exposed. Artwork is illustrative and is not benchmark evidence. The requested 2160 × 720
slot was returned as 2172 × 724; both have the same 3:1 aspect ratio, with no stretching or cropping.

### Selected banner prompt

```text
Use case: ads-marketing.
Asset type: a single finished FigTether GitHub README banner. Direction name: Canvas Playground.
Target dimensions: 2160 x 720 pixels, exactly 3:1 wide landscape. Compose naturally for this slot; never stretch typography.
Input image: the attached existing banner is the redesign target and brand/context reference only. Keep the exact FigTether spelling and the idea of connected editable frames; replace its severe serif/editorial style completely.
Brief: a fun, welcoming, carefully art-directed open-source design tool identity, readable by developers and designers unfamiliar with the product. It connects local AI agents to native editable Figma designs. Readability is the first priority. This is a banner, not a web page: no navigation, buttons, testimonials, badges, or invented metrics.
Visual system: warm ivory #F7F6EE background, near-black forest #153F2E typography/linework, restrained lime #D7F3AC in the illustration. Three flat colors only. Bold friendly geometric sans-serif with excellent letter shapes and intentional spacing; no serif, no novelty display font. Large, strong hierarchy and purposeful whitespace.
Composition: left 58% is typography, right 36% is one original playful illustration. Left-align the very large wordmark at x about 110, with ample clear space. The wordmark should dominate and remain readable at 360px displayed width. Under it, two generous lines of plain supporting copy. On the right, show one oversized editable design frame with chunky square selection handles and a simple typographic composition inside, connected by a loose looping forest-green tether to a single small command-prompt symbol. Give the tether a little human irregularity and the frame a subtle cheerful tilt, while keeping text horizontal. Flat graphic shapes with confident outlines; playfulness comes from the relationship of the frame and tether, not decorative confetti. No labels inside the illustration.
Text, verbatim, only these words:
"FigTether"
"Connect AI agents."
"Create in Figma."
The supporting copy must be no smaller than 64px at source size; wordmark about 185px. Dark text on ivory, strong contrast. All content within 90px margins; no fine print, no text along bottom edge.
Presentation: polished original flat brand illustration, clear information hierarchy, generous spacing. No generic AI/SaaS aesthetic, glowing gradients, glass, 3D render, sparkles, stars, robot heads, purple/blue gradients, rainbow colors, floating UI card stacks, fake charts, blurred shadows, stock icons, or third-party logos. One coherent banner, no option label, no mockup frame, no watermark.
```

## Icon generation record

The existing icon was generated on **21 October 2026** using built-in Image Gen, without a
reference image. The original alpha and the plugin derivative remain unchanged by this refresh.
Before the original generation, the [Figma brand page](https://www.figma.com/using-the-figma-brand/),
[MCP site](https://modelcontextprotocol.io/) and [Tether media page](https://tether.to/en/media/)
were reviewed as marks to avoid. This was not a comprehensive trademark clearance.

### Icon prompt

```text
Use case: logo-brand.
Asset type: a single original FigTether plugin app mark for a 36px header, in an editorial black-and-white design system.
Primary request: Create a rigorously simple FLAT black geometric mark on transparent background. Two upright rectangular outline frames of identical size stand side by side with a small gap. They are joined at their exact midpoints by one straight horizontal black tether. Each rectangle has a single tiny square selection handle at its upper outer corner, integrated into the line. Use crisp straight lines, sharp corners, uniform moderately thick strokes, ample interior negative space. This is a design-frame connection, not a chain, infinity, letter, or plug.
Composition: center one compact horizontal mark in a square transparent canvas, about 75% canvas width, 55% height. Uniform pure black ink only. Flat clean graphic suitable for deterministic tracing. Strong silhouette at small size. There is no app-tile background.
Constraints: NO lettering, monogram, captions, words, numbers, gradients, shadows, texture, glow, 3D, mockups, rounded-corner squircle background, mint, green, or multicolor. Not Figma's five-part F, not MCP's striped knot, not Tether's T/oval. Invent the mark from the described rectangle and connector geometry, without any reference image. Transparent background. One icon only.
```
