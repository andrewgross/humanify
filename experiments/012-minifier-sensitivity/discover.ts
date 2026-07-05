import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../../src/babel-utils.js";

/**
 * Discover all function names in source that addConsoleLogTo can target.
 * Matches the same patterns: FunctionDeclaration, FunctionExpression (named
 * or via VariableDeclarator), ArrowFunctionExpression via VariableDeclarator,
 * and ObjectMethod with identifier key.
 */
export function discoverPerturbableNames(source: string): string[] {
  const ast = parseSync(source, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse source");

  const names: string[] = [];
  const seen = new Set<string>();

  function add(name: string): void {
    if (name && !seen.has(name)) {
      seen.add(name);
      names.push(name);
    }
  }

  traverse(ast, {
    FunctionDeclaration(path: NodePath<t.FunctionDeclaration>) {
      if (path.node.id?.name) add(path.node.id.name);
    },
    FunctionExpression(path: NodePath<t.FunctionExpression>) {
      if (path.node.id?.name) {
        add(path.node.id.name);
        return;
      }
      const parent = path.parent;
      if (t.isVariableDeclarator(parent) && t.isIdentifier(parent.id)) {
        add(parent.id.name);
      }
    },
    ArrowFunctionExpression(path: NodePath<t.ArrowFunctionExpression>) {
      const parent = path.parent;
      if (t.isVariableDeclarator(parent) && t.isIdentifier(parent.id)) {
        add(parent.id.name);
      }
    },
    ObjectMethod(path: NodePath<t.ObjectMethod>) {
      if (t.isIdentifier(path.node.key) && !path.node.computed) {
        add(path.node.key.name);
      }
    }
  });

  return names;
}
