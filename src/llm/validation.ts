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

function checkIdentifierSyntax(name: string): ValidationResult | null {
  if (!isValidIdentifier(name)) {
    return { valid: false, reason: "Invalid identifier syntax" };
  }
  return null;
}

function checkReservedWord(name: string): ValidationResult | null {
  if (RESERVED_WORDS.has(name)) {
    return { valid: false, reason: `"${name}" is a reserved word` };
  }
  return null;
}

function checkNameConflict(
  name: string,
  context: LLMContext
): ValidationResult | null {
  if (context.usedIdentifiers.has(name)) {
    return { valid: false, reason: `"${name}" is already in use` };
  }
  return null;
}

function checkNameLength(name: string): ValidationResult | null {
  if (name.length > 50) {
    return { valid: false, reason: "Name too long (max 50 characters)" };
  }
  return null;
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

  return (
    checkIdentifierSyntax(name) ??
    checkReservedWord(name) ??
    checkNameConflict(name, context) ??
    checkNameLength(name) ?? { valid: true }
  );
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

  // Handle reserved words
  if (RESERVED_WORDS.has(sanitized)) {
    sanitized = `${sanitized}_`;
  }

  return sanitized;
}

function findWithSuffixes(name: string, usedNames: Set<string>): string | null {
  const suffixes = ["Val", "Var", "Ref", "Item", "Data", "Result", "Value"];
  for (const suffix of suffixes) {
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
