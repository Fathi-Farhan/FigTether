# FigTether Fixture Corpus

Status: `Published`. The files in this directory are the public fixture corpus of FigTether. They
are used by the test layers, the acceptance catalog and the benchmark plan.

## License

The fixture corpus is released under the **MIT License** (SPDX `MIT`), copyright (c) 2026 FigTether
maintainers. The MIT half of the project license `MIT OR Apache-2.0` applies, see
`docs/02-architecture/adr/ADR-009-licensing.md`.

The corpus contains no third party material. Every file is project owned work, so no attribution
from another project applies and no license duty is carried over.

## No client design

**No client design is included in this corpus.** Every node, name, text, colour and coordinate is
synthetic and was written for this repository. Nothing in the corpus is derived from a real
customer file, a real screen, a real brand, or any other party property. The text samples are
generic phrases in English, Japanese, Arabic and one emoji sequence, written for this corpus. The
identifiers are opaque strings in the Figma node id shape (`1:23`) and carry no routing key, no file
key and no user identity.

## Fixture identifiers

The identifiers are stable and must not be reused for a different shape. A content revision carries
a revision suffix and bumps the revision of the corpus in `fixtures/manifest.json`.

| ID | File | Nodes | Purpose | Consumed by |
| --- | --- | --- | --- | --- |
| `FX-SMALL` | `fx-small.json` | 100 | Baseline fresh reads, install checks and runs without throttling: simple frames, text, one image fill and one variant instance. | `AT02`, `AT03`, `AT18`, `N02` |
| `FX-MEDIUM` | `fx-medium.json` | 1,000 | Steady state traversal, cache behaviour and pagination at moderate size: nested auto layout, mixed style text runs, gradients, masks, aliases and modes, hidden and locked nodes, non Latin text. | `AT04`, `AT07`, `AT08`, `AT10`, `AT12`, `AT17` |
| `FX-LARGE` | `fx-large.json` | 10,000 | Pagination limits, cursor binding and truncation: deep nesting and a wide fan out. | `AT07`, `AT08`, `AT13` |
| `FX-CROSSPAGE` | `fx-crosspage.json` | 10,000 | Lazy loading and per page traversal: ten pages, one tree per page, so that no page has to be loaded up front. | `AT07`, `AT19` |
| `FX-MIXED` | `fx-mixed.json` | 201 | Normalization and export fidelity: one document that carries every unknown and edge case of the attribute checklist, used as the ground truth of the normalized output. | `AT04`, `AT05`, `AT10`, `AT11`, `AT12`, `AT17`, `AT18` |
| `FX-ADVERSARIAL` | `fx-adversarial.json` | 312 | Parser and limit hardening: the inputs of the negative tests. | `AT06`, `AT07`, `AT10`, `AT11`, `AT13` |

The node count of every fixture, its size in bytes and its sha256 are recorded in
`fixtures/manifest.json`. The benchmark plan references the identifiers and the hashes, never a
path, so that a run can be reproduced against a frozen corpus.

## Shape of a fixture

Every fixture is one plugin payload document, the shape of `PluginDocument` in
`crates/protocol/src/plugin.rs`:

```json
{
  "document_label": "FigTether fixture FX-SMALL",
  "page_name": "Small Page",
  "nodes": [ ... ],
  "capabilities": { ... }
}
```

1. `nodes` is a flat list. The tree is expressed by `parent_id` and `children`. A node without
   `parent_id` is a root of the traversed scope.
2. Every member of a node is optional except `id` and `node_type`. A member the plugin runtime
   could not obtain is absent and named in `unavailable_fields`, never sent as an empty value.
3. The files store one node per line, so a fixture stays greppable and diffable while the file
   stays small. The formatting is stable and part of the hash.
4. `capabilities` maps a capability name to a capability state of the model
   (`available`, `unavailable` with a reason, `unsupported` with a reason).

## Generator

`FX-MEDIUM`, `FX-LARGE` and `FX-CROSSPAGE` are produced by a generator, because writing 1,000 and
10,000 nodes by hand is not realistic. The generator lives in `fixtures/tools/generate.mjs` and
uses a fixed seed, so its output is byte identical on every run and on every machine.

```sh
node fixtures/tools/generate.mjs             # write the three large fixtures
node fixtures/tools/generate.mjs --check     # fail when a committed file differs
node fixtures/tools/generate.mjs --only FX-LARGE
node fixtures/tools/generate.mjs --manifest  # rewrite fixtures/manifest.json
```

`FX-SMALL`, `FX-MIXED` and `FX-ADVERSARIAL` are hand authored literal files. They are committed as
they are and are never rewritten by the generator.

`fixtures/tools/manifest.mjs` holds the static metadata of the corpus. The generator combines it
with the size and the hash of every committed file, so `fixtures/manifest.json` always describes
the files that are in the repository.

## Expected normalized output

`fixtures/expected/fx-small.normalized.json` is the expected result of normalizing `FX-SMALL`. It
is the ground truth of `AT10` and the reference of a normalizer implementation. The rules it
follows are the rules of `docs/04-data-contracts/04-normalized-model.md` and of the model in
`crates/model/src/normalized.rs`:

1. The file is a response envelope in `schema_version` `1.0`, with the normalized nodes in
   `data.nodes`, one entry per node of the fixture, in document order.
2. `summary.child_count` is the number of children of the node inside the traversed scope.
3. `geometry.local_transform` and `geometry.absolute_transform` take their translation from the
   origin of the bounding box, their scale from the first and the last member of the transform
   matrix, and their rotation from the rotation of the node. The coordinate space is carried over
   unchanged.
4. `layout.sizing` is the lower case form of the plugin value: `FIXED` is `fixed`, `HUG` is `hug`,
   `FILL` is `fill`. `layout.padding` keeps the order of the plugin padding, top, right, bottom,
   left.
5. `typography.runs` keeps the runs of the plugin payload one to one, including `font_available`.
   A text node without a run keeps an empty run list.
6. `fills`, `strokes` and `effects` use the lower case kinds of the model: `solid`, `gradient`,
   `image` and `unknown` for a paint, `drop_shadow`, `inner_shadow`, `blur`, `background_blur` and
   `unknown` for an effect. An effect kind the platform did not classify becomes `unknown` and
   stays visible.
7. `component.resolved` is true when the instance carries a `main_component_id`. An instance
   without one is unresolved and stays in the output.
8. Variable and style references keep their identifier, name, collection, mode, value, alias
   target, resolved flag and unresolved reason. An unresolved reference is never dropped.
9. `coverage.unavailable_fields` is the sorted union of the unavailable fields of every returned
   node.
10. `observation.content_hash` is the canonical hash of `data`, computed with the
    `canonical_hash` function of `crates/model/src/envelope.rs`.

An expected file is regenerated only by an explicit contract migration, never by hand during a fix.
A `schema_version` bump regenerates every expected file in the same commit, and the round trip
against `schemas/` runs again.

## Negative test inputs of `FX-ADVERSARIAL`

The fixture is hostile input on purpose. The cases and the identifiers they use are:

| Case | Where | Expected answer |
| --- | --- | --- |
| A node id that does not exist | The id `9:999999` is not in the document. `get_nodes` with that id must answer `NODE_NOT_FOUND`. | `NODE_NOT_FOUND` |
| A missing parent reference | `9:603` points to the parent `9:999999`, which does not exist. The node stays in the output and the broken reference is reported. | node kept, reference reported |
| A missing child reference | `9:604` lists the child `9:999998`, which does not exist. | node kept, reference reported |
| A cycle in the parent relation | `9:1` points to `9:2` and `9:2` points to `9:1`. A traversal must detect the cycle instead of looping. | traversal refuses the cycle |
| A very deep chain | `9:10` to `9:309` form a chain of 300 frames. The depth budget is 32, so a traversal must stop and report the truncation. | truncation with a cursor |
| A node that claims more than 10,000 descendants | `9:400` lists 10,001 child identifiers. The traversal budget of 10,000 visited nodes applies, so the read must be partial or must answer `SCOPE_TOO_LARGE`. | partial result or `SCOPE_TOO_LARGE` |
| A text node with empty runs | `9:500` has an empty text and an empty run list. | empty text, no invented run |
| An unknown node type | `9:600` has the type `QUANTUM_WIDGET`. The type is preserved verbatim. | type preserved |
| A very long name | `9:601` has a name of more than 500 characters. | name preserved |
| A very large text | `9:602` has a text of more than 20,000 characters. | text preserved |
| Every field unavailable | `9:606` reports all of its fields as unavailable. | fields reported, not invented |

## Provenance and hashing

Every fixture records its origin, its creation script, its revision and its sha256 in
`fixtures/manifest.json`, as the test strategy asks for. A gate run recomputes the hashes at the
start of the run, and a mismatch invalidates the run. A fixture change is never merged in the same
commit as a product change, so that every run keeps a single variable.
