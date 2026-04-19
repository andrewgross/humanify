import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { generate, traverse } from "../../../src/babel-utils.js";
import type { Perturbation, PerturbationResult } from "../types.js";

/**
 * Inject `console.log("perturbation-lab")` at the start of a specific
 * function's body. Structural hash of that function changes (and cascades
 * to every enclosing function), but the rest of the source is untouched.
 *
 * Critically: the minifier still sees a "different" source tree and will
 * reshuffle identifier names globally, so v1-min and v2-min end up with
 * very different mangled names — exactly what we want to stress-test
 * the matcher.
 */
export function addConsoleLogTo(targetFunctionName: string): Perturbation {
  return {
    id: `addConsoleLog:${targetFunctionName}`,
    description: `inject console.log into function "${targetFunctionName}"`,
    apply(source) {
      return applyConsoleLog(source, targetFunctionName);
    }
  };
}

function applyConsoleLog(
  source: string,
  targetFunctionName: string
): PerturbationResult {
  const ast = parseSync(source, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse source");

  let injected = false;

  traverse(ast, {
    FunctionDeclaration(path: NodePath<t.FunctionDeclaration>) {
      if (injected) return;
      if (path.node.id?.name !== targetFunctionName) return;
      injectLog(path.node.body);
      injected = true;
      path.stop();
    },
    FunctionExpression(path: NodePath<t.FunctionExpression>) {
      if (injected) return;
      if (path.node.id?.name === targetFunctionName) {
        injectLog(path.node.body);
        injected = true;
        path.stop();
        return;
      }
      const parent = path.parent;
      if (
        t.isVariableDeclarator(parent) &&
        t.isIdentifier(parent.id) &&
        parent.id.name === targetFunctionName
      ) {
        injectLog(path.node.body);
        injected = true;
        path.stop();
      }
    },
    ArrowFunctionExpression(path: NodePath<t.ArrowFunctionExpression>) {
      if (injected) return;
      const parent = path.parent;
      if (
        t.isVariableDeclarator(parent) &&
        t.isIdentifier(parent.id) &&
        parent.id.name === targetFunctionName
      ) {
        if (t.isBlockStatement(path.node.body)) {
          injectLog(path.node.body);
        } else {
          path.node.body = t.blockStatement([
            buildLogStatement(),
            t.returnStatement(path.node.body)
          ]);
        }
        injected = true;
        path.stop();
      }
    }
  });

  if (!injected) {
    throw new Error(
      `addConsoleLogTo: could not find function "${targetFunctionName}"`
    );
  }

  const generated = generate(ast, { retainLines: false }).code;

  return {
    source: generated,
    directlyModified: [targetFunctionName],
    added: [],
    removed: [],
    description: `inject console.log into ${targetFunctionName}`
  };
}

function buildLogStatement(): t.ExpressionStatement {
  return t.expressionStatement(
    t.callExpression(
      t.memberExpression(t.identifier("console"), t.identifier("log")),
      [t.stringLiteral("perturbation-lab")]
    )
  );
}

function injectLog(body: t.BlockStatement): void {
  body.body.unshift(buildLogStatement());
}
