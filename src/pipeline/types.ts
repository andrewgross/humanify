import type {
  BundlerType,
  DetectionTier,
  MinifierType
} from "../detection/types.js";
import type { CommentRegion } from "../library-detection/comment-regions.js";
import type { FunctionLibraryCarry } from "../library-detection/function-carry.js";

export interface PipelineConfig {
  bundlerType: BundlerType;
  bundlerTier: DetectionTier;
  minifierType: MinifierType;
  /** Name of the selected unpack adapter (e.g., "bun", "webcrack", "passthrough") */
  unpackAdapterName: string;
}

/** Per-file context passed to each plugin invocation. */
export interface FileContext {
  /** Path of the file being processed */
  filePath?: string;
  /** Library banner regions, in the RAW file text's coordinates. */
  commentRegions?: CommentRegion[];
  /** Set by the beautify stage when `commentRegions` is non-empty: each
   *  function's library, carried by ordinal into the beautified text (#32).
   *  The rename pass classifies from THIS, never from the raw regions. */
  functionLibraries?: FunctionLibraryCarry;
}
