import { parseSync } from "@babel/core";
import { readFileSync } from "fs";
import * as t from "@babel/types";
import { computeExactHash } from "../../../src/analysis/structural-hash.js";

export interface SourceFunction {
  id: string; // "file.ts::functionName"
  name: string;
  file: string;
  location: { startLine: number; endLine: number };
  bodyHash: string;
  arity: number;
}

export interface FunctionCorrespondence {
  sourceName: string;
  sourceFile: string;
  inV1: boolean;
  inV2: boolean;
  changeType: "unchanged" | "modified" | "added" | "removed";
  changeDetails?: {
    signatureChanged: boolean;
    bodyChanged: boolean;
  };
}

export interface GroundTruth {
  v1Functions: SourceFunction[];
  v2Functions: SourceFunction[];
  correspondence: FunctionCorrespondence[];
}

/**
 * Extract all functions from a TypeScript/JavaScript source file.
 * Uses babel with TS plugin to parse TypeScript directly.
 */
export function extractFunctions(filePath: string, relativeFile: string): SourceFunction[] {
  const code = readFileSync(filePath, "utf-8");
  const ast = parseSync(code, {
    filename: filePath,
    presets: [],
    plugins: filePath.endsWith(".ts") || filePath.endsWith(".tsx")
      ? [["@babel/plugin-transform-typescript", { isTSX: filePath.endsWith(".tsx") }]]
      : [],
    sourceType: "module",
  });

  if (!ast) {
    throw new Error(`Failed to parse ${filePath}`);
  }

  const functions: SourceFunction[] = [];

  visitNode(ast, (node: t.Node) => {
    if (!t.isFunction(node)) return;

    const name = inferFunctionName(node);
    if (!name) return; // Skip anonymous functions we can't identify

    const loc = node.loc;
    if (!loc) return;

    const bodyHash = computeExactHash(node);
    const arity = node.params.length;

    functions.push({
      id: `${relativeFile}::${name}`,
      name,
      file: relativeFile,
      location: { startLine: loc.start.line, endLine: loc.end.line },
      bodyHash,
      arity,
    });
  });

  return functions;
}

/**
 * Infer a function's name from its AST node and surrounding context.
 */
function inferFunctionName(node: t.Node): string | null {
  // Function declaration: function foo() {}
  if (t.isFunctionDeclaration(node) && node.id) {
    return node.id.name;
  }

  // Named function expression: const foo = function bar() {}
  if (t.isFunctionExpression(node) && node.id) {
    return node.id.name;
  }

  // Arrow or anonymous assigned to variable: const foo = () => {}
  // We need to check the parent - but since we're doing manual traversal,
  // we handle this by checking if the function is the init of a VariableDeclarator
  // This is handled in the visitor below.

  return null;
}

/**
 * Walk AST nodes, collecting function names including those assigned to variables.
 */
function visitNode(node: t.Node, callback: (node: t.Node, parentContext?: string) => void): void {
  if (!node || typeof node !== "object") return;

  // Handle variable declarations: const foo = () => {} or const foo = function() {}
  if (t.isVariableDeclaration(node)) {
    for (const decl of node.declarations) {
      if (t.isVariableDeclarator(decl) && t.isIdentifier(decl.id) && decl.init && t.isFunction(decl.init)) {
        // Temporarily set a name on the function node for extraction
        const fnNode = decl.init;
        const name = decl.id.name;

        // For arrow functions and anonymous function expressions, synthesize a name
        if (t.isArrowFunctionExpression(fnNode) || (t.isFunctionExpression(fnNode) && !fnNode.id)) {
          // Use a special property to carry the inferred name
          (fnNode as any)._inferredName = name;
        }
      }
    }
  }

  // Handle export default function: export default function mitt() {}
  if (t.isExportDefaultDeclaration(node) && t.isFunction(node.declaration)) {
    callback(node.declaration);
  }

  // Handle named exports: export function foo() {}
  if (t.isExportNamedDeclaration(node) && node.declaration && t.isFunction(node.declaration)) {
    callback(node.declaration);
  }

  callback(node);

  // Recurse into children
  for (const key of Object.keys(node)) {
    if (key === "type" || key === "loc" || key === "start" || key === "end") continue;

    const value = (node as unknown as Record<string, unknown>)[key];
    if (Array.isArray(value)) {
      for (const item of value) {
        if (item && typeof item === "object" && "type" in item) {
          visitNode(item as t.Node, callback);
        }
      }
    } else if (value && typeof value === "object" && "type" in value) {
      visitNode(value as t.Node, callback);
    }
  }
}

/**
 * Override inferFunctionName to also check the _inferredName property
 * set during variable declaration processing.
 */
const originalInferFunctionName = inferFunctionName;
function inferFunctionNameWithContext(node: t.Node): string | null {
  const direct = originalInferFunctionName(node);
  if (direct) return direct;

  // Check for inferred name from variable assignment
  if (t.isFunction(node) && (node as any)._inferredName) {
    return (node as any)._inferredName;
  }

  return null;
}

/**
 * Build ground truth by comparing source functions from two versions.
 */
export function buildGroundTruth(
  v1Files: Array<{ path: string; relative: string }>,
  v2Files: Array<{ path: string; relative: string }>
): GroundTruth {
  const v1Functions = v1Files.flatMap((f) => extractFunctionsWithInferredNames(f.path, f.relative));
  const v2Functions = v2Files.flatMap((f) => extractFunctionsWithInferredNames(f.path, f.relative));

  const correspondence: FunctionCorrespondence[] = [];
  const matchedV2Ids = new Set<string>();

  for (const v1Fn of v1Functions) {
    // Step 1: Try exact match by name + file
    let v2Match = v2Functions.find(
      (f) => f.name === v1Fn.name && f.file === v1Fn.file && !matchedV2Ids.has(f.id)
    );

    // Step 2: Fallback — match by bodyHash (function renamed in source)
    if (!v2Match) {
      v2Match = v2Functions.find(
        (f) => f.bodyHash === v1Fn.bodyHash && !matchedV2Ids.has(f.id)
      );
    }

    if (v2Match) {
      matchedV2Ids.add(v2Match.id);

      const bodyChanged = v1Fn.bodyHash !== v2Match.bodyHash;
      const signatureChanged = v1Fn.arity !== v2Match.arity;

      correspondence.push({
        sourceName: v1Fn.name,
        sourceFile: v1Fn.file,
        inV1: true,
        inV2: true,
        changeType: bodyChanged || signatureChanged ? "modified" : "unchanged",
        changeDetails:
          bodyChanged || signatureChanged
            ? { signatureChanged, bodyChanged }
            : undefined,
      });
    } else {
      correspondence.push({
        sourceName: v1Fn.name,
        sourceFile: v1Fn.file,
        inV1: true,
        inV2: false,
        changeType: "removed",
      });
    }
  }

  // Find added functions (in v2 but unmatched)
  for (const v2Fn of v2Functions) {
    if (!matchedV2Ids.has(v2Fn.id)) {
      correspondence.push({
        sourceName: v2Fn.name,
        sourceFile: v2Fn.file,
        inV1: false,
        inV2: true,
        changeType: "added",
      });
    }
  }

  return { v1Functions, v2Functions, correspondence };
}

/**
 * Extract functions using the enhanced name inference.
 * This includes:
 * - Function declarations: function foo() {}
 * - Named function expressions: const foo = function bar() {}
 * - Arrow functions assigned to variables: const foo = () => {}
 * - Object methods: { on() {}, off: function() {} }
 * - Arrow functions in object properties: { emit: () => {} }
 */
function extractFunctionsWithInferredNames(filePath: string, relativeFile: string): SourceFunction[] {
  const code = readFileSync(filePath, "utf-8");
  const ast = parseSync(code, {
    filename: filePath,
    presets: [],
    plugins: filePath.endsWith(".ts") || filePath.endsWith(".tsx")
      ? [["@babel/plugin-transform-typescript", { isTSX: filePath.endsWith(".tsx") }]]
      : [],
    sourceType: "module",
  });

  if (!ast) {
    throw new Error(`Failed to parse ${filePath}`);
  }

  const functions: SourceFunction[] = [];
  const seen = new Set<t.Node>();

  // First pass: identify and tag functions with inferred names
  tagFunctionNames(ast);

  // Second pass: extract functions
  visitNodeForExtraction(ast, (node: t.Node, inferredName?: string) => {
    if (!t.isFunction(node)) return;
    if (seen.has(node)) return;
    seen.add(node);

    const name = inferredName || inferFunctionNameWithContext(node);
    if (!name) return;

    const loc = node.loc;
    if (!loc) return;

    const bodyHash = computeExactHash(node);
    const arity = node.params.length;

    functions.push({
      id: `${relativeFile}::${name}`,
      name,
      file: relativeFile,
      location: { startLine: loc.start.line, endLine: loc.end.line },
      bodyHash,
      arity,
    });
  });

  return functions;
}

/**
 * Tag functions in the AST with their inferred names from context.
 */
function tagFunctionNames(node: t.Node): void {
  if (!node || typeof node !== "object") return;

  // Variable declarations: const foo = () => {}
  if (t.isVariableDeclaration(node)) {
    for (const decl of node.declarations) {
      if (t.isVariableDeclarator(decl) && t.isIdentifier(decl.id) && decl.init && t.isFunction(decl.init)) {
        (decl.init as any)._inferredName = decl.id.name;
      }
    }
  }

  // Object methods and properties
  if (t.isObjectExpression(node)) {
    for (const prop of node.properties) {
      if (t.isObjectMethod(prop)) {
        // Method shorthand: { on() {} }
        if (t.isIdentifier(prop.key)) {
          (prop as any)._inferredName = prop.key.name;
        }
      } else if (t.isObjectProperty(prop) && t.isFunction(prop.value)) {
        // Property with function value: { on: function() {} } or { on: () => {} }
        if (t.isIdentifier(prop.key)) {
          (prop.value as any)._inferredName = prop.key.name;
        }
      }
    }
  }

  // Recurse into children
  for (const key of Object.keys(node)) {
    if (key === "type" || key === "loc" || key === "start" || key === "end") continue;
    const value = (node as unknown as Record<string, unknown>)[key];
    if (Array.isArray(value)) {
      for (const item of value) {
        if (item && typeof item === "object" && "type" in item) {
          tagFunctionNames(item as t.Node);
        }
      }
    } else if (value && typeof value === "object" && "type" in value) {
      tagFunctionNames(value as t.Node);
    }
  }
}

/**
 * Visit nodes for extraction, passing inferred names along.
 */
function visitNodeForExtraction(
  node: t.Node,
  callback: (node: t.Node, inferredName?: string) => void
): void {
  if (!node || typeof node !== "object") return;

  // ObjectMethod (method shorthand) is a function itself
  if (t.isObjectMethod(node)) {
    const inferredName = (node as any)._inferredName;
    callback(node, inferredName);
  }

  // Regular function nodes (declarations, expressions, arrows)
  if (t.isFunction(node) && !t.isObjectMethod(node)) {
    const inferredName = (node as any)._inferredName;
    callback(node, inferredName);
  }

  // Recurse into children
  for (const key of Object.keys(node)) {
    if (key === "type" || key === "loc" || key === "start" || key === "end") continue;
    const value = (node as unknown as Record<string, unknown>)[key];
    if (Array.isArray(value)) {
      for (const item of value) {
        if (item && typeof item === "object" && "type" in item) {
          visitNodeForExtraction(item as t.Node, callback);
        }
      }
    } else if (value && typeof value === "object" && "type" in value) {
      visitNodeForExtraction(value as t.Node, callback);
    }
  }
}
