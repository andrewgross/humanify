import type { NodePath } from "@babel/core";
import * as t from "@babel/types";
import * as babelTraverse from "@babel/traverse";
import type { FunctionNode } from "./types.js";
import { computeFingerprint } from "./structural-hash.js";

const traverse: typeof babelTraverse.default =
  typeof babelTraverse.default === "function"
    ? babelTraverse.default
    : (babelTraverse.default as any).default;

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
        status: "pending"
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
      // Child depends on parent (even without a call relationship)
      fn.internalCallees.add(parentFn);
      parentFn.callers.add(fn);
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
    currentPath = currentPath.parentPath;
  }

  return null;
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
 */
export function findLeafFunctions(functions: FunctionNode[]): FunctionNode[] {
  return functions.filter((fn) => fn.internalCallees.size === 0);
}

/**
 * Detects cycles in the function dependency graph.
 * Returns arrays of strongly connected components with more than one node.
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
