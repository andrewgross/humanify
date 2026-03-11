import type { DetectionSignal } from "../types.js";

const PATTERNS: { regex: RegExp; pattern: string }[] = [
  { regex: /parcelRequire/, pattern: "parcelRequire" },
  { regex: /require\s*\(\s*["']_bundle_loader["']\s*\)/, pattern: 'require("_bundle_loader")' },
];

export function detectParcel(code: string): DetectionSignal[] {
  const signals: DetectionSignal[] = [];
  for (const { regex, pattern } of PATTERNS) {
    if (regex.test(code)) {
      signals.push({
        source: "parcel",
        pattern,
        bundler: "parcel",
        tier: "definitive",
      });
    }
  }
  return signals;
}
