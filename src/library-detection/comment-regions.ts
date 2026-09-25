/**
 * Intra-file library detection for Rollup/esbuild-style bundles.
 *
 * These bundlers scope-hoist everything into a single file, interleaving
 * library code with app code. This module scans for banner comments
 * throughout the file and maps functions to library regions.
 */

import { BANNER_PATTERNS, normalizeLibraryName } from "./banner-patterns.js";

export interface CommentRegion {
  libraryName: string;
  startOffset: number;
  /** null = extends to next region or EOF */
  endOffset: number | null;
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
    const regex = new RegExp(pattern.source, `${pattern.flags}g`);
    let match: RegExpExecArray | null = regex.exec(code);
    while (match !== null) {
      const libraryName = normalizeLibraryName(match[1]);
      matches.push({ libraryName, offset: match.index });
      match = regex.exec(code);
    }
  }

  if (matches.length === 0) {
    return [];
  }

  // Sort by offset and deduplicate overlapping matches at the same position
  matches.sort((a, b) => a.offset - b.offset);
  const deduped: BannerMatch[] = [];
  for (const m of matches) {
    if (
      deduped.length === 0 ||
      m.offset !== deduped[deduped.length - 1].offset
    ) {
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
      endOffset: next ? next.offset : null
    });
  }

  return regions;
}

/**
 * The library whose region contains `offset`, or null (app code).
 *
 * `offset` MUST be in the coordinate space the regions were found in — the
 * RAW file text `findCommentRegions` scanned. Beautify both expands minified
 * text and drops every comment, so an offset into the beautified text lands
 * in the wrong region (finding #32); the rename pass reaches this through
 * the ordinal carry in `function-carry.ts`, never with a beautified offset.
 */
export function libraryAtOffset(
  regions: CommentRegion[],
  offset: number
): string | null {
  const regionIndex = findRegion(regions, offset);
  return regionIndex === -1 ? null : regions[regionIndex].libraryName;
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
