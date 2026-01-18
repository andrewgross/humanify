import { createHash } from "crypto";
import * as t from "@babel/types";
import type { FunctionFingerprint, StructuralFeatures } from "./types.js";

// Known browser/Node.js built-in globals that indicate external calls
const KNOWN_GLOBALS = new Set([
  // Browser APIs
  "fetch",
  "setTimeout",
  "setInterval",
  "clearTimeout",
  "clearInterval",
  "requestAnimationFrame",
  "cancelAnimationFrame",
  "alert",
  "confirm",
  "prompt",
  "console",
  "document",
  "window",
  "navigator",
  "location",
  "history",
  "localStorage",
  "sessionStorage",
  "indexedDB",
  "XMLHttpRequest",
  "WebSocket",
  "Worker",
  "Blob",
  "File",
  "FileReader",
  "URL",
  "URLSearchParams",
  "FormData",
  "Headers",
  "Request",
  "Response",
  "AbortController",
  "CustomEvent",
  "MutationObserver",
  "IntersectionObserver",
  "ResizeObserver",
  "PerformanceObserver",
  // Node.js
  "require",
  "process",
  "Buffer",
  "global",
  "__dirname",
  "__filename",
  // Built-in constructors/objects
  "Array",
  "Object",
  "String",
  "Number",
  "Boolean",
  "Symbol",
  "BigInt",
  "Map",
  "Set",
  "WeakMap",
  "WeakSet",
  "Promise",
  "Proxy",
  "Reflect",
  "JSON",
  "Math",
  "Date",
  "RegExp",
  "Error",
  "TypeError",
  "RangeError",
  "SyntaxError",
  "ReferenceError",
  "URIError",
  "EvalError",
  "Function",
  "eval",
  "parseInt",
  "parseFloat",
  "isNaN",
  "isFinite",
  "encodeURI",
  "decodeURI",
  "encodeURIComponent",
  "decodeURIComponent",
  "atob",
  "btoa",
  // Common library patterns
  "jQuery",
  "$",
  "React",
  "Vue",
  "angular",
]);

/**
 * Computes a fingerprint for a function that can be used for caching
 * and cross-version matching.
 *
 * Includes both the exact hash and decomposed structural features
 * for multi-resolution matching.
 */
export function computeFingerprint(fnNode: t.Function): FunctionFingerprint {
  return {
    exactHash: computeExactHash(fnNode),
    features: extractStructuralFeatures(fnNode),
  };
}

/**
 * Extracts structural features from a function for fingerprinting.
 * These features are stable across minification and support fuzzy matching.
 */
export function extractStructuralFeatures(fnNode: t.Function): StructuralFeatures {
  const features: StructuralFeatures = {
    arity: fnNode.params.length,
    hasRestParam: fnNode.params.some((p) => t.isRestElement(p)),
    returnCount: 0,
    complexity: 1, // Base cyclomatic complexity
    cfgShape: "",
    loopCount: 0,
    branchCount: 0,
    tryCount: 0,
    stringLiterals: [],
    numericLiterals: [],
    externalCalls: [],
    propertyAccesses: [],
  };

  // Walk the AST to collect features
  // Using manual traversal since babel traverse doesn't work well with detached nodes
  function visit(node: t.Node | null | undefined): void {
    if (!node) return;

    // Count control flow structures
    if (t.isReturnStatement(node)) {
      features.returnCount++;
    } else if (t.isIfStatement(node)) {
      features.branchCount++;
      features.complexity++;
    } else if (t.isConditionalExpression(node)) {
      features.branchCount++;
      features.complexity++;
    } else if (t.isSwitchStatement(node)) {
      features.branchCount++;
      features.complexity += node.cases.length;
    } else if (t.isLogicalExpression(node)) {
      // && and || add to complexity
      if (node.operator === "&&" || node.operator === "||") {
        features.complexity++;
      }
    } else if (
      t.isForStatement(node) ||
      t.isWhileStatement(node) ||
      t.isDoWhileStatement(node) ||
      t.isForInStatement(node) ||
      t.isForOfStatement(node)
    ) {
      features.loopCount++;
      features.complexity++;
    } else if (t.isTryStatement(node)) {
      features.tryCount++;
    } else if (t.isStringLiteral(node)) {
      features.stringLiterals.push(node.value);
    } else if (t.isNumericLiteral(node)) {
      features.numericLiterals.push(node.value);
    } else if (t.isMemberExpression(node) && !node.computed) {
      if (t.isIdentifier(node.property)) {
        features.propertyAccesses.push("." + node.property.name);
      }
    } else if (t.isCallExpression(node)) {
      const callee = node.callee;
      if (t.isIdentifier(callee)) {
        if (KNOWN_GLOBALS.has(callee.name)) {
          features.externalCalls.push(callee.name);
        }
      } else if (t.isMemberExpression(callee)) {
        // e.g., console.log, JSON.parse, arr.map
        if (t.isIdentifier(callee.object) && KNOWN_GLOBALS.has(callee.object.name)) {
          if (t.isIdentifier(callee.property)) {
            features.externalCalls.push(callee.object.name + "." + callee.property.name);
          }
        } else if (t.isIdentifier(callee.property)) {
          // Generic method call like arr.map, str.split
          features.externalCalls.push("*." + callee.property.name);
        }
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

  visit(fnNode);

  // Build CFG shape string
  features.cfgShape = buildCfgShapeString(fnNode);

  // Sort and deduplicate arrays for deterministic comparison
  features.stringLiterals = [...new Set(features.stringLiterals)].sort();
  features.numericLiterals = [...new Set(features.numericLiterals)].sort((a, b) => a - b);
  features.externalCalls = [...new Set(features.externalCalls)].sort();
  features.propertyAccesses = [...new Set(features.propertyAccesses)].sort();

  return features;
}

/**
 * Builds a compact string representation of the function's control flow structure.
 * This captures the shape of control flow without variable names or details.
 */
export function buildCfgShapeString(fnNode: t.Function): string {
  const shapes: string[] = [];

  function walkStatements(statements: t.Statement[]): void {
    for (const stmt of statements) {
      if (t.isIfStatement(stmt)) {
        shapes.push("if");
        walkBlock(stmt.consequent);
        if (stmt.alternate) {
          shapes.push("else");
          walkBlock(stmt.alternate);
        }
      } else if (
        t.isForStatement(stmt) ||
        t.isWhileStatement(stmt) ||
        t.isForOfStatement(stmt) ||
        t.isForInStatement(stmt)
      ) {
        shapes.push("loop");
        walkBlock(stmt.body);
      } else if (t.isDoWhileStatement(stmt)) {
        shapes.push("do");
        walkBlock(stmt.body);
      } else if (t.isTryStatement(stmt)) {
        shapes.push("try");
        walkBlock(stmt.block);
        if (stmt.handler) {
          shapes.push("catch");
          walkBlock(stmt.handler.body);
        }
        if (stmt.finalizer) {
          shapes.push("finally");
          walkBlock(stmt.finalizer);
        }
      } else if (t.isReturnStatement(stmt)) {
        shapes.push("ret");
      } else if (t.isSwitchStatement(stmt)) {
        shapes.push("switch");
        for (const caseClause of stmt.cases) {
          shapes.push(caseClause.test ? "case" : "default");
          if (caseClause.consequent.length > 0) {
            walkStatements(caseClause.consequent);
          }
        }
      } else if (t.isThrowStatement(stmt)) {
        shapes.push("throw");
      } else if (t.isBreakStatement(stmt)) {
        shapes.push("break");
      } else if (t.isContinueStatement(stmt)) {
        shapes.push("cont");
      }
    }
  }

  function walkBlock(node: t.Statement | t.BlockStatement): void {
    if (t.isBlockStatement(node)) {
      walkStatements(node.body);
    } else {
      walkStatements([node]);
    }
  }

  if (t.isBlockStatement(fnNode.body)) {
    walkStatements(fnNode.body.body);
  } else {
    // Arrow function with expression body
    shapes.push("expr");
  }

  return shapes.join("-") || "empty";
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
