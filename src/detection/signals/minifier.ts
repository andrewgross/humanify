import type { DetectionSignal } from "../types.js";

/**
 * Detect generic minification and attribute it to terser as the default.
 *
 * `void 0` (for `undefined`) and `!0`/`!1` (for `true`/`false`) are emitted by
 * EVERY modern minifier, so they identify code as *minified* but not *which*
 * minifier produced it. We therefore emit them at the lowest tier ("unknown"),
 * attributed to terser as the fallback minifier: a genuinely distinctive
 * esbuild/bun/swc signal (tier "likely") always outranks this fallback in
 * `detectBundle`, while a bundle with no distinctive fingerprint still resolves
 * to a sensible default instead of being mislabelled.
 */
export function detectTerser(code: string): DetectionSignal[] {
  const signals: DetectionSignal[] = [];

  if (/void 0/.test(code)) {
    signals.push({
      source: "terser",
      pattern: "void 0",
      minifier: "terser",
      tier: "unknown"
    });
  }

  if (/!0\b/.test(code) || /!1\b/.test(code)) {
    signals.push({
      source: "terser",
      pattern: "!0/!1 boolean coercion",
      minifier: "terser",
      tier: "unknown"
    });
  }

  return signals;
}

/**
 * Detect esbuild minification signals.
 * esbuild uses short lowercase identifiers and has distinct formatting.
 */
export function detectEsbuildMinifier(code: string): DetectionSignal[] {
  const signals: DetectionSignal[] = [];

  // esbuild banner comment
  if (/\/\/ .+\.js\n/.test(code.slice(0, 200))) {
    signals.push({
      source: "esbuild-minifier",
      pattern: "esbuild banner comment",
      minifier: "esbuild",
      tier: "likely"
    });
  }

  return signals;
}

/**
 * Detect Bun minification signals.
 * Bun uses 3-char mixed-case identifiers with $ (e.g., $a0, $bC).
 */
export function detectBunMinifier(code: string): DetectionSignal[] {
  const signals: DetectionSignal[] = [];

  // Bun's characteristic $-prefixed identifiers — count with early exit
  const dollarVarPattern = /\$[a-zA-Z][a-zA-Z0-9]/g;
  const threshold = 10;
  let count = 0;
  while (dollarVarPattern.exec(code) && ++count <= threshold) {
    /* count */
  }
  if (count > threshold) {
    signals.push({
      source: "bun-minifier",
      pattern: "$-prefixed mixed-case identifiers",
      minifier: "bun",
      tier: "likely"
    });
  }

  return signals;
}

/**
 * Distinctive swc helper function names (snake_case, multi-word). swc injects
 * these runtime helpers when lowering modern syntax, and the snake_case spelling
 * uniquely fingerprints swc: Babel emits the same helpers in camelCase
 * (`_class_call_check` here vs Babel's `_classCallCheck`).
 *
 * The full helper set lives in the SWC list in `src/rename/skip-list.ts` (the
 * source of truth). We deliberately use only the multi-word subset for detection
 * and omit single-word helpers that collide with Babel (`_extends`, `_inherits`),
 * which would otherwise cause false positives — precision over recall.
 */
const SWC_HELPER_MARKERS = [
  "_interop_require_default",
  "_interop_require_wildcard",
  "_class_call_check",
  "_create_class",
  "_create_super",
  "_sliced_to_array",
  "_to_consumable_array",
  "_object_spread",
  "_object_spread_props",
  "_async_to_generator",
  "_ts_generator",
  "_define_property",
  "_object_destructuring_empty",
  "_object_without_properties",
  "_tagged_template_literal"
] as const;

const SWC_HELPER_RE = new RegExp(`\\b(?:${SWC_HELPER_MARKERS.join("|")})\\b`);

/**
 * Detect swc minification by its distinctive snake_case helper names. A single
 * marker suffices — these exact spellings are produced only by swc/@swc/helpers.
 */
export function detectSwcMinifier(code: string): DetectionSignal[] {
  if (SWC_HELPER_RE.test(code)) {
    return [
      {
        source: "swc-minifier",
        pattern: "swc snake_case helper names",
        minifier: "swc",
        tier: "likely"
      }
    ];
  }

  return [];
}

export function detectMinifier(code: string): DetectionSignal[] {
  return [
    ...detectTerser(code),
    ...detectEsbuildMinifier(code),
    ...detectBunMinifier(code),
    ...detectSwcMinifier(code)
  ];
}
