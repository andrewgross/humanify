import * as fs from "node:fs";
import * as parser from "@babel/parser";
const junk: { name: string; stmt: number; file: string }[] = JSON.parse(fs.readFileSync("/work/exp164-mint-census/shipped-located-exp161-names-hop.json", "utf8"));
const high = junk.filter((j) => j.stmt > 30888);
const byPkg = new Map<string, number>();
for (const j of high) {
  const parts = j.file.split("/");
  const p = parts[0] === "vendor" ? (parts[1].startsWith("@") ? `${parts[1]}/${parts[2]}` : parts[1]) : "src";
  byPkg.set(p, (byPkg.get(p) ?? 0) + 1);
}
console.log("junk with stmt > 30888:", high.length);
for (const [p, c] of [...byPkg.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10)) console.log(c, p);
console.log("sample:", high.slice(0, 5).map((j) => `${j.name} stmt ${j.stmt} ${j.file}`));
// shipped statement 35367 content
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const body: any[] = (ast.program.body[0] as any).expression.body.body;
const s = body[35367];
console.log("shipped stmt 35367:", shipped.slice(s.start, s.start + 200));
// and the shipped text's tail structure: last 3 statements
for (const i of [35900, 35901, 35902]) {
  const st = body[i];
  console.log(`shipped stmt ${i}:`, shipped.slice(st.start, st.start + 120));
}
