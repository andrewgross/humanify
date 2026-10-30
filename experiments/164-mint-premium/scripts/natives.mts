import * as fs from "node:fs";
import * as parser from "@babel/parser";
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sbody: any[] = (ast.program.body[0] as any).expression.body.body;
const hits: number[] = [];
sbody.forEach((s, i) => {
  if (shipped.slice(s.start, s.end).includes("image-processor.node")) hits.push(i);
});
console.log("shipped stmts mentioning image-processor.node:", hits);
for (const i of hits) {
  const s = sbody[i];
  console.log(`shipped ${i}:`, shipped.slice(s.start, Math.min(s.start + 150, s.end)).replace(/\n/g, " "));
}
// the native require wrappers: 5 of them in raw. count statements mentioning each .node
for (const n of ["computer-use-swift.node", "url-handler.node"]) {
  const h: number[] = [];
  sbody.forEach((s, i) => { if (shipped.slice(s.start, s.end).includes(n)) h.push(i); });
  console.log(n, "-> shipped stmts", h);
}
