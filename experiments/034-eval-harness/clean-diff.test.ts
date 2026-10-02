/**
 * The CLEAN-DIFF BREAKDOWN (Andrew, 2026-10-02): "I lean towards not changing
 * the actual code itself when possible, so we should probably just have a
 * flag in our scoring... we can always report both numbers (or a breakdown
 * with values assigned to each thing in the soft flow)."
 *
 * The raw vs clean concept he remembered is REAL and lives here: the layout
 * scorecard has always carried the raw charge (`churnLines`, `real`) plus the
 * ex-build clean variant (`churnLinesExBuild`, `realExBuild` — build-metadata
 * inlining is the one soft category it subtracted). This pins the extension:
 * the CLEAN diff now also tolerates wrapper spelling, and every clean number
 * is a DERIVATION of the raw charge — never a second charge — so recorded
 * labels stay byte-comparable and the raw trend line stays continuous.
 *
 * The contract under test, in one line each:
 *
 *   raw        = what the frozen rules charge (real INCLUDES the soft mass);
 *   cleanReal  = realExBuild - spellingIdenticalLines;
 *   cleanChurn = churnLinesExBuild - spellingIdenticalLines;
 *   and the composition's tolerant flag reads the same clean real directly.
 *
 *   npx tsx --test experiments/034-eval-harness/clean-diff.test.ts
 */
import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { afterEach, describe, it } from "node:test";
import { layoutChurn } from "./analyze.js";
import { buildConstantChurn } from "./build-constant-churn.js";
import type { Scorecard } from "./kpis.js";
import { summarizeCards } from "./summarize.js";
import { composeDiff } from "../037-noise-source-decomposition/diff-composition.js";

/**
 * A miniature of the 2.1.207→208 hop: one wrapper-spelling flip (the soft
 * noise), one build-metadata constant bumped in BOTH files (so it clears
 * build-constant-churn's MIN_FILES=2 guard and lands in the metadata
 * category), one naming churn, one unchanged statement. Every category of the
 * soft flow is present and each is charged to a known place.
 */
const WRAPPER_FLIP_PRIOR = [
  "var hookRegistration = (0, hookRegistry.registerHook)((event, payload) => {",
  "  var totalSeen = event + payload.count;",
  "  return totalSeen;",
  "});"
].join("\n");

const WRAPPER_FLIP_FRESH = [
  "var hookRegistration = (0, hookRegistry.registerHook)(function (session, shipment) {",
  "  var runningTotal = session + shipment.count;",
  "  return runningTotal;",
  "});"
].join("\n");

/**
 * The build-metadata literal in the shape build-constant-churn.ts actually
 * scores: one FIELD PER LINE (the bundler inlines the object at many sites,
 * and its rule matches `KEY:` at line start — a one-line `var x = {...}`
 * object is invisible to it, which the first revision of this fixture was).
 */
function stamp(version: string, name: string): string {
  return [
    `var ${name} = {`,
    `  VERSION: "${version}",`,
    '  BUILD_TIME: "t",',
    '  GIT_SHA: "s"',
    "};"
  ].join("\n");
}

const SRC_A_PRIOR = [
  stamp("2.1.207", "metadataStamp"),
  WRAPPER_FLIP_PRIOR,
  "var drawnTotal = tallyDraws(4);",
  "var stableStatement = keepExactly(9);"
].join("\n");

const SRC_A_FRESH = [
  stamp("2.1.208", "metadataStamp"),
  WRAPPER_FLIP_FRESH,
  "var countedSum = tallyDraws(4);",
  "var stableStatement = keepExactly(9);"
].join("\n");

const SRC_B_PRIOR = [
  stamp("2.1.207", "otherStamp"),
  "var untouchedHelper = helperOne(3);"
].join("\n");

const SRC_B_FRESH = [
  stamp("2.1.208", "otherStamp"),
  "var untouchedHelper = helperOne(3);"
].join("\n");

const TREES: Array<[dir: string, files: Record<string, string>]> = [
  ["prior", { "a.js": SRC_A_PRIOR, "b.js": SRC_B_PRIOR }],
  ["fresh", { "a.js": SRC_A_FRESH, "b.js": SRC_B_FRESH }]
];

const tmpDirs: string[] = [];

function fixturePair(): { priorDir: string; freshDir: string } {
  const out: Record<string, string> = {};
  for (const [dir, files] of TREES) {
    const abs = fs.mkdtempSync(path.join(os.tmpdir(), `clean-diff-${dir}-`));
    tmpDirs.push(abs);
    out[dir] = abs;
    for (const [name, code] of Object.entries(files)) {
      fs.writeFileSync(path.join(abs, name), `${code}\n`);
    }
  }
  return { priorDir: out.prior, freshDir: out.fresh };
}

afterEach(() => {
  for (const d of tmpDirs.splice(0))
    fs.rmSync(d, { recursive: true, force: true });
});

describe("layoutChurn's clean-diff breakdown", () => {
  it("derives clean churn and clean real from the raw charge, category by category", () => {
    const { priorDir, freshDir } = fixturePair();
    const l = layoutChurn(priorDir, freshDir);
    // The raw charge, pinned: 8 (flip) + 4 (two bumped constants, both sides)
    // real; 2 naming; the metadata category is the two constants' 4 lines.
    assert.strictEqual(l.churnLines, 14, "noise 2 + real 12 + files 0");
    assert.strictEqual(l.real, 12);
    assert.strictEqual(l.noise, 2);
    assert.strictEqual(l.buildConstantLines, 4, "VERSION modified in 2 files");
    assert.strictEqual(l.churnLinesExBuild, 10);
    assert.strictEqual(l.realExBuild, 8);
    assert.strictEqual(l.spellingIdenticalLines, 8, "the flip, both sides");
    // THE NEW CLEAN NUMBERS: raw minus both soft categories.
    assert.deepStrictEqual(
      { churnLinesClean: l.churnLinesClean, realClean: l.realClean },
      { churnLinesClean: 2, realClean: 0 },
      "clean churn = the naming churn alone; clean real = nothing"
    );
    // The contract stated as invariants, so a future field cannot drift:
    assert.strictEqual(
      l.churnLines,
      l.noise + l.real + l.fileAddRemove,
      "raw churn is what the frozen rules charge"
    );
    assert.strictEqual(
      l.churnLinesClean,
      l.churnLinesExBuild - l.spellingIdenticalLines,
      "clean churn = ex-build churn - spelling"
    );
    assert.strictEqual(
      l.realClean,
      l.realExBuild - l.spellingIdenticalLines,
      "clean real = ex-build real - spelling"
    );
  });

  it("agrees with the composition's tolerant flag and the metadata owner", () => {
    const { priorDir, freshDir } = fixturePair();
    const l = layoutChurn(priorDir, freshDir);
    const raw = composeDiff(priorDir, freshDir);
    const tolerant = composeDiff(priorDir, freshDir, undefined, {
      spellingTolerance: "tolerant"
    });
    const metadata = buildConstantChurn(priorDir, freshDir);
    // The raw scorecard fields ARE the raw composition — one charge, one owner.
    assert.strictEqual(l.real, raw.real);
    assert.strictEqual(l.spellingIdenticalLines, raw.spellingIdenticalLines);
    assert.strictEqual(l.buildConstantLines, metadata.lines);
    // And the clean number the flag produces directly:
    assert.strictEqual(
      tolerant.real,
      raw.real - raw.spellingIdenticalLines,
      "the tolerant charge is the raw charge minus the category"
    );
    assert.strictEqual(
      l.realClean,
      tolerant.real - metadata.lines,
      "clean real = the tolerant composition's real minus build metadata"
    );
  });
});

/** Minimal valid scorecard (the shape recorded-facts.test.ts drives), with the
 * layout block the clean-diff fields live in. */
function scorecard(layout?: Record<string, number>): Scorecard {
  return {
    pair: "2.1.215->2.1.216",
    determinism: {
      functions: {
        total: 1,
        deterministic: 1,
        closeMatchLLM: 0,
        coldLLM: 0,
        pctDeterministic: 100,
        pctReachingLLM: 0
      },
      mintedLeftovers: 0
    },
    churn: {
      statements: {
        total: 1,
        unchangedClean: 1,
        unchangedChurned: 0,
        novel: 0
      },
      lines: { namingNoiseLines: 0, realLines: 0 },
      relocations: { sameNameMovedFile: 0, novelNames: 0, freshNames: 1 },
      tree: { statementsCompared: 1, relocatedStatements: 0 },
      layout: {
        churnLines: 10,
        real: 7,
        noise: 3,
        naming: 1,
        alias: 0,
        reorder: 2,
        nameOnlyLines: 5,
        ...(layout ?? {})
      }
    }
  };
}

describe("summarizeCards totals the clean numbers additively", () => {
  it("clean churn and clean real reach totals without moving any pre-existing total", () => {
    const before = summarizeCards([scorecard()]);
    const after = summarizeCards([
      scorecard({ churnLinesClean: 2, realClean: 0 })
    ]);
    assert.strictEqual(after.totals.layoutChurnLinesClean, 2);
    assert.strictEqual(after.totals.layoutRealClean, 0);
    assert.strictEqual(before.totals.layoutChurnLinesClean, 0);
    assert.strictEqual(
      after.cleanDiffCards,
      1,
      "coverage is counted, not assumed"
    );
    assert.strictEqual(before.cleanDiffCards, 0);
    const { layoutChurnLinesClean, layoutRealClean, ...afterKpis } =
      after.totals;
    const {
      layoutChurnLinesClean: beforeClean,
      layoutRealClean: beforeReal,
      ...beforeKpis
    } = before.totals;
    assert.deepStrictEqual(
      afterKpis,
      beforeKpis,
      "every pre-existing total must be untouched by the clean fields"
    );
  });

  it("totals nameOnlyLines — the TOTAL row printed 0 for it since 2026-08-19", () => {
    // layoutNameOnlyLines was initialized and PRINTED in the layout table's
    // TOTAL row but never incremented, so the total always read 0 under a
    // per-pair column that held real numbers — a printed lie, found while
    // adding the clean breakdown beside it.
    const { totals } = summarizeCards([scorecard()]);
    assert.strictEqual(
      totals.layoutNameOnlyLines,
      5,
      "the per-pair nameOnly values must reach the printed total"
    );
  });
});
