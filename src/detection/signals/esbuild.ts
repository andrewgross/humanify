import type { DetectionSignal } from "../types.js";

const PATTERNS: { regex: RegExp; pattern: string }[] = [
  { regex: /\b__commonJS\b/, pattern: "__commonJS" },
  { regex: /\b__toESM\b/, pattern: "__toESM" },
  { regex: /\b__toCommonJS\b/, pattern: "__toCommonJS" },
  { regex: /\bvar\s+__export\b/, pattern: "__export (esbuild definition)" },
  { regex: /\b__require\b/, pattern: "__require" },
];

export function detectEsbuild(code: string): DetectionSignal[] {
  const signals: DetectionSignal[] = [];
  for (const { regex, pattern } of PATTERNS) {
    if (regex.test(code)) {
      signals.push({
        source: "esbuild-bundler",
        pattern,
        bundler: "esbuild",
        tier: "definitive",
      });
    }
  }
  return signals;
}
