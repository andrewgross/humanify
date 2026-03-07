import type * as t from "@babel/types";

/**
 * A single entry in the split ledger, tracking one top-level AST node.
 */
export interface SplitLedgerEntry {
  /** Stable ID: "filename:lineStart:nodeType" */
  id: string;
  /** The AST node */
  node: t.Statement;
  /** Node type (e.g., "FunctionDeclaration", "VariableDeclaration") */
  type: string;
  /** Source file path */
  source: string;
  /** Assigned output file, if any */
  outputFile?: string;
}

/**
 * Ledger tracking every top-level node to guarantee no code is dropped.
 */
export interface SplitLedger {
  /** All entries keyed by stable ID */
  entries: Map<string, SplitLedgerEntry>;
  /** Entries explicitly duplicated to multiple files */
  duplicated: Map<string, string[]>;
}

/**
 * A cluster of related functions that will become one output file.
 */
export interface Cluster {
  /** Cluster fingerprint: sha256(sorted member exactHashes).slice(0,16) */
  id: string;
  /** Root functions that seeded this cluster */
  rootFunctions: string[];
  /** All member function sessionIds */
  members: Set<string>;
  /** exactHash values of all members (sorted, for fingerprint computation) */
  memberHashes: string[];
}

/**
 * The complete plan for splitting a codebase.
 */
export interface SplitPlan {
  /** Named clusters (one per output file) */
  clusters: Cluster[];
  /** Functions shared across multiple clusters */
  shared: Set<string>;
  /** Functions with no callers and no callees (shouldn't happen often) */
  orphans: Set<string>;
  /** The ledger for correctness verification */
  ledger: SplitLedger;
  /** Summary statistics */
  stats: SplitStats;
}

/**
 * Summary statistics for the split manifest.
 */
export interface SplitStats {
  totalFunctions: number;
  totalClusters: number;
  avgClusterSize: number;
  sharedFunctions: number;
  sharedRatio: number;
  orphanFunctions: number;
  mqScore: number;
}
