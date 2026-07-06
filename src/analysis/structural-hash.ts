import type { Binding, NodePath } from "@babel/traverse";
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
  // Common library patterns. "$" deliberately absent: it is the first
  // name in minifier alphabets (esbuild), so treating it as jQuery makes
  // features rename-variant across versions.
  "jQuery",
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
export function computeFingerprint(
  fnPath: NodePath<t.Function>
): FunctionFingerprint {
  return {
    structuralHash: computeStructuralHash(fnPath),
    features: extractStructuralFeatures(fnPath.node)
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
  // Computed access (x[cb]()) references a BINDING, whose name is neither
  // minifier- nor version-stable — recording it would poison the feature.
  if (callee.computed || !t.isIdentifier(callee.property)) return;
  if (t.isIdentifier(callee.object) && KNOWN_GLOBALS.has(callee.object.name)) {
    features.externalCalls.push(
      `${callee.object.name}.${callee.property.name}`
    );
  } else {
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
 * different minified versions of the same code AND across renames of its
 * bindings (minifier→humanified, or different minifier runs).
 *
 * Placeholders are keyed by RESOLVED BINDING, not by name string, so any
 * consistent rename of bindings — including diversifying a reused name or
 * unifying distinct names — leaves the hash unchanged. Name-keyed
 * placeholders made 18.3% of hashes unstable under renaming; see
 * experiments/013-bun-cjs-classification/CLOSE-MATCH-ANOMALY.md.
 *
 * The hash ignores:
 * - Binding identifier names (replaced with per-binding positional
 *   placeholders, ordinals assigned by first occurrence in walk order)
 * - Label names (separate per-label placeholder namespace)
 * - String literal content (length marker), numeric literal exact values
 *   (magnitude bucket), source locations
 *
 * The hash KEEPS (they are minifier-stable and discriminate structure):
 * - Non-computed member property names and object/class member keys
 * - Free identifiers (true globals like `undefined`, `console`)
 */
export function computeStructuralHash(fnPath: NodePath<t.Function>): string {
  return hashAndMapPath(fnPath, false).hash;
}

// ---------------------------------------------------------------------------
// Rename-invariant serialization (binding-keyed placeholders)
// ---------------------------------------------------------------------------

const SERIALIZE_SKIP_KEYS = new Set([
  "type",
  "loc",
  "start",
  "end",
  "extra",
  "leadingComments",
  "trailingComments",
  "innerComments"
]);

/**
 * Identifier-node → resolved binding (null = free). Resolution is purely
 * position-based, so it is safe to cache across calls — ancestors re-hash
 * the same nested identifiers. Entries go stale if a scope is re-crawled
 * or bindings are renamed, but all fingerprinting happens at graph build,
 * before any renames.
 */
const bindingByIdentifierNode = new WeakMap<t.Identifier, Binding | null>();

/**
 * Resolves the binding an identifier occurrence refers to. Declaration ids
 * of function/class declarations need care: the id's own scope is the
 * function scope, where a same-named param/var would shadow the binding
 * the declaration creates (`function e(e) {}` is common minified output),
 * so resolve those from the parent scope.
 */
function resolveIdentifierBinding(p: NodePath<t.Identifier>): Binding | null {
  const cached = bindingByIdentifierNode.get(p.node);
  if (cached !== undefined) return cached;

  const parent = p.parentPath;
  let binding: Binding | null = null;
  if (
    (parent?.isFunctionDeclaration() || parent?.isClassDeclaration()) &&
    parent.node.id === p.node
  ) {
    binding =
      parent.scope.parent?.getBinding(p.node.name) ??
      parent.scope.getBinding(p.node.name) ??
      null;
  } else {
    binding = p.scope.getBinding(p.node.name) ?? null;
  }
  bindingByIdentifierNode.set(p.node, binding);
  return binding;
}

type IdentifierRole = "verbatim" | "label" | "slot";

/**
 * Classifies an identifier occurrence by its structural position:
 * - verbatim: non-computed member property names, object/class member
 *   keys, meta properties — minifier-stable content, never renamed
 * - label: label declarations/references — renamed by minifiers, but not
 *   bindings; normalized in their own namespace
 * - slot: binding references/declarations — normalized per binding
 */
function identifierRole(parent: t.Node | null, key: string): IdentifierRole {
  if (!parent) return "slot";
  if (
    (t.isMemberExpression(parent) || t.isOptionalMemberExpression(parent)) &&
    !parent.computed &&
    key === "property"
  ) {
    return "verbatim";
  }
  if (
    (t.isObjectProperty(parent) ||
      t.isObjectMethod(parent) ||
      t.isClassMethod(parent) ||
      t.isClassProperty(parent)) &&
    !parent.computed &&
    key === "key"
  ) {
    return "verbatim";
  }
  if (t.isMetaProperty(parent)) return "verbatim";
  if (
    (t.isLabeledStatement(parent) ||
      t.isBreakStatement(parent) ||
      t.isContinueStatement(parent)) &&
    key === "label"
  ) {
    return "label";
  }
  return "slot";
}

/** Pre-resolve bindings for every slot-position identifier in the subtree. */
function collectIdentifierBindings(rootPath: NodePath): void {
  if (rootPath.isIdentifier()) {
    resolveIdentifierBinding(rootPath as NodePath<t.Identifier>);
  }
  rootPath.traverse({
    Identifier(p: NodePath<t.Identifier>) {
      const role = identifierRole(p.parentPath?.node ?? null, String(p.key));
      if (role === "slot") resolveIdentifierBinding(p);
    }
  });
}

interface SerializeState {
  parts: string[];
  slotByBinding: Map<Binding, string>;
  labelSlots: Map<string, string>;
  /** placeholder → original name, binding slots only */
  mapping: Map<string, string>;
  counter: number;
  preserveLiterals: boolean;
}

function serializeIdentifier(
  node: t.Identifier,
  parent: t.Node | null,
  key: string,
  state: SerializeState
): void {
  const role = identifierRole(parent, key);
  if (role === "verbatim") {
    state.parts.push(`I=${node.name}`);
    return;
  }
  if (role === "label") {
    let slot = state.labelSlots.get(node.name);
    if (!slot) {
      slot = `L${state.labelSlots.size}`;
      state.labelSlots.set(node.name, slot);
    }
    state.parts.push(slot);
    return;
  }
  const binding = bindingByIdentifierNode.get(node);
  if (!binding) {
    // Free identifier (true global) — version-stable content.
    state.parts.push(`I=${node.name}`);
    return;
  }
  let slot = state.slotByBinding.get(binding);
  if (!slot) {
    slot = `$${state.counter++}`;
    state.slotByBinding.set(binding, slot);
    state.mapping.set(slot, node.name);
  }
  state.parts.push(slot);
}

/** Serialize literal node types; returns false when not a literal. */
function serializeLiteral(node: t.Node, state: SerializeState): boolean {
  const keep = state.preserveLiterals;
  if (t.isStringLiteral(node)) {
    state.parts.push(
      keep
        ? `S=${JSON.stringify(node.value)}`
        : `S=__STR_${node.value.length}__`
    );
    return true;
  }
  if (t.isNumericLiteral(node)) {
    const val = node.value;
    const magnitude = val === 0 ? 0 : Math.floor(Math.log10(Math.abs(val) + 1));
    state.parts.push(keep ? `N=${val}` : `N=${magnitude}`);
    return true;
  }
  if (t.isBigIntLiteral(node)) {
    state.parts.push(keep ? `B=${node.value}` : "B=0");
    return true;
  }
  if (t.isRegExpLiteral(node)) {
    state.parts.push(`R=${node.pattern}/${node.flags}`);
    return true;
  }
  if (t.isTemplateElement(node)) {
    state.parts.push(
      keep
        ? `Q=${JSON.stringify(node.value.raw)}`
        : `Q=${node.value.raw.length}`,
      `,tail=${node.tail}`
    );
    return true;
  }
  return false;
}

function serializeNode(
  node: t.Node,
  parent: t.Node | null,
  key: string,
  state: SerializeState
): void {
  if (t.isIdentifier(node)) {
    serializeIdentifier(node, parent, key, state);
    return;
  }
  if (t.isPrivateName(node)) {
    // Class-private names are member keys, not scope bindings; a nested
    // Identifier here must not resolve against same-named var bindings.
    state.parts.push(`P=#${node.id.name}`);
    return;
  }
  if (serializeLiteral(node, state)) return;

  state.parts.push(`${node.type}{`);
  for (const k of Object.keys(node)) {
    if (SERIALIZE_SKIP_KEYS.has(k)) continue;
    const value = (node as unknown as Record<string, unknown>)[k];
    if (value === undefined) continue;
    state.parts.push(`${k}:`);
    serializeValue(value, node, k, state);
    state.parts.push(";");
  }
  state.parts.push("}");
}

function serializeValue(
  value: unknown,
  parent: t.Node | null,
  key: string,
  state: SerializeState
): void {
  if (value === null) {
    state.parts.push("null");
    return;
  }
  if (Array.isArray(value)) {
    state.parts.push("[");
    for (const item of value) {
      serializeValue(item, parent, key, state);
      state.parts.push(",");
    }
    state.parts.push("]");
    return;
  }
  if (isASTNode(value)) {
    serializeNode(value, parent, key, state);
    return;
  }
  state.parts.push(JSON.stringify(value) ?? String(value));
}

/**
 * One walk producing both the structural hash and the placeholder mapping.
 * Slot ordinals are assigned by first occurrence in this walk, so two
 * structurally identical functions get aligned ordinals regardless of
 * binding names — the invariant translatePriorNames relies on.
 */
function hashAndMapPath(
  rootPath: NodePath,
  preserveLiterals: boolean
): { hash: string; mapping: Map<string, string> } {
  collectIdentifierBindings(rootPath);
  const state: SerializeState = {
    parts: [],
    slotByBinding: new Map(),
    labelSlots: new Map(),
    mapping: new Map(),
    counter: 0,
    preserveLiterals
  };
  serializeValue(rootPath.node, null, "root", state);
  const hash = createHash("sha256")
    .update(state.parts.join(""))
    .digest("hex")
    .slice(0, 16);
  return { hash, mapping: state.mapping };
}

/**
 * Builds a mapping from placeholder slots to original identifier names,
 * binding slots only — property names, object keys, labels, and free
 * identifiers never occupy slots. Used to transfer names between exact-
 * matched functions across versions.
 *
 * @returns Map<placeholder, originalName> e.g., "$0" → "a", "$1" → "b"
 */
export function buildPlaceholderMapping(
  fnPath: NodePath<t.Function>
): Map<string, string> {
  return hashAndMapPath(fnPath, false).mapping;
}

// ---------------------------------------------------------------------------
// Module binding fingerprinting
// ---------------------------------------------------------------------------

/**
 * Computes a content hash for a module binding's initializer or first
 * assignment. Used for cross-version matching of module binding names.
 * Literals are preserved (`var a = 4` and `var b = 2` must differ);
 * identifiers follow the same binding-keyed scheme as function hashes.
 *
 * @param initPath Path to the declarator's init (node may be null for `var a;`)
 * @param firstAssignmentRHSPath If init is null, path to the first assignment's RHS
 * @returns Content hash and source indicator, or null if unhashable
 */
export function computeBindingFingerprint(
  initPath: NodePath<t.Expression | null | undefined> | null | undefined,
  firstAssignmentRHSPath?: NodePath<t.Expression> | null
): { structuralHash: string; hashSource: "init" | "assignment" } | null {
  if (initPath?.node) {
    return {
      structuralHash: hashAndMapPath(initPath as NodePath, true).hash,
      hashSource: "init"
    };
  }
  if (firstAssignmentRHSPath?.node) {
    return {
      structuralHash: hashAndMapPath(firstAssignmentRHSPath, true).hash,
      hashSource: "assignment"
    };
  }
  return null;
}
