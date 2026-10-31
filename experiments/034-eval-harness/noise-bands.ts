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
 *
 * BY NAMING MODE (exp171): names mode files library code under vendor/
 * and names it, so its draws differ from off mode's. The top-level
 * `provenance`/`bands` are the OFF (pre-exp171 default) mode's, untouched;
 * `byMode.names` holds the names-mode references' bands. A mode with no
 * measured bands reads as NONE — never another mode's.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { SummaryTotals } from "./kpis.js";
import { KPIS, SPLIT_KPIS } from "./kpis.js";

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

/** The file: the off mode's bands at the top, other modes under byMode. */
interface NoiseBandsFile extends NoiseBands {
  byMode?: Record<string, NoiseBands>;
}

/** A naming mode (run-manifest.ts `NamingMode`). */
export type BandMode = "names" | "off";

const BANDS_PATH = path.join(import.meta.dirname, "noise-bands.json");

function readFile(file: string): NoiseBandsFile | null {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8")) as NoiseBandsFile;
  } catch {
    return null;
  }
}

/** One mode's bands out of `file`; null when that mode has none. */
export function readNoiseBands(
  mode: BandMode,
  file: string = BANDS_PATH
): NoiseBands | null {
  const f = readFile(file);
  if (!f) return null;
  if (mode === "off") return { provenance: f.provenance, bands: f.bands };
  return f.byMode?.[mode] ?? null;
}

const cached = new Map<BandMode, NoiseBands | null>();

export function loadNoiseBands(mode: BandMode = "off"): NoiseBands | null {
  if (!cached.has(mode)) cached.set(mode, readNoiseBands(mode));
  return cached.get(mode) ?? null;
}

/** The band for a KPI key in `mode`; null when unmeasured or no bands file exists. */
export function bandFor(kpiKey: string, mode: BandMode = "off"): number | null {
  const bands = loadNoiseBands(mode);
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
  for (const kpi of [...KPIS, ...SPLIT_KPIS]) {
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

/** Write one mode's bands, keeping every other mode's. */
export function writeNoiseBands(
  bands: NoiseBands,
  mode: BandMode = "off",
  file: string = BANDS_PATH
): string {
  const prior = readFile(file);
  const byMode = prior?.byMode ?? {};
  const next: NoiseBandsFile =
    mode === "off"
      ? { ...bands, ...(Object.keys(byMode).length ? { byMode } : {}) }
      : {
          provenance: prior?.provenance ?? bands.provenance,
          bands: prior?.bands ?? {},
          byMode: { ...byMode, [mode]: bands }
        };
  fs.writeFileSync(file, `${JSON.stringify(next, null, 2)}\n`);
  cached.clear();
  return file;
}
