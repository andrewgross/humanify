import type { NodePath } from "@babel/core";
import generate from "@babel/generator";
import * as t from "@babel/types";
import * as babelTraverse from "@babel/traverse";
import type { FunctionNode, LLMContext, CalleeSignature } from "../analysis/types.js";

const traverse: typeof babelTraverse.default =
  typeof babelTraverse.default === "function"
    ? babelTraverse.default
    : (babelTraverse.default as any).default;

/**
 * Builds context for the LLM to make informed renaming decisions.
 *
 * The context includes:
 * - The function's current code (with any renames already applied)
 * - Signatures of functions it calls (already humanified)
 * - Call sites where this function is used
 * - Set of identifiers already in use (to avoid conflicts)
 */
export function buildContext(fn: FunctionNode, ast: t.File): LLMContext {
  return {
    functionCode: generateCode(fn.path.node),
    calleeSignatures: getCalleeSignatures(fn),
    callsites: findCallsites(fn, ast),
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
 * Finds all call sites where this function is called.
 */
function findCallsites(fn: FunctionNode, ast: t.File): string[] {
  const callsites: string[] = [];
  const fnNode = fn.path.node;

  // Get the function's name(s) that might be used to call it
  const functionNames = getFunctionNames(fn);

  if (functionNames.size === 0) {
    return callsites;
  }

  traverse(ast, {
    CallExpression(path: NodePath<t.CallExpression>) {
      const callee = path.node.callee;

      // Check if this call is to our function
      if (t.isIdentifier(callee) && functionNames.has(callee.name)) {
        // Verify the binding resolves to our function
        const binding = path.scope.getBinding(callee.name);
        if (binding) {
          const bindingNode = getBindingFunctionNode(binding);
          if (bindingNode === fnNode) {
            // Get the call expression code as context
            const callCode = generateCode(path.node);
            if (callCode.length < 200) {
              // Limit size
              callsites.push(callCode);
            }
          }
        }
      }
    }
  });

  // Limit to first 5 call sites
  return callsites.slice(0, 5);
}

/**
 * Gets all names that might refer to this function.
 */
function getFunctionNames(fn: FunctionNode): Set<string> {
  const names = new Set<string>();
  const fnNode = fn.path.node;

  // Named function declaration/expression
  if (
    (t.isFunctionDeclaration(fnNode) || t.isFunctionExpression(fnNode)) &&
    fnNode.id
  ) {
    names.add(fnNode.id.name);
  }

  // Variable assignment
  const parent = fn.path.parent;
  if (t.isVariableDeclarator(parent) && t.isIdentifier(parent.id)) {
    names.add(parent.id.name);
  }

  // Object property
  if (t.isObjectProperty(parent) && t.isIdentifier(parent.key)) {
    names.add(parent.key.name);
  }

  return names;
}

/**
 * Gets the function node that a binding refers to.
 */
function getBindingFunctionNode(
  binding: babelTraverse.Binding
): t.Function | null {
  const path = binding.path;

  if (path.isFunctionDeclaration() || path.isFunctionExpression()) {
    return path.node;
  }

  if (path.isVariableDeclarator()) {
    const init = path.node.init;
    if (t.isFunction(init)) {
      return init;
    }
  }

  return null;
}

/**
 * Gets all identifiers currently in use in the function's scope.
 */
function getUsedIdentifiers(fnPath: NodePath<t.Function>): Set<string> {
  const used = new Set<string>();

  // Add all bindings in the current and parent scopes
  let scope: babelTraverse.Scope | null = fnPath.scope;
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
 * Builds a user prompt for the LLM based on context.
 */
export function buildPrompt(
  currentName: string,
  context: LLMContext
): string {
  let prompt = `Suggest a better name for the identifier "${currentName}" in this code:\n\n`;

  prompt += "```javascript\n" + context.functionCode + "\n```\n\n";

  if (context.calleeSignatures.length > 0) {
    prompt += "This function calls these (already named) functions:\n";
    for (const callee of context.calleeSignatures) {
      prompt += `- ${callee.name}(${callee.params.join(", ")})\n`;
    }
    prompt += "\n";
  }

  if (context.callsites.length > 0) {
    prompt += "This function is called like:\n";
    for (const site of context.callsites.slice(0, 3)) {
      prompt += `- ${site}\n`;
    }
    prompt += "\n";
  }

  const usedList = [...context.usedIdentifiers].slice(0, 50).join(", ");
  if (usedList) {
    prompt += `Names already in use (avoid these): ${usedList}\n`;
  }

  return prompt;
}
