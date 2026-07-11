/**
 * Exp026: execution-CORRECT CommonJS emission via live namespace bindings.
 *
 * The exp025 emitter destructured imports (`const { x } = require("./a")`),
 * which SNAPSHOTS x at load time — wrong under circular requires (x is
 * undefined mid-cycle) and wrong for mutable bindings (writes don't
 * propagate). The fix is uniform: every cross-file reference goes through
 * the declaring module's namespace via live accessors.
 *
 *   - Declaring file appends, per cross-file-referenced binding:
 *       Object.defineProperty(module.exports, "name",
 *         { get: () => name, set: v => { name = v; }, enumerable: true });
 *     (get-only when the binding is never written cross-file). The getter
 *     reflects the file's current local value — circular-safe (read after
 *     load) and mutation-correct.
 *   - Reader/writer files `const __base = require("./decl")` once per
 *     source file and rewrite each cross-file reference identifier to
 *     `__base.name` (reads AND write targets). Statements with no
 *     cross-file reference stay byte-sliced (exact); only statements that
 *     touch another file are regenerated from the transformed AST.
 *
 *   emit-cjs-live.ts <humanified.js> <ledger.json> <outDir>
 */
import fs from "node:fs";
import path from "node:path";
import { parseSync } from "@babel/core";
import type { Binding, NodePath, Scope } from "@babel/traverse";
import * as t from "@babel/types";
import { generate, traverse } from "../../src/babel-utils.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

const [input, ledgerPath, outDir] = process.argv.slice(2);
const code = fs.readFileSync(input, "utf-8");
const ledger: StableSplitLedger = JSON.parse(
  fs.readFileSync(ledgerPath, "utf-8")
);
const ast = parseSync(code, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
}) as t.File;

const wrapper = (ast.program.body[0] as t.ExpressionStatement)
  .expression as t.FunctionExpression;
const stmts = wrapper.body.body;
const stmtFile = ledger.order;
const ranges = stmts.map((s, i) => ({
  start: s.start ?? 0,
  end: s.end ?? 0,
  file: stmtFile[i]
}));
function fileOfPos(pos: number): string | null {
  let lo = 0;
  let hi = ranges.length - 1;
  while (lo <= hi) {
    const m = (lo + hi) >> 1;
    const r = ranges[m];
    if (pos < r.start) hi = m - 1;
    else if (pos >= r.end) lo = m + 1;
    else return r.file;
  }
  return null;
}
/** Which top-level statement index contains a position. */
function stmtIndexOfPos(pos: number): number {
  let lo = 0;
  let hi = ranges.length - 1;
  while (lo <= hi) {
    const m = (lo + hi) >> 1;
    const r = ranges[m];
    if (pos < r.start) hi = m - 1;
    else if (pos >= r.end) lo = m + 1;
    else return m;
  }
  return -1;
}

let wrapperScope: Scope | undefined;
traverse(ast, {
  FunctionExpression(p) {
    wrapperScope = p.scope;
    p.stop();
  }
});
const scope = wrapperScope as Scope;

// Per binding: declaring file, whether it is written cross-file, and the
// set of (identifier node) reference/write sites that cross a file boundary.
interface Cross {
  name: string;
  declFile: string;
  writable: boolean;
}
const crossByName = new Map<string, Cross>();
// statement index -> list of { node, name } cross-file sites to rewrite
const sitesByStmt = new Map<
  number,
  Array<{ node: t.Identifier; name: string }>
>();
// declFile -> Set of names to export via accessor
const exportsByFile = new Map<string, Set<string>>();
// readerFile -> Set of declFiles it must require
const requiresByFile = new Map<string, Set<string>>();

function noteSite(node: t.Identifier, name: string, declFile: string): void {
  const idx = stmtIndexOfPos(node.start ?? -1);
  if (idx < 0) return;
  const readerFile = stmtFile[idx];
  if (readerFile === declFile) return;
  const list = sitesByStmt.get(idx) ?? [];
  list.push({ node, name });
  sitesByStmt.set(idx, list);
  (
    exportsByFile.get(declFile) ??
    exportsByFile.set(declFile, new Set()).get(declFile)!
  ).add(name);
  (
    requiresByFile.get(readerFile) ??
    requiresByFile.set(readerFile, new Set()).get(readerFile)!
  ).add(declFile);
}

for (const name of Object.keys(scope.bindings)) {
  const binding: Binding = scope.bindings[name];
  const declFile = fileOfPos(binding.identifier.start ?? -1);
  if (!declFile) continue;
  let writable = false;
  const sites: t.Identifier[] = [];
  for (const ref of binding.referencePaths) {
    if (ref.isIdentifier()) sites.push(ref.node);
  }
  for (const v of binding.constantViolations) {
    // the write-target identifier(s) carrying this name
    const ids = t.getBindingIdentifiers(v.node, true);
    for (const entry of Object.values(ids)) {
      for (const id of Array.isArray(entry) ? entry : [entry]) {
        if (id.name === name) {
          sites.push(id);
          if (fileOfPos(id.start ?? -1) !== declFile) writable = true;
        }
      }
    }
  }
  const crossesFile = sites.some((s) => fileOfPos(s.start ?? -1) !== declFile);
  if (!crossesFile) continue;
  crossByName.set(name, { name, declFile, writable });
  for (const s of sites) noteSite(s, name, declFile);
}

console.log(
  JSON.stringify(
    {
      crossFileBindings: crossByName.size,
      writableCrossFileBindings: [...crossByName.values()].filter(
        (c) => c.writable
      ).length,
      statementsToRewrite: sitesByStmt.size,
      statementsTotal: stmts.length
    },
    null,
    2
  )
);

// ---- emit ------------------------------------------------------------------
function relRequire(from: string, to: string): string {
  let rel = path.relative(path.dirname(from), to).replace(/\\/g, "/");
  if (!rel.startsWith(".")) rel = `./${rel}`;
  return rel;
}
function baseVar(file: string): string {
  return `__${file.replace(/[^A-Za-z0-9_$]/g, "_")}`;
}

// Rewrite one statement's cross-file identifier sites to member accesses on
// the declaring module's require-namespace, then regenerate it.
function renderRewritten(
  idx: number,
  sites: Array<{ node: t.Identifier; name: string }>
): string {
  const src = code.slice(stmts[idx].start ?? 0, stmts[idx].end ?? 0);
  const stmtAst = parseSync(src, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File;
  const targetNames = new Set(sites.map((s) => s.name));
  traverse(stmtAst, {
    Identifier(p: NodePath<t.Identifier>) {
      if (!targetNames.has(p.node.name)) return;
      // only rewrite genuine references to the module binding: skip
      // property keys, non-reference positions.
      if (!p.isReferencedIdentifier() && !isWriteTarget(p)) return;
      const cross = crossByName.get(p.node.name);
      if (!cross) return;
      // guard against a shadowing local of the same name in this statement
      const bound = p.scope.getBinding(p.node.name);
      if (bound && bound.scope.block !== stmtAst.program) {
        // declared inside the statement (a local param/var) — leave it
        return;
      }
      p.replaceWith(
        t.memberExpression(
          t.identifier(baseVar(cross.declFile)),
          t.identifier(p.node.name)
        )
      );
      p.skip();
    }
  });
  return generate(stmtAst.program.body[0] as t.Node).code;
}
function isWriteTarget(p: NodePath<t.Identifier>): boolean {
  const parent = p.parent;
  return (
    (t.isAssignmentExpression(parent) && parent.left === p.node) ||
    (t.isUpdateExpression(parent) && parent.argument === p.node)
  );
}

// Assemble each file.
const bodyByFile = new Map<string, string[]>();
stmts.forEach((s, i) => {
  const file = stmtFile[i];
  const parts = bodyByFile.get(file) ?? [];
  const sites = sitesByStmt.get(i);
  parts.push(
    sites ? renderRewritten(i, sites) : code.slice(s.start ?? 0, s.end ?? 0)
  );
  bodyByFile.set(file, parts);
});

fs.mkdirSync(outDir, { recursive: true });
for (const file of ledger.files) {
  const lines: string[] = [];
  const reqs = requiresByFile.get(file);
  if (reqs) {
    for (const decl of [...reqs].sort()) {
      lines.push(
        `const ${baseVar(decl)} = require("${relRequire(file, decl)}");`
      );
    }
    lines.push("");
  }
  lines.push(...(bodyByFile.get(file) ?? []));
  const exps = exportsByFile.get(file);
  if (exps) {
    lines.push("");
    for (const name of [...exps].sort()) {
      const cross = crossByName.get(name)!;
      const accessor = cross.writable
        ? `{ get: () => ${name}, set: (v) => { ${name} = v; }, enumerable: true, configurable: true }`
        : `{ get: () => ${name}, enumerable: true, configurable: true }`;
      lines.push(
        `Object.defineProperty(module.exports, "${name}", ${accessor});`
      );
    }
  }
  const outPath = path.join(outDir, file);
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, `${lines.join("\n")}\n`);
}
fs.copyFileSync(ledgerPath, path.join(outDir, "_split-ledger.json"));

// ---- validate: babel-parse every file --------------------------------------
let parseOk = 0;
let parseFail = 0;
const parseFails: string[] = [];
for (const file of ledger.files) {
  const parsed = parseSync(fs.readFileSync(path.join(outDir, file), "utf-8"), {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (parsed) parseOk++;
  else {
    parseFail++;
    if (parseFails.length < 5) parseFails.push(file);
  }
}
console.log(
  JSON.stringify(
    { babelParseClean: parseOk, babelParseFailed: parseFail, parseFails },
    null,
    2
  )
);
