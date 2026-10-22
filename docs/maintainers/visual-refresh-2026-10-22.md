# Visual refresh: 22 October 2026

The user selected the Canvas Playground banner: a friendly sans-serif wordmark, a terminal
tethered to an editable frame, cream and forest green, with a small lime accent. The exact
Image Gen prompt, selected PNG hash and generation limits are in [visual provenance](../../assets/README.md).
The [design system](../../assets/DESIGN.md) records typography, colors and layout rules.

## Changes

- Installed the selected opaque 2172 × 724 banner without cropping or stretching.
- Redesigned architecture and setup as vertical diagrams with larger labels. The setup guide
  uses `figtether start`, canonical `dist/manifest.json`, pairing, sharing and Allow editing.
- Redesigned all three benchmark charts with directly labeled bars and larger numeric values.
  All five SVGs now regenerate from one script; the website diagnostic is no longer a separate
  manually maintained SVG. Release, debug and offline measurements remain separate.
- Applied the palette to light/dark Bridge and companion CSS, kept visible focus and semantic
  error colors, and updated the declared UI font/radius profile. Browser-default checkbox fonts
  now inherit the UI font consistently.

## Observations

The three benchmark source JSON files were compared byte-for-byte with their original Git
blobs and are unchanged. Units, sample counts, percentiles, failures and unmeasured billing
remain explicit. Actual editor/browser screenshots and dated reports are preserved.

All six current assets were rendered in Chrome at 800 px and 360 px widths. SVG text bounds
show no clipping or text overlap, and the page has no horizontal overflow. The enlarged
benchmark values remain prominent at the narrow width; small scope/footnote text still needs
full-size viewing. Corresponding measurements and setup steps also appear in selectable
Markdown. The first render detected overlapping numeric labels after their font was enlarged;
bar row spacing was increased and the failed observations were retained locally.

The production Bridge controller with synthetic ports passed its ten existing browser checks,
including frame sharing, clean-plan Auto, warning pause, activity clearing, retained focus and
320/440/740 px overflow checks. These clicks affected an isolated fixture, not the user's Figma
grants. The six unpaired theme/width captures each have 146 passed observations, zero failed,
eight inapplicable, eight uncertain naming checks and one untested manual-review outcome.
The initial token check exposed a browser-default checkbox font; explicit inheritance fixed it.

Measured light-theme body text contrast is 10.87:1; muted text on raised surfaces is 6.17:1.
Dark-theme body text is 14.82:1, with muted text on raised surfaces at 7.73:1. Action text and
error text exceed 4.5:1 in both themes; tested focus/control boundary pairs exceed 3:1. These
are scoped color measurements, not complete accessibility or task-success certification.

The plugin build and all 540 plugin tests pass. This refresh does not constitute another
native Figma execution test or a new latency benchmark. The native FILL width assertion
described in the [feature review](../06-testing/end-user-feature-review.md) remains pending
restored sharing/editing/Auto. Raw renders, observed DOM snapshots and failed attempts remain
in ignored local artifacts rather than the public evidence set.

Final publication checks cover 529 proposed public files and 118 Markdown documents, with
zero public-file or link/anchor issues. All five generated SVGs and three workflow outputs
are current. Root/plugin formatting, whitespace and six publication regression tests pass.
