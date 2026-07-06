/**
 * The single traversal that collects the bindings a function OWNS — params,
 * vars, block-scoped locals of nested (non-function) blocks, and the
 * function's own name binding.
 *
 * Two entry points serve the two consumers:
 * - collectOwnedBindingInfos — the LLM naming path. Excludes nested function
 *   declaration names: each function names itself in its own pass (leaf-first,
 *   with the best context), and a parent batch renaming the child overwrote
 *   the child's self-chosen name.
 * - buildOwnedBindingMap — the prior-version transfer path. Includes nested
 *   function declaration names: exact-match pairs legitimately carry them,
 *   and the child's own transfer then skips as no-binding, which is fine.
 */
import type { Binding, NodePath, Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import type { IsEligibleFn } from "./rename-eligibility.js";

/**
 * Binding info with the identifier node, its location, and owning scope.
 */
export interface BindingInfo {
  name: string;
  identifier: t.Identifier;
  /** The scope that OWNS this binding (needed for block-scoped vars in child scopes) */
  scope: Scope;
}

interface CollectorOptions {
  /** Include names of nested function declarations owned by this function. */
  includeNestedFunctionDeclNames: boolean;
}

/** All bindings owned by this function, for the LLM naming path. */
export function collectOwnedBindingInfos(
  fnPath: NodePath<t.Function>
): BindingInfo[] {
  return collectOwnedBindings(fnPath, {
    includeNestedFunctionDeclNames: false
  });
}

/**
 * All bindings owned by this function as Map<name, owning scope>, for the
 * prior-version transfer path. First name wins on sibling-block duplicates —
 * the shadowed-bindings second pass handles the rest.
 */
export function buildOwnedBindingMap(
  fnPath: NodePath<t.Function>
): Map<string, Scope> {
  const map = new Map<string, Scope>();
  for (const info of collectOwnedBindings(fnPath, {
    includeNestedFunctionDeclNames: true
  })) {
    if (!map.has(info.name)) map.set(info.name, info.scope);
  }
  return map;
}

function collectOwnedBindings(
  fnPath: NodePath<t.Function>,
  opts: CollectorOptions
): BindingInfo[] {
  const bindings: BindingInfo[] = [];
  collectScopeOwnBindings(fnPath.scope, opts, bindings);
  collectBodyScopeBindings(fnPath, opts, bindings);
  collectNestedBlockBindings(fnPath, opts, bindings);
  collectFunctionNameBinding(fnPath, bindings);
  return bindings;
}

function shouldInclude(binding: Binding, opts: CollectorOptions): boolean {
  return (
    opts.includeNestedFunctionDeclNames || !binding.path.isFunctionDeclaration()
  );
}

/** Collect bindings declared directly in the given scope. */
function collectScopeOwnBindings(
  scope: Scope,
  opts: CollectorOptions,
  bindings: BindingInfo[]
): void {
  for (const [name, binding] of Object.entries(scope.bindings)) {
    if (binding.scope !== scope) continue;
    if (!shouldInclude(binding, opts)) continue;
    bindings.push({
      name,
      identifier: binding.identifier,
      scope: binding.scope
    });
  }
}

/**
 * When parameters have defaults/destructuring/rest, Babel creates a separate
 * scope for the function body. Collect any bindings from that body scope.
 */
function collectBodyScopeBindings(
  fnPath: NodePath<t.Function>,
  opts: CollectorOptions,
  bindings: BindingInfo[]
): void {
  const bodyPath = fnPath.get("body");
  if (Array.isArray(bodyPath) || !bodyPath.isBlockStatement()) return;
  const bodyScope = bodyPath.scope;
  if (bodyScope === fnPath.scope) return;
  for (const [name, binding] of Object.entries(bodyScope.bindings)) {
    if (binding.scope !== bodyScope) continue;
    if (!shouldInclude(binding, opts)) continue;
    if (bindings.some((b) => b.name === name)) continue;
    bindings.push({
      name,
      identifier: binding.identifier,
      scope: binding.scope
    });
  }
}

/**
 * Traverse nested block scopes to collect let/const bindings inside
 * for/while/if/try blocks owned by this function but in child block scopes.
 */
function collectNestedBlockBindings(
  fnPath: NodePath<t.Function>,
  opts: CollectorOptions,
  bindings: BindingInfo[]
): void {
  const seen = new Set(bindings.map((b) => b.name));
  const collect = (path: NodePath) =>
    collectBlockBindings(path, opts, seen, bindings);
  fnPath.traverse({
    Function(path: NodePath<t.Function>) {
      if (path !== fnPath) path.skip();
    },
    BlockStatement(path: NodePath<t.BlockStatement>) {
      if (path.parentPath === fnPath) return;
      collect(path);
    },
    ForStatement: collect,
    ForInStatement: collect,
    ForOfStatement: collect,
    SwitchStatement: collect,
    CatchClause: collect
  });
}

/**
 * Collects bindings from a block scope that are declared directly in that scope.
 */
function collectBlockBindings(
  path: NodePath,
  opts: CollectorOptions,
  seen: Set<string>,
  bindings: BindingInfo[]
): void {
  const blockScope = path.scope;
  for (const [name, binding] of Object.entries(blockScope.bindings)) {
    if (binding.scope !== blockScope) continue;
    if (!shouldInclude(binding, opts)) continue;
    if (seen.has(name)) continue;
    seen.add(name);
    bindings.push({
      name,
      identifier: binding.identifier,
      scope: binding.scope
    });
  }
}

/** Include the function's own name binding for named function expressions/declarations. */
function collectFunctionNameBinding(
  fnPath: NodePath<t.Function>,
  bindings: BindingInfo[]
): void {
  if (!fnPath.isFunctionExpression() && !fnPath.isFunctionDeclaration()) return;
  const id = fnPath.node.id;
  if (!id) return;
  const nameBinding = fnPath.isFunctionDeclaration()
    ? fnPath.parentPath.scope.getBinding(id.name)
    : fnPath.scope.getBinding(id.name);
  if (nameBinding && !bindings.some((b) => b.name === id.name)) {
    bindings.push({
      name: id.name,
      identifier: nameBinding.identifier,
      scope: nameBinding.scope
    });
  }
}

/**
 * After the main rename pass, find block-scoped bindings that were skipped
 * because they shared a name with a function-scope binding at collection time.
 * Now that the function-scope binding has been renamed, these block-scoped
 * bindings are safe to collect. This handles catch clauses, for-loops,
 * if-blocks, switch cases, and any other block-creating statement.
 */
export function collectShadowedBlockBindings(
  fnPath: NodePath<t.Function>,
  isEligible: IsEligibleFn
): BindingInfo[] {
  const bindings: BindingInfo[] = [];
  const visitedScopes = new WeakSet();

  fnPath.traverse({
    Function(path: NodePath<t.Function>) {
      if (path !== fnPath) path.skip();
    },
    Scope(path) {
      const scope = path.scope;
      if (scope === fnPath.scope || visitedScopes.has(scope)) return;
      // Skip scopes belonging to nested functions
      if (scope.path.isFunction() && scope.path !== fnPath) return;
      visitedScopes.add(scope);

      for (const [name, binding] of Object.entries(scope.bindings)) {
        if (binding.scope !== scope) continue;
        if (!isEligible(name)) continue;
        bindings.push({
          name,
          identifier: binding.identifier,
          scope: binding.scope
        });
      }
    }
  });
  return bindings;
}
