import type {
  BundlerDetectionResult,
  BundlerType,
  MinifierType
} from "../detection/types.js";
import { selectAdapter } from "../unpack/index.js";
import type { PipelineConfig } from "./types.js";

interface BuildPipelineConfigOptions {
  bundlerOverride?: BundlerType;
  minifierOverride?: MinifierType;
}

/**
 * Build an immutable PipelineConfig from a detection result and optional CLI overrides.
 *
 * Selects the unpack adapter and freezes the config so downstream stages
 * cannot mutate it.
 */
export function buildPipelineConfig(
  detection: BundlerDetectionResult,
  options?: BuildPipelineConfigOptions
): PipelineConfig {
  const bundlerType =
    options?.bundlerOverride && options.bundlerOverride !== "unknown"
      ? options.bundlerOverride
      : (detection.bundler?.type ?? "unknown");
  const bundlerTier =
    options?.bundlerOverride && options.bundlerOverride !== "unknown"
      ? "definitive"
      : (detection.bundler?.tier ?? "unknown");

  const minifierType =
    options?.minifierOverride && options.minifierOverride !== "unknown"
      ? options.minifierOverride
      : (detection.minifier?.type ?? "unknown");
  const adapter = selectAdapter(detection, {
    bundlerOverride: options?.bundlerOverride
  });

  // `minifierTier` and `signals` used to be carried here and were read by
  // nothing — the verbose log reads `detection.signals`, the pre-config copy.
  // A detected fact nobody consults is not configuration; it is a field a
  // future reader assumes is load-bearing (exp058 follow-up).
  return Object.freeze({
    bundlerType,
    bundlerTier,
    minifierType,
    unpackAdapterName: adapter.name
  });
}
