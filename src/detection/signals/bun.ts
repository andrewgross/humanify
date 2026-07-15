import type { DetectionSignal } from "../types.js";

/** Bun emits a `// @bun …` banner as the FIRST line of every bundle it
 * builds (e.g. `// @bun @bytecode @bun-cjs`). Anchored to the start so a
 * stray `@bun` in a string or later comment never matches. This is the one
 * marker present on BOTH bundle shapes below. */
const BUN_BANNER = /^\s*\/\/\s*@bun\b/;

/**
 * Detect the Bun bundler. Two bundle shapes carry it:
 *   - ESM CJS-interop: `import{createRequire as X}from"node:module"` +
 *     a `{exports:{}}` CJS factory helper (the classic Bun bundle).
 *   - `@bun-cjs` bytecode: a `(function(exports, require, module, …){…})`
 *     wrapper with NO ESM createRequire import (createRequire comes from
 *     `require("module")`), so only the banner + factory helper identify it.
 *
 * The leading `// @bun` banner alone is definitive (Bun emits it verbatim);
 * the structural co-occurrence is kept as a fallback for inputs whose banner
 * was stripped. Helper names are unstable across builds — structure only.
 */
export function detectBunBundler(code: string): DetectionSignal[] {
  if (BUN_BANNER.test(code)) {
    return [
      {
        source: "bun-bundler",
        pattern: "// @bun banner",
        bundler: "bun",
        tier: "definitive"
      }
    ];
  }

  const hasFactory = /\{exports:\s*\{\}\}/.test(code);
  // Bun imports createRequire and aliases it to a minified name,
  // then calls ALIAS(import.meta.url). We check for the import statement.
  const hasCreateRequire =
    /import\s*\{[^}]*createRequire[^}]*\}\s*from\s*["']node:module["']/.test(
      code
    );

  if (hasFactory && hasCreateRequire) {
    return [
      {
        source: "bun-bundler",
        pattern: "{exports:{}} + createRequire import",
        bundler: "bun",
        tier: "definitive"
      }
    ];
  }

  return [];
}
