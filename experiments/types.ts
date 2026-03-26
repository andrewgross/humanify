/**
 * Shared types for the experimentation framework.
 */

export interface FixtureConfig {
  repo: string;
  tag: string;
  /** Entry point relative to repo root */
  entry: string;
  platform: "node" | "neutral" | "browser";
  format?: "esm" | "cjs";
  /** Packages to mark as external in esbuild */
  external?: string[];
  /** Custom install command (default: npm install) */
  installCmd?: string;
}

export interface GroundTruthMapping {
  /** functionId → original source file (relative path) */
  functionToFile: Map<string, string>;
  /** original source file → list of function IDs */
  fileToFunctions: Map<string, string[]>;
  /** All unique original source files */
  sourceFiles: string[];
}

export interface SplitAssignment {
  /** functionId → output file */
  functionToFile: Map<string, string>;
  /** output file → list of function IDs */
  fileToFunctions: Map<string, string[]>;
  /** All output files */
  outputFiles: string[];
}

export interface ClusteringMetrics {
  /** Adjusted Rand Index [-1, 1], 1 = perfect agreement */
  ari: number;
  /** Each output file contains functions from few original files [0, 1] */
  homogeneity: number;
  /** All functions from one original file land in same output file [0, 1] */
  completeness: number;
  /** Harmonic mean of homogeneity and completeness [0, 1] */
  vMeasure: number;
  /** Average purity: fraction of output file from dominant original [0, 1] */
  purity: number;
  /** Average inverse purity: fraction of original file in dominant output [0, 1] */
  inversePurity: number;
}

export interface ExperimentMetrics extends ClusteringMetrics {
  fileCountRatio: number;
  originalFileCount: number;
  splitFileCount: number;
  totalFunctions: number;
  functionsMatched: number;
  mqScore: number;
  /** Tree structure similarity (Jaccard index of directory edges) [0, 1] */
  treeSimilarity?: number;
}

export interface PerFileBreakdown {
  originalFile: string;
  functionCount: number;
  /** Which output files did functions from this source end up in */
  splitIntoFiles: string[];
  /** Output file with most functions from this source */
  dominantFile: string;
  /** Fraction of functions in dominant file */
  completeness: number;
}

export interface ExperimentResult {
  fixture: string;
  config: Record<string, unknown>;
  metrics: ExperimentMetrics;
  timing: {
    parseMs: number;
    splitMs: number;
    metricsMs: number;
    totalMs: number;
  };
  perFileBreakdown: PerFileBreakdown[];
}
