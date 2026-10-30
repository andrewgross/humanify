import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const ast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const body: any[] = (ast.program.body[0] as any).expression.body.body;
for (let i = 0; i <= 17; i++) {
  const s = body[i];
  console.log(i, `(${s.start},${s.end})`, JSON.stringify(raw.slice(s.start, Math.min(s.start + 90, s.end))));
}
