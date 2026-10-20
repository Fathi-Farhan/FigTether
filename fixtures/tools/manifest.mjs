/**
 * Metadata of the FigTether fixture corpus.
 *
 * The static part of the manifest lives here. The generator of the corpus
 * combines it with the size and the hash of every file, so that
 * `fixtures/manifest.json` always describes the files that are actually
 * committed.
 */

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/** Revision of the corpus. Bump it when a fixture changes. */
export const REVISION = "r1";

/** License of the corpus. */
export const LICENSE = "MIT";

/** Static metadata of every fixture of the corpus. */
export const FIXTURES = [
  {
    id: "FX-SMALL",
    file: "fx-small.json",
    origin: "hand-authored",
    purpose:
      "Baseline fresh reads, install checks and throttling free runs: 100 nodes, simple frames, text, one image fill and one variant instance.",
    attributes: [
      "simple frames",
      "text nodes",
      "solid fills",
      "strokes",
      "image fill",
      "variant instance",
      "auto layout",
      "drop shadow effect",
      "geometry",
    ],
    expected: "expected/fx-small.normalized.json",
  },
  {
    id: "FX-MEDIUM",
    file: "fx-medium.json",
    origin: "generated",
    purpose:
      "Steady state traversal, cache behaviour and pagination at moderate size: 1,000 nodes with nested auto layout, mixed style text runs, gradients, masks, aliases and modes, hidden and locked nodes and non Latin text.",
    attributes: [
      "nested auto layout",
      "mixed style text runs",
      "gradients",
      "masks",
      "variable aliases",
      "variable modes",
      "hidden nodes",
      "locked nodes",
      "non-Latin text",
      "component instances",
      "unavailable fields",
      "traversal scale",
    ],
    expected: null,
  },
  {
    id: "FX-LARGE",
    file: "fx-large.json",
    origin: "generated",
    purpose:
      "Pagination limits, cursor binding and truncation: 10,000 nodes in scope with deep nesting and a wide fan out.",
    attributes: [
      "traversal scale",
      "deep nesting",
      "wide fan out",
      "auto layout",
      "text nodes",
      "gradients",
      "component instances",
      "hidden nodes",
    ],
    expected: null,
  },
  {
    id: "FX-CROSSPAGE",
    file: "fx-crosspage.json",
    origin: "generated",
    purpose:
      "Lazy loading and per page traversal: 10,000 nodes spread over ten pages, one tree per page, so that no page has to be loaded up front.",
    attributes: [
      "multi page document",
      "lazy loading",
      "per page roots",
      "traversal scale",
      "auto layout",
      "text nodes",
      "gradients",
    ],
    expected: null,
  },
  {
    id: "FX-MIXED",
    file: "fx-mixed.json",
    origin: "hand-authored",
    purpose:
      "Normalization and export fidelity: one document that carries every unknown and edge case of the attribute checklist, used as the ground truth of the normalized output.",
    attributes: [
      "unknown node type",
      "missing node reference",
      "unresolved alias",
      "unresolved mode",
      "unsupported effect",
      "unknown paint kind",
      "non-Latin text",
      "missing font",
      "nested layout",
      "mixed text",
      "variants",
      "gradients",
      "masks",
      "image fills",
      "hidden nodes",
      "locked nodes",
      "unavailable fields",
      "geometry edge cases",
      "long names",
    ],
    expected: null,
  },
  {
    id: "FX-ADVERSARIAL",
    file: "fx-adversarial.json",
    origin: "hand-authored",
    purpose:
      "Parser and limit hardening: inputs for the negative tests, among them a node id that does not exist, a cycle in the parent relation, a very deep chain, a node that claims more than 10,000 descendants and a text node with empty runs.",
    attributes: [
      "missing node reference",
      "missing parent reference",
      "parent cycle",
      "deep chain",
      "over budget descendant claim",
      "empty text runs",
      "unknown node type",
      "long names",
      "large text",
      "all fields unavailable",
    ],
    expected: null,
  },
];

/**
 * Build the manifest of the corpus.
 *
 * The node count, the size and the hash are read from the committed files,
 * so the manifest can never describe a file that is not in the repository.
 */
export function buildManifest(root) {
  const fixtures = FIXTURES.map((entry) => {
    const bytes = readFileSync(join(root, entry.file));
    const document = JSON.parse(bytes.toString("utf8"));
    const fixture = {
      id: entry.id,
      file: entry.file,
      revision: REVISION,
      origin: entry.origin,
      purpose: entry.purpose,
      node_count: Array.isArray(document.nodes) ? document.nodes.length : 0,
      size_bytes: bytes.length,
      sha256: createHash("sha256").update(bytes).digest("hex"),
      attributes: entry.attributes,
    };
    if (entry.expected !== null) {
      const expectedBytes = readFileSync(join(root, entry.expected));
      fixture.expected = {
        file: entry.expected,
        size_bytes: expectedBytes.length,
        sha256: createHash("sha256").update(expectedBytes).digest("hex"),
      };
    }
    return fixture;
  });

  return {
    schema_version: "1.0",
    revision: REVISION,
    license: LICENSE,
    shape: "figtether_plugin_document",
    generator: "fixtures/tools/generate.mjs",
    fixtures,
  };
}
