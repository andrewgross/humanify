import globals from "globals";

/**
 * JavaScript reserved words that cannot be used as identifiers.
 */
export const RESERVED_WORDS = new Set([
  // Keywords
  "break",
  "case",
  "catch",
  "continue",
  "debugger",
  "default",
  "delete",
  "do",
  "else",
  "finally",
  "for",
  "function",
  "if",
  "in",
  "instanceof",
  "new",
  "return",
  "switch",
  "this",
  "throw",
  "try",
  "typeof",
  "var",
  "void",
  "while",
  "with",
  // ES6+ keywords
  "class",
  "const",
  "enum",
  "export",
  "extends",
  "import",
  "super",
  // Strict mode reserved (including implicit bindings that are
  // SyntaxError when used as parameter/binding names in strict mode / ESM)
  "arguments",
  "implements",
  "interface",
  "let",
  "package",
  "private",
  "protected",
  "public",
  "static",
  "yield",
  // ES2017+
  "await",
  // Literals
  "null",
  "true",
  "false",
  // Global values
  "undefined",
  "NaN",
  "Infinity"
]);

/**
 * High-risk host globals a rename must never shadow (review C1). This is a
 * deliberately CURATED list, not all of globals.browser/worker: the full
 * sets would forbid ~1,125 names including desirable identifiers (event,
 * status, length, name, ...) — measured at 935 prior-name transfer
 * rejections on the real 20MB bundle. Within-file capture is guarded
 * soundly by the free-name invariant (a rename may never bind a name the
 * file observes as free); this list only covers hosts whose shadowing is
 * hazardous even when the file never references them directly.
 */
const HIGH_RISK_HOST_GLOBALS = [
  "window",
  "document",
  "self",
  "location",
  "navigator",
  "$",
  "jQuery",
  "define",
  "Bun",
  "importScripts",
  "postMessage"
];

/**
 * Well-known global built-in names that must not be used as rename targets.
 * Unlike RESERVED_WORDS (which are syntax-level keywords), these are runtime
 * globals — shadowing them causes TypeError at runtime (e.g. Date.now()).
 *
 * Derived from the `globals` package (used by ESLint) — covers ES builtins,
 * Node.js globals, and identifiers shared between Node.js and browsers,
 * plus the curated host globals above.
 */
export const GLOBAL_BUILTINS = new Set([
  ...Object.keys(globals.builtin),
  ...Object.keys(globals.nodeBuiltin),
  ...Object.keys(globals["shared-node-browser"]),
  ...HIGH_RISK_HOST_GLOBALS
]);

/**
 * Validates if a string is a valid JavaScript identifier.
 */
export function isValidIdentifier(name: string): boolean {
  if (!name || name.length === 0) {
    return false;
  }

  // Check first character: must be letter, underscore, or $
  if (!/^[a-zA-Z_$]/.test(name)) {
    return false;
  }

  // Check remaining characters: must be alphanumeric, underscore, or $
  if (!/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(name)) {
    return false;
  }

  return true;
}

/**
 * Sanitizes a string to be a valid JavaScript identifier.
 * Used as a fallback when validation fails.
 */
export function sanitizeIdentifier(name: string): string {
  // Remove invalid characters
  let sanitized = name.replace(/[^a-zA-Z0-9_$]/g, "");

  // Ensure it doesn't start with a number
  if (/^[0-9]/.test(sanitized)) {
    sanitized = `_${sanitized}`;
  }

  // Ensure it's not empty
  if (!sanitized) {
    sanitized = "_unnamed";
  }

  // Handle reserved words and global built-ins
  if (RESERVED_WORDS.has(sanitized) || GLOBAL_BUILTINS.has(sanitized)) {
    sanitized = `${sanitized}_`;
  }

  return sanitized;
}

/**
 * The decoration words the conflict ladder appends, in ladder order.
 * Single source: the prior-name snap's stem stripper derives from this
 * list, so every producible decoration is also strippable (a `Result`
 * variant once escaped the stripper — consolidation audit 2026-07-23).
 */
export const DECORATION_WORDS = [
  "Val",
  "Var",
  "Ref",
  "Item",
  "Data",
  "Result",
  "Value"
] as const;

function findWithSuffixes(name: string, usedNames: Set<string>): string | null {
  for (const suffix of DECORATION_WORDS) {
    const candidate = name + suffix;
    if (!usedNames.has(candidate)) return candidate;
  }
  return null;
}

function findWithNumericSuffix(
  name: string,
  usedNames: Set<string>,
  start: number,
  end: number
): string | null {
  for (let i = start; i <= end; i++) {
    const candidate = name + i;
    if (!usedNames.has(candidate)) return candidate;
  }
  return null;
}

function findWithUnderscoreVariant(
  name: string,
  usedNames: Set<string>
): string | null {
  const variants = [`_${name}`, `${name}_`];
  for (const candidate of variants) {
    if (!usedNames.has(candidate)) return candidate;
  }
  return null;
}

function findWithContextualPrefix(
  name: string,
  usedNames: Set<string>
): string | null {
  const prefixes = ["local", "inner"];
  for (const prefix of prefixes) {
    const candidate = `${prefix}_${name}`;
    if (!usedNames.has(candidate)) return candidate;
  }
  return null;
}

function findWithIndexedUnderscore(
  name: string,
  usedNames: Set<string>
): string {
  for (let i = 1; i <= 999; i++) {
    const candidate = `_${name}_${i}`;
    if (!usedNames.has(candidate)) return candidate;
  }
  return `_${name}_fallback`;
}

/**
 * Resolves naming conflicts using smart strategies.
 */
export function resolveConflict(name: string, usedNames: Set<string>): string {
  return (
    findWithSuffixes(name, usedNames) ??
    findWithNumericSuffix(name, usedNames, 2, 100) ??
    findWithUnderscoreVariant(name, usedNames) ??
    findWithContextualPrefix(name, usedNames) ??
    findWithNumericSuffix(name, usedNames, 101, 999) ??
    findWithIndexedUnderscore(name, usedNames)
  );
}
