/**
 * Escape a string for literal use inside a RegExp pattern.
 *
 * The single owner of this question — it was copied verbatim in three
 * files (bun-helpers, module-detect, unpack/bun) before 2026-08-10.
 */
export function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
