/**
 * exp164 F1 simulation: a region raw statement -> era statement match key
 * of (statement kind, top-level declared binding names). Measured over the
 * whole container: how many raw statements have a UNIQUE era match, how
 * many collide, how many have none — per the precision rule, only unique
 * matches would be visited.
 */
import * as fs from "node:fs";
import * as parser from "@babel/parser";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";

const bundlePath = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const raw = fs.readFileSync(bundlePath, "utf8");
const rast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rbody: any[] = (rast.program.body[0] as any).expression.body.body;

const shipped = fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body: any[] = p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression" ? p0.expression.body.body : ast.program.body;

// the era statements' keys
function keyOf(s: any): string | null {
  const t = s.type;
  if (t === "VariableDeclaration") {
    const names = s.declarations.map((d: any) => {
      const id = d.id;
      if (id.type === "Identifier") return id.name;
      if (id.type === "ObjectPattern" || id.type === "ArrayPattern") return "{destructured}";
      return "?";
    });
    return `var:${s.kind}:${names.join(",")}`;
  }
  if (t === "FunctionDeclaration") return s.id ? `fn:${s.id.name}:${s.params.length}` : null;
  if (t === "ClassDeclaration") return s.id ? `class:${s.id.name}` : null;
  return null;
}
const eraKeys = new Map<string, number[]>();
body.forEach((s: any, i: number) => {
  const k = keyOf(s);
  if (!k) return;
  const a = eraKeys.get(k) ?? [];
  a.push(i);
  eraKeys.set(k, a);
});
console.log("era statements", body.length, "with keys", [...eraKeys.values()].flat().length, "distinct keys", eraKeys.size);

// the dump's regions' raw spans -> the bundle statements
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const regions = dumpDiag.eraRestoration.regions as { statements: number[]; raw_spans: [number, number][] }[];
const rawByStart = new Map<number, { i: number; s: any }>();
rbody.forEach((s: any, i: number) => rawByStart.set(s.start, { i, s }));
const spanToRaw = (sp: [number, number]) => rawByStart.get(sp[0]);

let unique = 0, collides = 0, noKey = 0, noEra = 0, total = 0;
const examples: string[] = [];
for (const r of regions) {
  for (const sp of r.raw_spans) {
    total++;
    const row = spanToRaw(sp);
    if (!row) { noKey++; continue; }
    const k = keyOf(row.s);
    if (!k) { noKey++; if (examples.length < 8) examples.push(`no-key: ${raw.slice(row.s.start, row.s.end).slice(0, 80)}`); continue; }
    const hits = eraKeys.get(k);
    if (!hits || hits.length === 0) { noEra++; continue; }
    if (hits.length === 1) unique++;
    else { collides++; if (examples.length < 16) examples.push(`collide x${hits.length}: ${k}  raw=${raw.slice(row.s.start, row.s.end).slice(0, 60)}`); }
  }
}
console.log(`\nregion raw statements ${total}: unique ${unique}, collide ${collides}, noEra ${noEra}, noKey ${noKey}`);
console.log("distinct raw keys vs era:");
for (const e of examples) console.log("  " + e);

// ALSO: does the unique match agree with the content hash where both exist?
const eraHash = new Map<number, string>();
body.forEach((s: any, i: number) => eraHash.set(i, statementHash(s)));
const rawHash = new Map<number, string>();
rbody.forEach((s: any, i: number) => rawHash.set(i, statementHash(s)));
// era_of's assignment for comparison
let agreeOld = 0, agreeNew = 0, checked = 0;
const removedBefore = new Map<number, number>();
let removed = 0;
rbody.forEach((s: any, i: number) => { removedBefore.set(i, removed); /* removed set unknown offline; approximate by span membership in regions? */ });
// proper check: for region entries where BOTH the old ordinal's statement and the matched statement exist
for (const r of regions) {
  r.statements.forEach((ord, idx) => {
    const sp = r.raw_spans[idx];
    if (!sp) return;
    const row = spanToRaw(sp);
    if (!row) return;
    const k = keyOf(row.s);
    if (!k) return;
    const hits = eraKeys.get(k);
    if (!hits || hits.length !== 1) return;
    checked++;
    const rawH = rawHash.get(row.i);
    const oldH = eraHash.get(ord);
    const newH = eraHash.get(hits[0]);
    // raw/era hash equality only when unrewritten; count agreement either way
    if (rawH && oldH && rawH === oldH) agreeOld++;
    if (rawH && newH && rawH === newH) agreeNew++;
    if (oldH && newH && oldH === newH) agreeNew++;
  });
}
console.log(`\nchecked ${checked}: content-hash agrees with OLD ordinal ${agreeOld}, with NEW key match ${agreeNew}`);
