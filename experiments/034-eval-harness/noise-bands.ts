/**
 * Per-KPI noise bands: how much two runs of IDENTICAL code disagree.
 *
 * Rule 11's failure mode is a gate printing a confident sign on an effect
 * smaller than its own noise floor. The floor was measured ONCE, for one
 * KPI family, on one pair (±2,800 src lines/hop, exp048) — every other
 * column has been quoted naked. This module gives every column a band and
 * the leaderboard renders any sub-band delta as `~0 (±band)`, making the
 * confident sub-floor sign unprintable rather than merely warned about.
 *
 * `noise-bands.json` holds the bands, keyed by KPI key (`kpis.ts`).
 * PROVISIONAL entries are seeded from recorded measurements and marked so;
 * a measured file is produced by `npm run eval -- bands <label> <label>…`
 * over 2+ SAME-COMMIT labels: for each KPI the band is the largest
 * pairwise disagreement between the repeats' totals. `null` means "never
 * measured" — rendered as `(±?)`, which is a claim of ignorance, not of
 * stability. A band of 0 is a real claim (draw-invariance) and only the
 * two hold columns have earned it.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { SummaryTotals } from "./kpis.js";
import { KPIS } from "./kpis.js";

export interface NoiseBands {
  provenance: {
    provisional: boolean;
    sources: string[];
    commit?: string;
    labels?: string[];
  };
  /** KPI key → band in that KPI's own units; null = never measured. */
  bands: Record<string, number | null>;
}

const BANDS_PATH = path.join(import.meta.dirname, "noise-bands.json");

let cached: NoiseBands | null | undefined;

export function loadNoiseBands(): NoiseBands | null {
  if (cached !== undefined) return cached;
  try {
    cached = JSON.parse(fs.readFileSync(BANDS_PATH, "utf8")) as NoiseBands;
  } catch {
    cached = null;
  }
  return cached;
}

/** The band for a KPI key; null when unmeasured or no bands file exists. */
export function bandFor(kpiKey: string): number | null {
  const bands = loadNoiseBands();
  const band = bands?.bands[kpiKey];
  return typeof band === "number" ? band : null;
}

/**
 * Compute measured bands from 2+ same-commit repeat labels' totals: per
 * KPI, the largest pairwise |disagreement|. The same-commit requirement is
 * the CALLER's to enforce (the eval `bands` verb refuses mixed commits) —
 * two labels from different code measure a change, not a floor.
 */
export function computeBands(
  totalsPerLabel: Array<Partial<SummaryTotals>>
): Record<string, number | null> {
  const out: Record<string, number | null> = {};
  for (const kpi of KPIS) {
    const values = totalsPerLabel
      .map((t) => t[kpi.total])
      .filter((v): v is number => typeof v === "number");
    if (values.length < 2) {
      out[kpi.key] = null;
      continue;
    }
    let band = 0;
    for (let i = 0; i < values.length; i++) {
      for (let j = i + 1; j < values.length; j++) {
        band = Math.max(band, Math.abs(values[i] - values[j]));
      }
    }
    out[kpi.key] = band;
  }
  return out;
}

export function writeNoiseBands(bands: NoiseBands): string {
  fs.writeFileSync(BANDS_PATH, `${JSON.stringify(bands, null, 2)}\n`);
  cached = undefined;
  return BANDS_PATH;
}
