/**
 * Skip-list for identifiers that must never be renamed.
 *
 * Organized by source: universal (Node.js module system), bundler-specific
 * (webpack, esbuild), and minifier-specific (SWC helpers).
 */

import type { BundlerType, MinifierType } from "../detection/types.js";

const UNIVERSAL = [
  "exports",
  "require",
  "module",
  "__filename",
  "__dirname"
] as const;

const WEBPACK = [
  "__webpack_require__",
  "__webpack_modules__",
  "__webpack_exports__",
  "__webpack_module_cache__"
] as const;

const ESBUILD = [
  "__commonJS",
  "__toESM",
  "__toCommonJS",
  "__export",
  "__require",
  "__name",
  "__publicField"
] as const;

const SWC = [
  "_interop_require_default",
  "_interop_require_wildcard",
  "_class_call_check",
  "_create_class",
  "_inherits",
  "_create_super",
  "_sliced_to_array",
  "_to_consumable_array",
  "_object_spread",
  "_object_spread_props",
  "_async_to_generator",
  "_ts_generator",
  "_define_property",
  "_object_destructuring_empty",
  "_extends",
  "_object_without_properties",
  "_tagged_template_literal"
] as const;

const cache = new Map<string, Set<string>>();

function cacheKey(
  bundlerType?: BundlerType,
  minifierType?: MinifierType
): string {
  return `${bundlerType ?? "none"}:${minifierType ?? "none"}`;
}

/**
 * Creates a Set of identifier names that must never be renamed.
 * Results are cached per bundler+minifier combination.
 */
export function createSkipSet(
  bundlerType?: BundlerType,
  minifierType?: MinifierType
): Set<string> {
  const key = cacheKey(bundlerType, minifierType);
  const cached = cache.get(key);
  if (cached) return cached;

  const set = new Set<string>(UNIVERSAL);

  if (bundlerType === "webpack") {
    for (const name of WEBPACK) set.add(name);
  }

  if (bundlerType === "esbuild") {
    for (const name of ESBUILD) set.add(name);
  }

  if (minifierType === "swc") {
    for (const name of SWC) set.add(name);
  }

  cache.set(key, set);
  return set;
}
