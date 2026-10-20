#!/usr/bin/env node
/**
 * Deterministic generator for the large FigTether fixtures.
 *
 * The generator produces the three large fixture documents of the public
 * corpus:
 *
 *   FX-MEDIUM     1,000 nodes, nested auto layout, mixed style text runs,
 *                 gradients, masks, aliases and modes, hidden and locked
 *                 nodes, non Latin text.
 *   FX-LARGE     10,000 nodes in scope, deep nesting, wide fan out.
 *   FX-CROSSPAGE 10,000 nodes spread over ten pages, one tree per page, so
 *                 that lazy loading and per page traversal can be exercised.
 *
 * Every fixture uses the plugin payload shape of
 * `crates/protocol/src/plugin.rs`, that is a `PluginDocument` with
 * `document_label`, `page_name`, `nodes` and `capabilities`.
 *
 * The output is deterministic: the same seed always produces byte identical
 * files. Run it from the repository root:
 *
 *   node fixtures/tools/generate.mjs             write the large fixtures
 *   node fixtures/tools/generate.mjs --check     verify the committed files
 *   node fixtures/tools/generate.mjs --only FX-LARGE
 *   node fixtures/tools/generate.mjs --manifest  rewrite fixtures/manifest.json
 *
 * The small fixtures (FX-SMALL, FX-MIXED, FX-ADVERSARIAL) are hand authored
 * literal files and are not produced by this script.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { buildManifest } from "./manifest.mjs";

/** Seed of every random choice in the generator. */
const SEED = 0x5eed1234;

/** Directory of the fixture corpus. */
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * Small deterministic pseudo random number generator.
 *
 * The algorithm is mulberry32. It is used instead of `Math.random` so that
 * the corpus never changes between two runs on two machines.
 */
function mulberry32(seed) {
  let state = seed >>> 0;
  return function random() {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Pick one element of a list. */
function pick(random, list) {
  return list[Math.floor(random() * list.length) % list.length];
}

/** Pick an integer between two bounds, both included. */
function between(random, low, high) {
  return low + Math.floor(random() * (high - low + 1));
}

/** Round a number to two decimals so that the output stays stable. */
function round(value) {
  return Math.round(value * 100) / 100;
}

/** Node types a generated tree may use. */
const NODE_TYPES = [
  "FRAME",
  "FRAME",
  "GROUP",
  "RECTANGLE",
  "ELLIPSE",
  "VECTOR",
  "LINE",
  "TEXT",
  "INSTANCE",
];

/** Font families used by the generated text nodes. */
const FONT_FAMILIES = [
  "Inter",
  "Inter",
  "Roboto",
  "Noto Sans JP",
  "Noto Sans Arabic",
  "Noto Color Emoji",
];

/** Text samples, including non Latin scripts and emoji. */
const TEXT_SAMPLES = [
  "Checkout",
  "Add to cart",
  "合計 1,280 円",
  "設定画面",
  "مرحبا بالعالم",
  "Order #4821 confirmed",
  "Ships in 2 days",
  "Available in 3 colors",
  "Pay now",
  "配送について",
  "Free returns",
  "Card ending 4242",
  "Sea 🐚 / Sun ☀ / Moon 🌙",
  "Prix : 24,99 EUR",
  "Lieferung am 14.03.",
];

/** Variable names used by the generated references. */
const VARIABLE_NAMES = [
  "color/brand/primary",
  "color/brand/surface",
  "color/text/primary",
  "space/gutter",
  "radius/medium",
  "number/opacity/disabled",
];

/** Style names used by the generated references. */
const STYLE_NAMES = [
  "Heading/Large",
  "Heading/Medium",
  "Body/Regular",
  "Effect/Elevation",
  "Fill/Brand",
];

/**
 * Build one plugin node.
 *
 * Undefined members are dropped so that the written file contains exactly
 * the members the plugin payload shape allows.
 */
function makeNode(id, parentId, nodeType, name, extra = {}) {
  const node = { id };
  if (parentId !== null) {
    node.parent_id = parentId;
  }
  node.node_type = nodeType;
  node.name = name;
  node.visible = extra.visible ?? true;
  node.locked = extra.locked ?? false;
  node.children = extra.children ?? [];
  if (extra.geometry) {
    node.geometry = extra.geometry;
  }
  if (extra.layout) {
    node.layout = extra.layout;
  }
  if (extra.text) {
    node.text = extra.text;
  }
  if (extra.fills) {
    node.fills = extra.fills;
  }
  if (extra.strokes) {
    node.strokes = extra.strokes;
  }
  if (extra.effects) {
    node.effects = extra.effects;
  }
  if (extra.component) {
    node.component = extra.component;
  }
  if (extra.variables) {
    node.variables = extra.variables;
  }
  if (extra.styles) {
    node.styles = extra.styles;
  }
  if (extra.unavailable_fields) {
    node.unavailable_fields = extra.unavailable_fields;
  }
  return node;
}

/** Build a geometry block. */
function makeGeometry(random, x, y, width, height, rotation = 0) {
  const scale = random() < 0.1 ? 1.5 : 1;
  return {
    local_transform: [scale, 0, 0, scale],
    absolute_transform: [scale, 0, 0, scale],
    bounding_box: [round(x), round(y), round(width), round(height)],
    rotation,
    constraints_horizontal: pick(random, ["LEFT", "RIGHT", "CENTER", "SCALE"]),
    constraints_vertical: pick(random, ["TOP", "BOTTOM", "CENTER", "SCALE"]),
    coordinate_space: "page",
  };
}

/** Build an auto layout block. */
function makeLayout(random, axis) {
  return {
    auto_layout: true,
    axis,
    gap: pick(random, [0, 4, 8, 12, 16, 24]),
    padding: [
      pick(random, [0, 8, 16, 24]),
      pick(random, [0, 8, 16, 24]),
      pick(random, [0, 8, 16, 24]),
      pick(random, [0, 8, 16, 24]),
    ],
    primary_axis_align: pick(random, ["MIN", "CENTER", "MAX", "SPACE_BETWEEN"]),
    counter_axis_align: pick(random, ["MIN", "CENTER", "MAX"]),
    sizing: pick(random, ["FIXED", "HUG", "FILL"]),
    wraps: random() < 0.2,
  };
}

/** Build one styled text run. */
function makeRun(random, start, end, size) {
  return {
    start,
    end,
    font_family: pick(random, FONT_FAMILIES),
    font_style: pick(random, ["Regular", "Medium", "Semi Bold", "Bold"]),
    font_weight: pick(random, [400, 500, 600, 700]),
    font_size: size,
    line_height: round(size * 1.4),
    letter_spacing: pick(random, [0, 0.2, 0.4, -0.1]),
    font_available: true,
  };
}

/** Build a text block with one or two styled runs. */
function makeText(random) {
  const content = pick(random, TEXT_SAMPLES);
  const size = pick(random, [12, 14, 16, 20, 24, 32]);
  const first = makeRun(random, 0, Math.max(1, Math.ceil(content.length / 2)), size);
  const runs = [first];
  if (content.length > 4 && random() < 0.5) {
    const second = makeRun(random, first.end, content.length, size);
    second.font_family = pick(random, FONT_FAMILIES);
    second.font_weight = pick(random, [400, 600, 700]);
    second.font_size = round(size * 1.25);
    runs.push(second);
  }
  return {
    text: content,
    runs,
    text_align: pick(random, ["LEFT", "CENTER", "RIGHT", "JUSTIFIED"]),
    truncated: random() < 0.1,
  };
}

/** Build a solid fill. */
function makeSolidFill(random) {
  return {
    kind: "SOLID",
    color: [round(random()), round(random()), round(random()), 1],
    gradient_stops: [],
    opacity: 1,
    visible: true,
  };
}

/** Build a gradient fill with two or three stops. */
function makeGradientFill(random) {
  const stops = [];
  const count = between(random, 2, 3);
  for (let index = 0; index < count; index += 1) {
    stops.push({
      position: round(index / (count - 1)),
      color: [round(random()), round(random()), round(random()), 1],
    });
  }
  return {
    kind: "GRADIENT",
    gradient_stops: stops,
    opacity: 1,
    visible: true,
  };
}

/** Build an effect. */
function makeEffect(random) {
  const kind = pick(random, ["DROP_SHADOW", "INNER_SHADOW", "BLUR", "BACKGROUND_BLUR"]);
  const effect = {
    kind,
    visible: true,
    radius: pick(random, [2, 4, 8, 16, 24]),
  };
  if (kind === "DROP_SHADOW" || kind === "INNER_SHADOW") {
    effect.color = [0, 0, 0, 0.25];
  }
  return effect;
}

/** Build a variable reference. */
function makeVariable(random, index) {
  const resolved = random() < 0.8;
  const reference = {
    id: `VariableID:1:${index}`,
    name: pick(random, VARIABLE_NAMES),
    collection: random() < 0.9 ? pick(random, ["Brand", "Core", "Semantic"]) : null,
    mode: random() < 0.8 ? pick(random, ["Light", "Dark", "High Contrast"]) : null,
    resolved,
  };
  if (reference.collection === null) {
    delete reference.collection;
  }
  if (reference.mode === null) {
    delete reference.mode;
  }
  if (resolved) {
    reference.value = pick(random, ["#0a5cff", 16, 1, "0.5"]);
  } else if (random() < 0.5) {
    reference.alias_target = `VariableID:1:${between(random, 100, 200)}`;
    reference.unresolved_reason = "alias target is outside the grant";
  } else {
    reference.unresolved_reason = pick(random, [
      "collection is not published for this seat",
      "mode is not available in this document",
    ]);
  }
  return reference;
}

/** Build a style reference. */
function makeStyle(random, index) {
  const reference = {
    id: `S:style:${index}`,
    name: pick(random, STYLE_NAMES),
    resolved: random() < 0.9,
  };
  if (reference.resolved) {
    reference.value = pick(random, ["Inter/16", "#0a5cff", 8]);
  } else {
    reference.unresolved_reason = "style is not published for this seat";
  }
  return reference;
}

/** Build a component reference of an instance node. */
function makeComponent(random, index) {
  const component = {
    instance_id: `2:${index}`,
    variant_properties: [],
  };
  if (random() < 0.85) {
    component.main_component_id = `10:${between(random, 1, 40)}`;
  }
  const variants = ["Size", "State", "Tone"];
  const count = between(random, 1, 3);
  for (let position = 0; position < count; position += 1) {
    component.variant_properties.push({
      name: pick(random, variants),
      value: pick(random, ["Small", "Medium", "Large", "Default", "Brand", "Neutral"]),
    });
  }
  return component;
}

/** Capabilities reported by the plugin runtime of a generated fixture. */
function makeCapabilities() {
  return {
    component_properties: { status: "available" },
    document_read: { status: "available" },
    export_png: { status: "available" },
    export_svg: { status: "unavailable", reason: "seat does not expose SVG export" },
    page_read: { status: "available" },
    variables: { status: "available" },
  };
}

/**
 * Decorate one node with the attributes a generated fixture must carry.
 *
 * The density scales how many members a node carries. The medium fixture
 * uses a rich density, the two large fixtures use a lean density because
 * they exist to exercise scale, not content variety.
 */
function decorate(node, random, depth, index, density = 1) {
  const isFrame = node.node_type === "FRAME" || node.node_type === "GROUP";
  const isText = node.node_type === "TEXT";
  const isInstance = node.node_type === "INSTANCE";

  node.geometry = makeGeometry(random, depth * 8, index * 4, 120, 48);

  if (isFrame && random() < 0.7 * density) {
    node.layout = makeLayout(random, pick(random, ["HORIZONTAL", "VERTICAL", "NONE"]));
  }
  if (isText) {
    node.text = makeText(random);
  }
  if (isInstance) {
    node.component = makeComponent(random, index);
  }
  if (random() < 0.5 * density) {
    node.fills = [random() < 0.25 ? makeGradientFill(random) : makeSolidFill(random)];
  }
  if (random() < 0.15 * density) {
    node.strokes = [makeSolidFill(random)];
  }
  if (random() < 0.2 * density) {
    node.effects = [makeEffect(random)];
  }
  if (random() < 0.3 * density) {
    node.variables = [makeVariable(random, index)];
  }
  if (random() < 0.2 * density) {
    node.styles = [makeStyle(random, index)];
  }
  if (random() < 0.08) {
    node.visible = false;
  }
  if (random() < 0.06) {
    node.locked = true;
  }
  const unavailable = [];
  if (node.node_type === "GROUP" && random() < 0.5) {
    // The plugin payload shape does not carry the mask flag, so the mask is
    // reported as unavailable instead of being invented.
    unavailable.push("mask.is_mask");
  }
  if (random() < 0.05) {
    unavailable.push(
      pick(random, [
        "geometry.bounding_box",
        "typography.line_height",
        "variables.alias_target",
        "fills.opacity",
      ]),
    );
  }
  if (unavailable.length > 0) {
    node.unavailable_fields = unavailable;
  }
  return node;
}

/**
 * Distribute `total` nodes over `levels` levels with a growing width.
 *
 * The first level stays narrow and every following level is wider, so that a
 * generated tree has both nesting and fan out.
 */
function levelSizes(total, levels, factor) {
  const weights = [];
  let weight = 1;
  for (let level = 0; level < levels; level += 1) {
    weights.push(weight);
    weight *= factor;
  }
  const sum = weights.reduce((total, value) => total + value, 0);
  const rest = total - 1;
  const sizes = weights.map((value) => Math.max(1, Math.floor((value * rest) / sum)));
  let assigned = sizes.reduce((total, value) => total + value, 0);
  let index = sizes.length - 1;
  while (assigned < rest) {
    sizes[index] += 1;
    assigned += 1;
    index = (index - 1 + sizes.length) % sizes.length;
  }
  return sizes;
}

/**
 * Build a tree of a given size.
 *
 * The nodes are written in breadth first order, which is the order a
 * document reader sees them in.
 */
function buildTree({ root, total, levels, factor, factory, prefix }) {
  const sizes = levelSizes(total, levels, factor);
  const nodes = [root];
  let current = [root];
  let counter = 0;
  for (const size of sizes) {
    const next = [];
    for (const parent of current) {
      const share = Math.ceil(size / current.length);
      for (let position = 0; position < share && next.length < size; position += 1) {
        counter += 1;
        const id = `${prefix}:${counter}`;
        const child = factory(id, parent, counter);
        parent.children.push(child.id);
        nodes.push(child);
        next.push(child);
      }
      if (next.length >= size) {
        break;
      }
    }
    current = next;
  }
  return nodes;
}

/** Build the FX-MEDIUM fixture. */
function buildMedium() {
  const random = mulberry32(SEED);
  const total = 1000;
  const nodes = [];
  let counter = 0;

  const root = makeNode("2:0", null, "FRAME", "Medium Fixture Root", { children: [] });
  decorate(root, random, 0, 0, 1);

  const tree = buildTree({
    root,
    total,
    levels: 5,
    factor: 3,
    prefix: "2",
    factory: (id, parent, index) => {
      counter += 1;
      const nodeType = pick(random, NODE_TYPES);
      const node = makeNode(id, parent.id, nodeType, `${nodeType.toLowerCase()}-${counter}`, {
        children: [],
      });
      decorate(node, random, index % 6, index, 1);
      return node;
    },
  });

  return {
    document_label: "FigTether fixture FX-MEDIUM",
    page_name: "Medium Page",
    nodes: tree,
    capabilities: makeCapabilities(),
  };
}

/** Build the FX-LARGE fixture. */
function buildLarge() {
  const random = mulberry32(SEED + 1);
  const total = 10000;
  let counter = 0;

  const root = makeNode("3:0", null, "FRAME", "Large Fixture Root", { children: [] });
  decorate(root, random, 0, 0, 0.35);

  const tree = buildTree({
    root,
    total,
    levels: 32,
    factor: 1.35,
    prefix: "3",
    factory: (id, parent, index) => {
      counter += 1;
      const nodeType = pick(random, NODE_TYPES);
      const node = makeNode(id, parent.id, nodeType, `${nodeType.toLowerCase()}-${counter}`, {
        children: [],
      });
      decorate(node, random, index % 32, index, 0.35);
      return node;
    },
  });

  return {
    document_label: "FigTether fixture FX-LARGE",
    page_name: "Large Page",
    nodes: tree,
    capabilities: makeCapabilities(),
  };
}

/** Build the FX-CROSSPAGE fixture. */
function buildCrosspage() {
  const random = mulberry32(SEED + 2);
  const pages = 10;
  const perPage = 1000;
  const nodes = [];

  for (let page = 1; page <= pages; page += 1) {
    const pageRoot = makeNode(`4:${page}:0`, null, "FRAME", `Page ${page} Root`, {
      children: [],
    });
    decorate(pageRoot, random, 0, page, 0.35);
    let counter = 0;
    const tree = buildTree({
      root: pageRoot,
      total: perPage,
      levels: 5,
      factor: 3,
      prefix: `4:${page}`,
      factory: (id, parent, index) => {
        counter += 1;
        const nodeType = pick(random, NODE_TYPES);
        const node = makeNode(id, parent.id, nodeType, `${nodeType.toLowerCase()}-${counter}`, {
          children: [],
        });
        decorate(node, random, index % 6, index, 0.35);
        return node;
      },
    });
    nodes.push(...tree);
  }

  return {
    document_label: "FigTether fixture FX-CROSSPAGE",
    page_name: "Pages 1 to 10",
    nodes,
    capabilities: makeCapabilities(),
  };
}

/** The generated fixtures. */
const GENERATED = {
  "FX-MEDIUM": { file: "fx-medium.json", build: buildMedium },
  "FX-LARGE": { file: "fx-large.json", build: buildLarge },
  "FX-CROSSPAGE": { file: "fx-crosspage.json", build: buildCrosspage },
};

/**
 * Render a JSON value.
 *
 * The corpus is stored with one node per line, so that a fixture stays
 * greppable and diffable while the file stays small. Every other value uses
 * the usual two space indentation.
 */
function renderValue(value, level, inlineItems) {
  const pad = "  ".repeat(level);
  if (Array.isArray(value)) {
    if (value.length === 0) {
      return "[]";
    }
    if (inlineItems) {
      const items = value.map((item) => `${pad}  ${JSON.stringify(item)}`);
      return `[\n${items.join(",\n")}\n${pad}]`;
    }
    const items = value.map((item) => `${pad}  ${renderValue(item, level + 1, false)}`);
    return `[\n${items.join(",\n")}\n${pad}]`;
  }
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value);
    if (keys.length === 0) {
      return "{}";
    }
    const items = keys.map((key) => {
      const child = value[key];
      const rendered =
        key === "nodes" && Array.isArray(child)
          ? renderValue(child, level + 1, true)
          : renderValue(child, level + 1, false);
      return `${pad}  ${JSON.stringify(key)}: ${rendered}`;
    });
    return `{\n${items.join(",\n")}\n${pad}}`;
  }
  return JSON.stringify(value);
}

/** Render one document of the corpus. */
function renderDocument(document) {
  const keys = Object.keys(document);
  const items = keys.map((key) => {
    const value = document[key];
    const rendered =
      key === "nodes" && Array.isArray(value)
        ? renderValue(value, 1, true)
        : renderValue(value, 1, false);
    return `  ${JSON.stringify(key)}: ${rendered}`;
  });
  return `{\n${items.join(",\n")}\n}\n`;
}

/** Serialize a document the way the corpus stores it. */
function serialize(document) {
  return renderDocument(document);
}

/** Read a file of the corpus. */
function read(file) {
  return readFileSync(join(ROOT, file), "utf8");
}

/** Write a file of the corpus. */
function write(file, text) {
  writeFileSync(join(ROOT, file), text, "utf8");
}

async function main() {
  const args = process.argv.slice(2);
  const check = args.includes("--check");
  const manifestOnly = args.includes("--manifest");
  const onlyIndex = args.indexOf("--only");
  const only = onlyIndex === -1 ? null : args[onlyIndex + 1];

  if (!manifestOnly) {
    for (const [id, entry] of Object.entries(GENERATED)) {
      if (only !== null && only !== id) {
        continue;
      }
      const text = serialize(entry.build());
      if (check) {
        if (read(entry.file) !== text) {
          console.error(`${entry.file} differs from the generated output`);
          process.exitCode = 1;
        } else {
          console.log(`${entry.file} is up to date`);
        }
      } else {
        write(entry.file, text);
        console.log(`${entry.file} written`);
      }
    }
  }

  if (check || manifestOnly) {
    const text = `${JSON.stringify(buildManifest(ROOT), null, 2)}\n`;
    if (check) {
      if (read("manifest.json") !== text) {
        console.error("manifest.json differs from the generated output");
        process.exitCode = 1;
      } else {
        console.log("manifest.json is up to date");
      }
    } else {
      write("manifest.json", text);
      console.log("manifest.json written");
    }
  }
}

await main();
