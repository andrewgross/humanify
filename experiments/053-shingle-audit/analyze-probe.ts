/**
 * 053 — what the shingle set's self-hash prefix costs on real close pairs.
 *
 *   npx tsx experiments/053-shingle-audit/analyze-probe.ts <run.log> [label]
 *
 * Reads the `shingle-probe` lines a run emits under `HUMANIFY_SHINGLE_PROBE=1`
 * and answers the only question that matters: does dropping the prefix change
 * any VERDICT, and on how many pairs.
 *
 * The population to look at is `aligned=0` — pairs where no statement aligned,
 * so the shingle score is the only corroboration left and its verdict decides
 * whether the pair transfers names or is downgraded to LLM context. A pair with
 * an aligned statement is corroborated regardless of what the shingles say, so
 * a flip there is free.
 */
import * as fs from "node:fs";

const [LOG, LABEL = ""] = process.argv.slice(2);
if (!LOG) {
  console.error("usage: analyze-probe.ts <run.log> [label]");
  process.exit(1);
}

const LINE =
  /shingle-probe (\S+): asis=([\d.]+) noprefix=([\d.]+) edges=(\d+)\/(\d+) tokens=(\d+)\/(\d+) aligned=(\d+) verdict=(pass|fail)\/(pass|fail)/;
const EMPTY =
  /shingle-probe (\S+): empty set \(prior (\d+), fresh (\d+)\), aligned=(\d+)/;

interface Row {
  id: string;
  asIs: number;
  noPrefix: number;
  edges: [number, number];
  tokens: [number, number];
  aligned: number;
  verdictAsIs: string;
  verdictNoPrefix: string;
}

const rows: Row[] = [];
let emptyTotal = 0;
let emptyUnaligned = 0;
for (const line of fs.readFileSync(LOG, "utf8").split("\n")) {
  const e = EMPTY.exec(line);
  if (e) {
    emptyTotal++;
    if (Number(e[4]) === 0) emptyUnaligned++;
    continue;
  }
  const m = LINE.exec(line);
  if (!m) continue;
  rows.push({
    id: m[1],
    asIs: Number(m[2]),
    noPrefix: Number(m[3]),
    edges: [Number(m[4]), Number(m[5])],
    tokens: [Number(m[6]), Number(m[7])],
    aligned: Number(m[8]),
    verdictAsIs: m[9],
    verdictNoPrefix: m[10]
  });
}

const pad = (n: number | string, w = 6) => String(n).padStart(w);
const pct = (n: number, d: number) =>
  d ? `${((100 * n) / d).toFixed(1)}%`.padStart(7) : "    n/a";

const unaligned = rows.filter((r) => r.aligned === 0);
const flips = unaligned.filter(
  (r) => r.verdictAsIs === "fail" && r.verdictNoPrefix === "pass"
);
const reverse = unaligned.filter(
  (r) => r.verdictAsIs === "pass" && r.verdictNoPrefix === "fail"
);
const passBoth = unaligned.filter(
  (r) => r.verdictAsIs === "pass" && r.verdictNoPrefix === "pass"
);
const failBoth = unaligned.filter(
  (r) => r.verdictAsIs === "fail" && r.verdictNoPrefix === "fail"
);

console.log(`=== SHINGLE PREFIX AUDIT — ${LABEL || LOG} ===`);
console.log(`  close pairs probed:          ${pad(rows.length + emptyTotal)}`);
console.log(
  `    with an aligned statement: ${pad(rows.length - unaligned.length)}  (shingles moot — corroborated anyway)`
);
console.log(
  `    empty shingle set:         ${pad(emptyTotal)}  (${emptyUnaligned} of them unaligned — refused, prefix irrelevant)`
);
console.log(
  `    NO aligned statement:      ${pad(unaligned.length)}  <- shingles decide these alone`
);

console.log(`\n  verdicts on the ${unaligned.length} shingle-decided pairs:`);
console.log(
  `    pass with prefix, pass without   ${pad(passBoth.length)} ${pct(passBoth.length, unaligned.length)}`
);
console.log(
  `    FAIL with prefix, PASS without   ${pad(flips.length)} ${pct(flips.length, unaligned.length)}  <- the cost`
);
console.log(
  `    pass with prefix, fail without   ${pad(reverse.length)} ${pct(reverse.length, unaligned.length)}`
);
console.log(
  `    fail both                        ${pad(failBoth.length)} ${pct(failBoth.length, unaligned.length)}`
);
console.log(
  `ROW|${LABEL}|${rows.length + emptyTotal}|${unaligned.length}|${passBoth.length}|${flips.length}|${reverse.length}|${failBoth.length}|${emptyTotal}`
);

if (unaligned.length) {
  const mean = (xs: number[]) =>
    xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : 0;
  console.log(
    `\n  mean score on shingle-decided pairs: asis ${mean(unaligned.map((r) => r.asIs)).toFixed(3)}` +
      ` -> noprefix ${mean(unaligned.map((r) => r.noPrefix)).toFixed(3)}`
  );
  const near = unaligned.filter(
    (r) => r.verdictAsIs === "fail" && r.asIs >= 0.35 && r.asIs < 0.5
  );
  console.log(
    `  refused pairs sitting just under the 0.50 floor (>=0.35): ${near.length}`
  );
}

if (flips.length) {
  console.log(`\n  the flipped pairs (fail -> pass), worst first:`);
  for (const r of flips.sort((a, b) => b.noPrefix - a.noPrefix).slice(0, 12)) {
    console.log(
      `    ${r.id}  asis=${r.asIs.toFixed(3)} -> ${r.noPrefix.toFixed(3)}` +
        `  edges=${r.edges[0]}/${r.edges[1]} tokens=${r.tokens[0]}/${r.tokens[1]}`
    );
  }
}
