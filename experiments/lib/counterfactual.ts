/**
 * Git-capped ceilings by building the counterfactual TREE, not by attributing
 * lines to a decomposition.
 *
 * ## Why this shape
 *
 * A decomposition attributes lines; it does not bound them. exp051 measured its
 * own ledger over-charging a population by 29%, and exp057 found the error
 * running the OTHER way on two sub-causes. So nothing is attributed here: the
 * real splitter and the real runnable emitter build the tree that would exist
 * if the change had been made, and the same `diff` a reviewer runs is re-run
 * against the prior release. Both sides are real texts, so the figure cannot
 * over-charge.
 *
 * This is exp058's construction, generalised. It is the only ceiling harness in
 * the repo that exercises production code end to end, and the shipped code
 * reproduced its prediction on 8 hops of 8.
 *
 * ## How a change is simulated WITHOUT pipeline code
 *
 * Most placement and emission changes are "this tier does not get to claim this
 * statement". The hash tier's only input is `prior.hashes`, so replacing an
 * entry with an unmatchable token in a COPY of the ledger makes the tier report
 * `absent` for exactly the statements carrying it and changes nothing else —
 * `alignEmissionOrder` reads `emitHashes`, a separate array. That lets a
 * ceiling be measured BEFORE any pipeline code exists, which is the order the
 * project requires.
 *
 * ## The control that makes it a bound
 *
 * `fidelity` is not optional. A reconstruction that does not reproduce the
 * shipped tree cannot bound anything, and the gap has to be CLASSIFIED rather
 * than assumed small: on 2.1.215→216 it is 1,031 of 1,497 files byte-identical,
 * 459 differing only by the vendor re-link and 7 only by the `using` desugar —
 * both post-tree passes this harness does not run, both placement-independent,
 * so they cancel in an ON−OFF delta. Independent confirmation: subtracting that
 * constant gap from the reconstruction's churn reproduces exp056's published
 * calm-hop numbers to within 30 lines.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { topLevelStatements } from "../../src/prior-version/statement-twin.js";
import { emitRunnableCjs } from "../../src/split/cjs-emit.js";
import { emptyPriorCarry } from "../../src/split/prior-carry.js";
import { stableSplitFromCode } from "../../src/split/stable-split.js";
import { changedLinesInTree } from "./diff.js";
import {
  type SplitLedger,
  readBundle,
  readLedger,
  readMatchMap
} from "./trees.js";

export interface CounterfactualInput {
  /** Tree whose bundle is being re-split (the FRESH release). */
  freshDir: string;
  /** Tree it is diffed against, and whose ledger it inherits (the PRIOR). */
  priorDir: string;
  /**
   * Return a modified COPY of the prior ledger, or the ledger unchanged for
   * the control leg. Never mutate the argument.
   */
  perturb?: (ledger: SplitLedger) => SplitLedger;
  /** Keep the emitted tree here instead of a temp dir that is deleted. */
  outDir?: string;
}

export interface CounterfactualResult {
  /** Changed lines between the rebuilt fresh tree and the prior tree. */
  churnVsPrior: number;
  /** Per file, for reporting removed and created separately — never netted. */
  byFile: Map<string, number>;
  /** Statements placed per tier — the mechanism trail (rule 11). */
  tiers: Record<string, number>;
  /**
   * Changed lines between the rebuilt tree and the SHIPPED fresh tree. This is
   * the control: it is the reconstruction's own error, and it must be
   * explained before any delta computed from `churnVsPrior` means anything.
   */
  fidelity: number;
  emittedFiles: number;
  outDir: string;
}

/**
 * Rebuild a release's tree from its bundle, optionally under a perturbed prior
 * ledger, and measure it.
 *
 * The prior carry is assembled the way the pipeline assembles it — statement
 * texts AND the real `prior-match-map.json`. Omitting the map is not a smaller
 * experiment: it silently switches the `preempt` and `fill` tiers off, and a
 * reconstruction with them off reported "identity never dissents" in exp058's
 * first pass. When the map is absent the caller is told, because absence is a
 * fact about the run, not a default.
 */
export async function counterfactual(
  input: CounterfactualInput
): Promise<CounterfactualResult> {
  const code = readBundle(input.freshDir);
  const priorLedger = readLedger(input.priorDir);
  const ledger = input.perturb
    ? input.perturb(structuredClone(priorLedger))
    : priorLedger;

  const matchMap = readMatchMap(input.freshDir);
  if (matchMap === null) {
    console.warn(
      `  note: no prior-match-map.json in ${input.freshDir} — the preempt and ` +
        "fill tiers will be inert. That is a property of this run, not a default."
    );
  }

  const result = await stableSplitFromCode(code, {
    prior: ledger,
    priorCarry: {
      ...emptyPriorCarry(),
      matchMap: matchMap ?? new Map(),
      statementTexts: priorStatementTexts(readBundle(input.priorDir))
    }
  });
  if (!result) throw new Error(`split produced nothing for ${input.freshDir}`);

  const files = emitRunnableCjs(code, result.ledger, result.wrapper, ledger);
  const outDir = input.outDir ?? fs.mkdtempSync("/tmp/counterfactual-");
  for (const [rel, text] of files) {
    const p = path.join(outDir, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, text);
  }

  const vsPrior = changedLinesInTree(
    path.join(input.priorDir, "src"),
    path.join(outDir, "src")
  );
  const vsShipped = changedLinesInTree(
    path.join(input.freshDir, "src"),
    path.join(outDir, "src")
  );
  if (!input.outDir) fs.rmSync(outDir, { recursive: true, force: true });

  return {
    churnVsPrior: vsPrior.total,
    byFile: vsPrior.byFile,
    tiers: result.stats.byTier as unknown as Record<string, number>,
    fidelity: vsShipped.total,
    emittedFiles: files.size,
    outDir
  };
}

/**
 * Prior statement texts — the content-anchor tier's evidence.
 *
 * Without them that tier abstains and the counterfactual measures a different
 * splitter than the one that ships. exp057's trail-check learned this by
 * reading 0 moves.
 */
function priorStatementTexts(priorCode: string): readonly string[] {
  const ast = parseSync(priorCode, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast) throw new Error("could not parse the prior bundle");
  return topLevelStatements(buildUnifiedGraph(ast, "prior")).map((p) =>
    p.node.start != null && p.node.end != null
      ? priorCode.slice(p.node.start, p.node.end)
      : ""
  );
}

/**
 * The standard perturbation: deny the hash tier every statement matching a
 * predicate, by poisoning those hashes in the prior ledger.
 *
 * `collateral` is returned rather than swallowed — if a poisoned hash is also
 * carried by a statement the predicate did NOT select, the ceiling silently
 * includes it. exp058 reported 0 on every hop; a non-zero value means the
 * measurement is charging something it did not intend to.
 */
export function refuseHashes(
  freshHashes: readonly string[],
  select: (index: number) => boolean
): {
  perturb: (l: SplitLedger) => SplitLedger;
  statements: number;
  collateral: number;
} {
  const refused = new Set<string>();
  let statements = 0;
  freshHashes.forEach((h, i) => {
    if (!select(i)) return;
    statements++;
    refused.add(h);
  });
  let collateral = 0;
  freshHashes.forEach((h, i) => {
    if (refused.has(h) && !select(i)) collateral++;
  });
  return {
    statements,
    collateral,
    // No prior hashes means the hash tier is already off for every statement;
    // poisoning nothing is the honest no-op rather than a crash.
    perturb: (l) =>
      l.hashes
        ? {
            ...l,
            hashes: l.hashes.map((h, i) =>
              refused.has(h) ? `refused-${i}` : h
            )
          }
        : l
  };
}
