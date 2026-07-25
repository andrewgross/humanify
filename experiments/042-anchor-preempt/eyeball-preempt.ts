/**
 * TASK A — read the minted-counter disagreements before writing a line of src/.
 *
 * The claim under test, from the brief:
 *
 *   1. the FRESH block's content is the PRIOR block's content, carried under a
 *      RECYCLED minted counter, and
 *   2. the name the vote followed belongs to a GENUINELY DIFFERENT prior block,
 *      which still exists in the prior release with its own stable home.
 *
 * If (1) holds and (2) does not — if the name's prior owner vanished — then the
 * name vote is not "following a different block", it is following a ghost, and
 * the mechanism is something else. Five hypotheses have been refuted across
 * exp040/041 by exactly this step, so print the evidence for BOTH halves.
 *
 * Usage: npx tsx eyeball-preempt.ts <priorOutDir> <freshOutDir> [--lines N]
 */
import {
  AnchorIndex,
  loadSide,
  nameToFilesFrom,
  readLedger,
  readMatchMap,
  recoveredLines,
  replay,
  SHIPPED,
  type Stmt
} from "../041-content-anchor/replay-lib.js";

/** Mirrors `hasMintedNumber` in src/split/stable-split.ts — a run of 2+ digits
 * that is not a known unit token. */
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

function head(s: Stmt, cap: number): string {
  return s.text
    .split("\n")
    .slice(0, cap)
    .map((l) => `      ${l.slice(0, 110)}`)
    .join("\n");
}

/** Prior statements that declare a given outer name. */
function indexByOuterName(stmts: Stmt[]): Map<string, Stmt[]> {
  const m = new Map<string, Stmt[]>();
  for (const s of stmts) {
    for (const n of s.outerNames) {
      const list = m.get(n) ?? [];
      list.push(s);
      m.set(n, list);
    }
  }
  return m;
}

function main(): void {
  const args = process.argv.slice(2);
  const cap = args.includes("--lines")
    ? Number(args[args.indexOf("--lines") + 1])
    : 6;
  const [priorDir, freshDir] = args;
  const priorLedger = readLedger(priorDir);
  const freshLedger = readLedger(freshDir);
  const prior = loadSide(priorDir, priorLedger);
  const fresh = loadSide(freshDir, freshLedger);
  const anchors = new AnchorIndex(prior, fresh);
  const priorByOuter = indexByOuterName(prior);
  const freshByOuter = indexByOuterName(fresh);

  // Replay the shipped ladder so each candidate carries the TIER that placed it
  // — a disagreement only matters if a NAME tier cast the losing vote.
  const anchorTier = new Array<string | undefined>(fresh.length);
  for (const f of fresh) {
    const twin = anchors.verdict(f);
    if (twin) anchorTier[f.idx] = twin.file;
  }
  const replayed = replay(
    {
      fresh,
      freshHashes: freshLedger.hashes ?? [],
      prior: priorLedger,
      priorNames: nameToFilesFrom(prior, false),
      matchMap: readMatchMap(freshDir),
      anchorTier
    },
    SHIPPED
  );

  const rows: Array<{ f: Stmt; twin: Stmt; ln: number }> = [];
  for (const f of fresh) {
    const twin = anchors.verdict(f);
    if (!twin || twin.file === f.file) continue;
    if (f.outerNames.length === 0 || !f.outerNames.every(hasMintedNumber)) {
      continue;
    }
    rows.push({ f, twin, ln: recoveredLines(f, twin) });
  }
  rows.sort((a, b) => b.ln - a.ln);

  console.log(`=== MINTED-COUNTER DISAGREEMENTS: ${priorDir} -> ${freshDir}`);
  console.log(
    `    ${rows.length} statements, ${rows.reduce((a, r) => a + r.ln, 0)} git lines\n`
  );

  for (const { f, twin, ln } of rows) {
    const sharedLits = new Set(
      f.literals.filter((l) => twin.literals.includes(l))
    );
    console.log(
      `########## fresh#${f.idx}  ${ln} ln  [placed by ${replayed.kind[f.idx]}]`
    );
    console.log(`  names:  ${f.outerNames.join(", ")}`);
    console.log(`  today:  ${f.file}`);
    console.log(`  anchor: ${twin.file}   (prior#${twin.idx})`);
    console.log(`  shared rare literals: ${sharedLits.size}`);
    console.log(
      `  sample: ${[...sharedLits]
        .slice(0, 3)
        .map((x) => JSON.stringify(x.slice(0, 55)))
        .join(", ")}`
    );

    // CLAIM 1 — is the fresh block the anchor twin's block, edited?
    const priorLines = new Set(twin.text.split("\n"));
    const freshLines = new Set(f.text.split("\n"));
    const freshOnly = f.text
      .split("\n")
      .filter((l) => !priorLines.has(l)).length;
    const priorOnly = twin.text
      .split("\n")
      .filter((l) => !freshLines.has(l)).length;
    console.log(
      `  CLAIM 1 (content carried): fresh ${f.lines} ln / prior ${twin.lines} ln,` +
        ` ${freshOnly} fresh-only lines, ${priorOnly} prior-only`
    );
    console.log(`    fresh#${f.idx} head:`);
    console.log(head(f, cap));
    console.log(
      `    prior#${twin.idx} head  (names: ${twin.outerNames.join(", ")}):`
    );
    console.log(head(twin, cap));
    console.log(
      `    counter recycled? prior name(s) ${twin.outerNames.join(",")} ` +
        `still declared in the FRESH release: ` +
        twin.outerNames
          .map(
            (n) =>
              `${n}=${(freshByOuter.get(n) ?? []).map((s) => `#${s.idx}`).join("/") || "ABSENT"}`
          )
          .join(", ")
    );

    // CLAIM 2 — whose block does the name the vote followed actually belong to?
    console.log(`  CLAIM 2 (the vote followed a different block):`);
    for (const n of f.outerNames) {
      const owners = priorByOuter.get(n) ?? [];
      const homes = priorLedger.nameToFiles[n];
      console.log(
        `    "${n}" -> prior homes ${JSON.stringify(homes ?? null)}` +
          `; declared by prior ${owners.map((s) => `#${s.idx}(${s.lines}ln)`).join(",") || "NOBODY"}`
      );
      for (const o of owners.slice(0, 2)) {
        const shared = new Set(f.literals.filter((l) => o.literals.includes(l)))
          .size;
        console.log(
          `      prior#${o.idx} [${o.file}] shares ${shared} rare literals with fresh#${f.idx}` +
            (o.idx === twin.idx ? "   <-- SAME as the anchor twin" : "")
        );
        console.log(head(o, Math.min(cap, 4)));
      }
    }
    console.log("");
  }
}

main();
