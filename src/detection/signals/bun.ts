import type { DetectionSignal } from "../types.js";

/**
 * Detect Bun CJS bundler by co-occurrence of two structural patterns:
 *   1. A variable declaration containing `{exports:{}}` — the CJS factory helper
 *   2. `import{createRequire as ...}from"node:module"` — Bun's Node.js require polyfill
 *
 * Both appear in the first ~300 bytes of any Bun CJS bundle.
 * Helper names are unstable across builds, so we match structure only.
 */
export function detectBunBundler(code: string): DetectionSignal[] {
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
