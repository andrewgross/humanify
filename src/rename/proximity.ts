/**
 * Proximity windowing for usedNames in large scopes: when a scope has many
 * bindings, prompts only carry the preserved names declared or referenced
 * near the batch's lines, plus well-known globals.
 */
import type { IsEligibleFn } from "./rename-eligibility.js";

/** Looser binding type for proximity windowing (only needs loc info, not path). */
export interface ProximityBinding {
  identifier?: { loc?: { start?: { line?: number } } | null };
  referencePaths?: Array<{
    node?: { loc?: { start?: { line?: number } } | null };
  }>;
}

/** Well-known names that should always appear in usedNames regardless of proximity */
const WELL_KNOWN_NAMES = new Set([
  "exports",
  "require",
  "module",
  "__filename",
  "__dirname",
  "console",
  "process",
  "Buffer",
  "Promise",
  "Object",
  "Array",
  "Map",
  "Set",
  "Error",
  "JSON",
  "Math",
  "undefined",
  "null",
  "NaN",
  "Infinity",
  "setTimeout",
  "setInterval",
  "clearTimeout",
  "clearInterval",
  "parseInt",
  "parseFloat",
  "isNaN",
  "isFinite",
  "encodeURI",
  "decodeURI",
  "encodeURIComponent",
  "decodeURIComponent"
]);

/** Minimum scope bindings before activating proximity windowing */
const WINDOWING_THRESHOLD = 100;

/** Line proximity radius for usedNames windowing */
const PROXIMITY_RADIUS = 100;

/**
 * Returns true if a name is within the proximity window given the binding info.
 */
function isNameInProximityWindow(
  _name: string,
  binding: ProximityBinding | undefined,
  minLine: number,
  maxLine: number,
  alreadyIncluded: boolean
): boolean {
  if (alreadyIncluded) return false;
  if (!binding) return true; // include if binding not found, to be safe

  const declLine = binding.identifier?.loc?.start?.line;
  if (declLine !== undefined && declLine >= minLine && declLine <= maxLine) {
    return true;
  }

  if (binding.referencePaths) {
    for (const refPath of binding.referencePaths) {
      const refLine = refPath.node?.loc?.start?.line;
      if (refLine !== undefined && refLine >= minLine && refLine <= maxLine) {
        return true;
      }
    }
  }

  return false;
}

/**
 * Computes a proximity-windowed subset of usedNames for module-level prompts.
 *
 * When the scope has >= WINDOWING_THRESHOLD bindings, only returns names whose
 * own declarations/references fall within +-PROXIMITY_RADIUS lines of the batch's
 * relevant lines. Always includes well-known names and excludes minified-looking names.
 *
 * When the scope has fewer bindings, returns all non-minified names.
 */
export function getProximateUsedNames(
  allUsedNames: Set<string>,
  batchLines: number[],
  scopeBindings: Record<string, ProximityBinding>,
  totalBindings: number,
  isEligible: IsEligibleFn
): Set<string> {
  const result = new Set<string>();

  // Always include well-known names that are in scope
  for (const name of allUsedNames) {
    if (WELL_KNOWN_NAMES.has(name)) {
      result.add(name);
    }
  }

  // Filter out eligible names (they'll be renamed) — keep only preserved names
  const preserved = [...allUsedNames].filter((n) => !isEligible(n));

  // If below threshold, return all non-minified names
  if (totalBindings < WINDOWING_THRESHOLD) {
    for (const name of preserved) {
      result.add(name);
    }
    return result;
  }

  // Compute the proximity window
  const minLine = Math.min(...batchLines) - PROXIMITY_RADIUS;
  const maxLine = Math.max(...batchLines) + PROXIMITY_RADIUS;

  for (const name of preserved) {
    if (
      isNameInProximityWindow(
        name,
        scopeBindings[name],
        minLine,
        maxLine,
        result.has(name)
      )
    ) {
      result.add(name);
    }
  }

  return result;
}
