import type { NodePath } from "@babel/core";
import type * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";

interface BabelBinding {
  path: babelTraverse.NodePath;
  referencePaths?: babelTraverse.NodePath[];
}
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
import { computeFingerprint } from "./structural-hash.js";
import type {
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
  profiler: Profiler = NULL_PROFILER
): FunctionNode[] {
  const functions = new Map<string, FunctionNode>();

  // First pass: collect all functions
  const fnSpan = profiler.startSpan("graph-build:functions", "graph");
  traverse(ast, {
    Function(path: NodePath<t.Function>) {
      const sessionId = getSessionId(path, filePath);
      const fingerprint = computeFingerprint(path.node);

      const node: FunctionNode = {
        sessionId,
        fingerprint,
        path,
        internalCallees: new Set(),
        externalCallees: new Set(),
        callers: new Set(),
        status: "pending",
        callSites: []
      };

      functions.set(sessionId, node);
    }
  });

  fnSpan.end({ functionCount: functions.size });

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
 * Finds leaf functions - functions that have no internal dependencies.
 * These can be processed first in the pipeline.
 *
 * This is the batch/synchronous equivalent of what RenameProcessor does
 * dynamically via isReady() when the done-set is empty. Used by the processor
 * for initial ready-set population, and useful for diagnostics/analysis tools.
 */
export function findLeafFunctions(functions: FunctionNode[]): FunctionNode[] {
  return functions.filter(
    (fn) => fn.internalCallees.size === 0 && !fn.scopeParent
  );
}

/**
 * State used by Tarjan's SCC algorithm, passed between helpers.
 */
interface TarjanState {
  index: Map<FunctionNode, number>;
  lowlink: Map<FunctionNode, number>;
  onStack: Set<FunctionNode>;
  stack: FunctionNode[];
  sccs: FunctionNode[][];
  currentIndex: number;
}

/**
 * Pops a completed SCC off the Tarjan stack and records it if it represents
 * a cycle (size > 1, or a single self-looping node).
 */
function finalizeSCC(fn: FunctionNode, state: TarjanState): void {
  const scc: FunctionNode[] = [];
  let w: FunctionNode;
  do {
    const popped = state.stack.pop();
    if (!popped) throw new Error("Tarjan stack underflow");
    w = popped;
    state.onStack.delete(w);
    scc.push(w);
  } while (w !== fn);

  if (scc.length > 1) {
    state.sccs.push(scc);
  } else if (scc.length === 1 && fn.internalCallees.has(fn)) {
    state.sccs.push(scc);
  }
}

/**
 * Tarjan's strongly-connected-components visit for one node.
 */
function strongconnect(fn: FunctionNode, state: TarjanState): void {
  state.index.set(fn, state.currentIndex);
  state.lowlink.set(fn, state.currentIndex);
  state.currentIndex++;
  state.stack.push(fn);
  state.onStack.add(fn);

  for (const callee of fn.internalCallees) {
    if (!state.index.has(callee)) {
      strongconnect(callee, state);
      state.lowlink.set(
        fn,
        Math.min(state.lowlink.get(fn) ?? 0, state.lowlink.get(callee) ?? 0)
      );
    } else if (state.onStack.has(callee)) {
      state.lowlink.set(
        fn,
        Math.min(state.lowlink.get(fn) ?? 0, state.index.get(callee) ?? 0)
      );
    }
  }

  if (state.lowlink.get(fn) === state.index.get(fn)) {
    finalizeSCC(fn, state);
  }
}

/**
 * Detects cycles in the function dependency graph using Tarjan's algorithm.
 * Returns arrays of strongly connected components with more than one node.
 *
 * Useful for diagnostics and analysis tools. The RenameProcessor handles
 * cycles dynamically by processing them when all non-cycle dependencies are done.
 */
export function detectCycles(functions: FunctionNode[]): FunctionNode[][] {
  const state: TarjanState = {
    index: new Map(),
    lowlink: new Map(),
    onStack: new Set(),
    stack: [],
    sccs: [],
    currentIndex: 0
  };

  for (const fn of functions) {
    if (!state.index.has(fn)) {
      strongconnect(fn, state);
    }
  }

  return state.sccs;
}

/**
 * Collects all functions that are part of any SCC cycle into a flat Set.
 */
function collectCycleMembers(cycles: FunctionNode[][]): Set<FunctionNode> {
  const cycleMembers = new Set<FunctionNode>();
  for (const cycle of cycles) {
    for (const fn of cycle) {
      cycleMembers.add(fn);
    }
  }
  return cycleMembers;
}

/**
 * Returns true when all of fn's non-cycle dependencies have been processed.
 */
function canProcessFn(
  fn: FunctionNode,
  processed: Set<FunctionNode>,
  cycleMembers: Set<FunctionNode>
): boolean {
  for (const callee of fn.internalCallees) {
    if (!processed.has(callee) && !cycleMembers.has(callee)) {
      return false;
    }
  }
  if (
    fn.scopeParent &&
    !processed.has(fn.scopeParent) &&
    !cycleMembers.has(fn.scopeParent)
  ) {
    return false;
  }
  return true;
}

/**
 * Gets the topological processing order for functions.
 * Functions with fewer dependencies come first.
 * Handles cycles by including them when their non-cycle dependencies are done.
 *
 * This is the batch/synchronous equivalent of the RenameProcessor's dynamic
 * ready-queue approach (isReady + checkNewlyReady). Useful for diagnostics
 * and a future `humanify analyze` command.
 */
export function getProcessingOrder(functions: FunctionNode[]): FunctionNode[] {
  const result: FunctionNode[] = [];
  const processed = new Set<FunctionNode>();
  const cycles = detectCycles(functions);
  const cycleMembers = collectCycleMembers(cycles);

  // Process non-cycle functions first
  let changed = true;
  while (changed) {
    changed = false;
    for (const fn of functions) {
      if (
        !processed.has(fn) &&
        !cycleMembers.has(fn) &&
        canProcessFn(fn, processed, cycleMembers)
      ) {
        result.push(fn);
        processed.add(fn);
        changed = true;
      }
    }
  }

  // Then add cycle members
  for (const cycle of cycles) {
    for (const fn of cycle) {
      if (!processed.has(fn)) {
        result.push(fn);
        processed.add(fn);
      }
    }
  }

  return result;
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
  maps: GraphMaps
): void {
  const { nodes, dependencies, dependents } = maps;

  for (const binding of bindings) {
    const sessionId = `module:${binding.name}`;
    const loc = binding.identifier.loc;

    const moduleNode: ModuleBindingNode = {
      sessionId,
      name: binding.name,
      identifier: binding.identifier,
      declaration: binding.declaration,
      declarationLine: loc?.start.line ?? 0,
      assignments: assignmentContext[binding.name] ?? [],
      usages: usageExamples[binding.name] ?? [],
      scope: targetScope,
      status: "pending"
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
  scopeBindings: Record<string, BabelBinding>,
  maps: GraphMaps
): void {
  for (const binding of bindings) {
    const babelBinding = scopeBindings[binding.name];
    if (!babelBinding) continue;

    const bindingPath = babelBinding.path;
    if (bindingPath.isVariableDeclarator?.()) {
      const init = bindingPath.get?.("init");
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
  scopeBindings: Record<string, BabelBinding>,
  fnByNode: Map<t.Node, FunctionNode>,
  maps: GraphMaps
): void {
  for (const binding of bindings) {
    const babelBinding = scopeBindings[binding.name];
    if (!babelBinding) continue;

    const bindingPath = babelBinding.path;
    if (!bindingPath.isVariableDeclarator?.()) continue;

    const init = bindingPath.get?.("init");
    if (!init?.node) continue;

    try {
      const checkCall = (
        callPath: babelTraverse.NodePath<t.CallExpression>
      ) => {
        const callee = callPath.node.callee;
        if (t.isIdentifier(callee)) {
          const calleeBinding = callPath.scope.getBinding(callee.name);
          if (calleeBinding) {
            const fnNode = findFnForBinding(calleeBinding, fnByNode);
            if (fnNode) {
              addDependency(
                `module:${binding.name}`,
                fnNode.sessionId,
                maps.dependencies,
                maps.dependents
              );
            }
          }
        }
      };

      if (init.isCallExpression?.()) {
        checkCall(init as babelTraverse.NodePath<t.CallExpression>);
      }
      init.traverse?.({ CallExpression: checkCall });
    } catch {
      // Skip if traversal fails
    }
  }
}

/**
 * Returns true if the given Babel binding represents a class or constructor.
 */
function isClassBinding(babelBinding: BabelBinding): boolean {
  const bindingPath = babelBinding.path;

  if (bindingPath.isClassDeclaration?.()) return true;

  if (bindingPath.isVariableDeclarator?.()) {
    const init = bindingPath.node?.init;
    if (t.isClassExpression(init)) return true;
  }

  if (babelBinding.referencePaths) {
    for (const refPath of babelBinding.referencePaths) {
      const parent = refPath.parent;
      if (t.isNewExpression(parent) && parent.callee === refPath.node) {
        return true;
      }
    }
  }

  return false;
}

/**
 * Identifies which module bindings are classes/constructors.
 */
function collectClassVars(
  bindings: Array<{ name: string }>,
  scopeBindings: Record<string, BabelBinding>
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

/**
 * Edge builder 4c: function -> class/constructor module var dependencies.
 */
function addFunctionToClassEdges(
  classVars: Set<string>,
  scopeBindings: Record<string, BabelBinding>,
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
 * 1. Build function graph (unchanged)
 * 2. Collect module-level bindings
 * 3. Create ModuleBindingNodes with context snippets
 * 4. Build cross-type dependency edges
 * 5. Return unified graph
 */
export function buildUnifiedGraph(
  ast: t.File,
  filePath: string = "unknown",
  profiler: Profiler = NULL_PROFILER,
  isEligible?: IsEligibleFn
): UnifiedGraph {
  // Step 1: Build function graph
  const functions = buildFunctionGraph(ast, filePath, profiler);

  const maps: GraphMaps = {
    nodes: new Map(),
    dependencies: new Map(),
    dependents: new Map(),
    scopeParentEdges: new Set()
  };

  addFunctionNodesToGraph(functions, maps);

  // Step 2: Collect module-level bindings
  const mbSpan = profiler.startSpan("graph-build:modules", "graph");
  const bindingsResult = getModuleLevelBindings(ast, isEligible);

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
    return { ...maps, targetScope };
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

  addModuleBindingNodesToGraph(
    bindings,
    assignmentContext,
    usageExamples,
    targetScope,
    maps
  );

  // Build a lookup from function path nodes to function nodes for cross-type edges
  const fnByNode = new Map<t.Node, FunctionNode>();
  for (const fn of functions) {
    fnByNode.set(fn.path.node, fn);
  }

  // Step 4: Build cross-type dependency edges
  const moduleBindingSet = new Set(allIdentifiers);
  const scopeBindings = targetScope.bindings as Record<string, BabelBinding>;

  addModuleToModuleEdges(bindings, moduleBindingSet, scopeBindings, maps);
  addModuleToFunctionEdges(bindings, scopeBindings, fnByNode, maps);

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
    wrapperPath
  };
}

/**
 * Checks references in an AST subtree for dependencies on other module bindings.
 */
function checkReferencesForDeps(
  path: babelTraverse.NodePath,
  ownerName: string,
  moduleBindingSet: Set<string>,
  scopeBindings: Record<string, BabelBinding>,
  dependencies: Map<string, Set<string>>,
  dependents: Map<string, Set<string>>
): void {
  try {
    path.traverse?.({
      Identifier(idPath: babelTraverse.NodePath<t.Identifier>) {
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
    });
  } catch {
    // Skip if traversal fails
  }
}

/**
 * Finds the FunctionNode for a Babel binding that resolves to a function.
 */
function findFnForBinding(
  binding: BabelBinding,
  fnByNode: Map<t.Node, FunctionNode>
): FunctionNode | null {
  const bindingPath = binding.path;

  // Direct function declaration
  if (
    bindingPath.isFunctionDeclaration?.() ||
    bindingPath.isFunctionExpression?.() ||
    bindingPath.isArrowFunctionExpression?.()
  ) {
    return fnByNode.get(bindingPath.node) ?? null;
  }

  // Variable assigned to a function
  if (bindingPath.isVariableDeclarator?.()) {
    const init = bindingPath.node?.init;
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
