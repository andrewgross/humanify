import { type PluginItem, parseSync, transform } from "@babel/core";
import type { GeneratorOptions, GeneratorResult } from "@babel/generator";
import * as babelGenerator from "@babel/generator";
import type { NodePath, Visitor } from "@babel/traverse";
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
    : ((babelGenerator.default as unknown as Record<string, unknown>)
        .default as GenerateFn);

/**
 * ESM/CJS compatibility helper for @babel/traverse.
 * Handles the double-default that occurs with some bundler configurations.
 */
export const traverse: TraverseFn =
  typeof babelTraverse.default === "function"
    ? (babelTraverse.default as unknown as TraverseFn)
    : ((babelTraverse.default as unknown as Record<string, unknown>)
        .default as TraverseFn);

/**
 * Drop @babel/traverse's module-level path + scope caches. These are keyed by
 * AST node, so they pin a NodePath (and its resolved Scope, with binding tables)
 * for every node of every tree traversed this process. After the rename pass
 * that is the whole multi-megabyte bundle's graph; the Bun re-link then parses
 * ~1500 small files fresh, and every GC it triggers must trace those millions of
 * now-useless cache entries — the loop goes from seconds to tens of minutes.
 * Clearing first is safe: any tree still needed re-crawls its scope on demand.
 */
export function clearBabelTraverseCache(): void {
  const mod = babelTraverse as unknown as {
    cache?: { clear?: () => void };
    default?: { cache?: { clear?: () => void } };
  };
  const cache = mod.cache ?? mod.default?.cache;
  cache?.clear?.();
}

/**
 * Parse standalone JS text with the pipeline's canonical options (no config
 * discovery, source type inferred). Returns null when Babel produces no AST.
 */
export function parseFileAst(code: string): t.File | null {
  return parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
}

/**
 * The identifier PATHS a constant violation actually writes to that carry
 * `name` — declarator ids, assignment targets (destructuring included),
 * update-expression arguments, for-in/of heads. This one definition backs
 * every consumer that needs write targets (rename line gates, occurrence
 * votes, runnable-split rewriting); keep it single-source.
 */
export function violationWriteTargetPaths(
  violation: NodePath,
  name: string
): NodePath<t.Identifier>[] {
  const out: NodePath<t.Identifier>[] = [];
  const ids = violation.getBindingIdentifierPaths(true);
  for (const entry of Object.values(ids)) {
    for (const idPath of Array.isArray(entry) ? entry : [entry]) {
      if (idPath.node.name === name) {
        out.push(idPath as NodePath<t.Identifier>);
      }
    }
  }
  return out;
}

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
