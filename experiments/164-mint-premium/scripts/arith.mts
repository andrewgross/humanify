/**
 * exp164: validate the exact era-ordinal arithmetic model.
 * era_ordinal(i) = sum over j<i of era_count(j), where era_count(j) = 0 for
 * a removed joined declaration, else the formatter's fragment count
 * (init-less group as one var first, then one per initialized declarator;
 * labeled stays whole). Greedy pointer alignment on content hashes,
 * tolerating rewritten statements (present, hash-mismatched) vs removed
 * ones (absent) by a one-step lookahead resolution.
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
const eraHash: string[] = body.map((s: any) => statementHash(s));

function fragHashes(s: any): string[] | null {
  if (s.type !== "VariableDeclaration" || s.declarations.length <= 1) return null;
  if (s.parentLabel) return null;
  const init = s.declarations.filter((d: any) => d.init);
  if (init.length === 0) return null;
  const empty = s.declarations.filter((d: any) => !d.init);
  const out: string[] = [];
  if (empty.length) out.push(statementHash({ type: "VariableDeclaration", kind: s.kind, declarations: empty }));
  for (const d of init) out.push(statementHash({ type: "VariableDeclaration", kind: s.kind, declarations: [d] }));
  return out;
}
function eraCount(s: any): number {
  const f = fragHashes(s);
  return f ? f.length : 1;
}

let p = 0, matched = 0, splitMatched = 0, treatedRemoved = 0, treatedRewritten = 0, gaps = 0;
const kindTally = new Map<string, number>();
for (let i = 0; i < rbody.length; i++) {
  const s: any = rbody[i];
  const h = statementHash(s);
  const f = fragHashes(s);
  let hit = false;
  if (eraHash[p] === h) { matched++; p += 1; hit = true; }
  else if (f && f.length > 1 && eraHash[p] === f[0]) {
    // fragments must match consecutively
    let ok = true;
    for (let k = 1; k < f.length; k++) if (eraHash[p + k] !== f[k]) { ok = false; break; }
    if (ok) { splitMatched++; p += f.length; hit = true; }
  }
  if (hit) continue;
  // resolve: rewritten-present (advance 1) vs removed (advance 0) — try to
  // re-sync on the NEXT raw statement
  const nh = i + 1 < rbody.length ? statementHash(rbody[i + 1]) : null;
  const nf = i + 1 < rbody.length ? fragHashes(rbody[i + 1]) : null;
  const nextMatches = (at: number) =>
    (nh && eraHash[at] === nh) || (nf && nf.length > 1 && eraHash[at] === nf[0]);
  if (nextMatches(p + 1)) { treatedRewritten++; p += 1; }
  else if (nextMatches(p)) { treatedRemoved++; p += 0; }
  else { gaps++; kindTally.set(s.type, (kindTally.get(s.type) ?? 0) + 1); p += eraCount(s); }
}
console.log(`container ${rbody.length}, era ${eraHash.length}, final pointer ${p}`);
console.log(`first-try ${matched}, split ${splitMatched}, treated rewritten ${treatedRewritten}, treated removed ${treatedRemoved}, unresolvable ${gaps}`);
for (const [k, n] of kindTally) console.log(`  unresolvable ${k}: ${n}`);
