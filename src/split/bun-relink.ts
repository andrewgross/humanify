/**
 * Bun factory module re-linking for the runnable split (the --split
 * default emit).
 *
 * The Bun unpack adapter extracts each `__commonJS`/`Q(...)` module factory
 * into its own file under vendor/ whose body is the RAW factory expression
 * (`(exports, module) => { ... }`), splices the declaration out of the
 * runtime, and rewrites every reference to a FREE identifier
 * (`runtimeIdentifier`, e.g. `lib_234a1f83`). That tree is a review
 * artifact — nothing binds those free identifiers, so it does not run.
 *
 * This module re-binds them into an executable CommonJS graph:
 *   - Each extracted factory file is wrapped as a real module that exports
 *     the memoizing thunk `module.exports = __commonJS(<factory>)` — the
 *     exact semantics of Bun's `Q = (H,_)=>()=>(_||H((_={exports:{}}).
 *     exports,_),_.exports)`: lazy, run-once, memoized.
 *   - Every file that references a factory (the split-tree files AND other
 *     factory bodies) gets `const <id> = require("<rel path>")` injected,
 *     so the bare `<id>()` call invokes the shared thunk. References are
 *     never rewritten — the module exports the callable directly.
 *   - A shared `__bun-runtime.js` provides `__commonJS`/`__esm`.
 *
 * Injection is a byte-splice after the directive prologue (never a
 * regenerate), so untouched code stays byte-exact. Deterministic.
 */

import { mkdir, readFile, writeFile, rm } from "node:fs/promises";
import * as path from "node:path";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import {
  clearBabelTraverseCache,
  parseFileAst,
  traverse
} from "../babel-utils.js";
import { debug } from "../debug.js";
import type { BunModulesManifest } from "../unpack/adapters/bun.js";
import { computeRelativeImportPath } from "./emitter.js";
import { METADATA_DIR } from "./layout.js";

/** The shared factory-helper runtime, a generated shim (like _bundle.js)
 * that lives with the metadata rather than the reviewable code. */
export const BUN_RELINK_RUNTIME_FILENAME = `${METADATA_DIR}/__bun-runtime.js`;

/**
 * Shared Bun factory helpers, matching the bundle's own `Q`/`__esm`:
 *   Q     = (H,_)=>()=>(_||H((_={exports:{}}).exports,_),_.exports)
 *   __esm = (H,_)=>()=>(H&&(_=H(H=0)),_)
 */
export const BUN_RELINK_RUNTIME = `// Bun CJS/ESM factory helpers, extracted for the runnable split graph.
// __commonJS wraps a (exports, module) factory into a lazy, run-once,
// memoized thunk; __esm does the same for an ESM init function.
const __commonJS = (factory) => {
  let mod;
  return () => (
    mod || factory((mod = { exports: {} }).exports, mod), mod.exports
  );
};
const __esm = (factory) => {
  let value;
  return () => (factory && (value = factory((factory = 0))), value);
};
module.exports = { __commonJS, __esm };
`;

/** runtimeIdentifier → the file that defines that factory. */
export type FactoryLookup = Map<string, { fileName: string }>;

export function factoryLookup(manifest: BunModulesManifest): FactoryLookup {
  const map: FactoryLookup = new Map();
  for (const f of manifest.factories) {
    if (f.runtimeIdentifier)
      map.set(f.runtimeIdentifier, { fileName: f.fileName });
  }
  return map;
}

/** The property on a relinked factory module that holds the memoizing
 * thunk. References read it live (`lib_x.f`) rather than snapshotting the
 * module's exports, so a factory reassigned mid-require-cycle still
 * resolves once its file finishes loading. */
const THUNK_PROP = "f";

interface FactoryRef {
  name: string;
  /** Byte offset just after the identifier (where `.f` is spliced in). */
  end: number;
}

/** The free (module-scope, unbound) references in `ast` whose name is a
 * known factory identifier — each needs a require binding and a `.f`
 * rewrite. */
function factoryRefs(ast: t.File, lookup: FactoryLookup): FactoryRef[] {
  const refs: FactoryRef[] = [];
  traverse(ast, {
    Identifier(p: NodePath<t.Identifier>) {
      const name = p.node.name;
      if (!lookup.has(name)) return;
      if (!p.isReferencedIdentifier()) return;
      if (p.scope.getBinding(name)) return; // shadowed by a local binding
      if (p.node.end == null) return;
      refs.push({ name, end: p.node.end });
    }
  });
  return refs;
}

/** Byte offset after the directive prologue (so injected requires never
 * displace a leading "use strict"), else the first statement, else the
 * end. Computed on the PRE-splice parse: every `.f` splice lands strictly
 * after an identifier inside a statement, so no splice can shift text at
 * or before this offset. */
function headerInsertOffset(ast: t.File, code: string): number {
  const directives = ast.program.directives;
  if (directives.length > 0) {
    return directives[directives.length - 1].end ?? 0;
  }
  const body = ast.program.body;
  if (body.length > 0) return body[0].start ?? 0;
  return code.length;
}

function requireLine(id: string, fromFile: string, toFile: string): string {
  return `const ${id} = require("${computeRelativeImportPath(fromFile, toFile)}");`;
}

function insertHeaderAt(code: string, at: number, lines: string[]): string {
  const block = lines.join("\n");
  if (at === 0) return `${block}\n${code}`;
  return `${code.slice(0, at)}\n${block}${code.slice(at)}`;
}

/**
 * Re-bind every free factory reference: inject
 * `const <id> = require("<rel>")` headers, and rewrite each reference
 * `<id>` → `<id>.f` (a live read of the memoizing thunk on the required
 * module's stable exports object). The live read is what makes the graph
 * survive require cycles — bundled code is full of them.
 *
 * One parse per file, shared by the reference scan and the header-offset
 * computation — this runs over every emitted file, so a second parse per
 * file doubled the loop's dominant cost.
 */
export function relinkFactoryReferences(
  code: string,
  fromFile: string,
  lookup: FactoryLookup
): string {
  const ast = parseFileAst(code);
  if (!ast) return code;
  const refs = factoryRefs(ast, lookup);
  if (refs.length === 0) return code;
  // Splice `.f` after each reference, right-to-left so earlier offsets stay
  // valid as later ones shift.
  let spliced = code;
  for (const ref of [...refs].sort((a, b) => b.end - a.end)) {
    spliced = `${spliced.slice(0, ref.end)}.${THUNK_PROP}${spliced.slice(ref.end)}`;
  }
  const ids = [...new Set(refs.map((r) => r.name))].sort();
  const lines = ids.map((id) =>
    requireLine(id, fromFile, lookup.get(id)?.fileName ?? id)
  );
  return insertHeaderAt(spliced, headerInsertOffset(ast, code), lines);
}

/**
 * Turn an extracted factory body (a raw `(exports, module) => {…}`
 * expression) into a runnable CJS module exporting the memoizing thunk,
 * with cross-module factory references re-bound.
 */
export function wrapExtractedFactory(
  body: string,
  fromFile: string,
  lookup: FactoryLookup
): string {
  const rt = computeRelativeImportPath(fromFile, BUN_RELINK_RUNTIME_FILENAME);
  // `exports.f = …` MUTATES the initial exports object rather than
  // reassigning `module.exports`, so the object identity a cyclic requirer
  // captured stays valid and its `.f` becomes visible once this file runs.
  const wrapped =
    `const { __commonJS } = require("${rt}");\n` +
    `exports.${THUNK_PROP} = __commonJS(${body.trim()});\n`;
  return relinkFactoryReferences(wrapped, fromFile, lookup);
}

/** Tunables for the per-file re-link loop; tests inject a counting clear. */
export interface RelinkOptions {
  /** Files between Babel-cache era resets (default: RELINK_CACHE_CLEAR_INTERVAL). */
  cacheClearInterval?: number;
  /** The era reset itself (default: clearBabelTraverseCache). */
  clearCache?: () => void;
}

/**
 * Era length for the per-file loop. Each file's parse + traverse fills
 * Babel's node-keyed cache with entries that are dead the moment the loop
 * advances; one shared era across ~1,500 files keeps the ephemeron table
 * dense with dead keys under continuous insertion, which V8 answers by
 * re-hashing the table on inserts — the nondeterministic split-phase hang
 * (docs/analysis-two-version-memory-flow.md §6a). Resetting every N files
 * bounds the table at N files' worth of entries.
 */
const RELINK_CACHE_CLEAR_INTERVAL = 100;

/**
 * Re-link a written unpack+split output tree into a runnable graph:
 * writes the shared runtime, wraps + re-binds every extracted factory
 * file, re-binds every split-tree file, and removes the stale runtime.
 */
export async function relinkBunModules(
  outputDir: string,
  manifest: BunModulesManifest,
  splitFiles: string[],
  opts: RelinkOptions = {}
): Promise<void> {
  const clearCache = opts.clearCache ?? clearBabelTraverseCache;
  const interval = opts.cacheClearInterval ?? RELINK_CACHE_CLEAR_INTERVAL;
  // This pass parses + traverses every factory file and every split file.
  // Start it on a fresh Babel path/scope cache era (the caller released its
  // big ASTs via releaseSplitSourceState; this drops their cached entries),
  // then reset the era every `interval` files so the per-file churn cannot
  // densify one shared table — see RELINK_CACHE_CLEAR_INTERVAL.
  clearCache();
  const lookup = factoryLookup(manifest);
  const runtimePath = path.join(outputDir, BUN_RELINK_RUNTIME_FILENAME);
  await mkdir(path.dirname(runtimePath), { recursive: true });
  await writeFile(runtimePath, BUN_RELINK_RUNTIME);

  const total = manifest.factories.length + splitFiles.length;
  let processed = 0;
  const advanceEra = () => {
    processed++;
    if (processed % interval === 0) clearCache();
    if (processed % RELINK_CACHE_CLEAR_INTERVAL === 0 || processed === total) {
      debug.log("bun-relink", `re-linked ${processed}/${total} file(s)`);
    }
  };

  for (const factory of manifest.factories) {
    const abs = path.join(outputDir, factory.fileName);
    const body = await readFile(abs, "utf-8");
    await writeFile(abs, wrapExtractedFactory(body, factory.fileName, lookup));
    advanceEra();
  }

  for (const rel of splitFiles) {
    const abs = path.join(outputDir, rel);
    const code = await readFile(abs, "utf-8");
    await writeFile(abs, relinkFactoryReferences(code, rel, lookup));
    advanceEra();
  }

  if (manifest.runtimeFile) {
    await rm(path.join(outputDir, manifest.runtimeFile), { force: true });
  }
}
