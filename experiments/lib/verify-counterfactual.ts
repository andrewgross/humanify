/**
 * Does the shared counterfactual harness reproduce exp058's published numbers?
 *
 *   npx tsx experiments/lib/verify-counterfactual.ts
 *
 * A measurement library nobody has checked against a known answer is a library
 * that will produce confident wrong ones. exp058 published, for the gate's
 * 2.1.215→216 pair, using its own single-purpose `ceiling-ab.ts`:
 *
 *   guard OFF (hash tier unrestricted)   66,352 churn vs prior
 *   guard ON  (shape refusal applied)    65,327          → −1,025
 *   fidelity vs the shipped tree         40,297 / 41,442
 *
 * The shipped `carriesNoContent` then reproduced the ON figure exactly, 8 hops
 * of 8. So these are the right answers, and the generalised harness has to hit
 * them. Anything else means the generalisation changed the measurement.
 */
import * as t from "@babel/types";
import { counterfactual, refuseHashes } from "./counterfactual.js";
import { bundleStatements, readBundle } from "./trees.js";
import { statementHash } from "../../src/split/statement-hash.js";

const FRESH = process.env.VERIFY_FRESH ?? "/work/exp050-cold/2.1.216";
const PRIOR = process.env.VERIFY_PRIOR ?? "/work/exp050-cold/2.1.215-rebased";

/** exp058's rule (A): a declaration whose masked form is only a declarator count. */
const isEmptyDeclaration = (s: t.Statement): boolean =>
  t.isVariableDeclaration(s) &&
  s.declarations.length > 0 &&
  s.declarations.every((d) => d.init == null);

/**
 * exp058's figures, adjusted for a KNOWN and located counting difference.
 *
 * exp058's `ceiling-ab.ts` counted `+`/`-` in `diff -u` output; this library
 * counts `<`/`>` in normal `diff`, matching the production `computeNormalDiff`
 * and most of the copies it replaces. On this pair the two disagree on exactly
 * ONE file of 1,497 — `lsp/plugin-management/file-history-tracker.js`, where
 * unified diff renders one changed line as context and normal diff counts it —
 * worth **+2 lines on each leg**.
 *
 * It cancels in the delta, which is why the delta below is exp058's unchanged.
 * The absolute totals published by exp058 are therefore 2 lines low on this
 * pair; no conclusion in that experiment depended on an absolute total.
 */
const COUNTING_BASIS_OFFSET = 2;
const EXPECTED = {
  off: { churn: 66352 + COUNTING_BASIS_OFFSET, fidelity: 40297 },
  on: { churn: 65327 + COUNTING_BASIS_OFFSET, fidelity: 41442 },
  delta: -1025
};

// The shape refusal SHIPS now (exp058, merged). So an unperturbed run is
// already the guarded one, and the honest OFF leg is the kill switch — not a
// ledger perturbation, which would be a no-op on statements the production
// code already refuses. Measuring a shipped guard by re-simulating it is how a
// harness reports a delta of 0 and looks broken.
const FLAG = "HUMANIFY_NO_EMPTY_DECL_HASH_GUARD";

// Retained as a cross-check: perturbing the ledger for the SAME statements the
// shipped guard refuses must add nothing.
const statements = bundleStatements(readBundle(FRESH), FRESH);
const hashes = statements.map((s) => statementHash(s));
const refusal = refuseHashes(hashes, (i) => isEmptyDeclaration(statements[i]));
console.log(
  `${refusal.statements} zero-initializer declarations, collateral ${refusal.collateral}`
);

process.env[FLAG] = "1";
const off = await counterfactual({ freshDir: FRESH, priorDir: PRIOR });
delete process.env[FLAG];
const on = await counterfactual({ freshDir: FRESH, priorDir: PRIOR });

const rows: Array<[string, number, number]> = [
  ["guard OFF churn", off.churnVsPrior, EXPECTED.off.churn],
  ["guard OFF fidelity", off.fidelity, EXPECTED.off.fidelity],
  ["guard ON  churn", on.churnVsPrior, EXPECTED.on.churn],
  ["guard ON  fidelity", on.fidelity, EXPECTED.on.fidelity],
  ["delta", on.churnVsPrior - off.churnVsPrior, EXPECTED.delta]
];

let bad = 0;
console.log(
  `\n${"metric".padEnd(20)} ${"got".padStart(8)} ${"exp058".padStart(8)}`
);
for (const [label, got, want] of rows) {
  const ok = got === want;
  if (!ok) bad++;
  console.log(
    `${label.padEnd(20)} ${String(got).padStart(8)} ${String(want).padStart(8)}  ${ok ? "OK" : "*** MISMATCH ***"}`
  );
}
console.log(`\ncollateral ${refusal.collateral} (exp058: 0)`);
if (refusal.collateral !== 0) bad++;

console.log(
  bad === 0
    ? "\nVERIFIED — the shared harness reproduces exp058 exactly."
    : `\n${bad} MISMATCH(ES) — the generalisation changed the measurement. Do not use it for a ceiling until this is explained.`
);
process.exit(bad === 0 ? 0 : 1);
