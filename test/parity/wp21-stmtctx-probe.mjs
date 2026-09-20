// probe: the enclosing-statement rung's binding evidence on REAL dump texts —
// replicates fingerprint-index.ts bindingNeighborContextHash (:380) for given
// identifier spans, plus getStatementParent / getPrevSibling / getNextSibling.
// Usage: node test/parity/wp21-stmtctx-probe.mjs <text.js> <spans.json>
//   spans.json: [[start,end],...] (the row spans to resolve)
// Prints one JSON line per span: {start,end,stmt,prev,next,hash}.
import { parseSync } from "@babel/core";
import traverseMod from "@babel/traverse";
import { hashStatementPath } from "../../src/analysis/enclosing-statement.ts";
import fs from "node:fs";

const traverse = traverseMod.default ?? traverseMod;
const [textPath, spansPath] = process.argv.slice(2);
const code = fs.readFileSync(textPath, "utf8");
const spans = JSON.parse(fs.readFileSync(spansPath, "utf8"));

const ast = parseSync(code, {
  sourceType: "unambiguous",
  filename: textPath,
  configFile: false,
  babelrc: false
});

// identifier start -> path
const byStart = new Map();
traverse(ast, {
  Identifier(p) {
    byStart.set(p.node.start, p);
  }
});

const fmt = (n) => (n == null ? null : [n.start, n.end]);

for (const [start, end] of spans) {
  const idPath = byStart.get(start);
  const row = { start, end };
  const binding = idPath?.scope.getBinding(idPath.node.name);
  const bindingPath = binding?.path;
  const stmt = bindingPath?.getStatementParent();
  row.stmt = fmt(stmt?.node ?? null);
  const prev = stmt?.getPrevSibling() ?? null;
  const next = stmt?.getNextSibling() ?? null;
  row.prev = fmt(prev?.node ?? null);
  row.next = fmt(next?.node ?? null);
  const memo = new Map();
  const ph = hashStatementPath(prev ?? null, memo);
  const nh = hashStatementPath(next ?? null, memo);
  row.prevHash = ph;
  row.nextHash = nh;
  row.hash = ph === null && nh === null ? null : `${ph ?? "^"}|${nh ?? "$"}`;
  console.log(JSON.stringify(row));
}
