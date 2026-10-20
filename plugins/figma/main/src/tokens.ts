/**
 * Variable and style resolution.
 *
 * A reference is never silently dropped. When a variable or a style cannot be
 * resolved, the reference keeps its id and carries the reason, so an agent can
 * tell a missing token from an absent one.
 */

import { assertJsonValue } from "@figtether/shared";
import type {
  FigmaApi,
  FigmaBoundVariable,
  FigmaStyle,
  FigmaVariable,
  FigmaVariableCollection,
} from "./figma-api.js";
import { asRecord } from "./figma-api.js";
import type { PluginVariable } from "./types.js";

/** Resolves variable and style references into the wire shape. */
export interface ReferenceResolver {
  /** Resolve one variable id. */
  resolveVariable(id: string): Promise<PluginVariable>;
  /** Resolve one style id. */
  resolveStyle(id: string): Promise<PluginVariable>;
}

/** Reason used when the variables API is not exposed. */
export const VARIABLES_UNAVAILABLE_REASON =
  "capability_unavailable: the variables API is not exposed by this editor or seat";

/** Reason used when a variable id does not resolve in this document. */
export const VARIABLE_UNRESOLVED_REASON = "the variable id does not resolve in this document";

/** Reason used when a style lives in a library outside the grant. */
export const STYLE_OUT_OF_GRANT_REASON = "not_in_grant";

/** Reason used when a style id does not resolve in this document. */
export const STYLE_UNRESOLVED_REASON = "the style id does not resolve in this document";

/** Maximum alias depth followed while resolving a variable value. */
const MAX_ALIAS_DEPTH = 8;

/** Resolves variables and styles against the local document. */
export class TokenResolver implements ReferenceResolver {
  private readonly variableCache = new Map<string, FigmaVariable | null>();
  private readonly styleCache = new Map<string, FigmaStyle | null>();
  private collectionCache: FigmaVariableCollection[] | null = null;

  constructor(private readonly api: FigmaApi) {}

  /** Whether the variables API is usable in this runtime. */
  get variablesAvailable(): boolean {
    const variables = this.api.variables;
    return variables !== undefined && typeof variables.getLocalVariablesAsync === "function";
  }

  /** Whether the local style API is usable in this runtime. */
  get stylesAvailable(): boolean {
    return (
      typeof this.api.getStyleByIdAsync === "function" ||
      typeof this.api.getStyleById === "function"
    );
  }

  /** Resolve one variable id. */
  async resolveVariable(id: string): Promise<PluginVariable> {
    const variables = this.api.variables;
    if (variables === undefined || typeof variables.getVariableByIdAsync !== "function") {
      return {
        id,
        name: "",
        resolved: false,
        unresolved_reason: VARIABLES_UNAVAILABLE_REASON,
      };
    }
    const variable = await this.lookupVariable(id);
    if (variable === null) {
      return {
        id,
        name: "",
        resolved: false,
        unresolved_reason: VARIABLE_UNRESOLVED_REASON,
      };
    }
    return this.describeVariable(variable);
  }

  /** Resolve one style id. */
  async resolveStyle(id: string): Promise<PluginVariable> {
    const style = await this.lookupStyle(id);
    if (style === null) {
      return {
        id,
        name: "",
        resolved: false,
        unresolved_reason: STYLE_UNRESOLVED_REASON,
      };
    }
    if (style.remote) {
      return {
        id: style.id,
        name: "",
        resolved: false,
        unresolved_reason: STYLE_OUT_OF_GRANT_REASON,
      };
    }
    return {
      id: style.id,
      name: style.name,
      resolved: true,
    };
  }

  /** All local variable collections, with their modes. */
  async listCollections(): Promise<FigmaVariableCollection[]> {
    const variables = this.api.variables;
    if (
      variables === undefined ||
      typeof variables.getLocalVariableCollectionsAsync !== "function"
    ) {
      return [];
    }
    if (this.collectionCache === null) {
      this.collectionCache = await variables.getLocalVariableCollectionsAsync();
    }
    return this.collectionCache;
  }

  /** All local variables. */
  async listVariables(): Promise<FigmaVariable[]> {
    const variables = this.api.variables;
    if (variables === undefined || typeof variables.getLocalVariablesAsync !== "function") {
      return [];
    }
    return await variables.getLocalVariablesAsync();
  }

  /** Describe a variable, including its alias target when it has one. */
  private async describeVariable(variable: FigmaVariable, depth = 0): Promise<PluginVariable> {
    const collections = await this.listCollections();
    const collection = collections.find(
      (candidate) => candidate.id === variable.variableCollectionId,
    );
    const modeId = this.modeOf(collection);
    const mode = collection?.modes.find((candidate) => candidate.modeId === modeId);
    const rawValue = modeId === undefined ? undefined : variable.valuesByMode?.[modeId];

    const aliasId = aliasIdOf(rawValue);
    if (aliasId !== undefined) {
      if (depth >= MAX_ALIAS_DEPTH) {
        return this.describe(
          {
            id: variable.id,
            name: variable.name,
            alias_target: aliasId,
            resolved: false,
            unresolved_reason: "the alias chain is too deep to resolve",
          },
          collection?.name,
          mode?.name,
        );
      }
      const target = await this.lookupVariable(aliasId);
      if (target === null) {
        return this.describe(
          {
            id: variable.id,
            name: variable.name,
            alias_target: aliasId,
            resolved: false,
            unresolved_reason: VARIABLE_UNRESOLVED_REASON,
          },
          collection?.name,
          mode?.name,
        );
      }
      const described = await this.describeVariable(target, depth + 1);
      return this.describe(
        {
          id: variable.id,
          name: variable.name,
          alias_target: aliasId,
          resolved: described.resolved,
          ...(described.unresolved_reason === undefined
            ? {}
            : { unresolved_reason: described.unresolved_reason }),
          ...(described.value === undefined ? {} : { value: described.value }),
        },
        collection?.name,
        mode?.name,
      );
    }

    return this.describe(
      {
        id: variable.id,
        name: variable.name,
        resolved: true,
        ...(rawValue === undefined ? {} : { value: jsonValue(rawValue) }),
      },
      collection?.name,
      mode?.name,
    );
  }

  /**
   * Add the optional members of a reference.
   *
   * The Rust contract treats an absent member and a member that holds
   * `None` the same way, so the optional members are only written when they
   * carry a value.
   */
  private describe(
    reference: PluginVariable,
    collection: string | undefined,
    mode: string | undefined,
  ): PluginVariable {
    if (collection !== undefined) {
      reference.collection = collection;
    }
    if (mode !== undefined) {
      reference.mode = mode;
    }
    return reference;
  }

  private modeOf(collection: FigmaVariableCollection | undefined): string | undefined {
    const modes = collection?.modes;
    if (modes === undefined || modes.length === 0) {
      return undefined;
    }
    const defaultMode = collection?.defaultModeId;
    if (defaultMode !== undefined && defaultMode !== "") {
      return defaultMode;
    }
    return modes[0]?.modeId;
  }

  private async lookupVariable(id: string): Promise<FigmaVariable | null> {
    const cached = this.variableCache.get(id);
    if (cached !== undefined) {
      return cached;
    }
    const variables = this.api.variables;
    if (variables === undefined) {
      return null;
    }
    let resolved: FigmaVariable | null = null;
    if (typeof variables.getVariableByIdAsync === "function") {
      resolved = await variables.getVariableByIdAsync(id);
    } else if (typeof variables.getLocalVariablesAsync === "function") {
      const all = await variables.getLocalVariablesAsync();
      resolved = all.find((candidate) => candidate.id === id) ?? null;
    }
    this.variableCache.set(id, resolved);
    return resolved;
  }

  private async lookupStyle(id: string): Promise<FigmaStyle | null> {
    const cached = this.styleCache.get(id);
    if (cached !== undefined) {
      return cached;
    }
    let resolved: FigmaStyle | null = null;
    if (typeof this.api.getStyleByIdAsync === "function") {
      resolved = await this.api.getStyleByIdAsync(id);
    } else if (typeof this.api.getStyleById === "function") {
      resolved = this.api.getStyleById(id);
    }
    this.styleCache.set(id, resolved);
    return resolved;
  }
}

/** Extract the target id of a variable alias value. */
export function aliasIdOf(value: unknown): string | undefined {
  const record = asRecord(value);
  if (record === null) {
    return undefined;
  }
  if (record["type"] !== "VARIABLE_ALIAS") {
    return undefined;
  }
  const id = record["id"];
  return typeof id === "string" ? id : undefined;
}

/**
 * Convert an unknown value into a JSON value.
 *
 * A value the contract cannot carry is reported as `null` instead of
 * breaking the answer of the whole operation.
 */
export function jsonValue(value: unknown): ReturnType<typeof assertJsonValue> {
  try {
    return assertJsonValue(value);
  } catch {
    return null;
  }
}

/** Extract the variable id of a bound variable entry. */
export function boundVariableId(binding: FigmaBoundVariable | undefined): string | undefined {
  if (binding === undefined) {
    return undefined;
  }
  if (binding.type !== "VARIABLE_ALIAS") {
    return undefined;
  }
  return binding.id;
}
