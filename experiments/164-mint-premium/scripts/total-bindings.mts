/** exp164 probe: total bindings per output tree bucket. argv: <label> <tree> */
import * as fs from "node:fs";
import * as path from "node:path";
import * as parser from "@babel/parser";
import traverseMod from "@babel/traverse";

const LABEL = process.argv[2] ?? "exp161-names-hop";
const TREE = process.argv[3] ?? "2.1.216";
const ROOT = `/tmp/eval-work/${LABEL}/${TREE}`;
function walk(dir: string, out: string[]): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name === ".humanify" || e.name === "node_modules") continue;
      walk(p, out);
    } else if (/\.(js|cjs)$/.test(e.name)) out.push(p);
  }
  return out;
}
let total = 0;
const byTop = new Map<string, number>();
for (const f of walk(ROOT, [])) {
  const rel = path.relative(ROOT, f);
  const top = rel.split("/")[0];
  let ast: parser.ParseResult<import("@babel/types").File>;
  try {
    ast = parser.parse(fs.readFileSync(f, "utf8"), { sourceType: "unambiguous", errorRecovery: true });
  } catch {
    continue;
  }
  let n = 0;
  traverseMod.default(ast, {
    Scopable: (p: { scope: { bindings: Record<string, unknown> } }) => {
      n += Object.keys(p.scope.bindings).length;
    },
  });
  total += n;
  byTop.set(top, (byTop.get(top) ?? 0) + n);
}
console.log("total bindings in tree:", total);
for (const [k, v] of [...byTop.entries()].sort((a, b) => b[1] - a[1])) console.log(v, k);
