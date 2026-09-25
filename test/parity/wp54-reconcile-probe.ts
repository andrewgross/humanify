// WP5.4 probe: the TS post-split reconcile + bundle carry over ANY tree,
// in place — `reconcilePostSplit` + `carryIntoBundle` of
// src/commands/unified.ts, line for line, with the pipeline's own
// eligibility (bun/bun). The extra regimes the oracle pairs cannot reach
// (lesson 17: their exp050 priors share no file path, so the pass
// considered 0 files) run through this and `humanify post-split-reconcile`
// on copies of the same tree, then compare trees + reports.
//
//   npx tsx test/parity/wp54-reconcile-probe.ts <tree> <prior humanified.js> <report.json>
//
// <tree> must be a scratch COPY: changed files, the ledger and
// .humanify/humanified.js are rewritten in place, as the pipeline does.
import fs from "node:fs";
import path from "node:path";
import { carryRenamesIntoBundle } from "../../src/split/bundle-carry.js";
import { postSplitReconcile } from "../../src/split/post-split-reconcile.js";
import { splitTreeRootOf } from "../../src/split/layout.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

const [tree, priorVersion, reportPath] = process.argv.slice(2);
if (!tree || !priorVersion || !reportPath) {
  throw new Error("usage: see header");
}
const ledgerPath = path.join(tree, ".humanify", "split-ledger.json");
const ledger = JSON.parse(
  fs.readFileSync(ledgerPath, "utf-8")
) as StableSplitLedger;
const priorRoot = splitTreeRootOf(priorVersion);
const read = (root: string, file: string): string | undefined => {
  try {
    return fs.readFileSync(path.join(root, file), "utf-8");
  } catch {
    return undefined;
  }
};
const messages: string[] = [];
const result = postSplitReconcile({
  ledger,
  readFresh: (file) => read(tree, file),
  readPrior: (file) => read(priorRoot, file),
  isEligible: createIsEligible("bun", "bun")
});
let carry: ReturnType<typeof carryRenamesIntoBundle> | null = null;
if (result.changed.size === 0) {
  messages.push(
    `Post-split reconcile: no changes (considered ${result.stats.considered} file(s))`
  );
} else {
  for (const [file, text] of result.changed) {
    fs.writeFileSync(path.join(tree, file), text);
  }
  fs.writeFileSync(ledgerPath, JSON.stringify(ledger));
  const bundlePath = path.join(tree, ".humanify", "humanified.js");
  const bundleCode = fs.readFileSync(bundlePath, "utf-8");
  carry = carryRenamesIntoBundle(bundleCode, ledger, result.renames);
  if (carry.code) fs.writeFileSync(bundlePath, carry.code);
}
const report = {
  messages,
  stats: result.stats,
  changedFiles: [...result.changed.keys()].sort(),
  renames: result.renames,
  carry: carry && {
    carried: carry.carried,
    wroteBundle: carry.code !== undefined,
    abstained: Object.fromEntries(
      [...carry.abstained].sort((a, b) => (a[0] < b[0] ? -1 : 1))
    )
  }
};
fs.writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
console.log(
  `considered ${result.stats.considered}, changed ${result.stats.changed}, renames ${result.renames.length}, ` +
    `discarded ${result.stats.discarded}, corpusGated ${result.stats.corpusGated}` +
    (carry ? `, carried ${carry.carried}` : "")
);
