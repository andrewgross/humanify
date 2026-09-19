/**
 * Canonical serialization rules for the artifact dump (07 §2).
 *
 * Every dump file is comparable BYTE FOR BYTE between two implementations of
 * the same decisions, which means no dump byte may depend on insertion or
 * iteration order:
 *
 * - every file carries `schemaVersion` first;
 * - all arrays are sorted by the `(text, start, end)` of their primary key;
 * - map-shaped data is emitted as arrays of pairs sorted by an explicit key
 *   order, never as object key order;
 * - string comparison is code-unit order (`<`/`>`), byte-stable for the
 *   BMP-only corpus and UTF-16-ordered in general — the same order the TS
 *   side already sorts with.
 *
 * One version constant (12 §2: the dump schema IS the version-record schema;
 * a bump re-derives priors, so it is a deliberate event).
 */

/** Bump on any schema change; the parity comparer refuses mismatched sets. */
export const DUMP_SCHEMA_VERSION = 1;

/** The join key (07 §1): which text a span indexes into, half-open. */
export interface SpanKey {
  /** Which anchored text — "fresh", "prior", or a tree-relative path. */
  text: string;
  /** Half-open [start, end) in UTF-8 BYTE offsets (converted at dump time). */
  start: number;
  end: number;
}

/**
 * Total order over SpanKeys: text, then start, then end. Code-unit string
 * comparison (`<`/`>`) — never locale-aware.
 */
export function spanKeyOrder(a: SpanKey, b: SpanKey): number {
  if (a.text !== b.text) return a.text < b.text ? -1 : 1;
  if (a.start !== b.start) return a.start - b.start;
  if (a.end !== b.end) return a.end - b.end;
  return 0;
}
