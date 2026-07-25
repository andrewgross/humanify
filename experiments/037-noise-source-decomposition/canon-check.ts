import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import { statementHash } from "../../src/split/statement-hash.js";
function stmts(code: string) {
  const a = parseSync(code, { sourceType: "unambiguous" })!;
  return a.program.body.map((s) => ({
    t: code.slice(s.start!, s.end!),
    h: statementHash(s)
  }));
}
const [f, g] = process.argv.slice(2);
const P = stmts(fs.readFileSync(f, "utf8"));
const F = stmts(fs.readFileSync(g, "utf8"));
const orig = (a: { t: string }[]) => a.map((s) => s.t).join("\n");
// canonical: sort by (hash,text) so identical statements line up regardless of bundle order
const canon = (a: { t: string; h: string }[]) =>
  [...a]
    .sort((x, y) => (x.h + x.t < y.h + y.t ? -1 : 1))
    .map((s) => s.t)
    .join("\n");
fs.writeFileSync("/tmp/p.orig.js", orig(P));
fs.writeFileSync("/tmp/f.orig.js", orig(F));
fs.writeFileSync("/tmp/p.canon.js", canon(P));
fs.writeFileSync("/tmp/f.canon.js", canon(F));
console.log(`prior stmts=${P.length} fresh stmts=${F.length}`);
