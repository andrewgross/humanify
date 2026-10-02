/**
 * The wrapper-SPELLING soft-noise category (2026-09-29) — the
 * `spellingIdenticalLines` tally under test.
 *
 * At the 2.1.207→208 walk hop upstream's packaging tool re-serialized its
 * bundler wrappers from `createModule((a, b) => {...})` to
 * `createModule(function (a, b) {...})` — identical code, different spelling.
 * The wrapper's AST TYPE is part of `statementHash`, so the flip breaks both
 * the tier-2 hash pairing AND the tier-3 masked-head repair, and `composeDiff`
 * charges each flipped statement FULL mass on both sides inside `real` (the
 * fake "+9,162 lines of real change"; the four files are named in
 * docs/rust-port/20-overnight-report.md §2). Andrew's decision: keep the
 * charge — the columns are frozen — but IDENTIFY the mass in a breakdown, as
 * soft noise.
 *
 * This file pins four things:
 *   1. what the detector flags (the flip, either direction, names masked);
 *   2. what it refuses (real edits, insertions, this/arguments bodies);
 *   3. that every pre-existing tally column is BYTE-IDENTICAL — the category
 *      is additive, so the same input must produce the same old numbers
 *      (the pinned values below were produced by the pre-change code);
 *   4. the CLEAN-DIFF SPELLING TOLERANCE flag (Andrew, 2026-10-02): "raw"
 *      keeps the frozen charge, "tolerant" moves the flagged mass out of
 *      `real` into the labeled category — and raw real must always equal
 *      tolerant real + the category, so the two modes are one number stated
 *      two ways, never two answers.
 *
 *   npx tsx --test experiments/037-noise-source-decomposition/wrapper-spelling.test.ts
 */
import assert from "node:assert";
import * as fs from "node:fs";
import { test } from "node:test";
import { composeFile } from "./diff-composition.js";
import { summarizeCards } from "../034-eval-harness/summarize.js";
import type { Scorecard } from "../034-eval-harness/kpis.js";

/** The real 2.1.207→208 wrapper flip, in miniature: a sequence-callee call
 * `(0, ns.createModule)(...)` whose function argument is spelled as an arrow
 * on one side and a function expression on the other. Identifiers are
 * DELIBERATELY different on the fresh side — the real hop renamed them too
 * (vendor repackaging), and the detector must be name-blind like the hash. */
const FLIP_ARROW = [
  "var hookRegistration = (0, hookRegistry.registerHook)((event, payload) => {",
  "  var totalSeen = event + payload.count;",
  "  return totalSeen;",
  "});"
].join("\n");

const FLIP_FUNCTION = [
  "var hookRegistration = (0, hookRegistry.registerHook)(function (session, shipment) {",
  "  var runningTotal = session + shipment.count;",
  "  return runningTotal;",
  "});"
].join("\n");

/** Same call, spelled as a single-parameter arrow with NO parentheses — the
 * other 207 form (`createModule)(runtimeConfigProvider => {`). */
const FLIP_ARROW_BARE = [
  "var configFactory = (0, host.createModule)(runtimeConfigProvider => {",
  "  var provider = runtimeConfigProvider;",
  "  return provider;",
  "});"
].join("\n");

const FLIP_FUNCTION_BARE = [
  "var configFactory = (0, host.createModule)(function (runtimeConfigModuleVal) {",
  "  var provider = runtimeConfigModuleVal;",
  "  return provider;",
  "});"
].join("\n");

test("an arrow -> function wrapper flip is flagged as spelling-identical", () => {
  const t = composeFile(FLIP_ARROW, FLIP_FUNCTION);
  assert.strictEqual(
    t.spellingIdenticalLines,
    8,
    "both sides' full mass (4 ln + 4 ln) is soft noise"
  );
  // ADDITIVE ONLY: the flip still charges real exactly as before.
  assert.strictEqual(t.real, 8, "the real charge must not move");
  assert.strictEqual(t.naming, 0);
});

test("a function -> arrow flip is flagged too (direction-agnostic)", () => {
  const t = composeFile(FLIP_FUNCTION, FLIP_ARROW);
  assert.strictEqual(t.spellingIdenticalLines, 8);
  assert.strictEqual(t.real, 8);
});

test("a single-parameter arrow without parentheses is flagged", () => {
  const t = composeFile(FLIP_ARROW_BARE, FLIP_FUNCTION_BARE);
  assert.strictEqual(t.spellingIdenticalLines, 8);
  assert.strictEqual(t.real, 8);
});

test("a real edit inside the flipped wrapper is NOT flagged", () => {
  // Same flip, but the body genuinely changed: `+` became `*`. Structure
  // differs, so the normalized hashes differ and the pair stays real change.
  const edited = FLIP_FUNCTION.replace(
    "session + shipment.count",
    "session * shipment.count"
  );
  const t = composeFile(FLIP_ARROW, edited);
  assert.strictEqual(
    t.spellingIdenticalLines,
    0,
    "an edited body is real change"
  );
  assert.strictEqual(t.real, 8);
});

test("literal-spelling changes (!0 vs true, string contents) are NOT the category", () => {
  // The hash covers literal values and node types, so `!0` vs `true` and
  // changed string contents both refuse — the category is wrapper SPELLING,
  // not general spelling.
  const truthyArrow = FLIP_ARROW.replace("  return totalSeen;", "  return !0;");
  const truthyFunction = FLIP_FUNCTION.replace(
    "  return runningTotal;",
    "  return true;"
  );
  assert.strictEqual(
    composeFile(truthyArrow, truthyFunction).spellingIdenticalLines,
    0,
    "!0 vs true is a node-type difference, refused"
  );
  const strArrow = FLIP_ARROW.replace("  return totalSeen;", '  return "one";');
  const strFunction = FLIP_FUNCTION.replace(
    "  return runningTotal;",
    '  return "two";'
  );
  assert.strictEqual(
    composeFile(strArrow, strFunction).spellingIdenticalLines,
    0,
    "different string literals are real change, refused"
  );
});

test("a flip whose body mentions `this` or `arguments` is refused", () => {
  // An arrow binds `this`/`arguments` lexically; flipping it to a function
  // expression changes what they refer to. Such a flip is a SEMANTIC change,
  // not spelling — refused even when the structure is otherwise identical.
  const thisArrow = FLIP_ARROW.replace(
    "  return totalSeen;",
    "  return this.count;"
  );
  const thisFunction = FLIP_FUNCTION.replace(
    "  return runningTotal;",
    "  return this.count;"
  );
  assert.strictEqual(
    composeFile(thisArrow, thisFunction).spellingIdenticalLines,
    0
  );
  const argsArrow = FLIP_ARROW.replace(
    "  return totalSeen;",
    "  return arguments.length;"
  );
  const argsFunction = FLIP_FUNCTION.replace(
    "  return runningTotal;",
    "  return arguments.length;"
  );
  assert.strictEqual(
    composeFile(argsArrow, argsFunction).spellingIdenticalLines,
    0
  );
});

test("`this` behind a nested class or function does NOT refuse (the real giants)", () => {
  // The 2.1.207->208 giants nest whole AWS client classes whose methods use
  // `this` — bound by the CLASS, not the wrapper, so the wrapper's spelling
  // flip is semantics-preserving and the pair must still be flagged. (A
  // whole-body regex guard refused exactly these and missed the 306-line
  // auth-token-manager statement.)
  const classArrow = FLIP_ARROW.replace(
    "  return totalSeen;",
    "  class NestedClient {\n" +
      "    constructor(options) {\n" +
      "      this.config = options;\n" +
      "    }\n" +
      "  }\n" +
      "  return new NestedClient(totalSeen);"
  );
  const classFunction = FLIP_FUNCTION.replace(
    "  return runningTotal;",
    "  class NestedClient {\n" +
      "    constructor(handler) {\n" +
      "      this.config = handler;\n" +
      "    }\n" +
      "  }\n" +
      "  return new NestedClient(runningTotal);"
  );
  assert.strictEqual(
    composeFile(classArrow, classFunction).spellingIdenticalLines,
    18,
    "9 lines per side, full mass both sides"
  );
});

test("a true insertion is never flagged", () => {
  // A wrapper statement with no prior counterpart is real new code, soft or
  // not — the category pairs one prior statement with one fresh statement.
  const prior = "var filler = makeCounter(1);";
  const t = composeFile(prior, `${prior}\n${FLIP_FUNCTION}`);
  assert.strictEqual(t.spellingIdenticalLines, 0);
  assert.ok(t.real > 0, "the inserted statement is still charged");
});

/** A mixed file: the flip above, an alias churn, a reorder, a naming churn, a
 * tier-3 edited pair, an insertion and a removal. Module-scope because the
 * tolerance tests below score the SAME input both ways. The five pre-change
 * columns were produced by the PRE-CHANGE code on this exact input. */
const PINNED_PRIOR = [
  FLIP_ARROW,
  'const legacyAlias = require("./shared/util.js");',
  "var keptCounter = makeCounter(1);",
  'var firstBlock = buildBlock("alpha");',
  'var secondBlock = buildBlock("beta");',
  "var drawnTotal = tallyDraws(4);",
  "var editedSource = renderPanel({ width: 10, height: 20 });",
  "var doomedStatement = retireLater(7);"
].join("\n");

const PINNED_FRESH = [
  FLIP_FUNCTION,
  'const renamedAlias = require("./shared/util.js");',
  "var keptCounter = makeCounter(1);",
  'var secondBlock = buildBlock("beta");',
  'var firstBlock = buildBlock("alpha");',
  "var countedSum = tallyDraws(4);",
  "var editedSource = renderPanel({ width: 12, height: 20 });",
  "var insertedStatement = arriveNewly(5);"
].join("\n");

test("pinned totals: every pre-existing column is byte-identical, plus the soft count", () => {
  // This test fails if the breakdown changes any of the frozen columns by a
  // single line.
  const t = composeFile(PINNED_PRIOR, PINNED_FRESH);
  // The frozen, pre-change values:
  assert.deepStrictEqual(
    {
      real: t.real,
      naming: t.naming,
      alias: t.alias,
      reorder: t.reorder,
      fileAddRemove: t.fileAddRemove
    },
    { real: 12, naming: 2, alias: 2, reorder: 2, fileAddRemove: 0 },
    "the existing columns must stay byte-identical to the pre-change code"
  );
  assert.strictEqual(
    t.spellingIdenticalLines,
    8,
    "only the flip is soft noise here (4 ln + 4 ln)"
  );
});

// ── the CLEAN-DIFF spelling tolerance flag (2026-10-02) ──────────────────────
//
// Andrew: "we should probably just have a flag in our scoring... we can always
// report both numbers (or a breakdown with values assigned to each thing in
// the soft flow)." The flag is the composition's `spellingTolerance`: "raw"
// (default) is the frozen charge every recorded label computes from, and
// "tolerant" is the clean diff — the flagged pairs are charged to their own
// labeled category instead of `real`.

test("tolerant mode moves the flip out of `real` into the labeled category", () => {
  const raw = composeFile(FLIP_ARROW, FLIP_FUNCTION);
  const tolerant = composeFile(FLIP_ARROW, FLIP_FUNCTION, {
    spellingTolerance: "tolerant"
  });
  assert.strictEqual(
    raw.real,
    8,
    "the raw charge is the default and unchanged"
  );
  assert.strictEqual(
    tolerant.real,
    0,
    "in the clean diff the flip is not real change"
  );
  assert.strictEqual(
    tolerant.spellingIdenticalLines,
    8,
    "the category holds the pair's full mass (both sides)"
  );
  assert.strictEqual(
    raw.real,
    tolerant.real + tolerant.spellingIdenticalLines,
    "raw real = tolerant real + the category, in either mode"
  );
});

test("tolerant mode moves ONLY the flip: every other column is byte-identical", () => {
  const raw = composeFile(PINNED_PRIOR, PINNED_FRESH);
  const tolerant = composeFile(PINNED_PRIOR, PINNED_FRESH, {
    spellingTolerance: "tolerant"
  });
  assert.deepStrictEqual(
    {
      naming: tolerant.naming,
      alias: tolerant.alias,
      reorder: tolerant.reorder,
      fileAddRemove: tolerant.fileAddRemove
    },
    {
      naming: raw.naming,
      alias: raw.alias,
      reorder: raw.reorder,
      fileAddRemove: raw.fileAddRemove
    },
    "the tolerance may not touch any charge but the flagged pairs'"
  );
  assert.strictEqual(
    tolerant.real,
    4,
    "the pinned file: 12 raw real - the 8-ln flip"
  );
  assert.strictEqual(
    tolerant.spellingIdenticalLines,
    raw.spellingIdenticalLines,
    "both modes report the same spelling mass"
  );
});

test("a semantically-loaded flip stays REAL in tolerant mode (refusals hold)", () => {
  // An arrow binds `this` lexically, so flipping it rebinds — the detector
  // refuses the pair and the clean diff keeps charging it as real change.
  const thisArrow = FLIP_ARROW.replace(
    "  return totalSeen;",
    "  return this.count;"
  );
  const thisFunction = FLIP_FUNCTION.replace(
    "  return runningTotal;",
    "  return this.count;"
  );
  const t = composeFile(thisArrow, thisFunction, {
    spellingTolerance: "tolerant"
  });
  assert.strictEqual(t.spellingIdenticalLines, 0);
  assert.strictEqual(
    t.real,
    8,
    "a `this`-rebinding flip is genuine change, clean or raw"
  );
});

/**
 * The real 2.1.207→208 hop, as a before/after proof: the four known files
 * (docs/rust-port/20-overnight-report.md §2) reproduce their RECORDED
 * pre-change tallies exactly, and the detector flags the wrapper flips.
 * Skipped when the walk trees are absent (CI without /work).
 */
const WALK = "/work/walk-rust-0926/trees";
const KNOWN_CASE: Array<[file: string, pre: Record<string, number>]> = [
  [
    "create-error-response-val/plugin-policy-detector.js",
    { real: 5090, naming: 0, alias: 2, reorder: 0, fileAddRemove: 0 }
  ],
  [
    "credential-provider-registry/credentials-provider-error/create-stshttp-auth-provider.js",
    { real: 1338, naming: 10, alias: 6, reorder: 0, fileAddRemove: 0 }
  ],
  [
    "shared-credentials-error/sso-token-refresh-threshold-ms.js",
    { real: 1220, naming: 2, alias: 6, reorder: 0, fileAddRemove: 0 }
  ],
  [
    "noop-data/auth-token-manager.js",
    { real: 780, naming: 4, alias: 6, reorder: 0, fileAddRemove: 0 }
  ]
];
const knownCasePresent = KNOWN_CASE.every(([f]) =>
  fs.existsSync(`${WALK}/2.1.207/src/${f}`)
);

const knownCase = knownCasePresent ? test : test.skip;
knownCase(
  "the four known 2.1.207->208 files: recorded tallies unchanged, flips flagged",
  () => {
    let soft = 0;
    for (const [file, pre] of KNOWN_CASE) {
      const t = composeFile(
        fs.readFileSync(`${WALK}/2.1.207/src/${file}`, "utf8"),
        fs.readFileSync(`${WALK}/2.1.208/src/${file}`, "utf8")
      );
      assert.deepStrictEqual(
        {
          real: t.real,
          naming: t.naming,
          alias: t.alias,
          reorder: t.reorder,
          fileAddRemove: t.fileAddRemove
        },
        pre,
        `${file}: pre-change tally must be reproduced byte-for-byte`
      );
      soft += t.spellingIdenticalLines;
    }
    // Measured 2026-09-29: the flag is 8,230 lines — EVERY wrapper flip in the
    // four files (17 statements, 4,115 statement lines, full mass on both
    // sides), and nothing else. The four files' full real charge is 8,428; the
    // 198-line difference is genuine text change (the repackaged vendor
    // require-lines) that the category must NOT swallow. Controls: the calm hop
    // 2.1.213->214 and the busy hop 2.1.215->216 both read 0.
    assert.strictEqual(
      soft,
      8230,
      "the four files' wrapper-flip mass (17 flips x both sides)"
    );
  }
);

/** The same four files under the tolerant flag: the exp094 census cross-check,
 * exact. The hop's 8,428 raw lines decompose into 8,230 spelling and 198
 * genuine (the repackaged require-paths the detector's structural-difference
 * refusal correctly keeps charged) — the numbers the clean-diff breakdown must
 * reproduce on real data (experiments/094-wrapper-spelling, measurement-side
 * table). */
knownCase(
  "tolerant mode on the four known files: 198 real + 8,230 spelling (the census)",
  () => {
    let real = 0;
    let spelling = 0;
    for (const [file] of KNOWN_CASE) {
      const t = composeFile(
        fs.readFileSync(`${WALK}/2.1.207/src/${file}`, "utf8"),
        fs.readFileSync(`${WALK}/2.1.208/src/${file}`, "utf8"),
        { spellingTolerance: "tolerant" }
      );
      real += t.real;
      spelling += t.spellingIdenticalLines;
    }
    assert.deepStrictEqual(
      { real, spelling },
      { real: 198, spelling: 8230 },
      "the clean diff must read the hop as 198 genuine lines, 8,230 spelling"
    );
  }
);

/** Minimal valid scorecard (the shape recorded-facts.test.ts drives). */
function scorecard(spelling?: number): Scorecard {
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
        ...(spelling === undefined ? {} : { spellingIdenticalLines: spelling })
      }
    }
  };
}

test("summarizeCards totals the soft-noise field without moving any KPI total", () => {
  const before = summarizeCards([scorecard()]);
  const after = summarizeCards([scorecard(42)]);
  assert.strictEqual(
    after.totals.layoutSpellingIdenticalLines,
    42,
    "the new field reaches a total"
  );
  assert.strictEqual(before.totals.layoutSpellingIdenticalLines, 0);
  const { layoutSpellingIdenticalLines, ...kpiTotals } = after.totals;
  const { layoutSpellingIdenticalLines: beforeSoft, ...beforeKpis } =
    before.totals;
  assert.deepStrictEqual(
    kpiTotals,
    beforeKpis,
    "every pre-existing total must be untouched by the soft-noise field"
  );
  assert.strictEqual(
    after.spellingCards,
    1,
    "coverage is counted, not assumed"
  );
  assert.strictEqual(before.spellingCards, 0);
});
