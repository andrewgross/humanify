import type { PipelineConfig } from "../pipeline/types.js";
import { BunUnpackAdapter } from "./adapters/bun.js";
import { PassthroughAdapter } from "./adapters/passthrough.js";
import { WebcrackAdapter } from "./adapters/webcrack.js";
import type { UnpackAdapter } from "./types.js";

const adapters: UnpackAdapter[] = [
  new WebcrackAdapter(),
  new BunUnpackAdapter(),
  new PassthroughAdapter() // must be last (fallback)
];

/**
 * Select the appropriate unpack adapter for a pipeline config.
 */
export function selectUnpackAdapter(config: PipelineConfig): UnpackAdapter {
  const match = adapters.find((a) => a.name === config.unpackAdapterName);
  if (!match)
    throw new Error(`No unpack adapter named "${config.unpackAdapterName}"`);
  return match;
}

export type { UnpackAdapter, UnpackResult } from "./types.js";
