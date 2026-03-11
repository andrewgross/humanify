import type { DetectionSignal } from "../types.js";

/**
 * Detect terser minification signals.
 * terser uses `void 0` for undefined, `!0`/`!1` for true/false,
 * and sequential single-char variable names (a, b, c, ...).
 */
export function detectTerser(code: string): DetectionSignal[] {
  const signals: DetectionSignal[] = [];

  if (/void 0/.test(code)) {
    signals.push({
      source: "terser",
      pattern: "void 0",
      minifier: "terser",
      tier: "likely",
    });
  }

  if (/!0\b/.test(code) || /!1\b/.test(code)) {
    signals.push({
      source: "terser",
      pattern: "!0/!1 boolean coercion",
      minifier: "terser",
      tier: "likely",
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
      tier: "likely",
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
  while (dollarVarPattern.exec(code) && ++count <= threshold) { /* count */ }
  if (count > threshold) {
    signals.push({
      source: "bun-minifier",
      pattern: "$-prefixed mixed-case identifiers",
      minifier: "bun",
      tier: "likely",
    });
  }

  return signals;
}

export function detectMinifier(code: string): DetectionSignal[] {
  return [
    ...detectTerser(code),
    ...detectEsbuildMinifier(code),
    ...detectBunMinifier(code),
  ];
}
