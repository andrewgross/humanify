import * as fs from "node:fs";
import * as parser from "@babel/parser";
const LABEL = "exp161-names-hop";
const ROOT = `/tmp/eval-work/${LABEL}/2.1.216`;
const shipped = fs.readFileSync(`${ROOT}/.humanify/humanified.js`, "utf8");
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
let body: any[] = ast.program.body as any[];
const p0: any = ast.program.body[0];
if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") body = p0.expression.body.body;
for (const k of [1129, 30238, 30280, 6472]) {
  const s = body[k];
  const text = shipped.slice(s.start, s.start + 220).replace(/\s+/g, "");
  console.log(`stmt ${k} type=${s.type} span=${s.start}-${s.end}`);
  console.log("  shipped head:", text.slice(0, 150));
  // search raw (normalized) for shrinking windows
  const norm = (t: string) => t.replace(/\s+/g, "");
  const nraw = norm(raw);
  let found = -1;
  for (const w of [150, 100, 60, 40, 25]) {
    const win = text.slice(30, 30 + w);
    const i = nraw.indexOf(win);
    if (i >= 0) { found = i; console.log(`  raw hit @win${w}: offset ${i} (norm len ${nraw.length}), context:`, nraw.slice(Math.max(0, i - 60), i + 40)); break; }
  }
  if (found < 0) console.log("  NO raw hit");
}
