import * as fs from "node:fs";
import * as parser from "@babel/parser";
import traverseMod from "@babel/traverse";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";

const RAW = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const dump = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dump.eraRestoration;
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");

const rawAst = parser.parse(fs.readFileSync(RAW, "utf8"), { sourceType: "unambiguous", errorRecovery: true });
const shipAst = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const body = (shipAst.program.body[0] as any).expression.body.body;
console.log("era statements (shipped):", body.length, "dump bodyLen", er.bodyLen);

const hashOf = (s: any) => statementHash(s);
// raw container statements by span
const rawBody = (rawAst.program.body[0] as any).expression.body.body;
console.log("raw container statements:", rawBody.length);
const rawBySpan = new Map<string, string>();
rawBody.forEach((s: any) => rawBySpan.set(`${s.start}:${s.end}`, hashOf(s)));
// era statements: ordinal -> hash
const eraHash: string[] = body.map(hashOf);

// per region: the pipeline's statements[k] claim vs content truth
let agree = 0, disagree = 0, missing = 0, total = 0;
const drifts: { region: number; claimed: number; truth: number }[] = [];
const regions = er.regions as { statements: number[]; raw_spans: [number, number][]; claims: (string | null)[] }[];
for (const r of regions) {
  r.raw_spans.forEach(([s, e], j) => {
    const rh = rawBySpan.get(`${s}:${e}`);
    const claimed = r.statements[j];
    const eh = eraHash[claimed];
    total++;
    if (rh === undefined || eh === undefined) { missing++; return; }
    if (rh === eh) agree++;
    else {
      disagree++;
      // find the era ordinal whose hash matches the raw statement's
      const truth = eraHash.findIndex((h, i) => h === rh && Math.abs(i - claimed) < 8000);
      if (truth >= 0 && drifts.length < 400) drifts.push({ region: regions.indexOf(r), claimed, truth });
    }
  });
}
console.log(`region statements: total ${total}, content-agree ${agree}, disagree ${disagree}, span-missing ${missing}`);
// drift = truth - claimed
const d = drifts.map((x) => x.truth - x.claimed);
if (d.length) {
  console.log(`drift (truth - claimed) over ${d.length} misaligned: min ${Math.min(...d)}, max ${Math.max(...d)}`);
  console.log("first 12:", d.slice(0, 12).join(","));
  console.log("claimed vs truth sample:", drifts.slice(0, 8).map((x) => `${x.claimed}->${x.truth}`).join(" "));
}
fs.writeFileSync("/tmp/exp164-region-align.json", JSON.stringify(drifts));
