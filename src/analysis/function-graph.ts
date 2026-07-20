import type { NodePath } from "@babel/core";
import type * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";

import { generate, traverse } from "../babel-utils.js";
import { debug } from "../debug.js";
import {
  collectAssignmentContext,
  collectUsageExamples,
  getModuleLevelBindings
} from "../rename/plugin.js";
import type { Profiler } from "../profiling/profiler.js";
import { NULL_PROFILER } from "../profiling/profiler.js";
import type { IsEligibleFn } from "../rename/rename-eligibility.js";
import { createIsEligible } from "../rename/rename-eligibility.js";
import { PENDING } from "../rename/lifecycle.js";
import {
  type BunModuleClassification,
  isInsideFactoryBody
} from "./bun-module-classification.js";
import {
  computeBindingFingerprint,
  computeFingerprintAndPlaceholders,
  hashPathWithMapping
} from "./structural-hash.js";
import type {
  FunctionFingerprint,
  FunctionNode,
  ModuleBindingNode,
  RenameNode,
  UnifiedGraph
} from "./types.js";

/**
 * Builds a dependency graph of all functions in an AST.
 *
 * The graph tracks:
 * - Internal callees: functions in our code that a function calls
 * - External callees: library/builtin calls
 * - Callers: reverse dependencies (who calls this function)
 *
 * This enables leaf-first processing where we humanify functions
 * that don't depend on other internal functions first.
 */
export function buildFunctionGraph(
  ast: t.File,
  filePath: string = "unknown",
  profiler: Profiler = NULL_PROFILER,
  classification?: BunModuleClassification | null
): FunctionNode[] {
  const functions = new Map<string, FunctionNode>();
  let skippedByClassification = 0;

  // First pass: collect all functions
  const fnSpan = profiler.startSpan("graph-build:functions", "graph");
  traverse(ast, {
    Function(path: NodePath<t.Function>) {
      // Skip functions inside any classified Bun CJS factory body — those
      // modules are treated as third-party and won't be processed.
      if (isInsideFactoryBody(path, classification ?? null)) {
        skippedByClassification++;
        path.skip();
        return;
      }
      const sessionId = getSessionId(path, filePath);
      // One serialize walk yields both the fingerprint and the placeholder
      // table — this loop runs for every function in the bundle.
      const { fingerprint, placeholders } =
        computeFingerprintAndPlaceholders(path);
      const loc = path.node.loc;

      const node: FunctionNode = {
        sessionId,
        position: loc
          ? { line: loc.start.line, column: loc.start.column }
          : null,
        fingerprint,
        placeholderMapping: placeholders.names,
        placeholderBindings: placeholders.bindings,
        path,
        internalCallees: new Set(),
        externalCallees: new Set(),
        callers: new Set(),
        state: PENDING,
        callSites: []
      };

      functions.set(sessionId, node);
    }
  });

  fnSpan.end({
    functionCount: functions.size,
    skippedByClassification
  });

  // Build node-to-FunctionNode map for O(1) lookups
  const nodeToFn = new Map<t.Node, FunctionNode>();
  for (const fn of functions.values()) {
    nodeToFn.set(fn.path.node, fn);
  }

  // Second pass: analyze call expressions to build dependencies
  const calleeSpan = profiler.startSpan("graph-build:callees", "graph");
  for (const fn of functions.values()) {
    analyzeCallees(fn, functions, nodeToFn);
  }
  calleeSpan.end();

  // Third pass: add scope nesting dependencies
  const scopeSpan = profiler.startSpan("graph-build:scopes", "graph");
  // Nested functions should depend on their parent function even without
  // call relationships, because they may reference variables from the parent
  // scope that need to be renamed first.
  addScopeNestingDependencies(functions, nodeToFn);
  scopeSpan.end();

  return Array.from(functions.values());
}

/**
 * Generates a session ID for a function based on its position in the file.
 */
function getSessionId(path: NodePath<t.Function>, filePath: string): string {
  const loc = path.node.loc;
  if (loc) {
    return `${filePath}:${loc.start.line}:${loc.start.column}`;
  }
  // Fallback to node start position if no loc
  const start = path.node.start ?? 0;
  return `${filePath}:pos:${start}`;
}

/**
 * Analyzes a function's call expressions to determine its dependencies.
 */
function analyzeCallees(
  fn: FunctionNode,
  allFunctions: Map<string, FunctionNode>,
  nodeToFn: Map<t.Node, FunctionNode>
): void {
  fn.path.traverse({
    CallExpression(callPath: NodePath<t.CallExpression>) {
      const callee = callPath.node.callee;

      if (t.isIdentifier(callee)) {
        handleIdentifierCallee(fn, callPath, callee, allFunctions, nodeToFn);
      } else if (t.isMemberExpression(callee)) {
        handleMemberExpressionCallee(fn, callee);
      } else if (t.isFunction(callee)) {
        // Immediately invoked function expression - check if it's in our graph
        const targetFn = nodeToFn.get(callee);
        if (targetFn) {
          fn.internalCallees.add(targetFn);
          targetFn.callers.add(fn);
        }
      }
    }
  });
}

/**
 * Adds scope nesting dependencies to the function graph.
 * Nested functions depend on their immediate parent function, ensuring
 * parents are processed before children (for proper variable renaming).
 */
function addScopeNestingDependencies(
  allFunctions: Map<string, FunctionNode>,
  nodeToFn: Map<t.Node, FunctionNode>
): void {
  for (const fn of allFunctions.values()) {
    const parentFn = findParentFunction(fn, nodeToFn);
    if (parentFn && parentFn !== fn) {
      // Track scope parent for processing order, but NOT in internalCallees/callers
      // so it doesn't pollute fingerprint callee shapes
      fn.scopeParent = parentFn;
    }
  }
}

/**
 * Finds the immediate parent function of a given function in the graph.
 * Uses nodeToFn map for O(1) lookup instead of scanning all functions.
 */
function findParentFunction(
  fn: FunctionNode,
  nodeToFn: Map<t.Node, FunctionNode>
): FunctionNode | null {
  // Walk up the AST to find the nearest enclosing function
  let currentPath: import("@babel/traverse").NodePath | null =
    fn.path.parentPath;

  while (currentPath) {
    if (currentPath.isFunction()) {
      const candidate = nodeToFn.get(currentPath.node);
      if (candidate) return candidate;
    }
    currentPath = currentPath.parentPath;
  }

  return null;
}

/**
 * Maximum call sites to record per function (to avoid huge prompts).
 */
const MAX_CALL_SITES = 5;

/**
 * Tries to expand a short statement's code by including up to 2 preceding siblings.
 * Returns the expanded code if it fits within 200 chars, otherwise returns null.
 */
function tryExpandWithSiblings(statementParent: NodePath): string | null {
  const parentPath = statementParent.parentPath;
  if (!parentPath?.isBlockStatement()) return null;

  const siblings = parentPath.get("body") as NodePath[];
  const idx = siblings.indexOf(statementParent);
  if (idx < 0) return null;

  const lines: string[] = [];
  const start = Math.max(0, idx - 2);
  for (let j = start; j <= idx; j++) {
    lines.push(generate(siblings[j].node, { compact: true }).code);
  }
  const combined = lines.join("\n");
  return combined.length <= 200 ? combined : null;
}

/**
 * Gathers the context code string for a call site, expanding to include
 * surrounding sibling statements when the statement is short enough.
 */
function gatherCallSiteCode(callPath: NodePath<t.CallExpression>): string {
  const statementParent = callPath.getStatementParent();
  const contextNode = statementParent?.node ?? callPath.node;
  let code = generate(contextNode, { compact: true }).code;

  if (code.length < 80 && statementParent) {
    const expanded = tryExpandWithSiblings(statementParent);
    if (expanded !== null) {
      code = expanded;
    }
  }

  if (code.length > 200) {
    code = `${code.slice(0, 197)}...`;
  }

  return code;
}

/**
 * Records a call site on the target function.
 */
function recordCallSite(
  targetFn: FunctionNode,
  callPath: NodePath<t.CallExpression>
): void {
  // Limit call sites to avoid huge prompts
  if (targetFn.callSites.length >= MAX_CALL_SITES) {
    return;
  }

  try {
    const code = gatherCallSiteCode(callPath);

    // Deduplicate (same statement may contain multiple calls to same function)
    if (targetFn.callSites.some((cs) => cs.code === code)) return;

    const loc = callPath.node.loc;
    targetFn.callSites.push({
      code,
      line: loc?.start.line ?? 0,
      column: loc?.start.column ?? 0
    });
  } catch {
    // Ignore code generation failures
  }
}

/**
 * Handles call expressions where the callee is an identifier (e.g., `foo()`).
 */
function handleIdentifierCallee(
  fn: FunctionNode,
  callPath: NodePath<t.CallExpression>,
  callee: t.Identifier,
  _allFunctions: Map<string, FunctionNode>,
  nodeToFn: Map<t.Node, FunctionNode>
): void {
  const binding = callPath.scope.getBinding(callee.name);

  if (binding) {
    // Check if the binding points to a function in our graph
    const bindingPath = binding.path;

    if (isFunctionBinding(bindingPath)) {
      const targetFn = nodeToFn.get(bindingPath.node);
      if (targetFn) {
        fn.internalCallees.add(targetFn);
        targetFn.callers.add(fn);
        recordCallSite(targetFn, callPath);
        return;
      }
    }

    // Check if it's a variable assigned to a function
    if (bindingPath.isVariableDeclarator()) {
      const init = bindingPath.get("init");
      if (init.isFunction()) {
        const targetFn = nodeToFn.get(init.node);
        if (targetFn) {
          fn.internalCallees.add(targetFn);
          targetFn.callers.add(fn);
          recordCallSite(targetFn, callPath);
          return;
        }
      }
    }
  }

  // No binding found or not a function - treat as external
  fn.externalCallees.add(callee.name);
}

/**
 * Handles call expressions where the callee is a member expression (e.g., `obj.method()`).
 */
function handleMemberExpressionCallee(
  fn: FunctionNode,
  callee: t.MemberExpression
): void {
  // For member expressions, we just track the method name as external
  // A more sophisticated implementation could track object methods
  if (t.isIdentifier(callee.property)) {
    fn.externalCallees.add(callee.property.name);
  } else if (t.isStringLiteral(callee.property)) {
    fn.externalCallees.add(callee.property.value);
  }
}

/**
 * Checks if a binding path refers to a function declaration or expression.
 */
function isFunctionBinding(bindingPath: NodePath): boolean {
  return (
    bindingPath.isFunctionDeclaration() ||
    bindingPath.isFunctionExpression() ||
    bindingPath.isArrowFunctionExpression()
  );
}

/**
 * Shared graph maps threaded through the unified-graph builder helpers.
 */
interface GraphMaps {
  nodes: Map<string, RenameNode>;
  dependencies: Map<string, Set<string>>;
  dependents: Map<string, Set<string>>;
  scopeParentEdges: Set<string>;
}

/**
 * Adds all FunctionNodes to the unified graph maps and populates
 * function->function (callee and scope-parent) dependency edges.
 */
function addFunctionNodesToGraph(
  functions: FunctionNode[],
  maps: GraphMaps
): void {
  const { nodes, dependencies, dependents, scopeParentEdges } = maps;

  for (const fn of functions) {
    nodes.set(fn.sessionId, { type: "function", node: fn });
    dependencies.set(fn.sessionId, new Set());
    dependents.set(fn.sessionId, new Set());
  }

  for (const fn of functions) {
    const deps = dependencies.get(fn.sessionId);
    if (!deps) throw new Error(`Missing dependency set for ${fn.sessionId}`);
    for (const callee of fn.internalCallees) {
      deps.add(callee.sessionId);
      let depSet = dependents.get(callee.sessionId);
      if (!depSet) {
        depSet = new Set();
        dependents.set(callee.sessionId, depSet);
      }
      depSet.add(fn.sessionId);
    }
    if (fn.scopeParent) {
      deps.add(fn.scopeParent.sessionId);
      let depSet = dependents.get(fn.scopeParent.sessionId);
      if (!depSet) {
        depSet = new Set();
        dependents.set(fn.scopeParent.sessionId, depSet);
      }
      depSet.add(fn.sessionId);
      scopeParentEdges.add(`${fn.sessionId}->${fn.scopeParent.sessionId}`);
    }
  }
}

/**
 * Computes a FunctionFingerprint for a module binding from its init expression.
 * Wraps the binding's content hash into the FunctionFingerprint shape used by the matching cascade.
 */
function buildBindingMatchFingerprint(
  scopeBindings: Record<string, babelTraverse.Binding>,
  bindingName: string
): FunctionFingerprint | null {
  const babelBinding = scopeBindings[bindingName];
  if (!babelBinding) return null;
  const bindingPath = babelBinding.path;
  // Class declarations have no init; their own body IS the hashable
  // content. Without this they were nameable (in the module pool since
  // exp016) but never MATCHABLE — both legs re-invented synonyms for
  // identical classes every run (ProcessEventManager→ProcessExitEmitter).
  if (bindingPath.isClassDeclaration()) {
    return {
      structuralHash: hashPathWithMapping(bindingPath).hash
    };
  }
  if (!bindingPath.isVariableDeclarator()) return null;

  const initPath = bindingPath.get("init") as babelTraverse.NodePath<
    t.Expression | null | undefined
  >;
  let firstAssignmentRHSPath: babelTraverse.NodePath<t.Expression> | null =
    null;
  if (!initPath.node) {
    const first = babelBinding.constantViolations[0];
    if (first?.node && t.isAssignmentExpression(first.node)) {
      firstAssignmentRHSPath = first.get(
        "right"
      ) as babelTraverse.NodePath<t.Expression>;
    }
  }

  const fp = computeBindingFingerprint(initPath, firstAssignmentRHSPath);
  if (!fp) return null;
  return { structuralHash: fp.structuralHash };
}

/**
 * Creates ModuleBindingNodes (step 3) and inserts them into the graph maps.
 */
function addModuleBindingNodesToGraph(
  bindings: Array<{
    name: string;
    identifier: t.Identifier;
    declaration: string;
  }>,
  assignmentContext: Record<string, string[]>,
  usageExamples: Record<string, string[]>,
  targetScope: babelTraverse.Scope,
  maps: GraphMaps,
  scopeBindings: Record<string, babelTraverse.Binding>
): void {
  const { nodes, dependencies, dependents } = maps;

  for (const binding of bindings) {
    const sessionId = `module:${binding.name}`;
    const loc = binding.identifier.loc;

    const moduleNode: ModuleBindingNode = {
      sessionId,
      position: loc ? { line: loc.start.line, column: loc.start.column } : null,
      name: binding.name,
      identifier: binding.identifier,
      declaration: binding.declaration,
      declarationLine: loc?.start.line ?? 0,
      assignments: assignmentContext[binding.name] ?? [],
      usages: usageExamples[binding.name] ?? [],
      scope: targetScope,
      state: PENDING,
      fingerprint: buildBindingMatchFingerprint(scopeBindings, binding.name),
      internalCallees: new Set(),
      callers: new Set(),
      externalCallees: new Set()
    };

    nodes.set(sessionId, { type: "module-binding", node: moduleNode });
    dependencies.set(sessionId, new Set());
    dependents.set(sessionId, new Set());
  }
}

/**
 * Edge builder 4a: module var -> module var dependencies via initializer references.
 */
function addModuleToModuleEdges(
  bindings: Array<{ name: string }>,
  moduleBindingSet: Set<string>,
  scopeBindings: Record<string, babelTraverse.Binding>,
  maps: GraphMaps
): void {
  for (const binding of bindings) {
    const babelBinding = scopeBindings[binding.name];
    if (!babelBinding) continue;

    const bindingPath = babelBinding.path;
    if (bindingPath.isVariableDeclarator()) {
      const init = bindingPath.get("init");
      if (init?.node) {
        checkReferencesForDeps(
          init as babelTraverse.NodePath,
          binding.name,
          moduleBindingSet,
          scopeBindings,
          maps.dependencies,
          maps.dependents
        );
      }
    }
  }
}

/**
 * Edge builder 4b: module var -> function dependencies via call expressions in initializers.
 */
function addModuleToFunctionEdges(
  bindings: Array<{ name: string }>,
  scopeBindings: Record<string, babelTraverse.Binding>,
  fnByNode: Map<t.Node, FunctionNode>,
  maps: GraphMaps
): void {
  for (const binding of bindings) {
    const babelBinding = scopeBindings[binding.name];
    if (!babelBinding) continue;

    const bindingPath = babelBinding.path;
    if (!bindingPath.isVariableDeclarator()) continue;

    const init = bindingPath.get("init");
    if (!init?.node) continue;

    try {
      const addFnEdge = (name: string, scope: babelTraverse.Scope) => {
        const refBinding = scope.getBinding(name);
        if (!refBinding) return;
        const fnNode = findFnForBinding(refBinding, fnByNode);
        if (fnNode) {
          addDependency(
            `module:${binding.name}`,
            fnNode.sessionId,
            maps.dependencies,
            maps.dependents
          );
        }
      };

      const checkCall = (
        callPath: babelTraverse.NodePath<t.CallExpression>
      ) => {
        const callee = callPath.node.callee;
        if (t.isIdentifier(callee)) {
          addFnEdge(callee.name, callPath.scope);
        }
      };

      // Bare references to functions (`var alias = someFn`, `{ handler: someFn }`)
      // carry the same dependency information as calls.
      const checkRef = (idPath: babelTraverse.NodePath<t.Identifier>) => {
        if (!idPath.isReferencedIdentifier()) return;
        addFnEdge(idPath.node.name, idPath.scope);
      };

      if (init.isCallExpression()) {
        checkCall(init as babelTraverse.NodePath<t.CallExpression>);
      }
      if (init.isIdentifier()) {
        checkRef(init as babelTraverse.NodePath<t.Identifier>);
      }
      init.traverse({ CallExpression: checkCall, Identifier: checkRef });
    } catch {
      // Skip if traversal fails
    }
  }
}

/**
 * Returns true if the given Babel binding represents a class or constructor.
 */
function isClassBinding(babelBinding: babelTraverse.Binding): boolean {
  const bindingPath = babelBinding.path;

  if (bindingPath.isClassDeclaration()) return true;

  if (bindingPath.isVariableDeclarator()) {
    const init = bindingPath.node.init;
    if (t.isClassExpression(init)) return true;
  }

  for (const refPath of babelBinding.referencePaths) {
    const parent = refPath.parent;
    if (t.isNewExpression(parent) && parent.callee === refPath.node) {
      return true;
    }
  }

  return false;
}

/**
 * Identifies which module bindings are classes/constructors.
 */
function collectClassVars(
  bindings: Array<{ name: string }>,
  scopeBindings: Record<string, babelTraverse.Binding>
): Set<string> {
  const classVars = new Set<string>();

  for (const binding of bindings) {
    const babelBinding = scopeBindings[binding.name];
    if (babelBinding && isClassBinding(babelBinding)) {
      classVars.add(binding.name);
    }
  }

  return classVars;
}

/**
 * Walks one reference path upward to the nearest enclosing function and records
 * a dependency from that function to the given class module var.
 */
function addClassEdgeForRef(
  refPath: babelTraverse.NodePath,
  className: string,
  fnByNode: Map<t.Node, FunctionNode>,
  maps: GraphMaps
): void {
  let current = refPath.parentPath;
  while (current) {
    if (current.isFunction()) {
      const fn = fnByNode.get(current.node);
      if (fn) {
        addDependency(
          fn.sessionId,
          `module:${className}`,
          maps.dependencies,
          maps.dependents
        );
      }
      break;
    }
    current = current.parentPath;
  }
}

/** Walk up from a reference path to find the enclosing FunctionNode. */
function findEnclosingFunction(
  refPath: babelTraverse.NodePath,
  fnByNode: Map<t.Node, FunctionNode>
): FunctionNode | null {
  let current = refPath.parentPath;
  while (current) {
    if (current.isFunction()) {
      return fnByNode.get(current.node) ?? null;
    }
    current = current.parentPath;
  }
  return null;
}

/**
 * Edge builder 4d: function -> binding reference edges (populates binding.callers).
 * For each module binding, walk referencePaths to find enclosing functions.
 */
function addFunctionToBindingReferenceEdges(
  bindings: Array<{ name: string }>,
  scopeBindings: Record<string, babelTraverse.Binding>,
  fnByNode: Map<t.Node, FunctionNode>,
  maps: GraphMaps
): void {
  for (const binding of bindings) {
    const babelBinding = scopeBindings[binding.name];
    if (!babelBinding?.referencePaths) continue;

    const moduleNode = maps.nodes.get(`module:${binding.name}`);
    if (!moduleNode || moduleNode.type !== "module-binding") continue;
    const mbNode = moduleNode.node;

    for (const refPath of babelBinding.referencePaths) {
      const fn = findEnclosingFunction(refPath, fnByNode);
      if (fn) mbNode.callers.add(fn);
    }
  }
}

/**
 * Wires internalCallees on ModuleBindingNodes from the dependency edges
 * already established by addModuleToFunctionEdges and addModuleToModuleEdges.
 */
function wireModuleBindingCallees(maps: GraphMaps): void {
  for (const [sessionId, renameNode] of maps.nodes) {
    if (renameNode.type !== "module-binding") continue;
    const mbNode = renameNode.node;
    const deps = maps.dependencies.get(sessionId);
    if (!deps) continue;

    for (const depId of deps) {
      const depNode = maps.nodes.get(depId);
      if (depNode) {
        mbNode.internalCallees.add(depNode.node);
      }
    }
  }
}

/**
 * Edge builder 4c: function -> class/constructor module var dependencies.
 */
function addFunctionToClassEdges(
  classVars: Set<string>,
  scopeBindings: Record<string, babelTraverse.Binding>,
  fnByNode: Map<t.Node, FunctionNode>,
  maps: GraphMaps
): void {
  for (const className of classVars) {
    const babelBinding = scopeBindings[className];
    if (!babelBinding?.referencePaths) continue;

    for (const refPath of babelBinding.referencePaths) {
      addClassEdgeForRef(refPath, className, fnByNode, maps);
    }
  }
}

/**
 * Builds a unified dependency graph containing both function nodes and module-level bindings.
 * This enables processing all renames in a single parallel pass.
 *
 * Steps:
 * 1. Collect module-level bindings (and Bun CJS classification, if applicable)
 * 2. Build function graph, skipping functions inside any third-party CJS factory body
 * 3. Create ModuleBindingNodes with context snippets
 * 4. Build cross-type dependency edges
 * 5. Return unified graph
 */
export function buildUnifiedGraph(
  ast: t.File,
  filePath: string = "unknown",
  profiler: Profiler = NULL_PROFILER,
  // Public-entry default: analysis-only callers (tests, experiments) have no
  // bundler/minifier context. The pipeline passes its RunConfig-resolved fn.
  isEligible: IsEligibleFn = createIsEligible(),
  source?: string
): UnifiedGraph {
  // Step 2 (run first so we can pass classification into the function pass):
  // collect module-level bindings.
  const mbSpan = profiler.startSpan("graph-build:modules", "graph");
  const bindingsResult = getModuleLevelBindings(ast, isEligible, source);
  const classification = bindingsResult?.classification ?? null;

  // Step 1: Build function graph, skipping functions inside classified
  // third-party CJS factory bodies.
  const functions = buildFunctionGraph(ast, filePath, profiler, classification);

  const maps: GraphMaps = {
    nodes: new Map(),
    dependencies: new Map(),
    dependents: new Map(),
    scopeParentEdges: new Set()
  };

  addFunctionNodesToGraph(functions, maps);

  // Default scope — use program scope when no bindings detected
  let targetScope: babelTraverse.Scope = null as unknown as babelTraverse.Scope;
  traverse(ast, {
    Program(path: babelTraverse.NodePath<t.Program>) {
      targetScope = path.scope;
      path.stop();
    }
  });

  if (!bindingsResult) {
    mbSpan.end({ bindingCount: 0 });
    return { ...maps, targetScope, classification };
  }

  const { bindings, targetScope: scope, wrapperPath } = bindingsResult;
  targetScope = scope;

  // Step 3: Create ModuleBindingNodes with context snippets
  const allIdentifiers = bindings.map((b) => b.name);
  const identifierSet = new Set(allIdentifiers);
  const assignmentContext = collectAssignmentContext(ast, identifierSet);
  const assignmentCounts: Record<string, number> = {};
  for (const id of allIdentifiers) {
    assignmentCounts[id] = assignmentContext[id]?.length ?? 0;
  }
  const usageExamples = collectUsageExamples(
    ast,
    identifierSet,
    assignmentCounts
  );

  // Step 4: Build cross-type dependency edges
  const moduleBindingSet = new Set(allIdentifiers);
  const scopeBindings = targetScope.bindings;

  addModuleBindingNodesToGraph(
    bindings,
    assignmentContext,
    usageExamples,
    targetScope,
    maps,
    scopeBindings
  );

  // Build a lookup from function path nodes to function nodes for cross-type edges
  const fnByNode = new Map<t.Node, FunctionNode>();
  for (const fn of functions) {
    fnByNode.set(fn.path.node, fn);
  }

  addModuleToModuleEdges(bindings, moduleBindingSet, scopeBindings, maps);
  addModuleToFunctionEdges(bindings, scopeBindings, fnByNode, maps);
  addFunctionToBindingReferenceEdges(bindings, scopeBindings, fnByNode, maps);

  // Wire internalCallees on ModuleBindingNodes from dependency edges
  wireModuleBindingCallees(maps);

  const classVars = collectClassVars(bindings, scopeBindings);
  addFunctionToClassEdges(classVars, scopeBindings, fnByNode, maps);

  mbSpan.end({ bindingCount: bindings.length, classVarCount: classVars.size });

  debug.log(
    "unified-graph",
    `Built unified graph: ${functions.length} functions, ${bindings.length} module bindings, ${classVars.size} class vars`
  );

  return {
    ...maps,
    targetScope,
    wrapperPath,
    classification
  };
}

/** Records a module-to-module dependency when idPath references another module binding. */
function recordModuleRefDep(
  idPath: babelTraverse.NodePath<t.Identifier>,
  ownerName: string,
  moduleBindingSet: Set<string>,
  scopeBindings: Record<string, babelTraverse.Binding>,
  dependencies: Map<string, Set<string>>,
  dependents: Map<string, Set<string>>
): void {
  const name = idPath.node.name;
  if (name === ownerName) return;
  if (!moduleBindingSet.has(name)) return;
  if (idPath.isBindingIdentifier()) return;

  // Cast needed: after isBindingIdentifier() narrows to `never`
  const p = idPath as babelTraverse.NodePath<t.Identifier>;
  const binding = p.scope.getBinding(name);
  if (binding && scopeBindings[name] === binding) {
    addDependency(
      `module:${ownerName}`,
      `module:${name}`,
      dependencies,
      dependents
    );
  }
}

/**
 * Checks references in an AST subtree for dependencies on other module bindings.
 * Visits the root node itself too — for alias inits (`var b = a`) the
 * subtree IS the identifier and traverse() alone would never see it.
 */
function checkReferencesForDeps(
  path: babelTraverse.NodePath,
  ownerName: string,
  moduleBindingSet: Set<string>,
  scopeBindings: Record<string, babelTraverse.Binding>,
  dependencies: Map<string, Set<string>>,
  dependents: Map<string, Set<string>>
): void {
  try {
    if (path.isIdentifier()) {
      recordModuleRefDep(
        path as babelTraverse.NodePath<t.Identifier>,
        ownerName,
        moduleBindingSet,
        scopeBindings,
        dependencies,
        dependents
      );
    }
    path.traverse({
      Identifier(idPath: babelTraverse.NodePath<t.Identifier>) {
        recordModuleRefDep(
          idPath,
          ownerName,
          moduleBindingSet,
          scopeBindings,
          dependencies,
          dependents
        );
      }
    });
  } catch {
    // Skip if traversal fails
  }
}

/**
 * Finds the FunctionNode for a Babel binding that resolves to a function.
 */
function findFnForBinding(
  binding: babelTraverse.Binding,
  fnByNode: Map<t.Node, FunctionNode>
): FunctionNode | null {
  const bindingPath = binding.path;

  // Direct function declaration
  if (
    bindingPath.isFunctionDeclaration() ||
    bindingPath.isFunctionExpression() ||
    bindingPath.isArrowFunctionExpression()
  ) {
    return fnByNode.get(bindingPath.node) ?? null;
  }

  // Variable assigned to a function
  if (bindingPath.isVariableDeclarator()) {
    const init = bindingPath.node.init;
    if (
      init &&
      (t.isFunctionExpression(init) || t.isArrowFunctionExpression(init))
    ) {
      return fnByNode.get(init) ?? null;
    }
  }

  return null;
}

/**
 * Computes the depth of the longest dependent chain for each node in the graph.
 * Used for critical-path priority dispatching: nodes with deeper dependent chains
 * should be processed first because they unblock more work.
 *
 * A node with no dependents has depth 1.
 * A node whose deepest dependent chain is N has depth N + 1.
 *
 * Computed via reverse BFS from sink nodes (zero dependents).
 * O(V+E) time complexity, computed once at graph build time.
 */
export function computeDependentDepths(
  graph: UnifiedGraph
): Map<string, number> {
  const depths = new Map<string, number>();
  const { nodes, dependents } = graph;
  const computing = new Set<string>();

  function getDepth(id: string): number {
    const cached = depths.get(id);
    if (cached !== undefined) return cached;
    if (computing.has(id)) return 1; // cycle — break with depth 1
    computing.add(id);

    const deps = dependents.get(id);
    if (!deps || deps.size === 0) {
      depths.set(id, 1);
      computing.delete(id);
      return 1;
    }

    let maxDependent = 0;
    for (const depId of deps) {
      maxDependent = Math.max(maxDependent, getDepth(depId));
    }

    const depth = 1 + maxDependent;
    depths.set(id, depth);
    computing.delete(id);
    return depth;
  }

  for (const id of nodes.keys()) {
    getDepth(id);
  }

  return depths;
}

/**
 * Adds a dependency edge to the graph.
 */
function addDependency(
  fromId: string,
  toId: string,
  dependencies: Map<string, Set<string>>,
  dependents: Map<string, Set<string>>
): void {
  let deps = dependencies.get(fromId);
  if (!deps) {
    deps = new Set();
    dependencies.set(fromId, deps);
  }
  deps.add(toId);

  let depSet = dependents.get(toId);
  if (!depSet) {
    depSet = new Set();
    dependents.set(toId, depSet);
  }
  depSet.add(fromId);
}
