/**
 * 078 Task 2d (sizing) — would GRADED similarity separate the enclosures that
 * exact hash-multiset overlap cannot?
 *
 *   npx tsx --max-old-space-size=16384 \
 *     experiments/078-durable-names/graded-similarity.ts <priorRoot> <freshRoot>
 *
 * ## Why
 *
 * The module matcher compares two enclosures by asking, per statement, "same
 * fingerprint or not" — one bit. A statement 95% identical and one 0%
 * identical both score zero. Traced on a real tied pair
 * (`create-env-proxy/feature-flags.js` 383 lines vs `gateway-config.js` 113
 * lines): their big `defineExports` statements DO hash differently, so we
 * noticed the difference and then discarded it, leaving only the one trivial
 * statement they share (`var X = {};`) — 1 of 7 = 0.14, identically for
 * every cross-pairing. The evidence that separates them exists and is thrown
 * away by the hashing step.
 *
 * Meanwhile `src/analysis/fingerprint-index.ts` already matches FUNCTIONS
 * with a graded cascade — shingle sets compared by Jaccard, callee/caller
 * shapes, two-hop shapes. The module matcher was ported from an experiment
 * script and uses none of it.
 *
 * ## Why graded, and not the object keys
 *
 * Andrew, 2026-08-17: object keys are shaky, because one upstream rename
 * shuffles things downstream. Correct — for EXACT-match evidence, which
 * fails completely the moment one token changes. The same information used
 * as a DEGREE degrades gracefully: one renamed key in forty moves a score by
 * 2% instead of flipping a verdict. That is the principle this probe tests —
 * brittle evidence versus soft evidence, not new evidence.
 *
 * ## What it measures
 *
 * For the enclosures the production matcher still leaves unmatched, does a
 * graded score produce a CLEAR winner (best clearly above runner-up)? And
 * does it agree with pairs the matcher already makes confidently — the
 * precision check, since a score that separates leftovers but disagrees with
 * known-good pairs is worse than nothing.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { matchFossilModules } from "../../src/split/fossil-match.js";

const [PRIOR_ROOT, FRESH_ROOT, MODE = "with-names"] = process.argv.slice(2);
/** `no-names` drops every token derived from an identifier — property keys
 * and member names — leaving only tree shape and literals. Andrew's caution
 * (2026-08-17) was that key-derived evidence shuffles when the LLM renames
 * upstream; this measures whether the graded score needs them at all. */
const USE_NAME_TOKENS = MODE !== "no-names";
if (!PRIOR_ROOT || !FRESH_ROOT) {
  console.error("usage: graded-similarity.ts <priorRoot> <freshRoot>");
  process.exit(1);
}

interface LedgerModule {
  file: string;
  hashes: string[];
  imports: number[];
}
const load = (root: string): LedgerModule[] =>
  JSON.parse(
    fs.readFileSync(path.join(root, ".humanify", "split-ledger.json"), "utf8")
  ).fossilModules;

const prior = load(PRIOR_ROOT);
const fresh = load(FRESH_ROOT);

/**
 * A file's token set — the module-level analogue of `computeShingleSet`.
 *
 * Deliberately NOT identifier names: those are what the pipeline rewrites and
 * what exp078 spent two days removing from identity. What it does keep is
 * everything the statement fingerprint already keeps (shape, literals) plus
 * the two things it throws away by hashing whole statements:
 *   - SHAPE N-GRAMS: parent→child→grandchild node-type triples, so a 185-line
 *     object literal and a 52-line one share tokens in proportion to how much
 *     structure they actually share, instead of "different, score 0".
 *   - LITERALS: string and numeric values, which survive renaming entirely.
 *
 * Property keys ARE included, as graded tokens among hundreds. That is the
 * distinction Andrew drew: as an exact key-set comparison they are brittle,
 * as 1/N of a similarity score they are not.
 */
function tokensOf(file: string): Set<string> {
  const tokens = new Set<string>();
  let src: string;
  try {
    src = fs.readFileSync(file, "utf8");
  } catch {
    return tokens;
  }
  let ast: t.File | null = null;
  try {
    ast = parseSync(src, {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    }) as t.File;
  } catch {
    return tokens;
  }
  if (!ast) return tokens;
  const walk = (node: t.Node, parent: string, grand: string): void => {
    tokens.add(`n3:${grand}>${parent}>${node.type}`);
    if (t.isStringLiteral(node)) tokens.add(`str:${node.value.slice(0, 40)}`);
    else if (t.isNumericLiteral(node)) tokens.add(`num:${node.value}`);
    else if (
      USE_NAME_TOKENS &&
      t.isObjectProperty(node) &&
      t.isIdentifier(node.key)
    ) {
      tokens.add(`key:${node.key.name}`);
    } else if (
      USE_NAME_TOKENS &&
      t.isMemberExpression(node) &&
      t.isIdentifier(node.property)
    ) {
      tokens.add(`prop:${node.property.name}`);
    }
    for (const key of t.VISITOR_KEYS[node.type] ?? []) {
      const child = (node as unknown as Record<string, unknown>)[key];
      for (const c of Array.isArray(child) ? child : [child]) {
        if (c && typeof c === "object" && "type" in c) {
          walk(c as t.Node, node.type, parent);
        }
      }
    }
  };
  for (const s of ast.program.body) walk(s, "root", "root");
  return tokens;
}

function jaccard(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 || b.size === 0) return 0;
  let inter = 0;
  const [small, big] = a.size < b.size ? [a, b] : [b, a];
  for (const x of small) if (big.has(x)) inter++;
  return inter / (a.size + b.size - inter);
}

// --- production outcome -------------------------------------------------
const stem = (f: string) => {
  const b = f.slice(f.lastIndexOf("/") + 1);
  return b.endsWith(".js") ? b.slice(0, -3) : b;
};
const sig = (m: LedgerModule) => ({
  hashes: m.hashes,
  imports: m.imports,
  stem: stem(m.file)
});
const { matches } = matchFossilModules(prior.map(sig), fresh.map(sig));
const claimedPrior = new Set(matches.values());

const tokenCache = new Map<string, Set<string>>();
const tokens = (root: string, file: string): Set<string> => {
  const key = `${root}/${file}`;
  const hit = tokenCache.get(key);
  if (hit) return hit;
  const set = tokensOf(path.join(root, file));
  tokenCache.set(key, set);
  return set;
};

// --- 1. does a graded score resolve the leftovers? ----------------------
const leftoverP = prior
  .map((_, pi) => pi)
  .filter((pi) => !claimedPrior.has(pi));
const leftoverF = fresh.map((_, fi) => fi).filter((fi) => !matches.has(fi));
console.log(`MODE: ${MODE}`);
console.log(
  `leftovers: ${leftoverP.length} prior, ${leftoverF.length} fresh\n`
);

/** Best clearly beats runner-up by this margin, or we abstain. Chosen to be
 * reported alongside the distribution rather than tuned: the point of the
 * probe is to show whether ANY clear separation exists. */
const MARGIN = 1.5;
let decisive = 0;
let noWinner = 0;
const rows: string[] = [];
for (const fi of leftoverF) {
  const ft = tokens(FRESH_ROOT, fresh[fi].file);
  const scored = leftoverP
    .map((pi) => ({ pi, s: jaccard(ft, tokens(PRIOR_ROOT, prior[pi].file)) }))
    .sort((a, b) => b.s - a.s);
  const best = scored[0];
  const second = scored[1];
  if (!best || best.s <= 0.02) {
    noWinner++;
    continue;
  }
  const clear = !second || second.s <= 0.02 || best.s >= second.s * MARGIN;
  if (clear) decisive++;
  else noWinner++;
  if (rows.length < 14) {
    rows.push(
      `  ${clear ? "DECIDES " : "ambig   "} ${best.s.toFixed(3)}` +
        ` (2nd ${second ? second.s.toFixed(3) : "-"})  ` +
        `${prior[best.pi].file}  ~>  ${fresh[fi].file}` +
        (stem(prior[best.pi].file) === stem(fresh[fi].file)
          ? "  [name agrees]"
          : "")
    );
  }
}
console.log(
  `graded score decides ${decisive} of ${leftoverF.length} leftovers ` +
    `(${noWinner} stay ambiguous)\n`
);
for (const r of rows) console.log(r);

// --- 2. precision: does it agree with pairs the matcher already trusts? --
const sample: Array<[number, number]> = [];
for (const [fi, pi] of matches) {
  if (sample.length >= 400) break;
  if (fi % 11 === 0) sample.push([pi, fi]);
}
let agree = 0;
const scores: number[] = [];
for (const [pi, fi] of sample) {
  const s = jaccard(
    tokens(PRIOR_ROOT, prior[pi].file),
    tokens(FRESH_ROOT, fresh[fi].file)
  );
  scores.push(s);
  if (s >= 0.5) agree++;
}
scores.sort((a, b) => a - b);
console.log(
  `\nprecision check on ${sample.length} pairs the matcher already trusts:` +
    `\n  median graded score ${scores[Math.floor(scores.length / 2)].toFixed(3)}` +
    `, 10th pct ${scores[Math.floor(scores.length / 10)].toFixed(3)}` +
    `, ${agree} of ${sample.length} score >= 0.5`
);
