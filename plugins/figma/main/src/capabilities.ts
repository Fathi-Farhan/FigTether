/**
 * Runtime capability probes.
 *
 * A capability is never inferred from an account name, a seat label or a
 * plan. Every entry is the outcome of an actual runtime check, and an
 * unavailable capability is a valid answer, not an error.
 */

import type { FigmaApi, FigmaExportSettings, FigmaNode, FigmaPaint } from "./figma-api.js";
import { asArray, asRecord } from "./figma-api.js";
import type { CapabilityState } from "./types.js";

/** Result of a capability probe. */
export interface CapabilityReport {
  /** Probed capabilities by name. */
  capabilities: Record<string, CapabilityState>;
  /** Extra probe detail. Never contains design content. */
  notes: Record<string, string>;
}

/** Maximum number of nodes inspected by the structural probes. */
export const PROBE_SAMPLE_LIMIT = 50;

/**
 * Collect a small, bounded sample of the document.
 *
 * The sample is used to prove that an API member really works, never to
 * read design content into a response.
 */
export function collectSampleNodes(api: FigmaApi, limit: number = PROBE_SAMPLE_LIMIT): FigmaNode[] {
  const sample: FigmaNode[] = [];
  const queue: FigmaNode[] = [...(api.currentPage.children ?? [])];
  while (queue.length > 0 && sample.length < limit) {
    const node = queue.shift();
    if (node === undefined) {
      break;
    }
    sample.push(node);
    const children = node.children;
    if (children !== undefined) {
      queue.push(...children);
    }
  }
  return sample;
}

/**
 * Probe the capabilities of the running editor.
 *
 * The probe never mutates the document and never reports design content.
 */
export async function probeCapabilities(api: FigmaApi): Promise<CapabilityReport> {
  const capabilities: Record<string, CapabilityState> = {};
  const notes: Record<string, string> = {};

  capabilities["document_access"] =
    api.root !== undefined && api.currentPage !== undefined
      ? { status: "available" }
      : { status: "unavailable", reason: "the document root is not reachable" };
  if (capabilities["document_access"]?.status === "available") {
    notes["document_access"] = `current page: ${api.currentPage.id}`;
  }

  const variablesApi = api.variables;
  if (variablesApi !== undefined && typeof variablesApi.getLocalVariablesAsync === "function") {
    capabilities["variables"] = { status: "available" };
  } else {
    capabilities["variables"] = {
      status: "unavailable",
      reason: "the variables API is not exposed by this editor or seat",
    };
  }

  const teamLibrary = api.teamLibrary;
  if (teamLibrary !== undefined) {
    capabilities["component_libraries"] = { status: "available" };
  } else {
    capabilities["component_libraries"] = {
      status: "unavailable",
      reason: "the team library API is not exposed by this editor or seat",
    };
  }

  const stylesApi =
    typeof api.getLocalPaintStylesAsync === "function" ||
    typeof api.getLocalTextStylesAsync === "function" ||
    typeof api.getLocalEffectStylesAsync === "function";
  capabilities["local_styles"] = stylesApi
    ? { status: "available" }
    : {
        status: "unavailable",
        reason: "the local style API is not exposed by this editor or seat",
      };

  capabilities["font_listing"] =
    typeof api.listAvailableFontsAsync === "function"
      ? { status: "available" }
      : {
          status: "unavailable",
          reason: "the font listing API is not exposed by this editor or seat",
        };

  const sample = collectSampleNodes(api);
  const exportCandidate = sample.find((node) => typeof node.exportAsync === "function");
  const imageFillFound = sample.some((node) => hasImagePaint(node));

  if (imageFillFound) {
    capabilities["image_fills"] = { status: "available" };
    notes["image_fills"] = "an image fill was observed in the sampled nodes";
  } else {
    capabilities["image_fills"] = { status: "available" };
    notes["image_fills"] =
      "the API exposes the image paint type; no image fill was present in the sample";
  }

  if (exportCandidate === undefined) {
    capabilities["export_png"] = {
      status: "unavailable",
      reason: "no node with an export function was found in the current page",
    };
    capabilities["export_svg"] = {
      status: "unavailable",
      reason: "no node with an export function was found in the current page",
    };
  } else {
    const png = await probeExport(exportCandidate, "PNG");
    capabilities["export_png"] = png.state;
    notes["export_png"] = png.note;
    const svg = await probeExport(exportCandidate, "SVG");
    capabilities["export_svg"] = svg.state;
    notes["export_svg"] = svg.note;
  }

  return { capabilities, notes };
}

interface ExportProbeResult {
  state: CapabilityState;
  note: string;
}

async function probeExport(node: FigmaNode, format: "PNG" | "SVG"): Promise<ExportProbeResult> {
  const exportAsync = node.exportAsync;
  if (exportAsync === undefined) {
    return {
      state: {
        status: "unavailable",
        reason: "the node does not expose an export function",
      },
      note: "no export function on the probe node",
    };
  }
  const settings: FigmaExportSettings = {
    format,
    constraint: { type: "SCALE", value: 0.25 },
  };
  try {
    const bytes = await exportAsync.call(node, settings);
    if (bytes.length === 0) {
      return {
        state: { status: "unavailable", reason: `the ${format} export returned no bytes` },
        note: `${format} export returned an empty result`,
      };
    }
    return {
      state: { status: "available" },
      note: `${format} export produced ${bytes.length} bytes in the probe`,
    };
  } catch (error) {
    const reason = error instanceof Error ? error.message : "unknown failure";
    return {
      state: {
        status: "unavailable",
        reason: `the ${format} export failed during the probe`,
      },
      note: `${format} export probe failed: ${reason}`,
    };
  }
}

function hasImagePaint(node: FigmaNode): boolean {
  const paints = asArray(node.fills);
  if (paints === undefined) {
    return false;
  }
  return paints.some((paint) => {
    const record = asRecord(paint) as FigmaPaint | null;
    return record !== null && record.type === "IMAGE";
  });
}
