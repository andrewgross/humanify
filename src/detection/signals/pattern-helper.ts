import type { BundlerType, DetectionSignal } from "../types.js";

export interface SignalPattern {
  regex: RegExp;
  pattern: string;
}

export function matchPatterns(
  code: string,
  source: string,
  bundler: BundlerType,
  patterns: SignalPattern[]
): DetectionSignal[] {
  const signals: DetectionSignal[] = [];
  for (const { regex, pattern } of patterns) {
    if (regex.test(code)) {
      signals.push({ source, pattern, bundler, tier: "definitive" });
    }
  }
  return signals;
}
