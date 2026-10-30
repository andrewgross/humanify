import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const rast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rbody: any[] = (rast.program.body[0] as any).expression.body.body;
// shape = tokens minus identifiers, whitespace normalized
function tokens(src: string, node: any): string {
  const s = src.slice(node.start, node.end);
  // keep string literals, numbers, punctuation; drop identifier-ish words
  return s.replace(/(['"`])(?:\\.|(?!\1)[^\\])*\1/g, "'S'")
    .replace(/\b[A-Za-z_$][A-Za-z0-9_$]*\b/g, "I")
    .replace(/\s+/g, "");
}
const rShapes = rbody.map((s) => tokens(raw, s));
const shapeOrd = new Map<string, number[]>();
rShapes.forEach((sh, i) => {
  const l = shapeOrd.get(sh) ?? [];
  l.push(i);
  shapeOrd.set(sh, l);
});
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sbody: any[] = (ast.program.body[0] as any).expression.body.body;
function probe(i: number) {
  const sh = tokens(shipped, sbody[i]);
  const cands = shapeOrd.get(sh) ?? [];
  console.log(`shipped ${i} -> raw candidates ${cands.length ? cands.join(",") : "NONE"} (len ${sh.length}) ${shipped.slice(sbody[i].start, sbody[i].start + 80).replace(/\n/g, " ")}`);
}
for (const i of [30880, 30885, 30889, 30890, 30900, 31000, 31500, 32000, 33000, 34000, 35000, 35367, 35900]) probe(i);
