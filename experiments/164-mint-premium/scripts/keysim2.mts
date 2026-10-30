import * as fs from "node:fs";
import * as parser from "@babel/parser";
const bundlePath = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const raw = fs.readFileSync(bundlePath, "utf8");
const rast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rbody: any[] = (rast.program.body[0] as any).expression.body.body;
const shipped = fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body: any[] = p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression" ? p0.expression.body.body : ast.program.body;
console.log("bundle container span:", rbody[0].start, "-", rbody[rbody.length - 1].end);
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const regions = dumpDiag.eraRestoration.regions as { statements: number[]; raw_spans: [number, number][] }[];
const zod = regions.find((r) => r.raw_spans.some(([s]) => s === 244924));
console.log("zod region raw spans:", JSON.stringify(zod?.raw_spans));
function keyOf(s: any): string | null {
  const t = s.type;
  if (t === "VariableDeclaration") return `var:${s.kind}:${s.declarations.map((d: any) => (d.id.type === "Identifier" ? d.id.name : "{d}")).join(",")}`;
  if (t === "FunctionDeclaration") return s.id ? `fn:${s.id.name}:${s.params.length}` : null;
  if (t === "ClassDeclaration") return s.id ? `class:${s.id.name}` : null;
  return null;
}
for (const [s, e] of zod?.raw_spans ?? []) {
  const row = rbody.find((x) => x.start === s);
  console.log(`raw ${s}:${e} len=${e - s} found=${!!row} key=${row ? keyOf(row) : "-"} src=${raw.slice(s, s + 70).replace(/\n/g, " ")}`);
}
// where does a statement starting at 244924 sit in the container?
const idx = rbody.findIndex((x) => x.start <= 244924 && 244924 < x.end);
console.log("244924 falls inside container statement", idx, "whose start =", rbody[idx]?.start, "src=", raw.slice(rbody[idx]?.start, rbody[idx].start + 80).replace(/\n/g, " "));
// era-side: find var skn
const ek = body.findIndex((s: any) => keyOf(s) === "var:var:skn");
console.log("era key var:var:skn at ordinal", ek);
