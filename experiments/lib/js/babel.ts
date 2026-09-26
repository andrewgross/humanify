/**
 * The harness's Babel funnel — the parse and traverse the scoring scripts
 * read humanified trees with. Moved from the TS pipeline's src/babel-utils.ts
 * at the cutover (docs/rust-port/19-cutover.md): the pipeline is the Rust
 * binary now, and Babel is a harness devDependency. Options are the ones every
 * KPI on record was computed with (no config discovery, source type inferred);
 * changing them changes what `statementHash` sees.
 */
import { parseSync } from "@babel/core";
import type { Visitor } from "@babel/traverse";
import * as babelTraverse from "@babel/traverse";
import type * as t from "@babel/types";

type TraverseFn = (
  parent: t.Node,
  opts: Visitor,
  scope?: unknown,
  state?: unknown,
  parentPath?: unknown
) => void;

/** @babel/traverse's default export, across the ESM/CJS double-default. */
export const traverse: TraverseFn =
  typeof babelTraverse.default === "function"
    ? (babelTraverse.default as unknown as TraverseFn)
    : ((babelTraverse.default as unknown as Record<string, unknown>)
        .default as TraverseFn);

/**
 * Sources at or above this size are whole bundles (17-32MB). Parsing one
 * clears Babel's module-level path/scope cache first: it is keyed by AST node,
 * so every parse-then-drop leaves millions of dead keys and V8 re-hashes the
 * tombstone-dense table on nearly every insert of the NEXT big parse (the
 * O(n²) hang of exp030). The scorer parses two bundles per pair.
 */
const BIG_SOURCE_BYTES = 5_000_000;

export function clearBabelTraverseCache(): void {
  const cache = (traverse as unknown as { cache?: { clear?: () => void } })
    .cache;
  if (typeof cache?.clear !== "function") {
    throw new Error(
      "@babel/traverse cache API not found on the resolved traverse function"
    );
  }
  cache.clear();
}

/** Parse JS text hermetically; null when Babel produces no AST. */
export function parseFileAst(code: string): t.File | null {
  if (code.length >= BIG_SOURCE_BYTES) clearBabelTraverseCache();
  return parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
}
