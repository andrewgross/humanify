/**
 * Classify unique-twin residual noise by flip kind, per statement:
 *  - privateOnly: texts equal after masking #privateNames (drift in private
 *    class members only — bridgeable with a PrivateName transfer)
 *  - mintDrift:  differing word-tokens are all minified-looking (unbound
 *    free-id / leftover-mint spelling drift; mostly unfixable by rename)
 *  - descriptive: at least one differing token is a real name (echo of a
 *    flipped root binding elsewhere, or an unbridged own binding)
 *
 *   npx tsx residual-classify.ts <fresh.js> <prior.js>
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

const maskPrivate = (t: string) => t.replace(/#[A-Za-z_$][\w$]*/g, "#_");
const words = (t: string) => t.match(/[A-Za-z_$][\w$]*/g) ?? [];
const isMintish = (w: string) =>
  w.length <= 4 || /^[A-Za-z]?[\w]?\d+_?$/.test(w) || /[_$]\d*$/.test(w);

function diffTokens(a: string, b: string): Set<string> {
  const ca = new Map<string, number>();
  for (const w of words(a)) ca.set(w, (ca.get(w) ?? 0) + 1);
  const cb = new Map<string, number>();
  for (const w of words(b)) cb.set(w, (cb.get(w) ?? 0) + 1);
  const out = new Set<string>();
  for (const [w, n] of ca) if ((cb.get(w) ?? 0) !== n) out.add(w);
  for (const [w, n] of cb) if ((ca.get(w) ?? 0) !== n) out.add(w);
  return out;
}

const [freshPath, priorPath] = process.argv.slice(2);
const fresh = statementsOf(fs.readFileSync(freshPath, "utf8"));
const prior = statementsOf(fs.readFileSync(priorPath, "utf8"));
const count = (l: { hash: string }[]) => {
  const m = new Map<string, number>();
  for (const s of l) m.set(s.hash, (m.get(s.hash) ?? 0) + 1);
  return m;
};
const fc = count(fresh);
const pc = count(prior);
const priorByHash = new Map<string, { text: string }[]>();
for (const s of prior) {
  const list = priorByHash.get(s.hash) ?? [];
  if (list.length === 0) priorByHash.set(s.hash, list);
  list.push(s);
}

const buckets = {
  privateOnly: { st: 0, ln: 0 },
  mintDrift: { st: 0, ln: 0 },
  descriptive: { st: 0, ln: 0 }
};
const descriptiveTokens = new Map<string, number>();
for (const s of fresh) {
  const twins = priorByHash.get(s.hash);
  if (!twins || twins.some((t) => t.text === s.text)) continue;
  if (fc.get(s.hash) !== 1 || pc.get(s.hash) !== 1) continue;
  const twin = twins[0];
  if (maskPrivate(s.text) === maskPrivate(twin.text)) {
    buckets.privateOnly.st++;
    buckets.privateOnly.ln += s.lines;
    continue;
  }
  const diff = diffTokens(maskPrivate(s.text), maskPrivate(twin.text));
  const descriptive = [...diff].filter((w) => !isMintish(w));
  if (descriptive.length === 0) {
    buckets.mintDrift.st++;
    buckets.mintDrift.ln += s.lines;
  } else {
    buckets.descriptive.st++;
    buckets.descriptive.ln += s.lines;
    for (const w of descriptive.slice(0, 6)) {
      descriptiveTokens.set(w, (descriptiveTokens.get(w) ?? 0) + 1);
    }
  }
}
console.log(JSON.stringify(buckets, null, 2));
console.log("\ntop descriptive flip tokens:");
for (const [w, n] of [...descriptiveTokens]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 25)) {
  console.log(`  ${n}  ${w}`);
}
