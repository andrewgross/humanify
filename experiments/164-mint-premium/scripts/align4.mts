import * as fs from "node:fs";
import * as parser from "@babel/parser";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";

const RAW = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const dump = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dump.eraRestoration;
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");

const rawAst = parser.parse(fs.readFileSync(RAW, "utf8"), { sourceType: "unambiguous", errorRecovery: true });
const shipAst = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const rawBody = (rawAst.program.body[0] as any).expression.body.body;
const body = (shipAst.program.body[0] as any).expression.body.body;
const rawBySpan = new Map<string, any>();
rawBody.forEach((s: any) => rawBySpan.set(`${s.start}:${s.end}`, s));
const eraHash = body.map(statementHash);
const hashAt = new Map<string, number[]>();
eraHash.forEach((h: string, i: number) => { const a = hashAt.get(h) ?? []; a.push(i); hashAt.set(h, a); });

// captured prints' statement ordinals
const capturedStmts = new Set<number>();
for (const s of er.sites as any[]) for (const p of s.prints) if (p.captured) capturedStmts.add(s.statement);

// per region statement: content-agree?
let agree = 0, disagree = 0;
let agreeCaptured = 0, disagreeCaptured = 0;
const regions = er.regions as { statements: number[]; raw_spans: [number, number][] }[];
for (const r of regions) {
  r.raw_spans.forEach(([s, e], j) => {
    const rs = rawBySpan.get(`${s}:${e}`);
    if (!rs) return;
    const claimed = r.statements[j];
    const rh = statementHash(rs);
    const eh = eraHash[claimed];
    const isCaptured = capturedStmts.has(claimed);
    if (rh === eh) { agree++; if (isCaptured) agreeCaptured++; }
    else { disagree++; if (isCaptured) disagreeCaptured++; }
  });
}
console.log(`content-agree ${agree} (captured among them: ${agreeCaptured}), disagree ${disagree} (captured among them: ${disagreeCaptured})`);
console.log(`captured statements total ${capturedStmts.size}`);
// are the captured ordinals' hashes equal to their raw twins? cross: for captured
// statements, does ANY raw statement share the era hash AND sit in a region?
