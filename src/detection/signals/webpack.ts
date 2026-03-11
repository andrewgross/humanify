import { matchPatterns, type SignalPattern } from "./pattern-helper.js";
import type { DetectionSignal } from "../types.js";

const PATTERNS: SignalPattern[] = [
  { regex: /__webpack_require__/, pattern: "__webpack_require__" },
  { regex: /__webpack_modules__/, pattern: "__webpack_modules__" },
  { regex: /webpackChunk/, pattern: "webpackChunk" },
];

export function detectWebpack(code: string): DetectionSignal[] {
  return matchPatterns(code, "webpack", "webpack", PATTERNS);
}
