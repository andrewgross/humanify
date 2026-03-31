import type {
  BundlerType,
  DetectionSignal,
  DetectionTier,
  MinifierType
} from "../detection/types.js";
import type { CommentRegion } from "../library-detection/comment-regions.js";

export interface PipelineConfig {
  bundlerType: BundlerType;
  bundlerTier: DetectionTier;
  minifierType: MinifierType;
  minifierTier: DetectionTier;
  signals: DetectionSignal[];
  /** Name of the selected unpack adapter (e.g., "bun", "webcrack", "passthrough") */
  unpackAdapterName: string;
}

/** Per-file context passed to each plugin invocation. */
export interface FileContext {
  commentRegions?: CommentRegion[];
}
