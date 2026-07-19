import { type PluginItem, parseSync, transform } from "@babel/core";
import type { GeneratorOptions, GeneratorResult } from "@babel/generator";
import * as babelGenerator from "@babel/generator";
import type { NodePath, Visitor } from "@babel/traverse";
import * as babelTraverse from "@babel/traverse";
import type * as t from "@babel/types";
import { resetAnalysisNodeCaches } from "./analysis/node-caches.js";

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
 * Sources at or above this size are full bundles (17-32MB in the walk), and
 * parsing one starts a new CACHE ERA — see maybeResetAstCaches. Emitted
 * split files (<500KB), vendor factories, and relink files sit far below;
 * bundles sit far above. Gate on code.length: cheap and era-faithful.
 */
export const BIG_SOURCE_BYTES = 5_000_000;

export interface ParseSourceOptions {
  sourceType?: "module" | "unambiguous" | "script";
  filename?: string;
  errorRecovery?: boolean;
  /**
   * Prior-bundle parse ONLY (prior-version.ts): it is born while the new
   * AST is still live, and the era's warm caches are load-bearing there —
   * (a) cross-version matching reads hash/binding entries keyed by BOTH
   * ASTs' nodes, and (b) the hermetic rename invariant deliberately
   * resolves through `bindingByIdentifierNode` entries captured BEFORE any
   * rename (structural-hash.ts resolveIdentifierBinding docs;
   * output-validation.ts). Resetting there would both slow matching and
   * change the invariant's failure-case semantics.
   */
  preserveAstCaches?: boolean;
}

/**
 * Start a fresh cache era when a full bundle is about to be parsed: swap the
 * node-keyed analysis WeakMaps (node-caches.ts) AND Babel's internal
 * path/scope cache. Both are module-level and keyed by AST nodes, so every
 * parse-then-drop cycle leaves millions of dead keys; V8 then re-hashes the
 * tombstone-dense ephemeron tables on nearly every insert of the NEXT big
 * parse — the systemic O(n²) 100%-CPU hang of exp030 (whack-a-mole
 * per-boundary resets were tried and reverted; the era must begin at the
 * parse itself). Deliberate, accepted cost: passes that traverse a
 * still-live older AST after a newer big parse (rename ledger, minted
 * census after the output re-parses) re-crawl scope cold — seconds, versus
 * multi-hour hangs.
 */
function maybeResetAstCaches(code: string, preserve?: boolean): void {
  if (code.length < BIG_SOURCE_BYTES || preserve) return;
  resetAnalysisNodeCaches();
  clearBabelTraverseCache();
}

/**
 * THE parse funnel for pipeline sources. Every full-bundle parse must come
 * through here (directly or via parseFileAst) so cache-era hygiene cannot
 * be forgotten at new call sites. Hermetic by construction: no babel config
 * discovery, source type inferred unless overridden.
 */
export function parseSourceAst(
  code: string,
  opts: ParseSourceOptions = {}
): t.File | null {
  maybeResetAstCaches(code, opts.preserveAstCaches);
  return parseSync(code, {
    sourceType: opts.sourceType ?? "unambiguous",
    filename: opts.filename,
    configFile: false,
    babelrc: false,
    parserOpts: opts.errorRecovery ? { errorRecovery: true } : undefined
  }) as t.File | null;
}

/**
 * Parse standalone JS text with the pipeline's canonical options (no config
 * discovery, source type inferred). Returns null when Babel produces no
 * AST. Thin wrapper over parseSourceAst, so big sources parsed through it
 * (the split re-parse, any future site) inherit the cache-era reset.
 */
export function parseFileAst(code: string): t.File | null {
  return parseSourceAst(code);
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
  // babel transform() parses AND fully traverses internally — on the full
  // bundle (the pre-rename babel plugin) that is a complete cache-era fill,
  // so it needs the same era boundary as parseSourceAst.
  maybeResetAstCaches(code);
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
