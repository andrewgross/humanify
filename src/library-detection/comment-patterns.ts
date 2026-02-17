/**
 * Patterns for detecting library names from comment banners in bundled code.
 *
 * Many bundlers and libraries preserve copyright/license banners that identify
 * the library name and version. We scan the first ~1KB of each file for these.
 */

const COMMENT_PATTERNS: RegExp[] = [
  // /*! library-name v1.2.3 */ or /*! library-name - v1.2.3 */
  /\/\*!\s*(\S+)\s+(?:-\s+)?v[\d.]+/,

  // /** @license library-name */ or /* @license library-name */
  /\/\*\*?\s*@license\s+(\S+)/,

  // /** @module library-name */
  /\/\*\*?\s*@module\s+(\S+)/,

  // * library-name v1.2.3  (inside a block comment)
  /\*\s+(\S+)\s+v\d+\.\d+\.\d+/,
];

/** Maximum bytes to scan for comment banners */
const SCAN_LIMIT = 1024;

/**
 * Extract library names from comment banners in the first ~1KB of code.
 * Returns the first library name found, or undefined if none detected.
 */
export function detectLibraryFromComments(code: string): string | undefined {
  const header = code.slice(0, SCAN_LIMIT);

  for (const pattern of COMMENT_PATTERNS) {
    const match = header.match(pattern);
    if (match?.[1]) {
      return normalizeLibraryName(match[1]);
    }
  }

  return undefined;
}

/**
 * Normalize a library name extracted from a comment.
 * Strips trailing punctuation, lowercases, etc.
 */
function normalizeLibraryName(name: string): string {
  return name
    .replace(/[,;:!]+$/, "") // trailing punctuation
    .replace(/^@/, "") // leading @ (scoped packages like @babel/runtime)
    .toLowerCase();
}
