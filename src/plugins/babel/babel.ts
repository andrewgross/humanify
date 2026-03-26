import type { PluginItem } from "@babel/core";
import * as t from "@babel/types";
import bautifier from "babel-plugin-transform-beautifier";
import { transformWithPlugins } from "../../babel-utils.js";
import type { Profiler } from "../../profiling/profiler.js";
import { NULL_PROFILER } from "../../profiling/profiler.js";

const convertVoidToUndefined: PluginItem = {
  visitor: {
    // Convert void 0 to undefined
    UnaryExpression(path) {
      if (
        path.node.operator === "void" &&
        path.node.argument.type === "NumericLiteral"
      ) {
        path.replaceWith({
          type: "Identifier",
          name: "undefined"
        });
      }
    }
  }
};

const flipComparisonsTheRightWayAround: PluginItem = {
  visitor: {
    // If a variable is compared to a literal, flip the comparison around so that the literal is on the right-hand side
    BinaryExpression(path) {
      const node = path.node;
      const mappings: Record<string, string> = {
        "==": "==",
        "!=": "!=",
        "===": "===",
        "!==": "!==",
        "<": ">",
        "<=": ">=",
        ">": "<",
        ">=": "<="
      };
      if (
        t.isLiteral(node.left) &&
        !t.isLiteral(node.right) &&
        mappings[node.operator]
      ) {
        path.replaceWith(
          t.binaryExpression(
            mappings[node.operator] as t.BinaryExpression["operator"],
            node.right,
            node.left
          )
        );
      }
    }
  }
};

const makeNumbersLonger: PluginItem = {
  visitor: {
    // Convert 5e3 to 5000
    NumericLiteral(path) {
      if (
        typeof path.node.extra?.raw === "string" &&
        path.node.extra?.raw?.includes("e")
      ) {
        path.replaceWith({
          type: "NumericLiteral",
          value: Number(path.node.extra.raw)
        });
      }
    }
  }
};

/**
 * Wrap bautifier to fix its SequenceExpression handler.
 * The original checks `parentPath.isStatement()` which is true for
 * ForStatement, causing compound update expressions like `i++, j+=2`
 * to be extracted out of the loop into `i++; for(...; j+=2)`.
 */
function createPatchedBautifier(): PluginItem {
  type BabelApi = { types: typeof t };
  type BautifierPlugin = { name: string; visitor: Record<string, unknown> };
  // Handle both ESM default import and CJS module object
  const bautifierFn = (
    typeof bautifier === "function"
      ? bautifier
      : (
          bautifier as unknown as {
            default: (api: BabelApi) => BautifierPlugin;
          }
        ).default
  ) as (api: BabelApi) => BautifierPlugin;
  return (babel: BabelApi) => {
    const plugin = bautifierFn(babel);
    const babelTypes = babel.types;

    // Override the SequenceExpression handler to skip for-loop positions
    plugin.visitor.SequenceExpression = (path: {
      node: { expressions: t.Expression[] };
      parentPath: {
        isStatement(): boolean;
        isForStatement(): boolean;
        insertBefore(nodes: t.Statement[]): void;
      };
      key: string;
      replaceWith(node: t.Expression): void;
    }) => {
      const exprs = path.node.expressions;
      const { parentPath } = path;
      if (!parentPath.isStatement()) {
        return;
      }
      // Don't extract from ForStatement update, init, or test positions
      if (
        parentPath.isForStatement() &&
        (path.key === "update" || path.key === "init" || path.key === "test")
      ) {
        return;
      }
      parentPath.insertBefore(
        exprs.slice(0, -1).map((exp) => babelTypes.expressionStatement(exp))
      );
      path.replaceWith(exprs[exprs.length - 1]);
    };

    return plugin;
  };
}

export function createBabelPlugin(options?: { profiler?: Profiler }) {
  const profiler = options?.profiler ?? NULL_PROFILER;
  return async (code: string): Promise<string> => {
    const span = profiler.startSpan("babel-transforms", "pipeline");
    const result = await transformWithPlugins(code, [
      convertVoidToUndefined,
      flipComparisonsTheRightWayAround,
      makeNumbersLonger,
      createPatchedBautifier()
    ]);
    span.end();
    return result;
  };
}

const _babelTransform = async (code: string): Promise<string> =>
  transformWithPlugins(code, [
    convertVoidToUndefined,
    flipComparisonsTheRightWayAround,
    makeNumbersLonger,
    createPatchedBautifier()
  ]);
