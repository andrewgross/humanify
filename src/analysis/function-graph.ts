import type { NodePath } from "@babel/core";
import * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import type { FunctionNode, ModuleBindingNode, RenameNode, UnifiedGraph } from "./types.js";
import { computeFingerprint } from "./structural-hash.js";
import { generate, traverse } from "../babel-utils.js";
import {
  getModuleLevelBindings,
  collectAssignmentContext,
  collectUsageExamples,
  truncateSnippet,
  MAX_CONTEXT_SNIPPETS
} from "../plugins/rename.js";
import { debug } from "../debug.js";

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
  filePath: string = "unknown"
): FunctionNode[] {
  const functions = new Map<string, FunctionNode>();

  // First pass: collect all functions
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

  // Second pass: analyze call expressions to build dependencies
  for (const fn of functions.values()) {
    analyzeCallees(fn, functions);
  }

  // Third pass: add scope nesting dependencies
  // Nested functions should depend on their parent function even without
  // call relationships, because they may reference variables from the parent
  // scope that need to be renamed first.
  addScopeNestingDependencies(functions);

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
  allFunctions: Map<string, FunctionNode>
): void {
  fn.path.traverse({
    CallExpression(callPath: NodePath<t.CallExpression>) {
      const callee = callPath.node.callee;

      if (t.isIdentifier(callee)) {
        handleIdentifierCallee(fn, callPath, callee, allFunctions);
      } else if (t.isMemberExpression(callee)) {
        handleMemberExpressionCallee(fn, callee);
      } else if (t.isFunction(callee)) {
        // Immediately invoked function expression - check if it's in our graph
        const calleeId = findFunctionInGraph(callee, allFunctions);
        if (calleeId) {
          const targetFn = allFunctions.get(calleeId);
          if (targetFn) {
            fn.internalCallees.add(targetFn);
            targetFn.callers.add(fn);
          }
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
  allFunctions: Map<string, FunctionNode>
): void {
  for (const fn of allFunctions.values()) {
    const parentFn = findParentFunction(fn, allFunctions);
    if (parentFn && parentFn !== fn) {
      // Track scope parent for processing order, but NOT in internalCallees/callers
      // so it doesn't pollute fingerprint callee shapes
      fn.scopeParent = parentFn;
    }
  }
}

/**
 * Finds the immediate parent function of a given function in the graph.
 */
function findParentFunction(
  fn: FunctionNode,
  allFunctions: Map<string, FunctionNode>
): FunctionNode | null {
  // Walk up the AST to find the nearest enclosing function
  let currentPath = fn.path.parentPath;

  while (currentPath) {
    if (currentPath.isFunction()) {
      // Check if this function is in our graph
      for (const candidate of allFunctions.values()) {
        if (candidate.path.node === currentPath.node) {
          return candidate;
        }
      }
    }
    currentPath = currentPath.parentPath!;
  }

  return null;
}

/**
 * Maximum call sites to record per function (to avoid huge prompts).
 */
const MAX_CALL_SITES = 5;

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
    const statementParent = callPath.getStatementParent();
    const contextNode = statementParent?.node ?? callPath.node;
    let code = generate(contextNode, { compact: true }).code;

    // For short statements, include surrounding sibling statements for richer context
    if (code.length < 80 && statementParent) {
      const parentPath = statementParent.parentPath;
      if (parentPath?.isBlockStatement()) {
        const siblings = parentPath.get("body") as NodePath[];
        const idx = siblings.indexOf(statementParent);
        if (idx >= 0) {
          const lines: string[] = [];
          // Grab up to 2 preceding siblings
          const start = Math.max(0, idx - 2);
          for (let j = start; j <= idx; j++) {
            lines.push(generate(siblings[j].node, { compact: true }).code);
          }
          const combined = lines.join("\n");
          if (combined.length <= 200) {
            code = combined;
          }
        }
      }
    }

    if (code.length > 200) {
      code = code.slice(0, 197) + "...";
    }

    // Deduplicate (same statement may contain multiple calls to same function)
    if (targetFn.callSites.some(cs => cs.code === code)) return;

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
  allFunctions: Map<string, FunctionNode>
): void {
  const binding = callPath.scope.getBinding(callee.name);

  if (binding) {
    // Check if the binding points to a function in our graph
    const bindingPath = binding.path;

    if (isFunctionBinding(bindingPath)) {
      const targetId = findFunctionNodeId(bindingPath, allFunctions);
      if (targetId) {
        const targetFn = allFunctions.get(targetId);
        if (targetFn) {
          // Include self-references for cycle detection
          fn.internalCallees.add(targetFn);
          targetFn.callers.add(fn);
          // Record call site on target
          recordCallSite(targetFn, callPath);
          return;
        }
      }
    }

    // Check if it's a variable assigned to a function
    if (bindingPath.isVariableDeclarator()) {
      const init = bindingPath.get("init");
      if (init.isFunction()) {
        const targetId = findFunctionNodeId(init, allFunctions);
        if (targetId) {
          const targetFn = allFunctions.get(targetId);
          if (targetFn) {
            // Include self-references for cycle detection
            fn.internalCallees.add(targetFn);
            targetFn.callers.add(fn);
            // Record call site on target
            recordCallSite(targetFn, callPath);
            return;
          }
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
 * Finds the function node ID for a given path by matching against the graph.
 */
function findFunctionNodeId(
  path: NodePath,
  allFunctions: Map<string, FunctionNode>
): string | null {
  for (const [id, fn] of allFunctions) {
    if (fn.path.node === path.node) {
      return id;
    }
  }
  return null;
}

/**
 * Finds a function AST node in the graph.
 */
function findFunctionInGraph(
  fnNode: t.Function,
  allFunctions: Map<string, FunctionNode>
): string | null {
  for (const [id, fn] of allFunctions) {
    if (fn.path.node === fnNode) {
      return id;
    }
  }
  return null;
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
  return functions.filter((fn) => fn.internalCallees.size === 0 && !fn.scopeParent);
}

/**
 * Detects cycles in the function dependency graph using Tarjan's algorithm.
 * Returns arrays of strongly connected components with more than one node.
 *
 * Useful for diagnostics and analysis tools. The RenameProcessor handles
 * cycles dynamically by processing them when all non-cycle dependencies are done.
 */
export function detectCycles(functions: FunctionNode[]): FunctionNode[][] {
  const index = new Map<FunctionNode, number>();
  const lowlink = new Map<FunctionNode, number>();
  const onStack = new Set<FunctionNode>();
  const stack: FunctionNode[] = [];
  const sccs: FunctionNode[][] = [];
  let currentIndex = 0;

  function strongconnect(fn: FunctionNode): void {
    index.set(fn, currentIndex);
    lowlink.set(fn, currentIndex);
    currentIndex++;
    stack.push(fn);
    onStack.add(fn);

    for (const callee of fn.internalCallees) {
      if (!index.has(callee)) {
        strongconnect(callee);
        lowlink.set(fn, Math.min(lowlink.get(fn)!, lowlink.get(callee)!));
      } else if (onStack.has(callee)) {
        lowlink.set(fn, Math.min(lowlink.get(fn)!, index.get(callee)!));
      }
    }

    if (lowlink.get(fn) === index.get(fn)) {
      const scc: FunctionNode[] = [];
      let w: FunctionNode;
      do {
        w = stack.pop()!;
        onStack.delete(w);
        scc.push(w);
      } while (w !== fn);

      // Only return SCCs with cycles (more than one node, or self-loop)
      if (scc.length > 1) {
        sccs.push(scc);
      } else if (scc.length === 1 && fn.internalCallees.has(fn)) {
        sccs.push(scc);
      }
    }
  }

  for (const fn of functions) {
    if (!index.has(fn)) {
      strongconnect(fn);
    }
  }

  return sccs;
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
  const cycleMembers = new Set<FunctionNode>();

  for (const cycle of cycles) {
    for (const fn of cycle) {
      cycleMembers.add(fn);
    }
  }

  function canProcess(fn: FunctionNode): boolean {
    for (const callee of fn.internalCallees) {
      if (!processed.has(callee) && !cycleMembers.has(callee)) {
        return false;
      }
    }
    // Also wait for scope parent
    if (fn.scopeParent && !processed.has(fn.scopeParent) && !cycleMembers.has(fn.scopeParent)) {
      return false;
    }
    return true;
  }

  // Process non-cycle functions first
  let changed = true;
  while (changed) {
    changed = false;
    for (const fn of functions) {
      if (!processed.has(fn) && !cycleMembers.has(fn) && canProcess(fn)) {
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
  filePath: string = "unknown"
): UnifiedGraph {
  // Step 1: Build function graph
  const functions = buildFunctionGraph(ast, filePath);

  const nodes = new Map<string, RenameNode>();
  const dependencies = new Map<string, Set<string>>();
  const dependents = new Map<string, Set<string>>();
  const scopeParentEdges = new Set<string>();

  // Add function nodes to the unified graph
  for (const fn of functions) {
    nodes.set(fn.sessionId, { type: "function", node: fn });
    dependencies.set(fn.sessionId, new Set());
    dependents.set(fn.sessionId, new Set());
  }

  // Populate function->function dependencies from existing edges
  for (const fn of functions) {
    const deps = dependencies.get(fn.sessionId)!;
    for (const callee of fn.internalCallees) {
      deps.add(callee.sessionId);
      let depSet = dependents.get(callee.sessionId);
      if (!depSet) { depSet = new Set(); dependents.set(callee.sessionId, depSet); }
      depSet.add(fn.sessionId);
    }
    if (fn.scopeParent) {
      deps.add(fn.scopeParent.sessionId);
      let depSet = dependents.get(fn.scopeParent.sessionId);
      if (!depSet) { depSet = new Set(); dependents.set(fn.scopeParent.sessionId, depSet); }
      depSet.add(fn.sessionId);
      scopeParentEdges.add(`${fn.sessionId}->${fn.scopeParent.sessionId}`);
    }
  }

  // Step 2: Collect module-level bindings
  const bindingsResult = getModuleLevelBindings(ast);

  // Default scope for output — use program scope when no bindings detected
  let targetScope: any = null;
  traverse(ast, {
    Program(path: babelTraverse.NodePath<t.Program>) {
      targetScope = path.scope;
      path.stop();
    }
  });

  if (!bindingsResult) {
    return { nodes, dependencies, dependents, scopeParentEdges, targetScope };
  }

  const { bindings, targetScope: scope, wrapperPath } = bindingsResult;
  targetScope = scope;

  // Step 3: Create ModuleBindingNodes with context snippets
  const allIdentifiers = bindings.map(b => b.name);
  const identifierSet = new Set(allIdentifiers);
  const assignmentContext = collectAssignmentContext(ast, identifierSet);
  const assignmentCounts: Record<string, number> = {};
  for (const id of allIdentifiers) {
    assignmentCounts[id] = assignmentContext[id]?.length ?? 0;
  }
  const usageExamples = collectUsageExamples(ast, identifierSet, assignmentCounts);

  // Build a lookup from function path nodes to function nodes for cross-type edges
  const fnByNode = new Map<t.Node, FunctionNode>();
  for (const fn of functions) {
    fnByNode.set(fn.path.node, fn);
  }

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

  // Step 4: Build cross-type dependency edges

  // 4a. Module var -> module var dependencies
  // Check if a module var's initialization/assignment references another module var
  const moduleBindingSet = new Set(allIdentifiers);
  const scopeBindings = targetScope.bindings as Record<string, any>;

  for (const binding of bindings) {
    const babelBinding = scopeBindings[binding.name];
    if (!babelBinding) continue;

    const bindingPath = babelBinding.path;

    // Check VariableDeclarator init for references to other module vars
    if (bindingPath.isVariableDeclarator?.()) {
      const init = bindingPath.get?.("init");
      if (init && init.node) {
        checkReferencesForDeps(
          init,
          binding.name,
          moduleBindingSet,
          scopeBindings,
          dependencies,
          dependents
        );
      }
    }
  }

  // 4b. Module var -> function dependencies
  // If a module var's assignment calls a function, the var depends on that function
  for (const binding of bindings) {
    const babelBinding = scopeBindings[binding.name];
    if (!babelBinding) continue;

    const bindingPath = babelBinding.path;
    if (!bindingPath.isVariableDeclarator?.()) continue;

    const init = bindingPath.get?.("init");
    if (!init || !init.node) continue;

    // Check the init itself and walk its children for CallExpressions
    // that resolve to functions in the graph
    try {
      const checkCall = (callPath: babelTraverse.NodePath<t.CallExpression>) => {
        const callee = callPath.node.callee;
        if (t.isIdentifier(callee)) {
          const calleeBinding = callPath.scope.getBinding(callee.name);
          if (calleeBinding) {
            const fnNode = findFnForBinding(calleeBinding, fnByNode);
            if (fnNode) {
              addDependency(
                `module:${binding.name}`,
                fnNode.sessionId,
                dependencies,
                dependents
              );
            }
          }
        }
      };

      // Check if init itself is a CallExpression
      if (init.isCallExpression?.()) {
        checkCall(init as babelTraverse.NodePath<t.CallExpression>);
      }

      // Also traverse children for nested calls
      init.traverse?.({
        CallExpression: checkCall
      });
    } catch {
      // Skip if traversal fails
    }
  }

  // 4c. Function -> module var (class/constructor) dependencies
  // Only for module vars that are classes/constructors to avoid over-constraining
  const classVars = new Set<string>();
  for (const binding of bindings) {
    const babelBinding = scopeBindings[binding.name];
    if (!babelBinding) continue;

    const bindingPath = babelBinding.path;

    // Check if this is a class declaration
    if (bindingPath.isClassDeclaration?.()) {
      classVars.add(binding.name);
      continue;
    }

    // Check if variable is assigned a class expression
    if (bindingPath.isVariableDeclarator?.()) {
      const init = bindingPath.node?.init;
      if (t.isClassExpression(init)) {
        classVars.add(binding.name);
        continue;
      }
    }

    // Check if binding has `new X()` usage
    if (babelBinding.referencePaths) {
      for (const refPath of babelBinding.referencePaths) {
        const parent = refPath.parent;
        if (t.isNewExpression(parent) && parent.callee === refPath.node) {
          classVars.add(binding.name);
          break;
        }
      }
    }
  }

  // For each function, check if it references a class module var
  if (classVars.size > 0) {
    for (const fn of functions) {
      try {
        fn.path.traverse({
          Identifier(idPath: babelTraverse.NodePath<t.Identifier>) {
            const name = idPath.node.name;
            if (!classVars.has(name)) return;
            // Skip if binding identifier (declaration, not usage)
            if (idPath.isBindingIdentifier()) return;

            // Cast needed: after isBindingIdentifier() narrows to `never`
            const p = idPath as babelTraverse.NodePath<t.Identifier>;
            const binding = p.scope.getBinding(name);
            if (binding && binding.scope === targetScope) {
              addDependency(
                fn.sessionId,
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
  }

  debug.log("unified-graph",
    `Built unified graph: ${functions.length} functions, ${bindings.length} module bindings, ${classVars.size} class vars`
  );

  return { nodes, dependencies, dependents, scopeParentEdges, targetScope, wrapperPath };
}

/**
 * Checks references in an AST subtree for dependencies on other module bindings.
 */
function checkReferencesForDeps(
  path: babelTraverse.NodePath,
  ownerName: string,
  moduleBindingSet: Set<string>,
  scopeBindings: Record<string, any>,
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
  binding: any,
  fnByNode: Map<t.Node, FunctionNode>
): FunctionNode | null {
  const bindingPath = binding.path;

  // Direct function declaration
  if (bindingPath.isFunctionDeclaration?.() || bindingPath.isFunctionExpression?.() || bindingPath.isArrowFunctionExpression?.()) {
    return fnByNode.get(bindingPath.node) ?? null;
  }

  // Variable assigned to a function
  if (bindingPath.isVariableDeclarator?.()) {
    const init = bindingPath.node?.init;
    if (init && (t.isFunctionExpression(init) || t.isArrowFunctionExpression(init))) {
      return fnByNode.get(init) ?? null;
    }
  }

  return null;
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
  if (!deps) { deps = new Set(); dependencies.set(fromId, deps); }
  deps.add(toId);

  let depSet = dependents.get(toId);
  if (!depSet) { depSet = new Set(); dependents.set(toId, depSet); }
  depSet.add(fromId);
}
