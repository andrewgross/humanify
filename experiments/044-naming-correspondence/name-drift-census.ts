/**
 * NAMING DRIFT, ATTRIBUTED — exp039's step 1 and step 2, on exp043's trees.
 *
 * Naming is now the largest reducible noise source (7,616 git lines against
 * relocation's 1,390), and 76% of it is one hop. exp039 measured its SHAPE in
 * `noiseLn` (statement mass, which overcharges: one edited line inside a
 * 5,000-line statement reads as 5,000) and left two questions open:
 *
 *   1. size the drift in GIT LINES, the unit the reader actually sees;
 *   2. attribute every drifted name to the TIER that produced it, to test the
 *      standing hypothesis — that the drift is two matchers disagreeing about
 *      WHICH prior binding a fresh one corresponds to, not the LLM being
 *      unstable. exp039 measured 97.2% of bindings named deterministically, so
 *      "the LLM re-rolled it" cannot be the main story.
 *
 * The population is exactly what `diff-composition.ts` charges as `naming`: a
 * fresh statement and a prior statement with the SAME rename-blind statement
 * hash but different text. Because the hash is rename-invariant, and property
 * names and free identifiers are hash CONTENT, two statements sharing a hash
 * differ ONLY in their binding identifiers — so the substitutions can be read
 * off by walking the two token streams in lockstep.
 *
 * Each pair is charged the same git lines `diff-composition` charges it, so the
 * totals here reconcile with the 034 gate's `layout.naming` rather than being a
 * parallel measurement with its own units.
 *
 * Usage: npx tsx name-drift-census.ts <priorSrcDir> <freshSrcDir> <diag.json> <label>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import { statementHash } from "../../src/split/statement-hash.js";

interface Stmt {
  hash: string;
  text: string;
  lines: string[];
}

/** Identifier-ish tokens, in source order. */
const WORD = /[A-Za-z_$][\w$]*/g;

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/**
 * `const alias = require("path")` -> alias -> path, for one emitted file.
 *
 * The split generates these aliases from the TARGET FILE PATH, not from the
 * renamer, which is why they never appear in `strategyTrails`. When the same
 * file imports the same path under a different alias next release
 * (`skillFiles` -> `apiResponseSkillFiles`, path byte-identical), the require
 * header is charged to `alias` — but every USAGE SITE of that alias sits inside
 * ordinary statements and is charged to `naming`. That is why `alias` reads 200
 * git lines across four hops while the same mechanism is the largest single
 * component of naming churn.
 */
const REQUIRE_DECL =
  /(?:^|\n)\s*const\s+([A-Za-z_$][\w$]*)\s*=\s*require\(\s*"([^"]+)"\s*\)/g;

function importAliases(code: string): Map<string, string> {
  const m = new Map<string, string>();
  for (const hit of code.matchAll(REQUIRE_DECL)) m.set(hit[1], hit[2]);
  return m;
}

function statementsOf(code: string): Stmt[] {
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch {
    return [];
  }
  if (!ast || ast.type !== "File") return [];
  return ast.program.body.map((s) => {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    return {
      hash: statementHash(s),
      text,
      lines: text ? text.split("\n") : []
    };
  });
}

function lcsLen(a: string[], b: string[]): number {
  let prev = new Array(b.length + 1).fill(0);
  let cur = new Array(b.length + 1).fill(0);
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      cur[j] =
        a[i - 1] === b[j - 1] ? prev[j - 1] + 1 : Math.max(prev[j], cur[j - 1]);
    }
    [prev, cur] = [cur, prev];
    cur.fill(0);
  }
  return prev[b.length];
}

/** The same charge diff-composition applies, so totals reconcile with the gate. */
function lineChurn(a: string[], b: string[]): number {
  return a.length + b.length - 2 * lcsLen(a, b);
}

/**
 * The distinct (priorName -> freshName) substitutions between two statements
 * that share a rename-blind hash. Returns null when the token streams do not
 * align — which should not happen for a hash twin, and is reported rather than
 * silently averaged away.
 */
function substitutions(
  prior: string,
  fresh: string
): Map<string, string> | null {
  const a = prior.match(WORD) ?? [];
  const b = fresh.match(WORD) ?? [];
  if (a.length !== b.length) return null;
  const subs = new Map<string, string>();
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) subs.set(a[i], b[i]);
  }
  return subs;
}

/** fresh name -> the strategy that settled it, from --diagnostics. */
function settledByIndex(diagPath: string): Map<string, string> {
  const diag = JSON.parse(fs.readFileSync(diagPath, "utf8")) as {
    strategyTrails?: {
      trails?: Array<{
        settledBy?: string;
        trail?: Array<{ newName?: string; outcome?: string }>;
      }>;
    };
  };
  // A name can be settled by different tiers in different scopes — 9,434 of
  // 58,522 applied names repeat on 85->86 — so recording the first would invent
  // precision. Contested names are marked and counted on their own.
  const index = new Map<string, string>();
  for (const t of diag.strategyTrails?.trails ?? []) {
    const last = t.trail?.[t.trail.length - 1];
    // Only an APPLIED entry names the binding; a trail ending in a vote lost.
    if (!last || last.outcome !== "applied" || !last.newName) continue;
    const tier = t.settledBy ?? "settled-by-missing";
    const prev = index.get(last.newName);
    if (prev === undefined) index.set(last.newName, tier);
    else if (prev !== tier) index.set(last.newName, "AMBIGUOUS");
  }
  return index;
}

/** Why a drifted name could not be attributed — reported per cause rather than
 * folded into one "unattributed" number, because the fix differs per cause. */
const missReasons = new Map<string, number>();
const missSamples = new Map<string, string[]>();
function noteMiss(why: string, pair: string): void {
  missReasons.set(why, (missReasons.get(why) ?? 0) + 1);
  const s = missSamples.get(why) ?? [];
  if (s.length < 4) s.push(pair);
  missSamples.set(why, s);
}

/**
 * What KIND of rename each substitution is. Named from the specimens the first
 * run surfaced, so the brief can size a mechanism instead of quoting anecdotes:
 *
 *   counter    React123 -> React93        the same name wearing a different
 *                                         ladder counter -- a slot number, the
 *                                         naming-axis twin of what exp042/043
 *                                         fixed for placement
 *   qualified  kairosCron -> logTaskEventKairosCron
 *                                         the prior name with a FILE-DERIVED
 *                                         qualifier bolted on (or removed), so
 *                                         placement churn becomes naming churn
 *   synonym    idx -> i, idx -> index     trivial loop-variable flips
 *   other      genuinely different names
 */
function renameKind(from: string, to: string): string {
  const stripDigits = (n: string) => n.replace(/\d+/g, "");
  if (from !== to && stripDigits(from) === stripDigits(to)) return "counter";
  const lf = from.toLowerCase();
  const lt = to.toLowerCase();
  if (lf.includes(lt) || lt.includes(lf)) return "qualified";
  if (from.length <= 5 && to.length <= 5) return "synonym";
  return "other";
}

function bucketOf(n: number): string {
  if (n === 1) return "1 substitution";
  if (n === 2) return "2 substitutions";
  if (n <= 5) return "3-5 substitutions";
  return "6+ substitutions (drift)";
}

function main(): void {
  const [priorDir, freshDir, diagPath, label] = process.argv.slice(2);
  const settled = settledByIndex(diagPath);

  const byBucket = new Map<string, { n: number; ln: number }>();
  const byTier = new Map<string, { n: number; ln: number }>();
  const byKind = new Map<string, { n: number; ln: number }>();
  const pairs: Array<{ from: string; to: string; ln: number; tier: string }> =
    [];
  let total = 0;
  let unaligned = 0;
  let identicalNames = 0;
  /** Lines whose prior twin was chosen from several same-hash candidates in the
   * file: the substitutions read off them are not trustworthy. */
  let ambiguousLn = 0;
  /** Every substitution on the hop, so the WHOLE hop can be tested for name
   * conservation: if the multiset of names being replaced equals the multiset
   * replacing them, the churn is a global redistribution rather than drift --
   * the same finding task A.2 made within single statements, asked globally.
   * Bucketed by substitution count because a 1-substitution statement cannot
   * permute internally; if those names are conserved it is a CROSS-statement
   * rotation, which is the same irreducible class. */
  const allFrom = new Map<string, number>();
  const allTo = new Map<string, number>();
  const bucketFrom = new Map<string, Set<string>>();
  const bucketTo = new Map<string, Set<string>>();
  /** Substitutions that are the SAME import re-aliased: prior file imported
   * path P as `from`, fresh file imports the SAME P as `to`. Not a rename at
   * all — the split generated both names from the path. */
  let reAliasLn = 0;
  let reAliasN = 0;
  const reAliasSamples: string[] = [];
  /** Both names are import aliases, but for DIFFERENT paths: the imported file
   * MOVED or was renamed, so its path-derived alias changed in every importer.
   * Placement churn propagating into the naming bucket. */
  const big6 = { n: 0, ln: 0 };
  let big6AllAlias = 0;
  let big6SomeAlias = 0;
  let big6NoAlias = 0;
  let movedImportLn = 0;
  let movedImportN = 0;
  const movedImportSamples: string[] = [];

  // A path that exists on BOTH sides is not a renamed module. Without this the
  // "import moved" class counts any statement that references a different
  // module as alias churn -- including genuine call-site changes and any
  // mis-paired hash twin. Measured: that inflates it several-fold.
  const priorFiles = new Set(walk(priorDir));
  const freshFiles = new Set(walk(freshDir));
  const isRenamedAway = (rel: string) => {
    const p = rel.replace(/^\.\//, "");
    return !freshFiles.has(p) && !freshFiles.has(`src/${p}`);
  };
  const files = new Set([...priorFiles, ...freshFiles]);
  for (const rel of files) {
    const pp = path.join(priorDir, rel);
    const fp = path.join(freshDir, rel);
    if (!fs.existsSync(pp) || !fs.existsSync(fp)) continue;
    const priorCode = fs.readFileSync(pp, "utf8");
    const freshCode = fs.readFileSync(fp, "utf8");
    const priorAlias = importAliases(priorCode);
    const freshAlias = importAliases(freshCode);
    const prior = statementsOf(priorCode);
    const fresh = statementsOf(freshCode);

    // Same pairing diff-composition uses: exact (hash+text) first, then the
    // leftovers matched by hash alone -> those are the naming population.
    const exactKey = (s: Stmt) => `${s.hash} ${s.text}`;
    const counts = new Map<string, number>();
    for (const s of prior) {
      counts.set(exactKey(s), (counts.get(exactKey(s)) ?? 0) + 1);
    }
    const freshRest: Stmt[] = [];
    for (const s of fresh) {
      const k = exactKey(s);
      const n = counts.get(k) ?? 0;
      if (n > 0) counts.set(k, n - 1);
      else freshRest.push(s);
    }
    const priorByHash = new Map<string, Stmt[]>();
    const still = new Map(counts);
    for (const s of prior) {
      const k = exactKey(s);
      const n = still.get(k) ?? 0;
      if (n > 0) {
        still.set(k, n - 1);
        const l = priorByHash.get(s.hash) ?? [];
        l.push(s);
        priorByHash.set(s.hash, l);
      }
    }

    for (const s of freshRest) {
      const bucket = priorByHash.get(s.hash);
      if (!bucket || bucket.length === 0) continue; // novel/edited, not naming
      // Was the choice of twin FORCED, or did the file hold several prior
      // statements with this hash? An arbitrary pick invents substitutions --
      // two re-export lines differing only in which module they name will pair
      // either way round and read as a rename that never happened.
      const ambiguousPairing = bucket.length > 1;
      const twin = bucket.shift() as Stmt;
      const ln = lineChurn(twin.lines, s.lines);
      if (ln === 0) continue;
      total += ln;
      if (ambiguousPairing) ambiguousLn += ln;
      const subs = substitutions(twin.text, s.text);
      if (!subs) {
        unaligned += ln;
        continue;
      }
      if (subs.size === 0) {
        identicalNames += ln;
        continue;
      }
      // Is a multi-substitution statement really "six names drifted", or one
      // statement referencing six IMPORTS whose aliases all moved together?
      let aliasSubs = 0;
      for (const [from, to] of subs) {
        const was = priorAlias.get(from);
        if (was === undefined) continue;
        const now = freshAlias.get(to);
        // Verified alias churn ONLY: the same module re-aliased, or a module
        // that genuinely vanished from the tree under its old path. "Both names
        // happen to be aliases" is not evidence -- it counts every statement
        // that calls a different module, and inflated this several-fold.
        if (
          now === was ||
          (now !== undefined &&
            isRenamedAway(path.posix.join(path.posix.dirname(rel), was)))
        ) {
          aliasSubs++;
        }
      }
      if (subs.size >= 6) {
        big6.n++;
        big6.ln += ln;
        if (aliasSubs === subs.size) big6AllAlias += ln;
        else if (aliasSubs > 0) big6SomeAlias += ln;
        else big6NoAlias += ln;
      }
      const b = bucketOf(subs.size);
      const rb = byBucket.get(b) ?? { n: 0, ln: 0 };
      rb.n++;
      rb.ln += ln;
      byBucket.set(b, rb);

      // Charge the statement's lines to the tier that settled each drifted
      // name, split evenly — a statement with one substitution attributes
      // cleanly, and the multi-substitution ones are reported separately above
      // so the split never masquerades as precision.
      const share = ln / subs.size;
      for (const [from, to] of subs) {
        const wasPath = priorAlias.get(from);
        const nowPath = freshAlias.get(to);
        if (wasPath !== undefined && nowPath === wasPath) {
          reAliasN++;
          reAliasLn += share;
          if (reAliasSamples.length < 5) {
            reAliasSamples.push(`${from} -> ${to}  (${wasPath})`);
          }
        } else if (
          wasPath !== undefined &&
          nowPath !== undefined &&
          // The module the prior alias pointed at must be GONE from the fresh
          // tree -- otherwise it still exists and this statement is simply
          // calling something else.
          isRenamedAway(path.posix.join(path.posix.dirname(rel), wasPath))
        ) {
          movedImportN++;
          movedImportLn += share;
          if (movedImportSamples.length < 5) {
            movedImportSamples.push(
              `${from} -> ${to}\n        ${wasPath}\n        ${nowPath}`
            );
          }
        }
        allFrom.set(from, (allFrom.get(from) ?? 0) + 1);
        allTo.set(to, (allTo.get(to) ?? 0) + 1);
        const bk = bucketOf(subs.size);
        if (!bucketFrom.has(bk)) bucketFrom.set(bk, new Set());
        if (!bucketTo.has(bk)) bucketTo.set(bk, new Set());
        bucketFrom.get(bk)?.add(from);
        bucketTo.get(bk)?.add(to);
        const k = renameKind(from, to);
        const rk = byKind.get(k) ?? { n: 0, ln: 0 };
        rk.n++;
        rk.ln += share;
        byKind.set(k, rk);
        let tier = settled.get(to) ?? "not-in-trail";
        if (tier === "AMBIGUOUS") tier = "ambiguous-name";
        if (tier === "not-in-trail" || tier === "ambiguous-name") {
          noteMiss(tier, `${from} -> ${to}`);
        }
        const rt = byTier.get(tier) ?? { n: 0, ln: 0 };
        rt.n++;
        rt.ln += share;
        byTier.set(tier, rt);
        if (subs.size === 1) pairs.push({ from, to, ln, tier });
      }
    }
  }

  const row = (label: string, r: { n: number; ln: number }, denom: number) =>
    `  ${label.padEnd(26)} ${String(r.n).padStart(6)}  ${String(Math.round(r.ln)).padStart(7)} ln  ${
      denom ? ((100 * r.ln) / denom).toFixed(1) : "-"
    }%`;

  console.log(`=== NAMING DRIFT CENSUS — ${label ?? ""} ===`);
  console.log(`  naming churn in scope: ${total} git lines`);
  if (unaligned) console.log(`  token streams unaligned: ${unaligned} ln`);
  if (identicalNames) {
    console.log(`  hash twins with NO name difference: ${identicalNames} ln`);
  }
  console.log(
    `  twin chosen from SEVERAL same-hash candidates: ${ambiguousLn} ln` +
      ` (${total ? ((100 * ambiguousLn) / total).toFixed(1) : "-"}% -- substitutions here are unreliable)`
  );
  console.log("  by number of distinct substitutions:");
  for (const [b, r] of [...byBucket.entries()].sort(
    (x, y) => y[1].ln - x[1].ln
  )) {
    console.log(row(b, r, total));
  }
  console.log(
    `  SAME IMPORT, re-aliased: ${reAliasN} substitutions / ${Math.round(reAliasLn)} git lines` +
      ` (${total ? ((100 * reAliasLn) / total).toFixed(1) : "-"}% of naming churn)`
  );
  for (const x of reAliasSamples) console.log(`      ${x}`);
  console.log(
    `  IMPORT MOVED (alias follows a changed path): ${movedImportN} substitutions / ` +
      `${Math.round(movedImportLn)} git lines` +
      ` (${total ? ((100 * movedImportLn) / total).toFixed(1) : "-"}%)`
  );
  for (const x of movedImportSamples) console.log(`      ${x}`);
  console.log(
    `  6+ SUBSTITUTION statements (${big6.n} / ${big6.ln} ln): ` +
      `every sub an import alias ${Math.round(big6AllAlias)} ln, ` +
      `some ${Math.round(big6SomeAlias)} ln, none ${Math.round(big6NoAlias)} ln`
  );
  {
    const conserved = [...allTo.keys()].filter((n) => allFrom.has(n)).length;
    console.log(
      `  NAME CONSERVATION over the whole hop: ${conserved}/${allTo.size} fresh names` +
        ` were also being replaced somewhere (${((100 * conserved) / Math.max(allTo.size, 1)).toFixed(1)}%)` +
        ` -- high means redistribution, not new naming`
    );
    for (const bk of [...bucketTo.keys()].sort()) {
      const f = bucketFrom.get(bk) ?? new Set();
      const t = bucketTo.get(bk) ?? new Set();
      const c = [...t].filter((n) => f.has(n)).length;
      console.log(
        `    ${bk.padEnd(26)} ${String(c).padStart(5)}/${String(t.size).padEnd(5)} conserved (${((100 * c) / Math.max(t.size, 1)).toFixed(0)}%)`
      );
    }
  }
  console.log("  by the KIND of rename:");
  const kindTotal = [...byKind.values()].reduce((a, r) => a + r.ln, 0);
  for (const [k, r] of [...byKind.entries()].sort(
    (x, y) => y[1].ln - x[1].ln
  )) {
    console.log(row(k, r, kindTotal));
  }
  console.log("  by the TIER that settled the fresh name:");
  const tierTotal = [...byTier.values()].reduce((a, r) => a + r.ln, 0);
  for (const [t, r] of [...byTier.entries()].sort(
    (x, y) => y[1].ln - x[1].ln
  )) {
    console.log(row(t, r, tierTotal));
  }
  if (missReasons.size) {
    console.log("  attribution misses, by cause:");
    for (const [why, n] of [...missReasons.entries()].sort(
      (a, b) => b[1] - a[1]
    )) {
      console.log(
        `    ${why.padEnd(16)} ${String(n).padStart(6)}  e.g. ${(missSamples.get(why) ?? []).join(" | ")}`
      );
    }
  }
  console.log(
    "  largest SINGLE-substitution drifts (EYEBALL BEFORE BELIEVING):"
  );
  for (const p of pairs.sort((a, b) => b.ln - a.ln).slice(0, 12)) {
    console.log(
      `    ${String(p.ln).padStart(5)} ln  [${p.tier}]  ${p.from} -> ${p.to}`
    );
  }
}

main();
