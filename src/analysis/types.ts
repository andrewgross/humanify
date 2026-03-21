import type { NodePath } from "@babel/core";
import type { Scope } from "@babel/traverse";
import type * as t from "@babel/types";

/**
 * Structural features extracted from a function for fingerprinting.
 * These features are stable across minification and can be used for
 * fuzzy matching and disambiguation.
 */
export interface StructuralFeatures {
  // Signature
  arity: number;
  hasRestParam: boolean;

  // Complexity
  returnCount: number;
  complexity: number; // Cyclomatic complexity estimate

  // Control flow shape
  cfgShape: string; // e.g., "if-loop-if-ret"
  loopCount: number;
  branchCount: number;
  tryCount: number;

  // Anchors (stable across minification)
  stringLiterals: string[];
  numericLiterals: number[];
  externalCalls: string[]; // ["fetch", "JSON.parse"]
  propertyAccesses: string[]; // [".then", ".catch"]
}

/**
 * Blurred representation of a callee's structure.
 * Used to describe call relationships without creating cascading dependencies.
 */
export interface CalleeShape {
  arity: number;
  complexity: number;
  cfgType: "linear" | "branching" | "looping" | "complex";
  hasExternalCalls: boolean;
}

/**
 * Content-based fingerprint for identifying functions across versions.
 *
 * Supports multi-resolution matching:
 * - Resolution 0: exactHash only (most stable, least distinctive)
 * - Resolution 1: exactHash + blurred callee shapes (balanced)
 * - Resolution 2: exactHash + exact callee hashes (most distinctive)
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

  /**
   * Decomposed structural features for fuzzy matching and disambiguation.
   */
  features?: StructuralFeatures;

  /**
   * Resolution 1: Blurred callee shapes (sorted for determinism).
   * These describe the structure of callees without identifying them exactly.
   */
  calleeShapes?: CalleeShape[];

  /**
   * Resolution 1: Blurred caller shapes (sorted for determinism).
   * Optional: who calls this function.
   */
  callerShapes?: CalleeShape[];

  /**
   * Resolution 2: Exact callee hashes (sorted for determinism).
   * The exactHash of each internal callee.
   */
  calleeHashes?: string[];

  /**
   * Two-hop shapes: blurred shapes of callees' callees.
   * Used for additional disambiguation at resolution 2.
   */
  twoHopShapes?: string[];
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

  /** Scope parent: the immediately enclosing function (for processing order, NOT fingerprinting) */
  scopeParent?: FunctionNode;

  /** Processing state */
  status: "pending" | "processing" | "done";

  /** Rename mapping after processing (placeholder -> humanified name) */
  renameMapping?: RenameMapping;

  /** Call sites where this function is invoked (pre-computed during graph building) */
  callSites: CallSiteInfo[];

  /** Per-identifier rename report (populated after processing) */
  renameReport?: RenameReport;
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

  /**
   * Parent-scope variable declarations for read-only context.
   * When a function is processed before its scopeParent (deadlock breaking),
   * these show surrounding scope variables to help the LLM understand context
   * without asking it to rename them.
   */
  contextVars?: string[];
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
 * Information about a call site where a function is invoked.
 * Pre-computed during graph building to avoid repeated AST traversals.
 */
export interface CallSiteInfo {
  /** The code of the call expression (e.g., "fetchUser(id, options)") */
  code: string;

  /** Line number in source */
  line: number;

  /** Column number in source */
  column: number;
}

/**
 * Outcome for a single identifier rename attempt.
 */
export type IdentifierOutcome =
  | { status: "renamed"; newName: string; round: number }
  | { status: "unchanged"; attempts: number; suggestion?: string }
  | { status: "missing"; attempts: number; lastFinishReason?: string }
  | {
      status: "duplicate";
      conflictedWith: string;
      attempts: number;
      suggestion?: string;
    }
  | { status: "invalid"; attempts: number; suggestion?: string }
  | { status: "not-collected" };

/**
 * Report tracking all identifier outcomes for a single rename target.
 */
export interface RenameReport {
  /** What was renamed */
  type: "function" | "module-binding";
  /** How it was renamed */
  strategy: "llm" | "library-prefix" | "fallback";
  /** Identifier for the target (function sessionId or module binding batch key) */
  targetId: string;
  /** Total identifiers that needed renaming */
  totalIdentifiers: number;
  /** Number successfully renamed */
  renamedCount: number;
  /** Per-identifier outcomes */
  outcomes: Record<string, IdentifierOutcome>;
  /** Total number of LLM calls made (only present for strategy: "llm") */
  totalLLMCalls?: number;
  /** Finish reasons from each LLM call (only present for strategy: "llm") */
  finishReasons?: (string | undefined)[];
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

  /** Progress callback (legacy - prefer metrics) */
  onProgress?: ProgressCallback;

  /** Metrics tracker for detailed observability */
  metrics?: import("../llm/metrics.js").MetricsTracker;

  /**
   * Functions to treat as already completed (e.g., library functions in mixed files).
   * These are added to the done set before processing begins so that functions
   * depending on them can become ready.
   */
  preDone?: FunctionNode[];

  /**
   * When true, only rename function parameters (not body locals).
   * Used for lightweight processing of library functions.
   */
  paramOnly?: boolean;

  /** Maximum identifiers per LLM batch (default: 10) */
  batchSize?: number;

  /** Per-identifier retry limit (default: 3) */
  maxRetriesPerIdentifier?: number;

  /** Cross-lane collision retry limit (default: 100) */
  maxFreeRetries?: number;

  /** Minimum bindings to enable parallel lanes (default: 25) */
  laneThreshold?: number;

  /** Profiler instance for performance instrumentation */
  profiler?: import("../profiling/profiler.js").Profiler;

  /** Custom rename-eligibility function (defaults to the built-in createIsEligible) */
  isEligible?: (name: string) => boolean;
}

/**
 * Represents a module-level binding (variable, import, etc.) in the unified rename graph.
 */
export interface ModuleBindingNode {
  /** Unique ID (e.g., "module:varName") */
  sessionId: string;
  /** The binding name */
  name: string;
  /** Babel identifier node */
  identifier: t.Identifier;
  /** Declaration text */
  declaration: string;
  /** Line number of declaration */
  declarationLine: number;
  /** Assignment context snippets (collected upfront) */
  assignments: string[];
  /** Usage context snippets (collected upfront) */
  usages: string[];
  /** The scope containing this binding */
  scope: Scope;
  /** Processing state */
  status: "pending" | "processing" | "done";
}

/**
 * Tagged union of node types that participate in the unified rename graph.
 */
export type RenameNode =
  | { type: "function"; node: FunctionNode }
  | { type: "module-binding"; node: ModuleBindingNode };

/**
 * Unified dependency graph containing both function nodes and module-level bindings.
 * Processed leaf-first in a single parallel pass.
 */
export interface UnifiedGraph {
  /** All nodes keyed by sessionId */
  nodes: Map<string, RenameNode>;
  /** Forward dependencies: sessionId -> set of dependency sessionIds */
  dependencies: Map<string, Set<string>>;
  /** Reverse dependencies: sessionId -> set of dependent sessionIds */
  dependents: Map<string, Set<string>>;
  /** Edges that come from scopeParent relationships (format: "childId->parentId") */
  scopeParentEdges: Set<string>;
  /** The target scope for module-level renames */
  targetScope: Scope;
  /** Path to wrapper IIFE function, if detected */
  wrapperPath?: NodePath<t.Function>;
}

/**
 * Index for efficient fingerprint lookup and matching.
 */
export interface FingerprintIndex {
  /** Primary index: exactHash → sessionIds */
  byExactHash: Map<string, string[]>;

  /** Secondary index: (exactHash + calleeShapesHash) → sessionIds */
  byResolution1: Map<string, string[]>;

  /** Full fingerprints keyed by sessionId */
  fingerprints: Map<string, FunctionFingerprint>;
}

/**
 * Result of matching functions across two versions.
 */
export interface MatchResult {
  /** Successfully matched: oldSessionId → newSessionId */
  matches: Map<string, string>;

  /** Multiple candidates found: oldSessionId → candidate newSessionIds */
  ambiguous: Map<string, string[]>;

  /** No match found: oldSessionIds with no candidates */
  unmatched: string[];
}
