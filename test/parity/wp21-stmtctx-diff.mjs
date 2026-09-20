// probe: TS-side statement contexts for EVERY function row of the Rust
// `humanify stmtctx` dump — the per-row diff that localizes the
// resolutionStats enclosingStmtAbstain divergence without a full matches run.
//
// Usage: npx tsx test/parity/wp21-stmtctx-diff.mjs <text.js> <rust-stmtctx.jsonl> <out.jsonl>
//
// The Rust dump rows carry BYTE offsets (oxc spans); babel's positions are
// UTF-16 CHAR offsets. One pass builds the char->byte table (byteAtChar);
// byte->char is a binary search over it. Rows are emitted with the statement
// span converted BACK to bytes so the diff is byte-vs-byte.
import { parseSync } from "@babel/core";
import traverseMod from "@babel/traverse";
import fs from "node:fs";

const traverse = traverseMod.default ?? traverseMod;
const [textPath, rustPath, outPath] = process.argv.slice(2);
const code = fs.readFileSync(textPath, "utf8");

// ── byte <-> char mapping, one pass ──────────────────────────────────────
// byteAtChar[i] = byte offset of UTF-16 code unit i; byteAtChar[len] = total.
const byteAtChar = new Int32Array(code.length + 1);
{
  let b = 0;
  for (let i = 0; i < code.length; i++) {
    byteAtChar[i] = b;
    const c = code.charCodeAt(i);
    if (c < 0x80) b += 1;
    else if (c >= 0xd800 && c <= 0xdbff) {
      b += 4;
      i++; // consume the low surrogate as part of the pair
    } else if (c >= 0x800) b += 3;
    else b += 2;
  }
  byteAtChar[code.length] = b;
}
const charAtByte = (byte) => {
  // binary search: largest i with byteAtChar[i] <= byte
  let lo = 0;
  let hi = code.length;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (byteAtChar[mid] <= byte) lo = mid;
    else hi = mid - 1;
  }
  return lo;
};

const ast = parseSync(code, {
  sourceType: "unambiguous",
  filename: textPath,
  configFile: false,
  babelrc: false
});

// char start -> function path (the graph rows' spans are unique starts)
const fnByStart = new Map();
traverse(ast, {
  Function(p) {
    fnByStart.set(p.node.start, p);
  }
});

const byteSpan = (node) => [byteAtChar[node.start], byteAtChar[node.end]];

const out = [];
for (const line of fs.readFileSync(rustPath, "utf8").split("\n")) {
  if (!line.trim()) continue;
  const row = JSON.parse(line);
  if (row.kind !== "function") continue;
  const charStart = charAtByte(row.start);
  const fnPath = fnByStart.get(charStart);
  const o = { start: row.start, end: row.end, sessionId: row.sessionId };
  if (!fnPath) {
    o.missing = "no function path at char start";
    out.push(o);
    continue;
  }
  const stmt = fnPath.getStatementParent();
  o.stmt = stmt ? byteSpan(stmt.node) : null;
  o.isOwn = stmt ? stmt.node === fnPath.node : null;
  if (stmt) {
    const [_s, _e] = o.stmt;
    o.lines = code.slice(byteAtChar[stmt.node.start], byteAtChar[stmt.node.end]).split("\n").length;
    o.stmtType = stmt.node.type;
  }
  out.push(o);
}
fs.writeFileSync(
  outPath,
  `${out.map((r) => JSON.stringify(r)).join("\n")}\n`
);
console.log(`stmtctx-diff: ${out.length} function row(s) -> ${outPath}`);
