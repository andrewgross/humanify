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
 * Sources at or above this size are full bundles (17-32MB in the walk);
 * parsing one clears Babel's module-level cache first — see
 * maybeClearBabelCache. Emitted split files (<500KB), vendor factories, and
 * relink files sit far below; bundles sit far above. Gate on code.length:
 * cheap and era-faithful.
 */
export const BIG_SOURCE_BYTES = 5_000_000;

export interface ParseSourceOptions {
  sourceType?: "module" | "unambiguous" | "script";
  filename?: string;
  errorRecovery?: boolean;
  /**
   * Prior-bundle parse ONLY (prior-version.ts): it is born while the new
   * AST is still the matcher's working set, and clearing Babel's path/scope
   * cache there would force the new AST's scopes to re-crawl on demand
   * mid-matching — pure re-compute cost with no hygiene benefit (the prior
   * AST is still live; its entries are not tombstones yet). The ANALYSIS
   * caches need no such flag: they are per-AST (analysis-cache.ts), so a
   * new parse never disturbs another tree's entries.
   */
  preserveAstCaches?: boolean;
}

/**
 * Swap Babel's module-level path/scope cache for a fresh one when a full
 * bundle is about to be parsed. Babel keys that cache by AST node, so every
 * parse-then-drop cycle leaves millions of dead keys; V8 then re-hashes the
 * tombstone-dense ephemeron table on nearly every insert of the NEXT big
 * parse — the systemic O(n²) 100%-CPU hang of exp030. Our own analysis
 * caches used to share this pathology and were reset here too; they are now
 * scoped per AST (analysis-cache.ts) and die with their tree, so Babel's is
 * the one module-level node-keyed table left to manage. Correctness is
 * unaffected by the clear: hashing keys slots by declaration node, so even
 * a walk that mixes pre- and post-clear scope resolutions unifies (see
 * SerializeState.slotByDeclId); any tree traversed later re-crawls its
 * scopes on demand — seconds, versus multi-hour hangs.
 */
function maybeClearBabelCache(code: string, preserve?: boolean): void {
  if (code.length < BIG_SOURCE_BYTES || preserve) return;
  clearBabelTraverseCache();
}

/**
 * THE parse funnel for pipeline sources. Every full-bundle parse must come
 * through here (directly or via parseFileAst) so Babel-cache hygiene cannot
 * be forgotten at new call sites. Hermetic by construction: no babel config
 * discovery, source type inferred unless overridden.
 */
export function parseSourceAst(
  code: string,
  opts: ParseSourceOptions = {}
): t.File | null {
  maybeClearBabelCache(code, opts.preserveAstCaches);
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
  // bundle (the pre-rename babel plugin) that is a complete Babel-cache
  // fill, so it needs the same boundary as parseSourceAst.
  maybeClearBabelCache(code);
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
