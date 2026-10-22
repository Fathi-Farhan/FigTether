# FigTether visual system: Canvas Playground

## Purpose

Help developers and designers understand the bridge before reading implementation details.
The user selected the Canvas Playground banner on 22 October 2026: bold, friendly sans-serif
type, a terminal tethered to an editable frame, and a restrained cream, forest and lime palette.
This replaces the earlier editorial serif direction. Playfulness comes from the connection
and native selection handles; data figures prioritize labels, alignment and accurate scales.

## Tokens

| Token        | Value                                     | Use                                                   |
| ------------ | ----------------------------------------- | ----------------------------------------------------- |
| Canvas       | `#f7f6ee`                                 | Opaque banner/figure background and light UI          |
| Forest       | `#153f2e`                                 | Text, primary actions, outlines and p95 bars          |
| Lime         | `#d7f3ac`                                 | Selection handles, step markers and outlined p50 bars |
| Rule         | `#748375`                                 | Axes, separators and light UI control boundaries      |
| Display      | Arial, Helvetica, sans-serif; 40 px / 800 | Figure headings                                       |
| Label        | Same sans-serif; 28 px / 700              | Workload and diagram labels                           |
| Body         | Same sans-serif; 22–24 px                 | Descriptions                                          |
| Value        | Same sans-serif; 32 px / 700              | Directly labeled benchmark measurements               |
| Code         | Consolas, monospace; 26 px / 700          | Actual commands                                       |
| Figure width | 800 px                                    | Natural README content width                          |
| Inset        | 40 px                                     | Figure margins                                        |

The image-generated banner approximates these tokens; its pixels are illustrative artwork,
not a mechanically exact token export. SVGs and CSS use explicit color values. Fonts are local
system stacks, with no network font dependency. Bridge uses compact 12–24 px system type,
8 px control corners, visible focus, and the same palette. The dark theme uses forest surfaces,
cream text and lime actions. Red is reserved for error/destructive states and accompanied by text.
The existing black frame-and-tether plugin icon remains on a cream tile.

## Layout and data

- Give each figure one clear question and a large, short title.
- Use a vertical flow for architecture and setup so labels have room at README width.
- Show the current `figtether start` path and manifest-owned port agreement.
- Keep benchmark p50/p95/p99 labels and values beside the bars. p50 has a lime fill and dark
  outline; p95 is forest; p99 is dashed. Meaning must survive monochrome viewing.
- Start every axis at zero. Desktop workload scales are independent and explicitly labeled;
  offline stages share one scale. Never imply a comparison between different workloads/builds.
- Keep measured JSON, sample counts, failures, units and scope unchanged. Link source data,
  build hashes and reproduction notes in surrounding selectable Markdown.
- Keep actual editor/browser screenshots and dated reports unchanged as historical evidence.
- Do not add generated performance claims, synthetic ratings, decorative dashboards or gradients.

## Readability and verification

Inspect rendered assets at 800 px and 360 px widths. Primary content must fit without clipping;
small figure details may require opening the image at full size on a phone. Repeat substantive
numbers and setup instructions in selectable Markdown so the image is never the only source.
All SVGs have a title and description. Verify actual rendered text/control color pairs separately,
including both Bridge themes. Contrast checks are partial measurements, not a full accessibility
or usability certification.

Run `node scripts/render-readme-assets.mjs --check` after regeneration. Render all five SVGs,
check text bounds and bar values, and inspect the banner in the README layout. After CSS
changes, build the plugin and check the controlled UI fixture at narrow/wide widths and in
light/dark themes. Never describe browser fixture checks as native Figma testing.

See [visual provenance](README.md) for the selected artwork and exact generation prompt.
