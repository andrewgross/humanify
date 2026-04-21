/**
 * Remove an injected `console.log("__PERTURB_<marker>__")` from minified code.
 *
 * Uses string-level regex removal rather than AST parse/generate to avoid
 * Babel re-serialization artifacts that change function hashes. The marker
 * string is globally unique (UUID-based), so regex matching is safe.
 *
 * Minifiers may transform the call in several ways:
 *
 * 1. Standalone statement: `console.log("__PERTURB_abc__");`
 *    → Remove the entire statement including trailing semicolon/comma
 *
 * 2. Comma operator (sequence): `console.log("__PERTURB_abc__"),foo()`
 *    → Remove `console.log("__PERTURB_abc__"),`
 *
 * 3. Leading in sequence: `foo(),console.log("__PERTURB_abc__")`
 *    → Remove `,console.log("__PERTURB_abc__")`
 */
export function removeMarker(minifiedCode: string, marker: string): string {
  // Escape the marker for regex (it only contains alnum + underscores, but be safe)
  const escapedMarker = marker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

  // The core call pattern: console.log("marker") or console.log('marker')
  const callPattern = `console\\.log\\(["']${escapedMarker}["']\\)`;

  // Try patterns from most specific to least:

  // Pattern 1: Standalone expression statement — `console.log("marker");`
  // May or may not have a semicolon after
  const standaloneRe = new RegExp(`${callPattern}[;,]?`, "g");

  // Pattern 2: Leading in comma expression — `console.log("marker"),rest`
  const leadingCommaRe = new RegExp(`${callPattern},`, "g");

  // Pattern 3: Trailing in comma expression — `,console.log("marker")`
  const trailingCommaRe = new RegExp(`,${callPattern}`, "g");

  // Try leading comma first (most specific for sequence expressions)
  let result = minifiedCode.replace(leadingCommaRe, "");
  if (result !== minifiedCode) return result;

  // Try trailing comma
  result = minifiedCode.replace(trailingCommaRe, "");
  if (result !== minifiedCode) return result;

  // Try standalone (catches simple expression statements)
  result = minifiedCode.replace(standaloneRe, "");
  if (result !== minifiedCode) return result;

  throw new Error(
    `removeMarker: could not find console.log with marker "${marker}" in minified code`
  );
}
