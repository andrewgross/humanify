import type { PipelineConfig } from "./types.js";

/** Every selection the pipeline made before any code was transformed. */
export interface PipelineSelectionRecord {
  bundler: string;
  /** How confidently the bundler was identified — a guess and a signature
   *  match are different provenance for everything downstream. */
  bundlerTier: string;
  minifier: string;
  unpackAdapter: string;
}

/**
 * The run's selection provenance, for the committed per-run record.
 *
 * `PipelineConfig` has always known this; it reached only a `-v` log line and a
 * profiler span, so a committed run could not be asked "which unpack adapter
 * processed this bundle?". `docs/responsibility.md` listed the adapter
 * selection points as the one part of the cascade story still unobservable
 * after naming, matching and placement were instrumented.
 *
 * Selection is a deterministic function of the input, so this cannot differ
 * between two runs of the same bundle — it is not a determinism check. What it
 * answers is which PATH a given pair took, which silently differs across pairs
 * and is what makes a detection change hard to attribute after the fact.
 *
 * One function so that adding a selection point is one edit, not a search for
 * every place a run describes itself.
 */
export function pipelineSelectionRecord(
  config: PipelineConfig
): PipelineSelectionRecord {
  return {
    bundler: config.bundlerType,
    bundlerTier: config.bundlerTier,
    minifier: config.minifierType,
    // Recorded verbatim, including "passthrough": "no adapter matched" decides
    // everything downstream and is a finding, not an absence.
    unpackAdapter: config.unpackAdapterName
  };
}
