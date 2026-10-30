import * as fs from "node:fs";
import * as parser from "@babel/parser";
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sbody: any[] = (ast.program.body[0] as any).expression.body.body;
// find the shipped ordinal of the region-0 statements by content marker
const markers = ["pluginDirNoMcp", 'trimStart().startsWith("{")', "--settings", "--dangerously-skip-permissions"];
for (const m of markers) {
  const hits: number[] = [];
  sbody.forEach((s, i) => {
    if (shipped.slice(s.start, s.end).includes(m)) hits.push(i);
  });
  console.log(`marker ${m}: shipped stmts ${hits.slice(0, 8).join(",")}`);
}
