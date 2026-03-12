import { PluginItem, transform } from "@babel/core";
import type { GeneratorOptions, GeneratorResult } from "@babel/generator";
import * as babelGenerator from "@babel/generator";
import type { Visitor } from "@babel/traverse";
import * as babelTraverse from "@babel/traverse";
import type * as t from "@babel/types";

type GenerateFn = (
  ast: t.Node,
  opts?: GeneratorOptions,
  code?: string
) => GeneratorResult;
type TraverseFn = (
  parent: t.Node,
  opts: Visitor,
  scope?: unknown,
  state?: unknown,
  parentPath?: unknown
) => void;

/**
 * ESM/CJS compatibility helper for @babel/generator.
 * Handles the double-default that occurs with some bundler configurations.
 */
export const generate: GenerateFn =
  typeof babelGenerator.default === "function"
    ? (babelGenerator.default as unknown as GenerateFn)
    : (babelGenerator.default as any).default;

/**
 * ESM/CJS compatibility helper for @babel/traverse.
 * Handles the double-default that occurs with some bundler configurations.
 */
export const traverse: TraverseFn =
  typeof babelTraverse.default === "function"
    ? (babelTraverse.default as unknown as TraverseFn)
    : (babelTraverse.default as any).default;

export const transformWithPlugins = async (
  code: string,
  plugins: PluginItem[]
): Promise<string> => {
  return await new Promise((resolve, reject) =>
    transform(
      code,
      {
        plugins,
        compact: false,
        minified: false,
        comments: false,
        sourceMaps: false,
        retainLines: false
      },
      (err, result) => {
        if (err || !result) {
          reject(err);
        } else {
          resolve(result.code as string);
        }
      }
    )
  );
};
