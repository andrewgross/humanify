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
  const minifierTier =
    options?.minifierOverride && options.minifierOverride !== "unknown"
      ? "definitive"
      : (detection.minifier?.tier ?? "unknown");

  const adapter = selectAdapter(detection, {
    bundlerOverride: options?.bundlerOverride
  });

  return Object.freeze({
    bundlerType,
    bundlerTier,
    minifierType,
    minifierTier,
    signals: detection.signals,
    unpackAdapterName: adapter.name
  });
}
