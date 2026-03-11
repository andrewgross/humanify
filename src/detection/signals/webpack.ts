import type { DetectionSignal } from "../types.js";

const PATTERNS: { regex: RegExp; pattern: string }[] = [
  { regex: /__webpack_require__/, pattern: "__webpack_require__" },
  { regex: /__webpack_modules__/, pattern: "__webpack_modules__" },
  { regex: /webpackChunk/, pattern: "webpackChunk" },
];

export function detectWebpack(code: string): DetectionSignal[] {
  const signals: DetectionSignal[] = [];
  for (const { regex, pattern } of PATTERNS) {
    if (regex.test(code)) {
      signals.push({
        source: "webpack",
        pattern,
        bundler: "webpack",
        tier: "definitive",
      });
    }
  }
  return signals;
}
