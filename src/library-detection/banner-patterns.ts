/**
 * Shared banner patterns for library detection.
 *
 * These patterns match copyright/license banners that libraries preserve
 * in bundled code. Used by both header-scan detection (first ~1KB) and
 * full-file region scanning.
 *
 * Patterns are defined WITHOUT the `g` flag — consumers add it as needed.
 */

export const BANNER_PATTERNS: RegExp[] = [
  // /*! library-name v1.2.3 */ or /*! library-name - v1.2.3 */
  /\/\*!\s*(\S+)\s+(?:-\s+)?v[\d.]+/,

  // /** @license library-name */ or /* @license library-name */
  /\/\*\*?\s*@license\s+(\S+)/,

  // /** @module library-name */
  /\/\*\*?\s*@module\s+(\S+)/,

  // * library-name v1.2.3  (inside a block comment)
  /\*\s+(\S+)\s+v\d+\.\d+\.\d+/
];

/**
 * Normalize a library name extracted from a comment.
 * Strips trailing punctuation, lowercases, etc.
 */
export function normalizeLibraryName(name: string): string {
  return name
    .replace(/[,;:!]+$/, "") // trailing punctuation
    .replace(/^@/, "") // leading @ (scoped packages like @babel/runtime)
    .toLowerCase();
}
