import { createHash } from "crypto";
import * as t from "@babel/types";
import type { FunctionFingerprint } from "./types.js";

/**
 * Computes a fingerprint for a function that can be used for caching
 * and cross-version matching.
 *
 * Currently computes only exactHash. Structured to allow future expansion
 * with additional hash types for fuzzy matching.
 */
export function computeFingerprint(fnNode: t.Function): FunctionFingerprint {
  return {
    exactHash: computeExactHash(fnNode)
    // Future: add structureHash, signatureHash, etc.
  };
}

/**
 * Computes an exact structural hash for a function that is stable across
 * different minified versions of the same code.
 *
 * The hash ignores:
 * - Identifier names (replaced with positional placeholders)
 * - String literal content (replaced with length marker)
 * - Numeric literal exact values (replaced with magnitude bucket)
 * - Source locations
 *
 * This allows caching humanification results and reusing them
 * when the same function appears with different minified names.
 */
export function computeExactHash(fnNode: t.Function): string {
  const cloned = t.cloneNode(fnNode, true);
  const normalized = normalizeAST(cloned);
  const serialized = serializeForHash(normalized);
  return createHash("sha256").update(serialized).digest("hex").slice(0, 16);
}

/**
 * @deprecated Use computeFingerprint() instead. Kept for backwards compatibility.
 */
export function computeStructuralHash(fnNode: t.Function): string {
  return computeExactHash(fnNode);
}

/**
 * Normalizes an AST node for structural comparison.
 * Replaces identifiers with positional placeholders and normalizes literals.
 * Uses manual tree walking since babel traverse doesn't work well with detached nodes.
 */
function normalizeAST(node: t.Function): t.Function {
  let placeholderCounter = 0;
  const identifierMap = new Map<string, string>();

  function getPlaceholder(name: string): string {
    if (!identifierMap.has(name)) {
      identifierMap.set(name, `$${placeholderCounter++}`);
    }
    return identifierMap.get(name)!;
  }

  function visit(node: t.Node | null | undefined): void {
    if (!node) return;

    // Remove location info
    const anyNode = node as t.Node & {
      loc?: t.SourceLocation | null;
      start?: number | null;
      end?: number | null;
      leadingComments?: t.Comment[] | null;
      trailingComments?: t.Comment[] | null;
    };
    delete anyNode.loc;
    delete anyNode.start;
    delete anyNode.end;
    delete anyNode.leadingComments;
    delete anyNode.trailingComments;

    // Normalize specific node types
    if (t.isIdentifier(node)) {
      node.name = getPlaceholder(node.name);
    } else if (t.isStringLiteral(node)) {
      const len = node.value.length;
      node.value = `__STR_${len}__`;
      // Also clear the extra field which contains raw string representation
      delete (node as any).extra;
    } else if (t.isNumericLiteral(node)) {
      const val = node.value;
      const magnitude = val === 0 ? 0 : Math.floor(Math.log10(Math.abs(val) + 1));
      node.value = magnitude;
      // Clear extra field which may contain raw representation
      delete (node as any).extra;
    } else if (t.isBigIntLiteral(node)) {
      node.value = "0";
      delete (node as any).extra;
    } else if (t.isTemplateLiteral(node)) {
      for (const quasi of node.quasis) {
        const len = quasi.value.raw.length;
        quasi.value.raw = `__TPL_${len}__`;
        quasi.value.cooked = `__TPL_${len}__`;
      }
    }

    // Recursively visit children
    for (const key of Object.keys(node)) {
      if (key === "type" || key === "loc" || key === "start" || key === "end") {
        continue;
      }

      const value = (node as Record<string, unknown>)[key];

      if (Array.isArray(value)) {
        for (const item of value) {
          if (item && typeof item === "object" && "type" in item) {
            visit(item as t.Node);
          }
        }
      } else if (value && typeof value === "object" && "type" in value) {
        visit(value as t.Node);
      }
    }
  }

  visit(node);
  return node;
}

/**
 * Serializes an AST node to a stable string representation for hashing.
 * Uses a custom serialization to ensure stability.
 */
function serializeForHash(node: t.Node): string {
  return JSON.stringify(node, (key, value) => {
    if (key === "loc" || key === "start" || key === "end") {
      return undefined;
    }
    if (key === "leadingComments" || key === "trailingComments") {
      return undefined;
    }
    return value;
  });
}

/**
 * Builds a mapping from placeholder names back to identifier names.
 * Used when applying cached renames to a new function.
 *
 * This walks the function in the same order as normalizeAST to ensure
 * placeholder assignments match.
 */
export function buildPlaceholderMapping(
  fnNode: t.Function
): Map<string, string> {
  const mapping = new Map<string, string>();
  let placeholderCounter = 0;
  const identifierMap = new Map<string, string>();

  function getPlaceholder(name: string): string {
    if (!identifierMap.has(name)) {
      const placeholder = `$${placeholderCounter++}`;
      identifierMap.set(name, placeholder);
      // Store the reverse mapping
      mapping.set(placeholder, name);
    }
    return identifierMap.get(name)!;
  }

  function visit(node: t.Node | null | undefined): void {
    if (!node) return;

    if (t.isIdentifier(node)) {
      getPlaceholder(node.name);
    }

    // Recursively visit children in same order as normalizeAST
    for (const key of Object.keys(node)) {
      if (key === "type" || key === "loc" || key === "start" || key === "end") {
        continue;
      }

      const value = (node as Record<string, unknown>)[key];

      if (Array.isArray(value)) {
        for (const item of value) {
          if (item && typeof item === "object" && "type" in item) {
            visit(item as t.Node);
          }
        }
      } else if (value && typeof value === "object" && "type" in value) {
        visit(value as t.Node);
      }
    }
  }

  visit(fnNode);
  return mapping;
}
