import * as fs from "node:fs";
import * as parser from "@babel/parser";
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body = p0.expression.body.body;
console.log("shipped body", body.length, "bodySpans", er.bodySpans.length);
let exact = 0, near = 0, off = 0;
for (let i = 0; i < body.length; i++) {
  const [s, e] = er.bodySpans[i];
  const sp = body[i];
  if (sp.start === s && sp.end === e) exact++;
  else if (Math.abs((sp.end - sp.start) - (e - s)) < 40) near++;
  else off++;
}
console.log("span exact", exact, "len-near", near, "off", off);
// how many joined var declarations survive in the shipped body?
let joined = 0, extra = 0;
for (const st of body) if (st.type === "VariableDeclaration" && st.declarations.length > 1) { joined++; extra += st.declarations.length - 1; }
console.log("shipped joined vars", joined, "extra declarators", extra);
// raw joined count was 2919/14804
// first 5 spans vs first 5 shipped statements
for (let i = 0; i < 5; i++) {
  const [s, e] = er.bodySpans[i];
  const sp = body[i];
  console.log(i, "era", s, e, e - s, "| shipped", sp.start, sp.end, sp.end - sp.start, JSON.stringify(shipped.slice(sp.start, Math.min(sp.start + 60, sp.end))));
}
