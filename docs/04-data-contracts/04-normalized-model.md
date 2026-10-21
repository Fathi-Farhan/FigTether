# Data Contracts: Normalized Design Model

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose: this document freezes the normalized design model produced by the Rust core from plugin
reads. It derives the eight domain tables from PRD section 8.5.3 exactly, defines the availability
annotation and coordinate-system contracts that make partial data safe for agents (FR08, FR09,
FR12), and fixes the rule that `JSON_REST_V1` is only an optional adapter input so the product
never claims REST-backed completeness (FR19). Gate G0 has not run, so all names and defaults are
pending confirmation and remain `Proposed` until evidence lands.

## 1. Producer, model, and consumers

- The plugin adapter in `plugins/figma/main` builds schema-validated DTOs from documented Plugin
  API reads (PRD 8.4). The core normalizer in `crates/core` canonicalizes them; types live in
  `crates/model`; the wire mapping lives in `crates/protocol`.
- Tools consume this model through the tool surface listed in
  `docs/04-data-contracts/01-mcp-envelope.md`, which also fixes the PRD 8.5.2 envelope.
- Test anchor: `AT10` compares normalized JSON against curated fixtures in `fixtures/` and must
  show that mixed text, gradients, and unresolved aliases are not silently dropped (P08).

## 2. Normalization inputs

The model has three ranked inputs (PRD 8.5.3):

1. **Plugin DTO (required).** Allowlisted Plugin API handlers produce typed DTOs for live reads
   and snapshot captures. This is the only mandatory input.
2. **`JSON_REST_V1` export (optional adapter input).** `exportAsync` with `JSON_REST_V1` (S4) may
   be used as an adapter convenience input. It is never the single internal model, and release
   evidence must record which Figma versions produced its fixtures.
3. **Plugin API supplementary fetches (allowed).** When an export omits a property, normalization
   may fetch the missing property through the documented Plugin API for the same node. This is the
   stated mitigation in PRD 8.5.3; it never replaces Plugin access with REST.

Large-subtree exports allocate inside the editor before chunking helps (PRD 8.5.3). The plugin
must therefore estimate traversal scope first and refuse risky exports (SCOPE_TOO_LARGE, see
`docs/04-data-contracts/05-error-catalog.md`) instead of claiming that chunking removes editor
memory risk.

## 3. Domain tables (derived from PRD 8.5.3)

Each table keeps the PRD wording for minimum data and behavior. Types sketch is a proposed mapping
into `crates/model` types; field names are the wire names projected by tools.

### 3.1 Identity and tree

| Column                   | Value                                                                                                                                                                                                                                                                                   |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Minimum data             | `node_id`, `type`, `name`, `children`, `parent` limited to scope, `visible`, `locked`                                                                                                                                                                                                   |
| Types sketch             | `node_id: string` (opaque, never parsed as a number), `type: string` verbatim from Figma, `name: string`, `children: array<node_id>`, `parent_id: node_id \| null`, `visible: bool`, `locked: bool`                                                                                     |
| Behavior when incomplete | Unknown node types are preserved as `unsupported` and never silently deleted (PRD 8.5.3, section 6 of this document). Parent outside the grant is reported as `parent_id: null` with availability reason `not_in_grant`; sibling and ancestor detail outside scope is omitted (PRD 8.4) |

### 3.2 Geometry

| Column                   | Value                                                                                                                                                                                                                                                           |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Minimum data             | Local transform, absolute transform, bounding box, rotation, constraints                                                                                                                                                                                        |
| Types sketch             | `local_transform`, `absolute_transform: 2x3 matrices` (numbers, px), `bounding_box`, `absolute_bbox: {x, y, width, height}`, `rotation_deg: number`, `constraints: {horizontal, vertical}`                                                                      |
| Behavior when incomplete | The coordinate system is always declared (section 5). Absolute coordinates are never presented as CSS-relative positions (PRD 8.5.3). A missing matrix leaves the field `unavailable` with reason `capability_unavailable`; it is never inferred from a sibling |

### 3.3 Layout

| Column                   | Value                                                                                                                                                                                |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Minimum data             | Auto Layout axis, gap, padding, alignment, sizing, wrapping if available                                                                                                             |
| Types sketch             | `mode: fixed \| auto_layout`, `axis`, `sizing: {horizontal, vertical} = fixed \| hug \| fill`, `padding_px: {top, right, bottom, left}`, `gap_px: number`, `wrap: bool`, `alignment` |
| Behavior when incomplete | `fixed`, `hug`, and `fill` are always distinguished (PRD 8.5.3). Web breakpoints are never invented for Auto Layout data (PRD 8.5.3)                                                 |

### 3.4 Typography

| Column                   | Value                                                                                                                                                                                                                                                                  |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Minimum data             | Text, styled runs, font family/style/weight, size, line height, letter spacing                                                                                                                                                                                         |
| Types sketch             | `text: string`, `runs: array<{start, end, font_family, font_style, font_weight, font_size_px, line_height_px, letter_spacing_px}>`, `fallback_total: run` when uniform                                                                                                 |
| Behavior when incomplete | Mixed style values become multiple runs (PRD 8.5.3). Missing or unsupported fonts keep the text and set `font_state = missing \| unsupported` (FONT_UNAVAILABLE family, PRD 8.7); per-character styling that the code API does not expose stays one run plus a warning |

### 3.5 Paint and effect

| Column                   | Value                                                                                                                                                                                                         |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Minimum data             | Solid/gradient/image fills, strokes, opacity, corners, shadows, masks/clip                                                                                                                                    |
| Types sketch             | `fills: array<{kind: solid \| gradient \| image, ...params}>`, `strokes: array<stroke>`, `opacity: number`, `corner_radius_px`, `effects: array<{kind: shadow \| blur, ...}>`, `masks`, `clips_content: bool` |
| Behavior when incomplete | Anything the JSON cannot represent gets a visual fallback to PNG (P07, PRD 8.5.3) and properties the API does not expose produce a warning (section 4). No silent zero-values                                 |

### 3.6 Components

| Column                   | Value                                                                                                                                                                                                                         |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Minimum data             | Instance/main component reference, variant properties, overrides when available                                                                                                                                               |
| Types sketch             | `component_ref: {instance_id, main_component_id}` as opaque IDs, `variant_properties: map`, `overrides: array`                                                                                                                |
| Behavior when incomplete | References stay as unresolved IDs until the user approves the dependency grant. Remote libraries outside the grant are never loaded automatically (PRD 8.5.3); out-of-scope definitions are shown as IDs only (PRD 8.4, FR10) |

### 3.7 Variables and styles

| Column                   | Value                                                                                                                                                                                                |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Minimum data             | References, available collections/modes, values and aliases, bindings                                                                                                                                |
| Types sketch             | `variable_ref: {variable_id, collection_id, mode_id}`, `aliases: array`, `bindings: array<{node_id, property}>`, `mode_values: map`                                                                  |
| Behavior when incomplete | Unresolved aliases, alias cycles, and ambiguous modes are reported (FR10, PRD 8.5.3). The document mode is never changed by a read (PRD 8.5.3). Coverage per seat is probed, not assumed (FR09, D09) |

### 3.8 Assets

| Column                   | Value                                                                                                                                                       |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Minimum data             | Opaque asset ID, MIME, dimensions, bytes, hash, export settings                                                                                             |
| Types sketch             | `asset_id: string` opaque, `mime: string`, `width`, `height: number`, `bytes: number`, `hash: "sha256:..."`, `export_settings: {format: PNG \| SVG, scale}` |
| Behavior when incomplete | Ephemeral image URLs are never treated as permanent assets (PRD 8.5.3, FR23). Resource URIs are grant-bound and expire with the grant (FR11, PRD 8.4)       |

## 4. Availability annotations

Every node may carry an `availability` map so agents cannot mistake missing data for empty data
(P08, PRD 8.5.2 null rule: null means empty only when availability says `available`). The envelope
`coverage.unavailable_fields` aggregates the union across returned nodes.

Shape: `field name -> {"status": "available" | "unavailable", "reason": <code>, "detail"?: string}`
where field names are dotted paths such as `typography.line_height_px`. The reason vocabulary is a
closed set:

| Reason                   | Meaning                                                                               |
| ------------------------ | ------------------------------------------------------------------------------------- |
| `capability_unavailable` | API, editor, or seat does not expose the property (FR09)                              |
| `projection_omitted`     | Omitted by the caller's field projection, not a data gap                              |
| `not_applicable`         | Property meaningless for this node type                                               |
| `visual_gap`             | Representable only visually; PNG fallback issued with a warning (PRD 8.5.3 paint row) |
| `not_in_grant`           | Entity outside grant; only its ID, if any, may appear (PRD 8.4)                       |

Unknown reasons are normalized to `capability_unavailable` (fail-closed, per
`docs/00-overview/01-prd-analysis.md` section 10). Example annotation:

```json
{
  "availability": {
    "typography.runs[0].line_height_px": {
      "status": "unavailable",
      "reason": "capability_unavailable"
    },
    "geometry.constraints": { "status": "available" }
  }
}
```

## 5. Coordinate-system declaration

Every geometry-bearing response must declare the coordinate system before coordinates are trusted
(PRD 8.5.3: "the coordinate system is declared; absolute x/y is never used as a CSS-relative
position"). The declaration is part of the node's `geometry` object and the envelope carries the
top-level statement:

```json
{
  "coordinate_system": {
    "units": "px",
    "node_relative_origin": "top-left",
    "x_axis_direction": "right",
    "y_axis_direction": "down",
    "absolute_space": "page",
    "declared_by_model_version": "fgt-model/1.0"
  }
}
```

Rules: (a) if `declared = false` for any geometry the whole `geometry` object is
`status=unavailable` with reason `capability_unavailable`; (b) consumers must convert explicitly
between local and absolute spaces; the model never silently mixes them; (c) the stacked DOM-style
offset chain (`offsetParent`, "85% down") must be computed by the agent from declared frames, not
encoded into the payload; (d) `absolute_bbox` is the page-space box and must not be styled as a
CSS `left/top` pair.

## 6. Unknown-type preservation

Identical to PRD 8.5.3 identity behavior: nonstandard node types (versions ahead of the schema,
or Community widget types) are kept.

- `type` keeps the verbatim Figma type string; `type_supported` is `false` when the normalizer has
  no window into it.
- `children` and `name` are still projected when reachable, so agents can see a hole in the tree
  instead of a missing subtree.
- The envelope's `coverage.unsupported_types` lists every encountered verbatim type once.
- Reads of such nodes give per-field warnings; they never become `NODE_NOT_FOUND`.

## 7. Appearance examples

These are minimal projections of the model in `docs/04-data-contracts/01-mcp-envelope.md`, which
also fixes envelope fields such as `coverage.unavailable_fields`; the examples below show the
per-node payload that slots into that envelope's `data`.

### 7.1 Text node

```json
{
  "node_id": "128:11",
  "type": "TEXT",
  "type_supported": true,
  "name": "Pay now",
  "parent_id": "4:3",
  "visible": true,
  "locked": false,
  "geometry": {
    "coordinate_system": {
      "units": "px",
      "node_relative_origin": "top-left",
      "x_axis_direction": "right",
      "y_axis_direction": "down",
      "absolute_space": "page",
      "declared_by_model_version": "fgt-model/1.0"
    },
    "bounding_box": { "x_px": 0, "y_px": 0, "width_px": 76, "height_px": 20 },
    "absolute_bbox": { "x_px": 142, "y_px": 34, "width_px": 76, "height_px": 20 },
    "rotation_deg": 0
  },
  "typography": {
    "text": "Pay now",
    "font_state": "available",
    "runs": [
      {
        "start": 0,
        "end": 7,
        "font_family": "Inter",
        "font_style": "Semi Bold",
        "font_weight": 600,
        "font_size_px": 14,
        "letter_spacing_px": 0.2,
        "line_height_px": 20
      }
    ]
  },
  "availability": {
    "typography.runs[0].letter_spacing_px": { "status": "available" }
  }
}
```

### 7.2 Frame

```json
{
  "node_id": "4:3",
  "type": "FRAME",
  "type_supported": true,
  "name": "Cart header",
  "parent_id": "1:2",
  "visible": true,
  "locked": false,
  "geometry": {
    "coordinate_system": {
      "units": "px",
      "node_relative_origin": "top-left",
      "x_axis_direction": "right",
      "y_axis_direction": "down",
      "absolute_space": "page",
      "declared_by_model_version": "fgt-model/1.0"
    },
    "local_transform": [
      [1, 0, 142],
      [0, 1, 34]
    ],
    "bounding_box": { "x_px": 0, "y_px": 0, "width_px": 320, "height_px": 56 },
    "absolute_bbox": { "x_px": 142, "y_px": 34, "width_px": 320, "height_px": 56 },
    "rotation_deg": 0,
    "constraints": { "horizontal": "LEFT", "vertical": "TOP" }
  },
  "layout": {
    "mode": "auto_layout",
    "axis": "horizontal",
    "sizing": { "horizontal": "hug", "vertical": "fixed" },
    "padding_px": { "top": 16, "right": 16, "bottom": 16, "left": 16 },
    "gap_px": 8,
    "wrap": false,
    "alignment": "MIN"
  },
  "paint": {
    "fills": [{ "kind": "solid", "color_rgba": [1, 1, 1, 1] }],
    "strokes": [],
    "opacity": 1,
    "corner_radius_px": 8,
    "effects": [],
    "masks": [],
    "clips_content": true,
    "visual_fallback_asset_id": null
  },
  "components": { "kind": "plain", "component_ref": null },
  "variables": { "bindings": [] },
  "assets": { "referenced_ids": [] },
  "availability": {
    "geometry.local_transform": { "status": "available" },
    "layout.gap_px": { "status": "available" },
    "paint.effects": { "status": "available" }
  }
}
```

## 8. `capability_unavailable` versus the REST commitment

- A property the Plugin API does not expose yields per-field `status = unavailable` with reason
  `capability_unavailable` and an explanatory warning in the envelope (FR08, FR09, PRD 8.5.3:
  "an unavailable API yields capability unavailable").
- The core never fetches the missing property from official REST or MCP endpoints. There is no
  silent REST fallback anywhere on the path (FR19, PRD 8.5.3, `AT01`, `AT02`), consistent with the
  commitment registry in `docs/00-overview/01-prd-analysis.md` section 10.
- Unavailable capability remains a valid answer, not a failure (D09). It denies the claim, not the
  request: the read succeeds with honest coverage.
- `CAPABILITY_UNAVAILABLE` as a whole-response error is reserved for requests that asked only for
  the unavailable capability (see `docs/04-data-contracts/05-error-catalog.md`).

## 9. Open questions

| ID    | Question                                                                                              | Owner / closure                                          |
| ----- | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| QNM01 | Exact `JSON_REST_V1` field deltas per editor version; needs a fixture diff gate per release           | G0/G1 evidence, see PRD 8.5.3 and prd-analysis section 9 |
| QNM02 | Whether mixed per-character styles stay one run on every seat, or splitting behavior varies           | Plugin owner, probe matrix (D09)                         |
| QNM03 | Reason vocabulary stability: is `visual_gap` distinct enough from `capability_unavailable` for agents | Contract review with tool surface                        |
| QNM04 | Coordinate-space (`absolute_space = page`) when page origin is not addressable on the requested node  | Probe with real editor at G0                             |
