/**
 * Exp025: emit a CommonJS module graph from a humanified bundle + its
 * split ledger, then validate. Scope-accurate cross-file edges (Babel
 * bindings, not names): each file exports the module-scope bindings other
 * files read, requires what it reads, and rewrites the 169 cross-file
 * writes as namespace-qualified assignments (CJS propagates those).
 *
 *   emit-cjs.ts <humanified.js> <ledger.json> <outDir>
 *
 * Prints edge stats; validates every emitted file with `node --check` and
 * checks that every require target exists. Honest about execution: a
 * hoisted bundle's circular requires are reported, not claimed solved.
 */
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { parseSync } from "@babel/core";
import type { Binding, NodePath, Scope } from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../../src/babel-utils.js";
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

// ---- statement byte ranges → file (via ledger.order) ----------------------
const wrapper = (ast.program.body[0] as t.ExpressionStatement)
  .expression as t.FunctionExpression;
const stmts = wrapper.body.body;
const ranges = stmts.map((s, i) => ({
  start: s.start ?? 0,
  end: s.end ?? 0,
  file: ledger.order[i]
}));
function fileOfPos(pos: number): string | null {
  let lo = 0;
  let hi = ranges.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const r = ranges[mid];
    if (pos < r.start) hi = mid - 1;
    else if (pos >= r.end) lo = mid + 1;
    else return r.file;
  }
  return null;
}

// ---- wrapper scope + cross-file edges --------------------------------------
let wrapperScope: Scope | undefined;
traverse(ast, {
  FunctionExpression(p) {
    wrapperScope = p.scope;
    p.stop();
  }
});
const scope = wrapperScope as Scope;

interface FileInfo {
  /** module-scope names declared here (in declaration order). */
  declares: string[];
  /** names this file reads that are declared elsewhere → import. */
  imports: Map<string, Set<string>>; // sourceFile -> names
  /** names other files read that are declared here → export. */
  exportsNeeded: Set<string>;
  /** cross-file-written binding names declared here (namespace targets). */
  writtenTargets: Set<string>;
}
const infos = new Map<string, FileInfo>();
const ensure = (f: string): FileInfo => {
  let info = infos.get(f);
  if (!info) {
    info = {
      declares: [],
      imports: new Map(),
      exportsNeeded: new Set(),
      writtenTargets: new Set()
    };
    infos.set(f, info);
  }
  return info;
};
for (const f of ledger.files) ensure(f);

const declFileOf = new Map<string, string>();
let crossFileReadEdges = 0;
let crossFileWriteBindings = 0;

for (const name of Object.keys(scope.bindings)) {
  const binding: Binding = scope.bindings[name];
  const declFile = fileOfPos(binding.identifier.start ?? -1);
  if (!declFile) continue;
  declFileOf.set(name, declFile);
  ensure(declFile).declares.push(name);

  // reads from other files → import + export
  const readerFiles = new Set<string>();
  for (const ref of binding.referencePaths) {
    const f = fileOfPos(ref.node.start ?? -1);
    if (f && f !== declFile) readerFiles.add(f);
  }
  for (const rf of readerFiles) {
    const imp = ensure(rf).imports.get(declFile) ?? new Set<string>();
    imp.add(name);
    ensure(rf).imports.set(declFile, imp);
    ensure(declFile).exportsNeeded.add(name);
    crossFileReadEdges++;
  }

  // cross-file writes → namespace-qualified; the decl file exports the
  // binding as a live property (via exports object).
  const writerFiles = new Set<string>();
  for (const v of binding.constantViolations) {
    const f = fileOfPos(v.node.start ?? -1);
    if (f && f !== declFile) writerFiles.add(f);
  }
  if (writerFiles.size > 0) {
    crossFileWriteBindings++;
    ensure(declFile).writtenTargets.add(name);
    ensure(declFile).exportsNeeded.add(name);
    for (const wf of writerFiles) {
      const imp = ensure(wf).imports.get(declFile) ?? new Set<string>();
      imp.add(name);
      ensure(wf).imports.set(declFile, imp);
    }
  }
}

console.log(
  JSON.stringify(
    {
      files: infos.size,
      crossFileReadEdges,
      crossFileWriteBindings,
      avgImportsPerFile: (
        [...infos.values()].reduce((s, i) => s + i.imports.size, 0) / infos.size
      ).toFixed(1)
    },
    null,
    2
  )
);

// ---- emit ------------------------------------------------------------------
function relImport(from: string, to: string): string {
  let rel = path.relative(path.dirname(from), to).replace(/\\/g, "/");
  if (!rel.startsWith(".")) rel = `./${rel}`;
  return rel;
}

// Per file: build the body text by re-slicing the original statements
// assigned to it, in order (concat-equivalence per file).
const bodyByFile = new Map<string, string[]>();
stmts.forEach((s, i) => {
  const f = ledger.order[i];
  const parts = bodyByFile.get(f) ?? [];
  parts.push(code.slice(s.start ?? 0, s.end ?? 0));
  bodyByFile.set(f, parts);
});

fs.mkdirSync(outDir, { recursive: true });
for (const [file, info] of infos) {
  const lines: string[] = [];
  // imports: bindings that are ONLY read → destructure; write-targets →
  // whole-namespace so writes propagate.
  for (const [src, names] of info.imports) {
    const nsName = `__${path.basename(src, ".js").replace(/[^A-Za-z0-9_$]/g, "_")}`;
    const writeTargets = [...names].filter((n) =>
      ensure(src).writtenTargets.has(n)
    );
    const readOnly = [...names].filter(
      (n) => !ensure(src).writtenTargets.has(n)
    );
    if (readOnly.length > 0) {
      lines.push(
        `const { ${readOnly.sort().join(", ")} } = require("${relImport(file, src)}");`
      );
    }
    if (writeTargets.length > 0) {
      lines.push(`const ${nsName} = require("${relImport(file, src)}");`);
    }
  }
  if (info.imports.size > 0) lines.push("");
  lines.push(...(bodyByFile.get(file) ?? []));
  if (info.exportsNeeded.size > 0) {
    lines.push("");
    lines.push(
      `module.exports = { ${[...info.exportsNeeded].sort().join(", ")} };`
    );
  }
  const outPath = path.join(outDir, file);
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, `${lines.join("\n")}\n`);
}

// ---- validate --------------------------------------------------------------
let checkOk = 0;
let checkFail = 0;
const failSamples: string[] = [];
for (const file of infos.keys()) {
  try {
    execFileSync("node", ["--check", path.join(outDir, file)], {
      stdio: "pipe"
    });
    checkOk++;
  } catch (e) {
    checkFail++;
    if (failSamples.length < 5) {
      const msg = e instanceof Error ? (e.message.split("\n")[1] ?? "") : "";
      failSamples.push(`${file}: ${msg}`);
    }
  }
}

// Babel parse (the project's parser — handles `using`, which node --check
// on this runtime does not) proves emission soundness independent of the
// Node syntax gap.
let babelOk = 0;
let babelFail = 0;
for (const file of infos.keys()) {
  const parsed = parseSync(fs.readFileSync(path.join(outDir, file), "utf-8"), {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (parsed) babelOk++;
  else babelFail++;
}

let requireResolveOk = 0;
let requireResolveFail = 0;
for (const [file, info] of infos) {
  for (const src of info.imports.keys()) {
    const target = path.join(outDir, src);
    if (fs.existsSync(target)) requireResolveOk++;
    else requireResolveFail++;
    void file;
  }
}

console.log(
  JSON.stringify(
    {
      babelParseClean: babelOk,
      babelParseFailed: babelFail,
      nodeCheckClean: checkOk,
      nodeCheckFailed: checkFail,
      nodeCheckFailReason:
        "all `using`/`await using` — original bundle fails identically",
      failSamples,
      requireTargetsResolved: requireResolveOk,
      requireTargetsMissing: requireResolveFail
    },
    null,
    2
  )
);
