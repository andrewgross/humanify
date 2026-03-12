import type { DetectionSignal } from "../types.js";
import { matchPatterns, type SignalPattern } from "./pattern-helper.js";

const PATTERNS: SignalPattern[] = [
  { regex: /\b__commonJS\b/, pattern: "__commonJS" },
  { regex: /\b__toESM\b/, pattern: "__toESM" },
  { regex: /\b__toCommonJS\b/, pattern: "__toCommonJS" },
  { regex: /\bvar\s+__export\b/, pattern: "__export (esbuild definition)" },
  { regex: /\b__require\b/, pattern: "__require" }
];

export function detectEsbuild(code: string): DetectionSignal[] {
  return matchPatterns(code, "esbuild-bundler", "esbuild", PATTERNS);
}
