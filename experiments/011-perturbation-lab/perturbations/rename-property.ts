import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { generate, traverse } from "../../../src/babel-utils.js";
import type { Perturbation, PerturbationResult } from "../types.js";

/**
 * Rename a property key in an ObjectExpression/ObjectMethod.
 * The function body is unchanged — only the key name changes.
 * This means the function at the old key should be unmatched in v2.
 */
export function renameProperty(from: string, to: string): Perturbation {
  return {
    id: `renameProperty:${from}->${to}`,
    description: `rename property key "${from}" to "${to}"`,
    apply(source) {
      return applyRename(source, from, to);
    }
  };
}

function applyRename(
  source: string,
  from: string,
  to: string
): PerturbationResult {
  const ast = parseSync(source, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse source");

  let renamed = false;

  traverse(ast, {
    ObjectProperty(path: NodePath<t.ObjectProperty>) {
      if (renamed) return;
      if (t.isIdentifier(path.node.key) && path.node.key.name === from) {
        path.node.key = t.identifier(to);
        renamed = true;
      }
    }
  });

  if (!renamed) {
    throw new Error(`renameProperty: could not find property "${from}"`);
  }

  const generated = generate(ast, { retainLines: false }).code;

  return {
    source: generated,
    directlyModified: [],
    added: [],
    removed: [],
    description: `rename property ${from} -> ${to}`
  };
}
