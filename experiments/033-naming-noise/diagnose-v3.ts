/**
 * B2 measurement: "distrust generic/minted names in the name-vote tier".
 * Uses the cached oracle (from v2) — fast, no match. Replicates
 * stable-split.ts isRejectedStem VERBATIM so the demote gate is exactly what a
 * production change would use.
 *
 * Measures, 215->216:
 *   - renamed-binding relocations: baseline vs demote (confident-wrong teleports removed)
 *   - same-named generic stability regression: same-named generic bindings that
 *     were file-stable under baseline (all-same pinned) but move under demote.
 *
 * Run: NODE_OPTIONS=--max-old-space-size=8192 npx tsx <this> [priorVer] [newVer]
 */
import * as fs from "node:fs";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import {
  STATEMENT_HASH_VERSION,
  statementHash
} from "../../src/split/statement-hash.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

const VERSIONS = "/Users/andrewgross/Development/unpacked-claude-code/versions";
const priorVer = process.argv[2] ?? "2.1.215";
const newVer = process.argv[3] ?? "2.1.216";
// Oracle cache written by diagnose-v2.ts (run that first). Portable temp path.
const CACHE = `/tmp/humanify-split-oracle-${priorVer}-${newVer}.json`;

const humanified = (v: string) =>
  fs.readFileSync(
    `${VERSIONS}/claude-code-${v}/.humanify/humanified.js`,
    "utf8"
  );
const ledger = (v: string): StableSplitLedger =>
  JSON.parse(
    fs.readFileSync(
      `${VERSIONS}/claude-code-${v}/.humanify/split-ledger.json`,
      "utf8"
    )
  );

// ---- VERBATIM isRejectedStem from stable-split.ts ----
const BAD_STEM =
  /^(no[-_]?ops?\w*|doNothing\w*|silent[-_]?noops?\w*|empty(function|callback|operation|handler)s?\d*|idle[-_]?operation\d*|initializeModule\d+|placeholder\w*|_+\d*|reactLib\d+|\w+Val\d*)$/i;
const KNOWN_NUMBER_TOKENS = new Set([
  "8",
  "16",
  "32",
  "64",
  "128",
  "256",
  "512",
  "1024"
]);
function hasMintedNumber(name: string): boolean {
  const runs = name.match(/\d+/g);
  if (!runs) return false;
  return runs.some((run) => run.length >= 2 && !KNOWN_NUMBER_TOKENS.has(run));
}
const LEADING_STOPWORD = /^(and|or|but|nor|the|an|a)(?=[A-Z0-9]|$)/;
function isRejectedStem(name: string): boolean {
  return (
    BAD_STEM.test(name) || hasMintedNumber(name) || LEADING_STOPWORD.test(name)
  );
}

// ---- tier logic + demote flag ----
function declaredNames(stmt: t.Statement): string[] {
  return Object.keys(t.getBindingIdentifiers(stmt, false));
}
function countOccurrences(body: t.Statement[]): Map<string, number> {
  const c = new Map<string, number>();
  for (const s of body)
    for (const n of declaredNames(s)) c.set(n, (c.get(n) ?? 0) + 1);
  return c;
}
function hashTier(
  cur: string[],
  prior: StableSplitLedger
): Array<string | undefined> {
  if (
    !prior.hashes ||
    prior.hashVersion !== STATEMENT_HASH_VERSION ||
    prior.hashes.length !== prior.order.length
  )
    return new Array(cur.length);
  const pf = new Map<string, string[]>();
  for (let i = 0; i < prior.hashes.length; i++) {
    const l = pf.get(prior.hashes[i]) ?? [];
    l.push(prior.order[i]);
    pf.set(prior.hashes[i], l);
  }
  const counts = new Map<string, number>();
  for (const h of cur) counts.set(h, (counts.get(h) ?? 0) + 1);
  return cur.map((h) => {
    const f = pf.get(h);
    if (!f || f.length !== counts.get(h)) return undefined;
    return f.every((x) => x === f[0]) ? f[0] : undefined;
  });
}
function assign(
  body: t.Statement[],
  prior: StableSplitLedger,
  hashes: string[],
  demoteGeneric: boolean
): string[] {
  const priorNames = new Map(Object.entries(prior.nameToFiles));
  const newCounts = countOccurrences(body);
  const viaHash = hashTier(hashes, prior);
  const seen = new Map<string, number>();
  const a: string[] = new Array(body.length);
  for (let i = 0; i < body.length; i++) {
    const votes = new Set<string>();
    for (const name of declaredNames(body[i])) {
      const ord = seen.get(name) ?? 0;
      seen.set(name, ord + 1);
      // demote: a generic/minted name casts NO vote.
      if (demoteGeneric && isRejectedStem(name)) continue;
      const files = priorNames.get(name);
      if (!files || files.length === 0) continue;
      if (files.every((f) => f === files[0])) votes.add(files[0]);
      else if (newCounts.get(name) === files.length && ord < files.length)
        votes.add(files[ord]);
    }
    if (viaHash[i] !== undefined) {
      a[i] = viaHash[i] as string;
      continue;
    }
    if (votes.size === 1) {
      a[i] = [...votes][0];
      continue;
    }
    a[i] = i > 0 ? a[i - 1] : prior.files[0];
  }
  return a;
}

function main() {
  const priorLedger = ledger(priorVer);
  const newCode = humanified(newVer);
  const ast = parseFileAst(newCode);
  if (!ast) throw new Error("parse");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("wrapper");
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) throw new Error("block");
  const body = bodyNode.body;
  const hashes = body.map(statementHash);

  const oracle = new Map<string, string>(
    JSON.parse(fs.readFileSync(CACHE, "utf8"))
  );
  const base = assign(body, priorLedger, hashes, false);
  const demote = assign(body, priorLedger, hashes, true);

  const firstDecl = new Map<string, number>();
  for (let i = 0; i < body.length; i++)
    for (const n of declaredNames(body[i]))
      if (!firstDecl.has(n)) firstDecl.set(n, i);

  // 1. Renamed-binding relocations: baseline vs demote.
  const reloc = (a: string[]) => {
    let r = 0;
    for (const [nn, p] of oracle) {
      const fp = priorLedger.nameToFiles[p]?.[0];
      if (fp === undefined) continue;
      const i = firstDecl.get(nn);
      if (i === undefined) continue;
      if (a[i] !== fp) r++;
    }
    return r;
  };
  const rb = reloc(base);
  const rd = reloc(demote);
  console.log(`\nB2 (distrust generic votes) ${priorVer}->${newVer}`);
  console.log(
    `renamed-binding relocations: baseline ${rb} -> demote ${rd}  (removed ${rb - rd})`
  );

  // 2. Same-named generic bindings: stability regression.
  //    A generic name present in BOTH ledgers, file-stable under baseline,
  //    that MOVES under demote (its all-same vote was suppressed).
  const b = ledger(newVer); // shipped 216 ledger for same-name identity
  let sameNamedGeneric = 0;
  let stableBaseline = 0;
  let regressed = 0;
  for (const n of Object.keys(b.nameToFiles)) {
    if (!isRejectedStem(n)) continue;
    if (!priorLedger.nameToFiles[n]) continue; // not same-named
    const i = firstDecl.get(n);
    if (i === undefined) continue;
    sameNamedGeneric++;
    const fp = priorLedger.nameToFiles[n][0];
    const wasStable = base[i] === fp;
    if (wasStable) stableBaseline++;
    if (wasStable && demote[i] !== fp) regressed++;
  }
  console.log(
    `same-named generic bindings: ${sameNamedGeneric}, baseline-stable ${stableBaseline}, REGRESSED under demote ${regressed}`
  );
  console.log(`net (removed teleports - regressions): ${rb - rd - regressed}`);

  // 3. Total statement moves demote vs baseline (churn footprint).
  let moved = 0;
  for (let i = 0; i < body.length; i++) if (base[i] !== demote[i]) moved++;
  console.log(`total statements reassigned demote vs baseline: ${moved}`);
}
main();
