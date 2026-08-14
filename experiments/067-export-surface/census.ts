/**
 * 067 Task 0 — the export-surface census.
 *
 *   npx tsx experiments/067-export-surface/census.ts census <srcDir>
 *   npx tsx experiments/067-export-surface/census.ts churn <priorSrc> <freshSrc> [label]
 *
 * Classes, read off the emitted shapes (verified on real files):
 *   alias-decl   `const X = require("...")` header lines
 *   export-def   `Object.defineProperty(module.exports, "Name", ...)` lines
 *   other        everything else
 *
 * `census` also computes DEAD exports — names no importer ever accesses —
 * under a conservative escape rule: if any file uses an alias of F as a
 * bare namespace value (passed around, spread, computed access), every
 * export of F is presumed live. The runnable scaffold (run.cjs,
 * runtime.js, index.js, vendor/) is included as an importer, so exports
 * only the scaffold touches stay live.
 *
 * `churn` prices the diff-line exposure per class with the system diff
 * (the same instrument the ledgers trust), and computes the STRICT
 * whole-pair ceilings (exp063 method) for the two consolidation
 * candidates:
 *   - dead-export pruning: changed export-def lines whose name is dead
 *     on their own side (they would not exist in a pruned tree);
 *   - forwarder bypass: changed alias-decl lines whose target is a pure
 *     forwarding stub (rewiring importers to the origin deletes the
 *     line class entirely).
 */
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

const [MODE, A, B, LABEL = ""] = process.argv.slice(2);
if (!MODE || !A || (MODE === "churn" && !B)) {
  console.error(
    "usage: census.ts census <srcDir> | census.ts churn <priorSrc> <freshSrc> [label]"
  );
  process.exit(1);
}

const ALIAS_RE = /^const ([A-Za-z_$][\w$]*) = require\("([^"]+)"\);?\s*$/;
const EXPORT_RE = /^Object\.defineProperty\(module\.exports, "([^"]+)"/;

function walk(dir: string, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith(".js") || e.name.endsWith(".cjs")) out.push(p);
  }
  return out;
}

interface FileFacts {
  aliasLines: number;
  exportLines: number;
  exports: string[];
  /** alias name -> resolved absolute target path */
  aliasTargets: Map<string, string>;
}

function readFacts(file: string): FileFacts {
  const text = fs.readFileSync(file, "utf8");
  const facts: FileFacts = {
    aliasLines: 0,
    exportLines: 0,
    exports: [],
    aliasTargets: new Map()
  };
  for (const line of text.split("\n")) {
    const a = ALIAS_RE.exec(line);
    if (a) {
      facts.aliasLines++;
      facts.aliasTargets.set(
        a[1],
        path.resolve(path.dirname(file), a[2])
      );
      continue;
    }
    const e = EXPORT_RE.exec(line);
    if (e) {
      facts.exportLines++;
      facts.exports.push(e[1]);
    }
  }
  return facts;
}

/** Member accesses + escape detection for every alias in a file. */
function aliasUses(
  file: string,
  aliases: Map<string, string>
): { members: Map<string, Set<string>>; escaped: Set<string> } {
  const text = fs.readFileSync(file, "utf8");
  const members = new Map<string, Set<string>>();
  const escaped = new Set<string>();
  for (const [alias, target] of aliases) {
    const uses = new Set<string>();
    const re = new RegExp(`\\b${alias.replace(/\$/g, "\\$")}\\b(\\.([A-Za-z_$][\\w$]*)|\\s*\\[)?`, "g");
    let m: RegExpExecArray | null = re.exec(text);
    let sawDecl = false;
    while (m !== null) {
      if (m[0].endsWith("[")) escaped.add(target);
      else if (m[2]) uses.add(m[2]);
      else if (!sawDecl) sawDecl = true; // the declaration itself
      else escaped.add(target); // bare namespace value use
      m = re.exec(text);
    }
    const prev = members.get(target) ?? new Set();
    for (const u of uses) prev.add(u);
    members.set(target, prev);
  }
  return { members, escaped };
}

/** Whole-tree liveness: file -> set of accessed member names; escaped files. */
function computeLiveness(root: string): {
  accessed: Map<string, Set<string>>;
  escaped: Set<string>;
} {
  const treeRoot = path.dirname(root); // include scaffold + vendor as importers
  const importers = walk(treeRoot);
  const accessed = new Map<string, Set<string>>();
  const escaped = new Set<string>();
  for (const f of importers) {
    const facts = readFacts(f);
    if (facts.aliasTargets.size === 0) continue;
    const { members, escaped: esc } = aliasUses(f, facts.aliasTargets);
    for (const e of esc) escaped.add(e);
    for (const [target, names] of members) {
      const prev = accessed.get(target) ?? new Set();
      for (const n of names) prev.add(n);
      accessed.set(target, prev);
    }
  }
  return { accessed, escaped };
}

function runCensus(src: string): void {
  const files = walk(src);
  let aliasLines = 0;
  let exportLines = 0;
  let totalLines = 0;
  const { accessed, escaped } = computeLiveness(src);
  let deadExports = 0;
  let deadInEscaped = 0;
  let exportedNames = 0;
  for (const f of files) {
    totalLines += fs.readFileSync(f, "utf8").split("\n").length;
    const facts = readFacts(f);
    aliasLines += facts.aliasLines;
    exportLines += facts.exportLines;
    exportedNames += facts.exports.length;
    const abs = path.resolve(f);
    const used = accessed.get(abs) ?? new Set();
    for (const name of facts.exports) {
      if (!used.has(name)) {
        if (escaped.has(abs)) deadInEscaped++;
        else deadExports++;
      }
    }
  }
  console.log(`census ${src}`);
  console.log(`  files                 ${files.length}`);
  console.log(`  total lines           ${totalLines}`);
  console.log(`  alias-decl lines      ${aliasLines}`);
  console.log(`  export-def lines      ${exportLines} (${exportedNames} names)`);
  console.log(`  DEAD exports (safe)   ${deadExports}`);
  console.log(`  dead-but-escaped      ${deadInEscaped} (alias escapes as value — presumed live)`);
  console.log(
    `ROW|census|${src}|${files.length}|${totalLines}|${aliasLines}|${exportLines}|${deadExports}|${deadInEscaped}`
  );
}

// ── churn mode ────────────────────────────────────────────────────────────
function classify(line: string): "alias" | "export" | "other" {
  if (ALIAS_RE.test(line)) return "alias";
  if (EXPORT_RE.test(line)) return "export";
  return "other";
}

function diffLines(a: string, b: string): { left: string[]; right: string[] } {
  let out = "";
  try {
    out = execFileSync("diff", [a, b], {
      encoding: "utf8",
      maxBuffer: 256 * 1024 * 1024
    });
  } catch (e) {
    const err = e as { status?: number; stdout?: string };
    if (err.status === 1 && err.stdout !== undefined) out = err.stdout;
    else throw e;
  }
  const left: string[] = [];
  const right: string[] = [];
  for (const line of out.split("\n")) {
    if (line.startsWith("< ")) left.push(line.slice(2));
    else if (line.startsWith("> ")) right.push(line.slice(2));
  }
  return { left, right };
}

function relFiles(root: string): Set<string> {
  return new Set(walk(root).map((f) => path.relative(root, f)));
}

function runChurn(prior: string, fresh: string): void {
  const priorLive = computeLiveness(prior);
  const freshLive = computeLiveness(fresh);
  const priorFiles = relFiles(prior);
  const tally = {
    alias: 0,
    export: 0,
    other: 0,
    deadExportChurn: 0,
    aliasNameOnlyPairs: 0
  };
  for (const rel of relFiles(fresh)) {
    if (!priorFiles.has(rel)) continue;
    const pAbs = path.join(prior, rel);
    const fAbs = path.join(fresh, rel);
    const { left, right } = diffLines(pAbs, fAbs);
    if (left.length === 0 && right.length === 0) continue;
    const sides: [string[], string, Map<string, Set<string>>, Set<string>][] = [
      [left, pAbs, priorLive.accessed, priorLive.escaped],
      [right, fAbs, freshLive.accessed, freshLive.escaped]
    ];
    for (const [lines, abs, accessed, escaped] of sides) {
      const used = accessed.get(path.resolve(abs)) ?? new Set();
      for (const line of lines) {
        const cls = classify(line);
        tally[cls]++;
        if (cls === "export") {
          const name = EXPORT_RE.exec(line)?.[1];
          if (
            name !== undefined &&
            !used.has(name) &&
            !escaped.has(path.resolve(abs))
          ) {
            tally.deadExportChurn++;
          }
        }
      }
    }
    // alias name-only pairs: same require target, different alias name
    const keyOf = (l: string) => ALIAS_RE.exec(l)?.[2];
    const lAlias = new Map(
      left.filter((l) => classify(l) === "alias").map((l) => [keyOf(l), l])
    );
    for (const r of right.filter((l) => classify(l) === "alias")) {
      const k = keyOf(r);
      if (k !== undefined && lAlias.has(k)) tally.aliasNameOnlyPairs++;
    }
  }
  const total = tally.alias + tally.export + tally.other;
  console.log(`churn ${LABEL || `${prior} -> ${fresh}`}`);
  console.log(`  diff lines total          ${total}`);
  console.log(`  alias-decl lines          ${tally.alias}`);
  console.log(`    same-target name-only   ${tally.aliasNameOnlyPairs} pairs`);
  console.log(`  export-def lines          ${tally.export}`);
  console.log(`    DEAD-export churn       ${tally.deadExportChurn} (strict pruning ceiling)`);
  console.log(`  other lines               ${tally.other}`);
  console.log(
    `ROW|churn|${LABEL}|${total}|${tally.alias}|${tally.aliasNameOnlyPairs}|${tally.export}|${tally.deadExportChurn}|${tally.other}`
  );
}

if (MODE === "census") runCensus(A);
else runChurn(A, B);
