import type { NodePath } from "@babel/core";
import * as t from "@babel/types";
import type { Scope } from "@babel/traverse";
import type { FunctionNode, LLMContext, CalleeSignature } from "../analysis/types.js";
import { generate } from "../babel-utils.js";

/**
 * Builds context for the LLM to make informed renaming decisions.
 *
 * The context includes:
 * - The function's current code (with any renames already applied)
 * - Signatures of functions it calls (already humanified)
 * - Call sites where this function is used (pre-computed during graph building)
 * - Set of identifiers already in use (to avoid conflicts)
 */
export function buildContext(fn: FunctionNode, _ast: t.File): LLMContext {
  return {
    functionCode: generateCode(fn.path.node),
    calleeSignatures: getCalleeSignatures(fn),
    callsites: fn.callSites.map((cs) => cs.code),
    usedIdentifiers: getUsedIdentifiers(fn.path)
  };
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

