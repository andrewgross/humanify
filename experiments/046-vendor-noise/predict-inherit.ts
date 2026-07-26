/**
 * How many vendor files would `createVendorBodyInheritor` reuse on an
 * ALREADY-BUILT pair of trees? Uses the shipped decision function, so it
 * predicts the lever's reach without a 75-minute pipeline run — and, run
 * after the fact, confirms the gate moved for the reason claimed.
 *
 *   npx tsx experiments/046-vendor-noise/predict-inherit.ts \
 *     <priorTreeRoot> <freshTreeRoot> [label]
 *
 * Roots are TREE ROOTS (the directories holding `vendor/`), matching what
 * `--prior-version` resolves to via `findPriorTreeRoot`.
 *
 * Measured on the exp046-factoryvar trees, 2026-07-26:
 *   85->86    files 1592, changed 1592, would inherit 1575
 *   215->216  files 1647, changed 1621, would inherit 1613
 * i.e. essentially every common vendor file changes each release and ~99% of
 * them are the same program — Task A's ceiling, reached.
 */
import fs from "node:fs";
import path from "node:path";
import { createVendorBodyInheritor } from "../../src/split/vendor-body-inherit.js";

const [priorRoot, freshRoot, label] = process.argv.slice(2);
if (!priorRoot || !freshRoot) {
  console.error(
    "usage: predict-inherit.ts <priorTreeRoot> <freshTreeRoot> [label]"
  );
  process.exit(1);
}

const inherit = createVendorBodyInheritor(priorRoot);
if (!inherit) {
  throw new Error(
    `no inheritor for ${priorRoot} — missing tree, or the kill switch is set`
  );
}

const files: string[] = [];
const walk = (dir: string) => {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, e.name);
    if (e.isDirectory()) walk(full);
    else if (e.name.endsWith(".js")) files.push(path.relative(freshRoot, full));
  }
};
walk(path.join(freshRoot, "vendor"));

let changed = 0;
for (const rel of files) {
  const fresh = fs.readFileSync(path.join(freshRoot, rel), "utf-8");
  try {
    if (fs.readFileSync(path.join(priorRoot, rel), "utf-8") !== fresh)
      changed++;
  } catch {
    /* no prior counterpart */
  }
  inherit.bytesFor(rel, fresh);
}

const { considered, inherited } = inherit.stats();
console.log(
  `${label ?? ""}: files ${files.length}, changed-vs-prior ${changed}, ` +
    `considered ${considered}, WOULD INHERIT ${inherited}`
);
