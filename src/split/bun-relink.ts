/**
 * Bun factory module re-linking for the runnable split (--split-runnable).
 *
 * The Bun unpack adapter extracts each `__commonJS`/`Q(...)` module factory
 * into its own file whose body is the RAW factory expression
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

import { readFile, writeFile, rm } from "node:fs/promises";
import * as path from "node:path";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { parseFileAst, traverse } from "../babel-utils.js";
import type { BunModulesManifest } from "../unpack/adapters/bun.js";
import { computeRelativeImportPath } from "./emitter.js";

export const BUN_RELINK_RUNTIME_FILENAME = "__bun-runtime.js";

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

/** The free (module-scope, unbound) references in `code` whose name is a
 * known factory identifier — the ones that need a require binding. */
function referencedFactories(code: string, lookup: FactoryLookup): Set<string> {
  const found = new Set<string>();
  const ast = parseFileAst(code);
  if (!ast) return found;
  traverse(ast, {
    Identifier(p: NodePath<t.Identifier>) {
      const name = p.node.name;
      if (!lookup.has(name) || found.has(name)) return;
      if (!p.isReferencedIdentifier()) return;
      if (p.scope.getBinding(name)) return; // shadowed by a local binding
      found.add(name);
    }
  });
  return found;
}

/** Byte offset after the directive prologue (so injected requires never
 * displace a leading "use strict"), else the first statement, else 0. */
function headerInsertOffset(code: string): number {
  const ast = parseFileAst(code);
  if (!ast) return 0;
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

/**
 * Inject `const <id> = require("<rel>")` for every free factory reference.
 * References stay bare; the required module exports the callable thunk.
 */
export function relinkFactoryReferences(
  code: string,
  fromFile: string,
  lookup: FactoryLookup
): string {
  const refs = referencedFactories(code, lookup);
  if (refs.size === 0) return code;
  const lines = [...refs]
    .sort()
    .map((id) => requireLine(id, fromFile, lookup.get(id)?.fileName ?? id));
  const at = headerInsertOffset(code);
  const block = lines.join("\n");
  if (at === 0) return `${block}\n${code}`;
  return `${code.slice(0, at)}\n${block}${code.slice(at)}`;
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
  const wrapped =
    `const { __commonJS } = require("${rt}");\n` +
    `module.exports = __commonJS(${body.trim()});\n`;
  return relinkFactoryReferences(wrapped, fromFile, lookup);
}

/**
 * Re-link a written unpack+split output tree into a runnable graph:
 * writes the shared runtime, wraps + re-binds every extracted factory
 * file, re-binds every split-tree file, and removes the stale runtime.
 */
export async function relinkBunModules(
  outputDir: string,
  manifest: BunModulesManifest,
  splitFiles: string[]
): Promise<void> {
  const lookup = factoryLookup(manifest);
  await writeFile(
    path.join(outputDir, BUN_RELINK_RUNTIME_FILENAME),
    BUN_RELINK_RUNTIME
  );

  for (const factory of manifest.factories) {
    const abs = path.join(outputDir, factory.fileName);
    const body = await readFile(abs, "utf-8");
    await writeFile(abs, wrapExtractedFactory(body, factory.fileName, lookup));
  }

  for (const rel of splitFiles) {
    const abs = path.join(outputDir, rel);
    const code = await readFile(abs, "utf-8");
    await writeFile(abs, relinkFactoryReferences(code, rel, lookup));
  }

  if (manifest.runtimeFile) {
    await rm(path.join(outputDir, manifest.runtimeFile), { force: true });
  }
}
