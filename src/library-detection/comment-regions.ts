/**
 * Intra-file library detection for Rollup/esbuild-style bundles.
 *
 * These bundlers scope-hoist everything into a single file, interleaving
 * library code with app code. This module scans for banner comments
 * throughout the file and maps functions to library regions.
 */

import type { FunctionNode } from "../analysis/types.js";

export interface CommentRegion {
  libraryName: string;
  startOffset: number;
  /** null = extends to next region or EOF */
  endOffset: number | null;
}

/**
 * Regex patterns that identify library banners.
 * Same patterns as comment-patterns.ts but used with matchAll across the full file.
 */
const BANNER_PATTERNS: RegExp[] = [
  // /*! library-name v1.2.3 */ or /*! library-name - v1.2.3 */
  /\/\*!\s*(\S+)\s+(?:-\s+)?v[\d.]+/g,

  // /** @license library-name */ or /* @license library-name */
  /\/\*\*?\s*@license\s+(\S+)/g,

  // /** @module library-name */
  /\/\*\*?\s*@module\s+(\S+)/g,

  // * library-name v1.2.3  (inside a block comment)
  /\*\s+(\S+)\s+v\d+\.\d+\.\d+/g,
];

/**
 * Normalize a library name extracted from a comment.
 * Strips trailing punctuation, lowercases, etc.
 */
function normalizeLibraryName(name: string): string {
  return name
    .replace(/[,;:!]+$/, "")
    .replace(/^@/, "")
    .toLowerCase();
}

interface BannerMatch {
  libraryName: string;
  offset: number;
}

/**
 * Scan the entire file for banner comments and return sorted regions.
 *
 * Each banner starts a new region that extends until the next banner.
 * The last region extends to EOF.
 */
export function findCommentRegions(code: string): CommentRegion[] {
  const matches: BannerMatch[] = [];

  for (const pattern of BANNER_PATTERNS) {
    // Reset lastIndex since we reuse the regex
    const regex = new RegExp(pattern.source, pattern.flags);
    let match: RegExpExecArray | null;
    while ((match = regex.exec(code)) !== null) {
      const libraryName = normalizeLibraryName(match[1]);
      matches.push({ libraryName, offset: match.index });
    }
  }

  if (matches.length === 0) {
    return [];
  }

  // Sort by offset and deduplicate overlapping matches at the same position
  matches.sort((a, b) => a.offset - b.offset);
  const deduped: BannerMatch[] = [];
  for (const m of matches) {
    if (deduped.length === 0 || m.offset !== deduped[deduped.length - 1].offset) {
      deduped.push(m);
    }
  }

  // Convert to regions: each region extends from its banner to the next banner
  const regions: CommentRegion[] = [];
  for (let i = 0; i < deduped.length; i++) {
    const current = deduped[i];
    const next = deduped[i + 1];
    regions.push({
      libraryName: current.libraryName,
      startOffset: current.offset,
      endOffset: next ? next.offset : null,
    });
  }

  return regions;
}

/**
 * Classify functions by which comment region they fall in.
 *
 * Uses binary search on sorted regions. Functions outside any region
 * are treated as app code (not returned in the set).
 *
 * @returns Set of sessionIds for functions classified as library code
 */
export function classifyFunctionsByRegion(
  functions: FunctionNode[],
  regions: CommentRegion[]
): Set<string> {
  if (regions.length === 0) {
    return new Set();
  }

  const libraryIds = new Set<string>();

  for (const fn of functions) {
    const start = fn.path.node.start;
    if (start == null) continue;

    const regionIndex = findRegion(regions, start);
    if (regionIndex !== -1) {
      libraryIds.add(fn.sessionId);
    }
  }

  return libraryIds;
}

/**
 * Binary search to find which region (if any) contains the given offset.
 * Returns the region index, or -1 if offset is before all regions.
 */
function findRegion(regions: CommentRegion[], offset: number): number {
  let lo = 0;
  let hi = regions.length - 1;

  // Find the last region whose startOffset <= offset
  let result = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >>> 1;
    if (regions[mid].startOffset <= offset) {
      result = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }

  if (result === -1) {
    return -1;
  }

  // Check if offset is within the region's bounds
  const region = regions[result];
  if (region.endOffset === null || offset < region.endOffset) {
    return result;
  }

  return -1;
}
