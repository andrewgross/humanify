import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const rast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rbody: any[] = (rast.program.body[0] as any).expression.body.body;
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sbody: any[] = (ast.program.body[0] as any).expression.body.body;
for (let i = 0; i <= 16; i++) {
  const rs = rbody[i], ss = sbody[i];
  console.log(`raw ${i}: ${raw.slice(rs.start, Math.min(rs.start + 70, rs.end))}`);
  console.log(`shp ${i}: ${shipped.slice(ss.start, Math.min(ss.start + 70, ss.end)).replace(/\n/g, " ")}`);
  console.log();
}
