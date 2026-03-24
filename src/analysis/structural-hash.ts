import * as t from "@babel/types";
import { createHash } from "node:crypto";
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
  "angular"
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
    features: extractStructuralFeatures(fnNode)
  };
}

// ---------------------------------------------------------------------------
// Shared AST child-visiting utility
// ---------------------------------------------------------------------------

const SKIP_KEYS = new Set(["type", "loc", "start", "end"]);

function isASTNode(value: unknown): value is t.Node {
  return Boolean(value && typeof value === "object" && "type" in value);
}

function visitArrayValue(
  arr: unknown[],
  visitor: (child: t.Node) => void
): void {
  for (const item of arr) {
    if (isASTNode(item)) {
      visitor(item);
    }
  }
}

/**
 * Visits all child AST nodes of `node`, calling `visitor` on each.
 * Skips non-AST fields (type, loc, start, end).
 */
function visitChildren(node: t.Node, visitor: (child: t.Node) => void): void {
  for (const key of Object.keys(node)) {
    if (SKIP_KEYS.has(key)) continue;
    const value = (node as unknown as Record<string, unknown>)[key];
    if (Array.isArray(value)) {
      visitArrayValue(value, visitor);
    } else if (isASTNode(value)) {
      visitor(value);
    }
  }
}

// ---------------------------------------------------------------------------
// extractStructuralFeatures helpers
// ---------------------------------------------------------------------------

/** Handles control-flow and loop/try counting for a single node. */
function collectControlFlow(node: t.Node, features: StructuralFeatures): void {
  if (t.isReturnStatement(node)) {
    features.returnCount++;
  } else if (t.isIfStatement(node) || t.isConditionalExpression(node)) {
    features.branchCount++;
    features.complexity++;
  } else if (t.isSwitchStatement(node)) {
    features.branchCount++;
    features.complexity += node.cases.length;
  } else if (t.isLogicalExpression(node)) {
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
  }
}

/** Handles literal collection for a single node. */
function collectLiterals(node: t.Node, features: StructuralFeatures): void {
  if (t.isStringLiteral(node)) {
    features.stringLiterals.push(node.value);
  } else if (t.isNumericLiteral(node)) {
    features.numericLiterals.push(node.value);
  }
}

/** Handles member-expression property-access collection. */
function collectPropertyAccess(
  node: t.Node,
  features: StructuralFeatures
): void {
  if (t.isMemberExpression(node) && !node.computed) {
    if (t.isIdentifier(node.property)) {
      features.propertyAccesses.push(`.${node.property.name}`);
    }
  }
}

/** Handles call-expression external-call collection. */
function collectCallPatterns(node: t.Node, features: StructuralFeatures): void {
  if (!t.isCallExpression(node)) return;
  const callee = node.callee;
  if (t.isIdentifier(callee)) {
    if (KNOWN_GLOBALS.has(callee.name)) {
      features.externalCalls.push(callee.name);
    }
  } else if (t.isMemberExpression(callee)) {
    collectMemberCallee(callee, features);
  }
}

function collectMemberCallee(
  callee: t.MemberExpression,
  features: StructuralFeatures
): void {
  if (t.isIdentifier(callee.object) && KNOWN_GLOBALS.has(callee.object.name)) {
    if (t.isIdentifier(callee.property)) {
      features.externalCalls.push(
        `${callee.object.name}.${callee.property.name}`
      );
    }
  } else if (t.isIdentifier(callee.property)) {
    // Generic method call like arr.map, str.split
    features.externalCalls.push(`*.${callee.property.name}`);
  }
}

/**
 * Extracts structural features from a function for fingerprinting.
 * These features are stable across minification and support fuzzy matching.
 */
export function extractStructuralFeatures(
  fnNode: t.Function
): StructuralFeatures {
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
    propertyAccesses: []
  };

  // Walk the AST to collect features
  // Using manual traversal since babel traverse doesn't work well with detached nodes
  function visit(node: t.Node | null | undefined): void {
    if (!node) return;
    collectControlFlow(node, features);
    collectLiterals(node, features);
    collectPropertyAccess(node, features);
    collectCallPatterns(node, features);
    visitChildren(node, visit);
  }

  visit(fnNode);

  // Build CFG shape string
  features.cfgShape = buildCfgShapeString(fnNode);

  // Sort and deduplicate arrays for deterministic comparison
  features.stringLiterals = [...new Set(features.stringLiterals)].sort();
  features.numericLiterals = [...new Set(features.numericLiterals)].sort(
    (a, b) => a - b
  );
  features.externalCalls = [...new Set(features.externalCalls)].sort();
  features.propertyAccesses = [...new Set(features.propertyAccesses)].sort();

  return features;
}

// ---------------------------------------------------------------------------
// buildCfgShapeString helpers
// ---------------------------------------------------------------------------

function encodeIfStatement(
  stmt: t.IfStatement,
  shapes: string[],
  walkBlock: (node: t.Statement | t.BlockStatement) => void
): void {
  shapes.push("if");
  walkBlock(stmt.consequent);
  if (stmt.alternate) {
    shapes.push("else");
    walkBlock(stmt.alternate);
  }
}

function encodeTryStatement(
  stmt: t.TryStatement,
  shapes: string[],
  walkBlock: (node: t.Statement | t.BlockStatement) => void
): void {
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
}

function encodeSwitchStatement(
  stmt: t.SwitchStatement,
  shapes: string[],
  walkStatements: (stmts: t.Statement[]) => void
): void {
  shapes.push("switch");
  for (const caseClause of stmt.cases) {
    shapes.push(caseClause.test ? "case" : "default");
    if (caseClause.consequent.length > 0) {
      walkStatements(caseClause.consequent);
    }
  }
}

function encodeLoopStatement(
  stmt:
    | t.ForStatement
    | t.WhileStatement
    | t.ForOfStatement
    | t.ForInStatement
    | t.DoWhileStatement,
  shapes: string[],
  walkBlock: (node: t.Statement | t.BlockStatement) => void
): void {
  shapes.push(t.isDoWhileStatement(stmt) ? "do" : "loop");
  walkBlock(stmt.body);
}

function encodeSimpleStatement(stmt: t.Statement, shapes: string[]): boolean {
  if (t.isReturnStatement(stmt)) {
    shapes.push("ret");
    return true;
  }
  if (t.isThrowStatement(stmt)) {
    shapes.push("throw");
    return true;
  }
  if (t.isBreakStatement(stmt)) {
    shapes.push("break");
    return true;
  }
  if (t.isContinueStatement(stmt)) {
    shapes.push("cont");
    return true;
  }
  return false;
}

type LoopStatement =
  | t.ForStatement
  | t.WhileStatement
  | t.ForOfStatement
  | t.ForInStatement
  | t.DoWhileStatement;

function isLoopStatement(stmt: t.Statement): stmt is LoopStatement {
  return (
    t.isForStatement(stmt) ||
    t.isWhileStatement(stmt) ||
    t.isForOfStatement(stmt) ||
    t.isForInStatement(stmt) ||
    t.isDoWhileStatement(stmt)
  );
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
        encodeIfStatement(stmt, shapes, walkBlock);
      } else if (isLoopStatement(stmt)) {
        encodeLoopStatement(stmt, shapes, walkBlock);
      } else if (t.isTryStatement(stmt)) {
        encodeTryStatement(stmt, shapes, walkBlock);
      } else if (t.isSwitchStatement(stmt)) {
        encodeSwitchStatement(stmt, shapes, walkStatements);
      } else {
        encodeSimpleStatement(stmt, shapes);
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

// ---------------------------------------------------------------------------
// normalizeAST helpers
// ---------------------------------------------------------------------------

type AnyNodeWithMeta = t.Node & {
  loc?: t.SourceLocation | null;
  start?: number | null;
  end?: number | null;
  extra?: Record<string, unknown>;
  leadingComments?: t.Comment[] | null;
  trailingComments?: t.Comment[] | null;
  innerComments?: t.Comment[] | null;
};

function stripNodeMeta(node: t.Node): void {
  const anyNode = node as AnyNodeWithMeta;
  delete anyNode.loc;
  delete anyNode.start;
  delete anyNode.end;
  delete anyNode.extra;
  delete anyNode.leadingComments;
  delete anyNode.trailingComments;
  delete anyNode.innerComments;
}

function normalizeLiterals(
  node: t.Node,
  getPlaceholder: (name: string) => string
): void {
  if (t.isIdentifier(node)) {
    node.name = getPlaceholder(node.name);
  } else if (t.isStringLiteral(node)) {
    const len = node.value.length;
    node.value = `__STR_${len}__`;
  } else if (t.isNumericLiteral(node)) {
    const val = node.value;
    const magnitude = val === 0 ? 0 : Math.floor(Math.log10(Math.abs(val) + 1));
    node.value = magnitude;
  } else if (t.isBigIntLiteral(node)) {
    node.value = "0";
  } else if (t.isTemplateLiteral(node)) {
    for (const quasi of node.quasis) {
      const len = quasi.value.raw.length;
      // Replace the value object entirely rather than mutating properties,
      // because t.cloneNode() shares TemplateElement.value by reference
      quasi.value = { raw: `__TPL_${len}__`, cooked: `__TPL_${len}__` };
    }
  }
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
    return identifierMap.get(name) ?? `$${name}`;
  }

  function visit(node: t.Node | null | undefined): void {
    if (!node) return;
    stripNodeMeta(node);
    normalizeLiterals(node, getPlaceholder);
    visitChildren(node, visit);
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
    if (key === "loc" || key === "start" || key === "end" || key === "extra") {
      return undefined;
    }
    if (
      key === "leadingComments" ||
      key === "trailingComments" ||
      key === "innerComments"
    ) {
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
function _buildPlaceholderMapping(fnNode: t.Function): Map<string, string> {
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
    return identifierMap.get(name) ?? `$${name}`;
  }

  function visit(node: t.Node | null | undefined): void {
    if (!node) return;
    if (t.isIdentifier(node)) {
      getPlaceholder(node.name);
    }
    // Recursively visit children in same order as normalizeAST
    visitChildren(node, visit);
  }

  visit(fnNode);
  return mapping;
}
