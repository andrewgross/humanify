import type { NodePath } from "@babel/core";
import * as t from "@babel/types";
import type { Scope } from "@babel/traverse";
import type { FunctionNode, LLMContext, CalleeSignature } from "../analysis/types.js";
import { generate } from "../babel-utils.js";
import { looksMinified as defaultLooksMinified } from "./minified-heuristic.js";
import type { LooksMinifiedFn } from "./minified-heuristic.js";

/**
 * Builds context for the LLM to make informed renaming decisions.
 *
 * The context includes:
 * - The function's current code (with any renames already applied)
 * - Signatures of functions it calls (already humanified)
 * - Call sites where this function is used (pre-computed during graph building)
 * - Set of identifiers already in use (to avoid conflicts)
 * - Parent-scope variable declarations (when scopeParent hasn't been processed yet)
 */
export function buildContext(fn: FunctionNode, _ast: t.File, looksMinified?: LooksMinifiedFn): LLMContext {
  const context: LLMContext = {
    functionCode: generateCode(fn.path.node),
    calleeSignatures: getCalleeSignatures(fn),
    callsites: fn.callSites.map((cs) => cs.code),
    usedIdentifiers: getUsedIdentifiers(fn.path)
  };

  // When scopeParent exists but isn't done yet (deadlock-broken processing),
  // include parent-scope variable declarations as read-only context
  if (fn.scopeParent && fn.scopeParent.status !== "done") {
    const parentVars = getParentScopeContextVars(fn.scopeParent, looksMinified);
    if (parentVars.length > 0) {
      context.contextVars = parentVars;
    }
  }

  return context;
}

/**
 * Generates code from an AST node.
 */
function generateCode(node: t.Node): string {
  try {
    const result = generate(node, {
      compact: false,
      comments: false
    });
    return result.code;
  } catch {
    return "[code generation failed]";
  }
}

/**
 * Gets signatures of all internal callees (already humanified functions).
 */
function getCalleeSignatures(fn: FunctionNode): CalleeSignature[] {
  const signatures: CalleeSignature[] = [];

  for (const callee of fn.internalCallees) {
    const calleeNode = callee.path.node;

    // Get function name
    let name = "anonymous";
    if (t.isFunctionDeclaration(calleeNode) && calleeNode.id) {
      name = calleeNode.id.name;
    } else if (t.isFunctionExpression(calleeNode) && calleeNode.id) {
      name = calleeNode.id.name;
    } else {
      // Try to get name from variable declarator parent
      const parent = callee.path.parent;
      if (t.isVariableDeclarator(parent) && t.isIdentifier(parent.id)) {
        name = parent.id.name;
      }
    }

    // Get parameter names
    const params = calleeNode.params.map((param) => {
      if (t.isIdentifier(param)) {
        return param.name;
      } else if (t.isRestElement(param) && t.isIdentifier(param.argument)) {
        return "..." + param.argument.name;
      } else if (t.isAssignmentPattern(param) && t.isIdentifier(param.left)) {
        return param.left.name;
      }
      return generateCode(param);
    });

    // Get first few lines of body
    const snippet = getBodySnippet(calleeNode.body, 3);

    signatures.push({ name, params, snippet });
  }

  return signatures;
}

/**
 * Gets the first N lines of a function body.
 */
function getBodySnippet(body: t.BlockStatement | t.Expression, lines: number): string {
  const code = generateCode(body);
  const codeLines = code.split("\n");
  return codeLines.slice(0, lines).join("\n");
}

/**
 * Gets all identifiers currently in use in the function's scope.
 */
function getUsedIdentifiers(fnPath: NodePath<t.Function>): Set<string> {
  const used = new Set<string>();

  // Add all bindings in the current and parent scopes
  let scope: Scope | null = fnPath.scope;
  while (scope) {
    for (const name of Object.keys(scope.bindings)) {
      used.add(name);
    }
    scope = scope.parent;
  }

  // Add globals
  for (const name of Object.keys(fnPath.scope.globals || {})) {
    used.add(name);
  }

  return used;
}

/**
 * Collects parent-scope variable declarations for read-only context.
 * Returns up to 30 declaration snippets for minified-looking bindings
 * in the parent function's scope. These help the LLM understand the
 * surrounding scope without being asked to rename them.
 */
function getParentScopeContextVars(parent: FunctionNode, looksMinified?: LooksMinifiedFn): string[] {
  const contextVars: string[] = [];
  const MAX_CONTEXT_VARS = 30;
  const isMinified = looksMinified ?? defaultLooksMinified;

  try {
    const scope = parent.path.scope;
    for (const [name, binding] of Object.entries(scope.bindings) as [string, any][]) {
      if (contextVars.length >= MAX_CONTEXT_VARS) break;
      if (!isMinified(name)) continue;

      // Skip function/class declarations — not useful as variable context
      const bindingPath = binding.path;
      if (bindingPath.isFunctionDeclaration?.() || bindingPath.isClassDeclaration?.()) {
        continue;
      }

      // Get a short declaration snippet
      try {
        let declCode = "";
        if (bindingPath.isVariableDeclarator?.()) {
          const declPath = bindingPath.parentPath;
          if (declPath) {
            declCode = generate(declPath.node).code;
          }
        } else {
          declCode = generate(bindingPath.node).code;
        }

        if (declCode) {
          // Take only the first line and truncate
          const line = declCode.split("\n")[0].trim();
          if (line.length <= 120) {
            contextVars.push(line);
          }
        }
      } catch {
        // Skip if generation fails
      }
    }
  } catch {
    // Skip if scope traversal fails
  }

  return contextVars;
}

