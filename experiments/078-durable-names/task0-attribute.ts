/**
 * 078 Task 0 — of the enclosures that minted a fresh identity, how many
 * actually EXISTED in the prior release?
 *
 *   npx tsx experiments/078-durable-names/task0-attribute.ts \
 *     <priorTreeRoot> <freshTreeRoot>
 *
 * This gates the whole experiment. A genuinely new module SHOULD get a new
 * file — that is correct behaviour and an honest diff. The 12,902 file
 * add/remove lines exp076 measured are only a COST for enclosures that
 * existed and were not recognised.
 *
 * ## Ground truth must not come from the thing under test
 *
 * "Existed in prior" is established WITHOUT the production matcher's later
 * tiers and WITHOUT any name:
 *
 *   1. SEED with unique hash-multiset signatures present exactly once on
 *      each side. Unambiguous, content-only, name-free — this is tier A and
 *      it is not the tier in question.
 *   2. For every fresh enclosure the production matcher LEFT UNMATCHED, map
 *      its importer and importee sets through the seeds and look for a prior
 *      enclosure whose mapped sets agree. Exactly one unclaimed candidate
 *      with positive edge agreement = "existed in prior".
 *
 * exp076 retracted a measurement that set its identity floor at the
 * matcher's own threshold and therefore reported a confident zero. Seeds
 * here are strictly stronger than the tiers being judged, and edges are a
 * different KIND of evidence from both content overlap and names.
 *
 * ## What it cannot tell you
 *
 * Edge agreement is itself evidence, not proof. A pair it accepts could be
 * two different enclosures that happen to sit in the same graph position;
 * the report prints overlap per class so that can be judged rather than
 * assumed. Lines are counted from the files on disk, so a class's line total
 * is what git would actually show for those files, not an estimate.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { matchFossilModules } from "../../src/split/fossil-match.js";

const [PRIOR_ROOT, FRESH_ROOT] = process.argv.slice(2);
if (!PRIOR_ROOT || !FRESH_ROOT) {
  console.error(
    "usage: task0-attribute.ts <priorTreeRoot> <freshTreeRoot>\n" +
      "  each root contains .humanify/split-ledger.json and src/"
  );
  process.exit(1);
}

interface LedgerModule {
  file: string;
  hashes: string[];
  imports: number[];
}

function loadLedger(root: string): LedgerModule[] {
  const p = path.join(root, ".humanify", "split-ledger.json");
  const led = JSON.parse(fs.readFileSync(p, "utf8")) as {
    fossilModules?: LedgerModule[];
  };
  if (!led.fossilModules) {
    throw new Error(`${p} has no fossilModules — not a fossil-layout tree`);
  }
  return led.fossilModules;
}

const prior = loadLedger(PRIOR_ROOT);
const fresh = loadLedger(FRESH_ROOT);

/** Lines in a tree file, or 0 when it is absent (a module whose file was
 * merged away). Counted from disk so a class total is what git shows. */
function lineCount(root: string, file: string): number {
  try {
    return fs.readFileSync(path.join(root, file), "utf8").split("\n").length;
  } catch {
    return 0;
  }
}

// --- what production decided -------------------------------------------------
// Stems are supplied exactly as assignFossil supplies them, so this reproduces
// the real outcome including tier C.
const priorStem = (f: string) => {
  const base = f.slice(f.lastIndexOf("/") + 1);
  return base.endsWith(".js") ? base.slice(0, -3) : base;
};
const { matches: prodMatches, tiers } = matchFossilModules(
  prior.map((m) => ({
    hashes: m.hashes,
    imports: m.imports,
    stem: priorStem(m.file)
  })),
  fresh.map((m) => ({
    hashes: m.hashes,
    imports: m.imports,
    stem: priorStem(m.file)
  }))
);

// --- independent ground truth: seeds, then edges -----------------------------
function signatureIndex(mods: LedgerModule[]): Map<string, number[]> {
  const idx = new Map<string, number[]>();
  mods.forEach((m, i) => {
    const k = m.hashes.join("|");
    const list = idx.get(k) ?? [];
    list.push(i);
    idx.set(k, list);
  });
  return idx;
}
const seedPriorToFresh = new Map<number, number>();
const seedFreshToPrior = new Map<number, number>();
{
  const bp = signatureIndex(prior);
  const bf = signatureIndex(fresh);
  for (const [k, ps] of bp) {
    const fsIdx = bf.get(k);
    if (ps.length === 1 && fsIdx?.length === 1) {
      seedPriorToFresh.set(ps[0], fsIdx[0]);
      seedFreshToPrior.set(fsIdx[0], ps[0]);
    }
  }
}

function importersOf(mods: LedgerModule[]): Map<number, Set<number>> {
  const rev = new Map<number, Set<number>>();
  mods.forEach((m, i) => {
    for (const imp of m.imports) {
      if (imp === i) continue;
      const s = rev.get(imp) ?? new Set<number>();
      s.add(i);
      rev.set(imp, s);
    }
  });
  return rev;
}
const pImporters = importersOf(prior);
const fImporters = importersOf(fresh);

function overlap(a: string[], b: string[]): number {
  const ca = new Map<string, number>();
  for (const h of a) ca.set(h, (ca.get(h) ?? 0) + 1);
  let inter = 0;
  const cb = new Map<string, number>();
  for (const h of b) cb.set(h, (cb.get(h) ?? 0) + 1);
  for (const [h, n] of ca) inter += Math.min(n, cb.get(h) ?? 0);
  const union = a.length + b.length - inter;
  return union === 0 ? 0 : inter / union;
}

/** Edge agreement between a prior and a fresh enclosure, counted ONLY through
 * seed pairs — so it never depends on the tiers under test, nor on names. */
function seedEdgeAgreement(pi: number, fi: number): number {
  let agree = 0;
  const fImports = new Set(fresh[fi].imports);
  for (const imp of prior[pi].imports) {
    const mapped = seedPriorToFresh.get(imp);
    if (mapped !== undefined && fImports.has(mapped)) agree++;
  }
  const fUp = fImporters.get(fi) ?? new Set<number>();
  for (const up of pImporters.get(pi) ?? []) {
    const mapped = seedPriorToFresh.get(up);
    if (mapped !== undefined && fUp.has(mapped)) agree++;
  }
  return agree;
}

/** Prior enclosures reachable from a fresh one by seed-mapped edges — the
 * candidate set, built without content thresholds and without names. */
function candidatesFor(fi: number): number[] {
  const cands = new Set<number>();
  for (const imp of fresh[fi].imports) {
    const pImp = seedFreshToPrior.get(imp);
    if (pImp === undefined) continue;
    for (const up of pImporters.get(pImp) ?? []) cands.add(up);
    for (const [pi, m] of prior.entries()) {
      if (m.imports.includes(pImp)) cands.add(pi);
    }
  }
  for (const up of fImporters.get(fi) ?? []) {
    const pUp = seedFreshToPrior.get(up);
    if (pUp === undefined) continue;
    for (const imp of prior[pUp].imports) cands.add(imp);
  }
  return [...cands];
}

interface Row {
  files: number;
  lines: number;
  overlaps: number[];
}
const blank = (): Row => ({ files: 0, lines: 0, overlaps: [] });
const classes = {
  matched: blank(),
  existedDeclined: blank(),
  existedDeclinedContentOnly: blank(),
  genuinelyNew: blank(),
  ambiguous: blank(),
  reMintedSamePath: blank(),
  reMintedMovedPath: blank()
};
const priorClaimed = new Set<number>(prodMatches.values());
const examples: string[] = [];

fresh.forEach((fm, fi) => {
  const lines = lineCount(FRESH_ROOT, fm.file);
  if (prodMatches.has(fi)) {
    classes.matched.files++;
    classes.matched.lines += lines;
    return;
  }
  // Unmatched by production. Did it exist?
  const cands = candidatesFor(fi)
    .filter((pi) => !priorClaimed.has(pi))
    .map((pi) => ({ pi, agree: seedEdgeAgreement(pi, fi) }))
    .filter((c) => c.agree > 0)
    .sort((a, b) => b.agree - a.agree);
  if (cands.length === 0) {
    classes.genuinelyNew.files++;
    classes.genuinelyNew.lines += lines;
    return;
  }
  const best = cands[0];
  const tied = cands.filter((c) => c.agree === best.agree).length > 1;
  const ov = overlap(prior[best.pi].hashes, fm.hashes);
  const bucket = tied ? classes.ambiguous : classes.existedDeclined;
  bucket.files++;
  bucket.lines += lines;
  bucket.overlaps.push(ov);
  // CRITICAL SPLIT. Re-minting an identity is only a GIT cost when the path
  // changes. An enclosure that lost its match but re-derived the same path
  // shows up as an edited file — the diff carries its changed lines, which it
  // would carry anyway. Counting the whole file for those overstates the
  // recoverable mass, which the first version of this script did.
  if (prior[best.pi].file === fm.file) {
    classes.reMintedSamePath.files++;
    classes.reMintedSamePath.lines += lines;
  } else {
    classes.reMintedMovedPath.files++;
    // Both sides: git shows the old file deleted and the new one added.
    classes.reMintedMovedPath.lines +=
      lines + lineCount(PRIOR_ROOT, prior[best.pi].file);
  }
  if (!tied) {
    // Would the CONTENT floor alone explain the refusal? Tier B needs >= 0.5
    // and tier C needs >= 0.7; below 0.5 no content tier could ever fire, so
    // the refusal is structural rather than a threshold that could be nudged.
    if (ov < 0.5) {
      classes.existedDeclinedContentOnly.files++;
      classes.existedDeclinedContentOnly.lines += lines;
    }
    if (examples.length < 12) {
      examples.push(
        `  ${String(lines).padStart(5)} ln  ov=${ov.toFixed(2)} agree=${best.agree}  ` +
          `${prior[best.pi].file}  ~>  ${fm.file}`
      );
    }
  }
});

// Prior enclosures nothing claimed: the delete side of the diff.
let deletedFiles = 0;
let deletedLines = 0;
prior.forEach((pm, pi) => {
  if (priorClaimed.has(pi)) return;
  deletedFiles++;
  deletedLines += lineCount(PRIOR_ROOT, pm.file);
});

const med = (xs: number[]) =>
  xs.length === 0
    ? "-"
    : [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)].toFixed(2);

console.log(`prior enclosures ${prior.length}, fresh ${fresh.length}`);
console.log(
  `production matched ${prodMatches.size} — tiers ${JSON.stringify(tiers)}`
);
console.log(`seed pairs used as ground truth: ${seedPriorToFresh.size}\n`);
console.log(
  "class                                files      lines   median overlap"
);
const row = (name: string, r: Row) =>
  console.log(
    `${name.padEnd(36)}${String(r.files).padStart(6)}${String(r.lines).padStart(11)}   ${med(r.overlaps)}`
  );
row("matched, name held", classes.matched);
row("EXISTED in prior, declined", classes.existedDeclined);
row(
  "  …of those, below every content floor",
  classes.existedDeclinedContentOnly
);
row("existed but AMBIGUOUS (tied candidates)", classes.ambiguous);
row("genuinely new", classes.genuinelyNew);
console.log("\n-- the re-minted, split by whether the PATH actually moved --");
row("re-minted to the SAME path (free)", classes.reMintedSamePath);
row("re-minted to a MOVED path (costs)", classes.reMintedMovedPath);
console.log(
  `${"prior enclosures nothing claimed (deletes)".padEnd(36)}${String(deletedFiles).padStart(6)}${String(deletedLines).padStart(11)}`
);
// Only a MOVED path costs git lines; a same-path re-mint shows as an edit.
const cost = classes.reMintedMovedPath.lines;
const honest = classes.genuinelyNew.lines;
console.log(
  `\nVERDICT INPUT: ${cost} lines belong to enclosures that EXISTED and were ` +
    `re-minted;\n  ${honest} lines are genuinely new files and are an honest diff.`
);
console.log(
  `  ${cost + honest === 0 ? "-" : ((100 * cost) / (cost + honest)).toFixed(1)}% of the added-file mass is recoverable in principle.`
);
console.log("\nlargest recoverable cases:");
for (const e of examples) console.log(e);
