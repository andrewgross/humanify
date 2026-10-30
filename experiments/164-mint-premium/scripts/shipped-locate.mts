/**
 * exp164: locate each shipped-text junk binding: its containing top-level
 * statement of the era's body, that statement's output file (split ledger
 * `order`), and the bucket (vendor package / src). argv: <label>
 */
import * as fs from "node:fs";
import * as parser from "@babel/parser";
import traverseMod from "@babel/traverse";

const LABEL = process.argv[2] ?? "exp161-names-hop";
const ROOT = `/tmp/eval-work/${LABEL}/2.1.216`;
const diag = JSON.parse(fs.readFileSync(`/tmp/eval-work/${LABEL}/2.1.216.diag.json`, "utf8"));
const mintedNames: string[] = diag.coverage.mintedCensus.names;
const minted = new Set(mintedNames);
const shipped = fs.readFileSync(`${ROOT}/.humanify/humanified.js`, "utf8");
const ledger = JSON.parse(fs.readFileSync(`${ROOT}/.humanify/split-ledger.json`, "utf8"));
const order: string[] = ledger.order;
console.log(`shipped ${shipped.length} bytes, order ${order.length} statements`);

const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
// the era body: program.body[0] is the ExpressionStatement wrapping the
// outer FunctionExpression; its body statements are the era's statements.
let body: any[] = [];
const p0: any = ast.program.body[0];
if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") {
  body = p0.expression.body.body;
} else if (p0.type === "FunctionDeclaration") {
  body = p0.body.body;
} else {
  body = ast.program.body;
}
console.log("top-level statements:", body.length);
const spans: { start: number; end: number; i: number }[] = body.map((s, i) => ({
  start: s.start,
  end: s.end,
  i,
}));
spans.sort((a, b) => a.start - b.start);

function stmtOf(start: number): number {
  // innermost statement whose range contains start
  let lo = 0;
  let hi = spans.length - 1;
  let best = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (spans[mid].start <= start) {
      best = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return best >= 0 && spans[best].end > start ? spans[best].i : -1;
}

const rows: { name: string; start: number; end: number; stmt: number; file: string }[] = [];
const seen = new Set<string>();
traverseMod.default(ast, {
  Scopable: {
    enter: (p: any) => {
      for (const [name, binding] of Object.entries(p.scope.bindings as Record<string, any>)) {
        if (minted.has(name)) {
          const key = `${name}@${binding.identifier.start}`;
          if (seen.has(key)) continue;
          seen.add(key);
          const stmt = stmtOf(binding.identifier.start);
          rows.push({
            name,
            start: binding.identifier.start,
            end: binding.identifier.end,
            stmt,
            file: stmt >= 0 ? (order[stmt] ?? "?") : "?",
          });
        }
      }
    },
  },
});

function pkgOf(rel: string): string {
  const parts = rel.split("/");
  if (parts[0] === "vendor") {
    if (parts.length === 1) return "vendor-root";
    return parts[1].startsWith("@") ? `${parts[1]}/${parts[2]}` : parts[1];
  }
  return "src";
}
const byPkg = new Map<string, number>();
const byFile = new Map<string, number>();
for (const r of rows) {
  const p = r.file === "?" ? "UNMAPPED" : pkgOf(r.file);
  byPkg.set(p, (byPkg.get(p) ?? 0) + 1);
  byFile.set(r.file, (byFile.get(r.file) ?? 0) + 1);
}
console.log("--- junk bindings by bucket ---");
for (const [p, c] of [...byPkg.entries()].sort((a, b) => b[1] - a[1])) console.log(`${c}\t${p}`);
console.log("--- top files ---");
for (const [f, c] of [...byFile.entries()].sort((a, b) => b[1] - a[1]).slice(0, 20))
  console.log(`${c}\t${f}`);
fs.writeFileSync(`/work/exp164-mint-census/shipped-located-${LABEL}.json`, JSON.stringify(rows));
