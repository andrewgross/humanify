import type { NodePath } from "@babel/core";
import type * as t from "@babel/types";

/**
 * Content-based fingerprint for identifying functions across versions.
 *
 * Currently only uses exactHash, but structured to allow future expansion
 * with additional hash types for fuzzy matching (e.g., structure-only hash,
 * signature hash, etc.)
 */
export interface FunctionFingerprint {
  /**
   * Exact structural hash - normalized AST with all identifiers replaced
   * by positional placeholders ($0, $1, etc.) and literals normalized.
   *
   * Two functions match only if their structure is identical.
   * Format: 16-character hex string (truncated SHA-256)
   */
  exactHash: string;

  // Future expansion:
  // structureHash?: string;  // Ignore leaf expressions, keep control flow
  // signatureHash?: string;  // Just params count + return presence + size bucket
}

/**
 * Represents a function in the dependency graph.
 * Each function tracks its callees (what it calls) and callers (what calls it).
 */
export interface FunctionNode {
  /**
   * Unique identifier for this function during processing.
   * Format: "filepath:line:column" referencing position in the webcrack output
   * (after bundle unpacking, before humanification).
   *
   * Used as a key in the dependency graph and for debugging/logging.
   * Not stable across runs - positions change if code is reformatted.
   */
  sessionId: string;

  /**
   * Content-based fingerprint for caching and cross-version matching.
   * See FunctionFingerprint for details on the different hash types.
   */
  fingerprint: FunctionFingerprint;

  /** Babel path reference to the function */
  path: NodePath<t.Function>;

  /** Functions in our code that this function calls */
  internalCallees: Set<FunctionNode>;

  /** Library/builtin calls (names only) */
  externalCallees: Set<string>;

  /** Reverse dependencies - functions that call this one */
  callers: Set<FunctionNode>;

  /** Processing state */
  status: "pending" | "processing" | "done";

  /** Rename mapping after processing (placeholder -> humanified name) */
  renameMapping?: RenameMapping;
}

/**
 * Maps placeholder identifiers to humanified names.
 * Used for caching and applying renames.
 */
export interface RenameMapping {
  /** Maps original minified name to humanified name */
  names: Record<string, string>;

  /** Which LLM model produced these renames */
  model?: string;
}

/**
 * Tracks a single rename decision for source map generation.
 */
export interface RenameDecision {
  /**
   * Position of the identifier in the webcrack output (post-unpack, pre-humanify).
   *
   * Note: This is NOT the position in the original minified bundle. To map back
   * to the original bundle, we'd need webcrack to produce a source map and chain
   * them together. For now, this position is relative to the unpacked module files.
   */
  originalPosition: { line: number; column: number };

  /** Original minified name */
  originalName: string;

  /** New humanified name */
  newName: string;

  /** Which function this rename belongs to */
  functionId: string;

  /** Whether this came from cache */
  fromCache?: boolean;
}

/**
 * Context provided to the LLM for renaming decisions.
 */
export interface LLMContext {
  /** The function being processed (current minified code) */
  functionCode: string;

  /** Functions this calls (already humanified) */
  calleeSignatures: CalleeSignature[];

  /** Where this function is called from (may still be minified) */
  callsites: string[];

  /** Names already used in scope (to avoid conflicts) */
  usedIdentifiers: Set<string>;
}

/**
 * Signature of a callee function for context.
 */
export interface CalleeSignature {
  /** Humanified function name */
  name: string;

  /** Humanified parameter names */
  params: string[];

  /** First few lines of the function body */
  snippet: string;
}

/**
 * Progress reporting for the processing pipeline.
 */
export interface ProcessingProgress {
  total: number;
  done: number;
  processing: number;
  ready: number;
  pending: number;

  currentFunction?: string;
  estimatedTimeRemaining?: number;
}

/**
 * Callback for progress updates.
 */
export type ProgressCallback = (progress: ProcessingProgress) => void;

/**
 * Options for the rename processor.
 */
export interface ProcessorOptions {
  /** Maximum number of functions to process in parallel */
  concurrency?: number;

  /** Progress callback */
  onProgress?: ProgressCallback;
}
