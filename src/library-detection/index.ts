export { detectLibraries } from "./detector.js";
export { isLibraryPath, extractLibraryNameFromPath } from "./detector.js";
export { detectLibraryFromComments } from "./comment-patterns.js";
export { findCommentRegions, classifyFunctionsByRegion } from "./comment-regions.js";
export type { CommentRegion } from "./comment-regions.js";
export type { LibraryDetection, DetectionResult, MixedFileDetection } from "./types.js";
