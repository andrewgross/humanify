/**
 * Detects wrapper IIFE patterns in bundled code.
 *
 * Extracted from plugin.ts to break circular dependency between
 * plugin.ts and prior-version.ts.
 */

import type * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import { debug } from "../debug.js";

/** Minimum number of bindings for an IIFE to be considered a wrapper */
const WRAPPER_IIFE_BINDING_THRESHOLD = 50;

/**
 * Result of wrapper function detection.
 */
export interface WrapperFunctionResult {
  /** The scope of the wrapper function (replaces programScope for bindings) */
  scope: babelTraverse.Scope;
  /** The path to the wrapper function (for marking as pre-done) */
  functionPath: babelTraverse.NodePath<t.Function>;
}

/**
 * Extract the callee function from a CallExpression node, or return null.
 * Handles: direct IIFE, .call/.apply IIFE.
 */
function extractCalleeFromCall(expr: t.CallExpression): t.Expression | null {
  const fn = expr.callee;

  // (function(){...})() or (() => {...})()
  if (t.isFunctionExpression(fn) || t.isArrowFunctionExpression(fn)) {
    return fn;
  }

  // (function(){}).call(this, ...) or .apply(...)
  if (
    t.isMemberExpression(fn) &&
    t.isIdentifier(fn.property) &&
    (fn.property.name === "call" || fn.property.name === "apply") &&
    (t.isFunctionExpression(fn.object) ||
      t.isArrowFunctionExpression(fn.object))
  ) {
    return fn.object;
  }

  return null;
}

/**
 * Detects a giant wrapper function pattern where the entire program body
 * is a single expression statement containing a function.
 *
 * Handles:
 * - (function(exports, require, module) { ... })()           — IIFE
 * - !function() { ... }()                                     — negated IIFE
 * - (function(){}).call(this, ...)                             — .call/.apply
 * - (() => { ... })()                                         — arrow IIFE
 * - (function(exports, require, module) { ... });             — Bun CJS bytecode (bare, not called)
 *
 * Only triggers if the wrapper has more bindings than WRAPPER_IIFE_BINDING_THRESHOLD,
 * to avoid interfering with small per-module IIFEs (Webpack style).
 */
export function findWrapperFunction(ast: t.File): WrapperFunctionResult | null {
  const body = ast.program.body;

  // Must be a single expression statement
  if (body.length !== 1 || !t.isExpressionStatement(body[0])) return null;

  const expr = body[0].expression;
  let callee: t.Expression | null = null;

  if (t.isCallExpression(expr)) {
    callee = extractCalleeFromCall(expr);
  }

  // !function(){...}()
  if (
    !callee &&
    t.isUnaryExpression(expr) &&
    t.isCallExpression(expr.argument)
  ) {
    const fn = expr.argument.callee;
    if (t.isFunctionExpression(fn) || t.isArrowFunctionExpression(fn)) {
      callee = fn;
    }
  }

  // Bun CJS bytecode: (function(exports, require, module) { ... });
  // A bare function expression (not called) wrapping the entire bundle
  if (!callee && t.isFunctionExpression(expr)) {
    callee = expr;
  }

  if (!callee) return null;

  // Now traverse to find the actual path and check binding count
  let result: WrapperFunctionResult | null = null;

  traverse(ast, {
    Function(path: babelTraverse.NodePath<t.Function>) {
      if (path.node === callee) {
        const bindingCount = Object.keys(path.scope.bindings).length;
        if (bindingCount >= WRAPPER_IIFE_BINDING_THRESHOLD) {
          result = { scope: path.scope, functionPath: path };
          debug.log(
            "wrapper",
            `Detected wrapper function with ${bindingCount} bindings`
          );
        }
        path.stop();
      }
    }
  });

  return result;
}
