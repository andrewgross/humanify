import type { LLMContext } from "../analysis/types.js";
import type { NameSuggestion, ValidationResult } from "./types.js";

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
  // Strict mode reserved
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
 * Validates LLM responses before applying renames.
 *
 * Checks:
 * - Valid JavaScript identifier syntax
 * - Not a reserved word
 * - Not already in use in scope
 * - Reasonable length
 */
export function validateSuggestion(
  suggestion: NameSuggestion,
  context: LLMContext
): ValidationResult {
  const { name } = suggestion;

  // Must be valid JS identifier
  if (!isValidIdentifier(name)) {
    return { valid: false, reason: "Invalid identifier syntax" };
  }

  // Must not be reserved word
  if (RESERVED_WORDS.has(name)) {
    return { valid: false, reason: `"${name}" is a reserved word` };
  }

  // Must not conflict with existing bindings
  if (context.usedIdentifiers.has(name)) {
    return { valid: false, reason: `"${name}" is already in use` };
  }

  // Sanity check: not too long
  if (name.length > 50) {
    return { valid: false, reason: "Name too long (max 50 characters)" };
  }

  // Sanity check: not too short (single char is suspicious but allowed)
  // Single char names are valid but often indicate the LLM didn't try hard

  return { valid: true };
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
    sanitized = "_" + sanitized;
  }

  // Ensure it's not empty
  if (!sanitized) {
    sanitized = "_unnamed";
  }

  // Handle reserved words
  if (RESERVED_WORDS.has(sanitized)) {
    sanitized = sanitized + "_";
  }

  return sanitized;
}

/**
 * Resolves naming conflicts using smart strategies.
 */
export function resolveConflict(name: string, usedNames: Set<string>): string {
  // Strategy 1: Try common suffixes
  const suffixes = ["Val", "Var", "Ref", "Item", "Data", "Result", "Value"];
  for (const suffix of suffixes) {
    const candidate = name + suffix;
    if (!usedNames.has(candidate)) {
      return candidate;
    }
  }

  // Strategy 2: Try numeric suffix
  for (let i = 2; i <= 100; i++) {
    const candidate = name + i;
    if (!usedNames.has(candidate)) {
      return candidate;
    }
  }

  // Strategy 3: Underscore variants (single underscore only, never stack)
  const underscoreVariants = [`_${name}`, `${name}_`];
  for (const candidate of underscoreVariants) {
    if (!usedNames.has(candidate)) {
      return candidate;
    }
  }

  // Strategy 4: Contextual prefixes
  const contextualPrefixes = ["local", "inner"];
  for (const prefix of contextualPrefixes) {
    const candidate = `${prefix}_${name}`;
    if (!usedNames.has(candidate)) {
      return candidate;
    }
  }

  // Strategy 5: Extended numeric range (last resort)
  for (let i = 101; i <= 999; i++) {
    const candidate = name + i;
    if (!usedNames.has(candidate)) {
      return candidate;
    }
  }

  // Absolute last resort: single underscore + numeric
  for (let i = 1; i <= 999; i++) {
    const candidate = `_${name}_${i}`;
    if (!usedNames.has(candidate)) {
      return candidate;
    }
  }
  return `_${name}_fallback`;
}
