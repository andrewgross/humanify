/**
 * exp173: the 2.1.197->2.1.198 cold-variance question. For each label,
 * the fresh statements the scorer calls NOISE (hash unchanged, text
 * changed), keyed by hash, with their line counts — written as JSON so
 * repeats can be compared statement by statement (which hashes churn in
 * one repeat and stay clean in another).
 *
 * usage: tsx noise-statements.ts <freshHumanified> <priorHumanified> <out.json>
 */
import fs from "node:fs";
import { statementsOf } from "../034-eval-harness/statements.js";
import { statementKinds } from "../034-eval-harness/surfaces.js";

const [fresh, prior, out] = process.argv.slice(2);
if (!fresh || !prior || !out) {
  throw new Error("usage: noise-statements.ts <fresh> <prior> <out.json>");
}
const f = statementsOf(fs.readFileSync(fresh, "utf8"));
const p = statementsOf(fs.readFileSync(prior, "utf8"));
const kinds = statementKinds(f, p);
const priorByHash = new Map<string, string>();
for (const s of p)
  if (!priorByHash.has(s.hash)) priorByHash.set(s.hash, s.text);
const rows = f.flatMap((s, i) =>
  kinds[i] === "noise"
    ? [
        {
          hash: s.hash,
          lines: s.lines,
          fresh: s.text,
          prior: priorByHash.get(s.hash)
        }
      ]
    : []
);
fs.writeFileSync(out, JSON.stringify(rows));
console.log(
  `${rows.length} noise statements, ${rows.reduce((a, r) => a + r.lines, 0)} lines`
);
