import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { generate, traverse } from "../../../src/babel-utils.js";
import type { Perturbation, PerturbationResult } from "../types.js";

/**
 * Reverse the order of properties in ObjectExpressions.
 * No structural change to any function body, no key changes.
 * Expected: 100% match on every minifier — tests that we ignore sibling order.
 */
export const swapPropertyOrder: Perturbation = {
  id: "swapPropertyOrder",
  description: "reverse ObjectExpression property order",
  apply(source) {
    return applySwap(source);
  }
};

function applySwap(source: string): PerturbationResult {
  const ast = parseSync(source, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse source");

  traverse(ast, {
    ObjectExpression(path: NodePath<t.ObjectExpression>) {
      path.node.properties = [...path.node.properties].reverse();
    }
  });

  const generated = generate(ast, { retainLines: false }).code;

  return {
    source: generated,
    directlyModified: [],
    added: [],
    removed: [],
    description: "reverse property order in all ObjectExpressions"
  };
}
