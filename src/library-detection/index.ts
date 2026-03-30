export type { CommentRegion } from "./comment-regions.js";
export type { LibraryDetector, MixedFileDetection } from "./types.js";

import type { BundlerAdapter } from "../detection/types.js";
import { BunLibraryDetector } from "./adapters/bun.js";
import { DefaultLibraryDetector } from "./adapters/default.js";
import type { LibraryDetector } from "./types.js";

const detectors: LibraryDetector[] = [
  new BunLibraryDetector(),
  new DefaultLibraryDetector() // must be last (fallback — always matches)
];

export function selectLibraryDetector(
  adapter: BundlerAdapter
): LibraryDetector {
  const detector = detectors.find((d) => d.supports(adapter));
  // DefaultLibraryDetector always matches, so this should never be undefined
  if (!detector) throw new Error("No library detector found for adapter");
  return detector;
}
