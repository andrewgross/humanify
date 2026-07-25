/**
 * TASK A — the ceiling, measured at the point of decision, for every candidate
 * that targets the same population: the statements the split places by "follow
 * your preceding neighbour" because every identity tier abstained.
 *
 * Candidates:
 *
 *   anchor        the brief's proposal — a fresh statement whose RARE string
 *                 literals resolve to exactly one prior statement, and which
 *                 passes the >=50% token-overlap gate, inherits its file.
 *   outer         vote only on the statement's OUTER bindings. The shipped
 *                 `declaredNames` is `getBindingIdentifiers`, which includes
 *                 FUNCTION PARAMETERS, so `function f(inputData)` casts a vote
 *                 for wherever `inputData` was declared in the prior release.
 *   allsame       when the voters disagree, let a UNANIMOUS subset of "all-same"
 *                 votes (names with exactly one home file in the prior release)
 *                 decide instead of dropping the statement to locality. An
 *                 ordinal vote is a positional guess across a name declared in
 *                 dozens of files; an all-same vote is not.
 *   combinations of the above.
 *
 * Two levels of measurement, because the first one lies by omission:
 *
 *   DECIDES  how many residue statements the candidate places with evidence.
 *   NET      what it does to relocation across the whole tree — including the
 *            cascade, since moving one statement shifts the locality fallback
 *            for every statement after it. `healed` minus `broken` is the
 *            number that survives contact with a real diff.
 *
 * Usage:
 *   npx tsx ceiling.ts <priorOutDir> <freshOutDir> <label>
 *                      [--rare N] [--list K] [--json out.json]
 * where each dir holds .humanify/{humanified.js,split-ledger.json}.
 */
import * as fs from "node:fs";
import {
  AnchorIndex,
  SHIPPED,
  type Stmt,
  TwinIndex,
  type VoteRule,
  isLocality,
  loadSide,
  nameToFilesFrom,
  pct,
  readLedger,
  readMatchMap,
  recoveredLines,
  replay
} from "./replay-lib.js";

interface Move {
  recovered: number;
  freshIdx: number;
  from: string;
  to: string;
  freshLines: number;
  head: string;
}

/**
 * The bottom line. For each fresh statement that has a priced prior twin: is it
 * placed in the twin's file (aligned — the diff shows an in-place edit) or not
 * (misaligned — a delete in one file and an add in another)? Count transitions.
 * A candidate that heals 200 and breaks 150 is not a 200-line win.
 */
interface NetEffect {
  priced: number;
  alignedToday: number;
  healed: number;
  healedLines: number;
  broken: number;
  brokenLines: number;
}

interface CandidateResult {
  name: string;
  decides: number;
  riskyRePlaced: number;
  net: NetEffect;
  moves: Move[];
}

function netEffect(
  fresh: Stmt[],
  candidate: string[],
  twinOf: Array<Stmt | undefined>
): NetEffect {
  const n: NetEffect = {
    priced: 0,
    alignedToday: 0,
    healed: 0,
    healedLines: 0,
    broken: 0,
    brokenLines: 0
  };
  for (let i = 0; i < fresh.length; i++) {
    const twin = twinOf[i];
    if (!twin) continue;
    n.priced++;
    const wasAligned = fresh[i].file === twin.file;
    const nowAligned = candidate[i] === twin.file;
    if (wasAligned) n.alignedToday++;
    if (!wasAligned && nowAligned) {
      n.healed++;
      n.healedLines += recoveredLines(fresh[i], twin);
    } else if (wasAligned && !nowAligned) {
      n.broken++;
      n.brokenLines += recoveredLines(fresh[i], twin);
    }
  }
  return n;
}

function main(): void {
  const args = process.argv.slice(2);
  const [priorDir, freshDir, label] = args;
  const num = (flag: string, dflt: number) =>
    args.includes(flag) ? Number(args[args.indexOf(flag) + 1]) : dflt;
  const rareMax = num("--rare", 1);
  const listN = num("--list", 0);
  const jsonOut = args.includes("--json")
    ? args[args.indexOf("--json") + 1]
    : undefined;

  const priorLedger = readLedger(priorDir);
  const freshLedger = readLedger(freshDir);
  const prior = loadSide(priorDir, priorLedger);
  const fresh = loadSide(freshDir, freshLedger);
  const matchMap = readMatchMap(freshDir);
  const freshHashes = freshLedger.hashes ?? [];

  const shippedNames = new Map(Object.entries(priorLedger.nameToFiles));
  const outerNames = nameToFilesFrom(prior, true);
  const base = replay(
    {
      fresh,
      freshHashes,
      prior: priorLedger,
      priorNames: shippedNames,
      matchMap
    },
    SHIPPED
  );

  console.log(`=== CEILING — ${label ?? ""} ===`);
  console.log(`  statements: prior ${prior.length}, fresh ${fresh.length}`);
  console.log("  --- replay self-check (compare with the pipeline's log) ---");
  console.log(
    `    hash ${base.counts.hash}, name ${base.counts.name}, ordinal ${base.counts.ordinal}, ` +
      `identity-fill ${base.counts.fill}, ` +
      `locality ${base.counts.conflict + base.counts.novote} ` +
      `(conflict ${base.counts.conflict} + novote ${base.counts.novote})`
  );
  console.log(
    `    replayed assignment matches the ledger for ${base.ledgerAgreement}/${fresh.length}` +
      ` (${pct(base.ledgerAgreement, fresh.length)})`
  );
  const rebuilt = nameToFilesFrom(prior, false);
  let rebuildMismatch = 0;
  for (const [k, v] of rebuilt) {
    const s = shippedNames.get(k);
    if (!s || s.length !== v.length || s.some((f, i) => f !== v[i])) {
      rebuildMismatch++;
    }
  }
  console.log(
    `    nameToFiles rebuilt from the prior bundle differs on ${rebuildMismatch}/${rebuilt.size} names` +
      " (0 = the replay reproduces buildLedger exactly)"
  );

  const residue = base.counts.conflict + base.counts.novote;
  const anchors = new AnchorIndex(prior, fresh);
  const twins = new TwinIndex(prior);
  // Priced once and shared: the twin is for ACCOUNTING only, never a decision.
  const twinOf = fresh.map((f) => twins.find(f, anchors));
  const anchorTier = fresh.map((f) => anchors.verdict(f)?.file);

  const candidates: Array<{
    name: string;
    rule: VoteRule;
    anchor: boolean;
  }> = [
    { name: `anchor(rare<=${rareMax})`, rule: SHIPPED, anchor: true },
    {
      name: "outer",
      rule: { useOuterNames: true, allSameFirst: false },
      anchor: false
    },
    {
      name: "allsame",
      rule: { useOuterNames: false, allSameFirst: true },
      anchor: false
    },
    {
      name: "outer+allsame",
      rule: { useOuterNames: true, allSameFirst: true },
      anchor: false
    },
    {
      name: "allsame+anchor",
      rule: { useOuterNames: false, allSameFirst: true },
      anchor: true
    },
    {
      name: "outer+allsame+anchor",
      rule: { useOuterNames: true, allSameFirst: true },
      anchor: true
    }
  ];

  const results: CandidateResult[] = [];
  for (const c of candidates) {
    const alt = replay(
      {
        fresh,
        freshHashes,
        prior: priorLedger,
        priorNames: c.rule.useOuterNames ? outerNames : shippedNames,
        matchMap,
        anchorTier: c.anchor ? anchorTier : undefined
      },
      c.rule
    );
    const r: CandidateResult = {
      name: c.name,
      decides: 0,
      riskyRePlaced: 0,
      net: netEffect(fresh, alt.assignment, twinOf),
      moves: []
    };
    for (let i = 0; i < fresh.length; i++) {
      const wasLocality = isLocality(base.kind[i]);
      if (wasLocality && !isLocality(alt.kind[i])) r.decides++;
      // A statement the shipped rule placed WITH evidence that the candidate
      // puts somewhere else. Zero by construction for a tier that only fires on
      // residue; a change to the vote rule itself must be measured.
      if (!wasLocality && alt.assignment[i] !== fresh[i].file)
        r.riskyRePlaced++;
      const twin = twinOf[i];
      if (
        twin &&
        fresh[i].file !== twin.file &&
        alt.assignment[i] === twin.file
      ) {
        r.moves.push({
          recovered: recoveredLines(fresh[i], twin),
          freshIdx: i,
          from: fresh[i].file,
          to: twin.file,
          freshLines: fresh[i].lines,
          head: fresh[i].text.split("\n", 1)[0].slice(0, 110)
        });
      }
    }
    results.push(r);
  }

  console.log(`  --- DECIDES (residue = ${residue} statements) ---`);
  console.log(
    "    candidate              places  % of residue  re-places non-residue"
  );
  for (const r of results) {
    console.log(
      `    ${r.name.padEnd(22)} ${String(r.decides).padStart(6)}  ${pct(r.decides, residue).padStart(12)}  ${String(r.riskyRePlaced).padStart(21)}`
    );
  }
  console.log(
    `  --- NET relocation effect (priced twins: ${results[0].net.priced}, aligned today ${results[0].net.alignedToday}) ---`
  );
  console.log(
    "    candidate              healed   +lines   broken   -lines      NET lines"
  );
  for (const r of results) {
    const net = r.net.healedLines - r.net.brokenLines;
    console.log(
      `    ${r.name.padEnd(22)} ${String(r.net.healed).padStart(6)}  ${String(r.net.healedLines).padStart(7)}  ${String(r.net.broken).padStart(7)}  ${String(r.net.brokenLines).padStart(7)}  ${String(net).padStart(13)}`
    );
  }

  if (listN > 0) {
    for (const r of results) {
      console.log(`  largest heals — ${r.name} (EYEBALL THESE):`);
      for (const m of r.moves
        .sort((a, b) => b.recovered - a.recovered)
        .slice(0, listN)) {
        console.log(
          `    ${String(m.recovered).padStart(5)} ln  fresh#${m.freshIdx} (${m.freshLines} ln)`
        );
        console.log(`        today:  ${m.from}\n        moves:  ${m.to}`);
        console.log(`        ${m.head}`);
      }
    }
  }

  if (jsonOut) {
    fs.writeFileSync(
      jsonOut,
      JSON.stringify(
        {
          label,
          rareMax,
          statements: { prior: prior.length, fresh: fresh.length },
          replay: { ...base.counts, ledgerAgreement: base.ledgerAgreement },
          residue,
          candidates: results.map((r) => ({
            ...r,
            moves: r.moves
              .sort((a, b) => b.recovered - a.recovered)
              .slice(0, 60)
          }))
        },
        null,
        2
      )
    );
  }
  for (const r of results) {
    console.log(
      `ROW|${label ?? ""}|${r.name}|${residue}|${r.decides}|${r.net.healed}|${r.net.healedLines}|${r.net.broken}|${r.net.brokenLines}|${r.riskyRePlaced}`
    );
  }
}

main();
