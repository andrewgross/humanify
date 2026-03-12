import { parseSync } from "@babel/core";
import { readFileSync } from "node:fs";
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
export function extractFunctions(
  filePath: string,
  relativeFile: string
): SourceFunction[] {
  const code = readFileSync(filePath, "utf-8");
  const ast = parseSync(code, {
    filename: filePath,
    presets: [],
    plugins:
      filePath.endsWith(".ts") || filePath.endsWith(".tsx")
        ? [
            [
              "@babel/plugin-transform-typescript",
              { isTSX: filePath.endsWith(".tsx") }
            ]
          ]
        : [],
    sourceType: "module"
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
      arity
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
 * Tag variable-assigned functions with inferred names during traversal.
 */
function tagVariableDeclarationNames(node: t.Node): void {
  if (!t.isVariableDeclaration(node)) return;
  for (const decl of node.declarations) {
    if (
      t.isVariableDeclarator(decl) &&
      t.isIdentifier(decl.id) &&
      decl.init &&
      t.isFunction(decl.init)
    ) {
      const fnNode = decl.init;
      const name = decl.id.name;
      if (
        t.isArrowFunctionExpression(fnNode) ||
        (t.isFunctionExpression(fnNode) && !fnNode.id)
      ) {
        (fnNode as any)._inferredName = name;
      }
    }
  }
}

/**
 * Invoke callback for exported function declarations.
 */
function visitExportedFunctions(
  node: t.Node,
  callback: (node: t.Node) => void
): void {
  if (t.isExportDefaultDeclaration(node) && t.isFunction(node.declaration)) {
    callback(node.declaration);
  }
  if (
    t.isExportNamedDeclaration(node) &&
    node.declaration &&
    t.isFunction(node.declaration)
  ) {
    callback(node.declaration);
  }
}

const SKIP_KEYS = new Set(["type", "loc", "start", "end"]);

function isAstNode(value: unknown): value is t.Node {
  return (
    value !== null &&
    typeof value === "object" &&
    "type" in (value as Record<string, unknown>)
  );
}

/**
 * Recurse into child AST nodes.
 */
function recurseIntoChildren(
  node: t.Node,
  visitor: (child: t.Node) => void
): void {
  for (const key of Object.keys(node)) {
    if (SKIP_KEYS.has(key)) continue;
    const value = (node as unknown as Record<string, unknown>)[key];
    if (Array.isArray(value)) {
      for (const item of value) {
        if (isAstNode(item)) visitor(item);
      }
    } else if (isAstNode(value)) {
      visitor(value);
    }
  }
}

/**
 * Walk AST nodes, collecting function names including those assigned to variables.
 */
function visitNode(
  node: t.Node,
  callback: (node: t.Node, parentContext?: string) => void
): void {
  if (!node || typeof node !== "object") return;

  tagVariableDeclarationNames(node);
  visitExportedFunctions(node, callback);
  callback(node);
  recurseIntoChildren(node, (child) => visitNode(child, callback));
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
 * Find a v2 match for a v1 function by name+file or bodyHash fallback.
 */
function findV2Match(
  v1Fn: SourceFunction,
  v2Functions: SourceFunction[],
  matchedV2Ids: Set<string>
): SourceFunction | undefined {
  return (
    v2Functions.find(
      (f) =>
        f.name === v1Fn.name && f.file === v1Fn.file && !matchedV2Ids.has(f.id)
    ) ??
    v2Functions.find(
      (f) => f.bodyHash === v1Fn.bodyHash && !matchedV2Ids.has(f.id)
    )
  );
}

/**
 * Build a correspondence entry for a matched v1→v2 function pair.
 */
function buildMatchedCorrespondence(
  v1Fn: SourceFunction,
  v2Match: SourceFunction
): FunctionCorrespondence {
  const bodyChanged = v1Fn.bodyHash !== v2Match.bodyHash;
  const signatureChanged = v1Fn.arity !== v2Match.arity;
  return {
    sourceName: v1Fn.name,
    sourceFile: v1Fn.file,
    inV1: true,
    inV2: true,
    changeType: bodyChanged || signatureChanged ? "modified" : "unchanged",
    changeDetails:
      bodyChanged || signatureChanged
        ? { signatureChanged, bodyChanged }
        : undefined
  };
}

/**
 * Build ground truth by comparing source functions from two versions.
 */
export function buildGroundTruth(
  v1Files: Array<{ path: string; relative: string }>,
  v2Files: Array<{ path: string; relative: string }>
): GroundTruth {
  const v1Functions = v1Files.flatMap((f) =>
    extractFunctionsWithInferredNames(f.path, f.relative)
  );
  const v2Functions = v2Files.flatMap((f) =>
    extractFunctionsWithInferredNames(f.path, f.relative)
  );

  const correspondence: FunctionCorrespondence[] = [];
  const matchedV2Ids = new Set<string>();

  for (const v1Fn of v1Functions) {
    const v2Match = findV2Match(v1Fn, v2Functions, matchedV2Ids);

    if (v2Match) {
      matchedV2Ids.add(v2Match.id);
      correspondence.push(buildMatchedCorrespondence(v1Fn, v2Match));
    } else {
      correspondence.push({
        sourceName: v1Fn.name,
        sourceFile: v1Fn.file,
        inV1: true,
        inV2: false,
        changeType: "removed"
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
        changeType: "added"
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
function extractFunctionsWithInferredNames(
  filePath: string,
  relativeFile: string
): SourceFunction[] {
  const code = readFileSync(filePath, "utf-8");
  const ast = parseSync(code, {
    filename: filePath,
    presets: [],
    plugins:
      filePath.endsWith(".ts") || filePath.endsWith(".tsx")
        ? [
            [
              "@babel/plugin-transform-typescript",
              { isTSX: filePath.endsWith(".tsx") }
            ]
          ]
        : [],
    sourceType: "module"
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
      arity
    });
  });

  return functions;
}

/**
 * Tag variable-declared functions with their variable name.
 */
function tagVariableFunctions(node: t.Node): void {
  if (!t.isVariableDeclaration(node)) return;
  for (const decl of node.declarations) {
    if (
      t.isVariableDeclarator(decl) &&
      t.isIdentifier(decl.id) &&
      decl.init &&
      t.isFunction(decl.init)
    ) {
      (decl.init as any)._inferredName = decl.id.name;
    }
  }
}

/**
 * Tag object methods and function-valued properties with their key name.
 */
function tagObjectFunctions(node: t.Node): void {
  if (!t.isObjectExpression(node)) return;
  for (const prop of node.properties) {
    if (t.isObjectMethod(prop) && t.isIdentifier(prop.key)) {
      (prop as any)._inferredName = prop.key.name;
    } else if (
      t.isObjectProperty(prop) &&
      t.isFunction(prop.value) &&
      t.isIdentifier(prop.key)
    ) {
      (prop.value as any)._inferredName = prop.key.name;
    }
  }
}

/**
 * Tag functions in the AST with their inferred names from context.
 */
function tagFunctionNames(node: t.Node): void {
  if (!node || typeof node !== "object") return;

  tagVariableFunctions(node);
  tagObjectFunctions(node);
  recurseIntoChildren(node, tagFunctionNames);
}

/**
 * Visit nodes for extraction, passing inferred names along.
 */
function visitNodeForExtraction(
  node: t.Node,
  callback: (node: t.Node, inferredName?: string) => void
): void {
  if (!node || typeof node !== "object") return;

  // ObjectMethod (method shorthand) or regular function nodes
  if (
    t.isObjectMethod(node) ||
    (t.isFunction(node) && !t.isObjectMethod(node))
  ) {
    callback(node, (node as any)._inferredName);
  }

  recurseIntoChildren(node, (child) => visitNodeForExtraction(child, callback));
}
