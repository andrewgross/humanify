import { matchPatterns, type SignalPattern } from "./pattern-helper.js";
import type { DetectionSignal } from "../types.js";

const PATTERNS: SignalPattern[] = [
  { regex: /parcelRequire/, pattern: "parcelRequire" },
  { regex: /require\s*\(\s*["']_bundle_loader["']\s*\)/, pattern: 'require("_bundle_loader")' },
];

export function detectParcel(code: string): DetectionSignal[] {
  return matchPatterns(code, "parcel", "parcel", PATTERNS);
}
