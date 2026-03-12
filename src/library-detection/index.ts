export { detectLibraryFromComments } from "./comment-patterns.js";
export type { CommentRegion } from "./comment-regions.js";
export {
  classifyFunctionsByRegion,
  findCommentRegions
} from "./comment-regions.js";
export {
  detectLibraries,
  extractLibraryNameFromPath,
  isLibraryPath
} from "./detector.js";
export type {
  DetectionResult,
  LibraryDetection,
  MixedFileDetection
} from "./types.js";
