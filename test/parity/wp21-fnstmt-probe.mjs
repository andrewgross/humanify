// probe: the FUNCTION enclosing-statement rung's evidence on REAL dump texts —
// replicates enclosing-statement.ts getStatementParent/statementUsability for
// given FUNCTION node spans. Usage: node test/parity/wp21-fnstmt-probe.mjs <text.js> <spans.json>
//   spans.json: [[byteStart,byteEnd],...] (converted to babel char offsets here)
// Prints one JSON line per span: {start,end,stmt,lines,hash,reason}.
import { parseSync } from "@babel/core";
import traverseMod from "@babel/traverse";
import {
  hashStatementPath,
  statementUsability
} from "../../src/analysis/enclosing-statement.ts";
import fs from "node:fs";

const traverse = traverseMod.default ?? traverseMod;
const [textPath, spansPath] = process.argv.slice(2);
const code = fs.readFileSync(textPath, "utf8");
const byteSpans = JSON.parse(fs.readFileSync(spansPath, "utf8"));

// byte -> char offset conversion (the dump spans are BYTE offsets; babel's
// positions are char offsets — the non-ascii region past ~14M needs this).
const textBuf = Buffer.from(code, "utf8");
const conv = (byteOff) => {
  if (byteOff <= 0) return 0;
  const sliced = textBuf.slice(0, byteOff);
  return sliced.toString("utf8").length;
};

const ast = parseSync(code, {
  sourceType: "unambiguous",
  filename: textPath,
  configFile: false,
  babelrc: false
});

const byStart = new Map();
traverse(ast, {
  Function(p) {
    byStart.set(p.node.start, p);
  },
  ObjectProperty(p) {
    byStart.set(p.node.start, p);
  },
  ObjectExpression(p) {
    byStart.set(p.node.start, p);
  },
  CallExpression(p) {
    byStart.set(p.node.start, p);
  }
});

for (const [bStart, bEnd] of byteSpans) {
  const start = conv(bStart);
  const fnPath = byStart.get(start);
  const row = { start: bStart, end: bEnd };
  if (!fnPath) {
    row.missing = "no function path at converted start";
    console.log(JSON.stringify(row));
    continue;
  }
  const stmt = fnPath.getStatementParent();
  row.stmt = stmt ? [stmt.node.start, stmt.node.end] : null;
  row.sameNode = stmt ? stmt.node === fnPath.node : null;
  if (stmt) {
    const lines = code.slice(stmt.node.start, stmt.node.end).split("\n").length;
    row.lines = lines;
    row.reason = statementUsability(stmt.node).reason;
    row.hash = hashStatementPath(stmt, new Map());
  }
  console.log(JSON.stringify(row));
}
