/**
 * 079 — the INDIRECTION CENSUS: where does a name or an edge exist in the
 * code and fail to reach the matcher?
 *
 *   npx tsx --max-old-space-size=16384 \
 *     experiments/079-ambiguity/indirection-census.ts <treeRoot> [maxFiles]
 *
 * ## Why a census and not another hop
 *
 * exp079 task 1 found its hop by reading one failing case: a function
 * assigned to a variable, then used as a property value, whose key we
 * dropped. Following that one reference took zustand from 71%/83% to 100%
 * against ground truth. Andrew then asked the right question — what ELSE is
 * disconnected like that — and the honest answer is that reading cases does
 * not scale and guessing is how a lever gets built for three sites.
 *
 * So: enumerate the candidate patterns, count the SITES each has in a real
 * emitted tree, and report how many currently resolve. Same shape as the
 * switch census and the clone census. A pattern with three sites is not
 * worth code however elegant the hop.
 *
 * ## What "resolved" means per row
 *
 * For the name patterns it is literal: does `extractMemberKey` return
 * something for that function today? That is the production owner and the
 * only honest test. For the edge patterns there is no single owner to ask,
 * so those rows report SITES ONLY and say so — a count with no resolution
 * column is a question, not a finding.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildFunctionGraph } from "../../src/analysis/function-graph.js";
import { extractMemberKey } from "../../src/analysis/function-fingerprint.js";

const [ROOT, MAX = "400"] = process.argv.slice(2);
if (!ROOT) {
  console.error("usage: indirection-census.ts <treeRoot> [maxFiles]");
  process.exit(1);
}

function walkFiles(dir: string, out: string[], limit: number): void {
  if (out.length >= limit) return;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (out.length >= limit) return;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walkFiles(p, out, limit);
    else if (e.name.endsWith(".js")) out.push(p);
  }
}
const files: string[] = [];
walkFiles(path.join(ROOT, "src"), files, Number(MAX));

interface Row {
  sites: number;
  resolved: number | null;
  note: string;
}
const rows = new Map<string, Row>();
const bump = (name: string, resolved: boolean | null, note: string) => {
  const r = rows.get(name) ?? {
    sites: 0,
    resolved: resolved === null ? null : 0,
    note
  };
  r.sites++;
  if (resolved === true && r.resolved !== null) r.resolved++;
  rows.set(name, r);
};

let parsed = 0;
for (const file of files) {
  let ast: t.File | null = null;
  try {
    ast = parseSync(fs.readFileSync(file, "utf8"), {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    }) as t.File;
  } catch {
    continue;
  }
  if (!ast) continue;
  parsed++;

  // --- NAME patterns: ask the production owner directly ------------------
  for (const fn of buildFunctionGraph(ast, file)) {
    const parent = fn.path.parent;
    const node = fn.path.node;
    const key = extractMemberKey(fn);
    if (t.isObjectProperty(parent) && parent.value === node) {
      bump("fn written directly as a property value", key !== undefined, "");
    } else if (
      t.isVariableDeclarator(parent) &&
      parent.init === node &&
      t.isIdentifier(parent.id)
    ) {
      bump(
        "fn assigned to a variable (task 1 target)",
        key !== undefined,
        "resolved = the variable reaches a property"
      );
    } else if (t.isAssignmentExpression(parent) && parent.right === node) {
      bump("fn assigned to obj.member directly", key !== undefined, "");
    } else if (
      t.isCallExpression(parent) &&
      parent.arguments.includes(node as t.Expression)
    ) {
      bump(
        "fn passed straight to a call",
        key !== undefined,
        "a string arg beside it may name it"
      );
    } else {
      bump("fn somewhere else entirely", key !== undefined, "");
    }
  }

  // --- EDGE / alias patterns: sites only, no single owner to ask ---------
  const visit = (node: t.Node): void => {
    if (
      t.isVariableDeclarator(node) &&
      t.isIdentifier(node.id) &&
      t.isMemberExpression(node.init) &&
      t.isIdentifier(node.init.property)
    ) {
      bump("SITES ONLY: const g = obj.method", null, "call edge via a var");
    }
    if (t.isVariableDeclarator(node) && t.isObjectPattern(node.id)) {
      bump("SITES ONLY: const { a, b } = obj", null, "destructured members");
    }
    if (
      t.isVariableDeclarator(node) &&
      t.isIdentifier(node.id) &&
      t.isIdentifier(node.init)
    ) {
      bump("SITES ONLY: const a = b (alias)", null, "binding alias");
    }
    if (t.isExportSpecifier(node) && t.isIdentifier(node.exported)) {
      bump("SITES ONLY: export { f as name }", null, "export alias");
    }
    for (const k of t.VISITOR_KEYS[node.type] ?? []) {
      const c = (node as unknown as Record<string, unknown>)[k];
      for (const x of Array.isArray(c) ? c : [c]) {
        if (x && typeof x === "object" && "type" in x) visit(x as t.Node);
      }
    }
  };
  visit(ast.program);
}

console.log(`census over ${parsed} files of ${ROOT}\n`);
console.log(
  `${"pattern".padEnd(44)}${"sites".padStart(8)}${"resolved".padStart(10)}  note`
);
const sorted = [...rows.entries()].sort((a, b) => b[1].sites - a[1].sites);
for (const [name, r] of sorted) {
  const res =
    r.resolved === null
      ? "-"
      : `${r.resolved} (${((100 * r.resolved) / r.sites).toFixed(0)}%)`;
  console.log(
    `${name.padEnd(44)}${String(r.sites).padStart(8)}${res.padStart(10)}  ${r.note}`
  );
}
console.log(
  "\n'-' = no single production owner answers this, so sites only. A count\n" +
    "without a resolution column is a question, not a finding."
);
