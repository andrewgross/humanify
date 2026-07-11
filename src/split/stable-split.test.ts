import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import { type StableSplitLedger, stableSplitFromCode } from "./stable-split.js";

/** Tiny budgets so a handful of statements exercise every boundary. */
const BUDGETS = {
  minSeg: 2,
  maxSeg: 4,
  maxLines: 200,
  window: 2,
  minFolder: 1,
  maxFolder: 2
};

/** Filler declarations so the fixture clears the 50-binding wrapper
 * detection threshold (WRAPPER_IIFE_BINDING_THRESHOLD). */
const PAD_COUNT = 60;
const PADDING = Array.from(
  { length: PAD_COUNT },
  (_, i) => `var padFiller${i} = ${i};`
);

function wrap(bodyLines: string[]): string {
  return [
    "(function (exports, require, module, __filename, __dirname) {",
    ...bodyLines.map((l) => `  ${l}`),
    ...PADDING.map((l) => `  ${l}`),
    "});"
  ].join("\n");
}

const FIXTURE = wrap([
  "function alphaCore(x) {",
  "  return betaHelper(x) + 1;",
  "}",
  "function betaHelper(x) {",
  "  return x * 2;",
  "}",
  "var alphaConfig = alphaCore(1);",
  "function gammaRender(y) {",
  "  return deltaFormat(y);",
  "}",
  "function deltaFormat(y) {",
  "  return String(y);",
  "}",
  "var gammaState = gammaRender(2);",
  "console.log(alphaConfig, gammaState);"
]);

describe("stableSplitFromCode", () => {
  it("returns null for non-wrapper code (caller falls back)", () => {
    const result = stableSplitFromCode("var a = 1;\nvar b = 2;", {
      budgets: BUDGETS
    });
    assert.strictEqual(result, null);
  });

  it("splits fresh: complete, parseable, name-preserving, deterministic", () => {
    const result = stableSplitFromCode(FIXTURE, { budgets: BUDGETS });
    assert.ok(result);
    // Every wrapper-body statement assigned exactly once, in order.
    assert.strictEqual(result.ledger.order.length, 7 + PAD_COUNT);
    // Every emitted file parses standalone.
    for (const [file, content] of result.fileContents) {
      assert.ok(
        parseSync(content, { sourceType: "unambiguous", configFile: false }),
        `${file} must parse`
      );
      assert.match(file, /\.js$/);
    }
    // Declared names all survive somewhere in the tree.
    const all = [...result.fileContents.values()].join("\n");
    for (const name of [
      "alphaCore",
      "betaHelper",
      "alphaConfig",
      "gammaRender",
      "deltaFormat",
      "gammaState"
    ]) {
      assert.ok(all.includes(name), `${name} must be emitted`);
    }
    // Deterministic: same input, same tree.
    const again = stableSplitFromCode(FIXTURE, { budgets: BUDGETS });
    assert.ok(again);
    assert.deepStrictEqual(
      [...again.fileContents.keys()],
      [...result.fileContents.keys()]
    );
    assert.deepStrictEqual(again.ledger, result.ledger);
  });

  it("names files after their most externally-referenced binding", () => {
    const result = stableSplitFromCode(FIXTURE, { budgets: BUDGETS });
    assert.ok(result);
    // betaHelper/deltaFormat are each referenced from another statement;
    // segment names must come from real bindings, in folder/file paths.
    for (const file of result.fileContents.keys()) {
      assert.match(
        file,
        /^[A-Za-z_$][A-Za-z0-9_$]*\/[A-Za-z_$][A-Za-z0-9_$-]*\.js$/,
        `path must be folder/name.js, got ${file}`
      );
    }
  });

  it("inherits the prior file for a matched name, overriding fresh grouping", () => {
    const prior: StableSplitLedger = {
      version: 1,
      files: ["zed/custom.js"],
      nameToFiles: {
        alphaCore: ["zed/custom.js"],
        betaHelper: ["zed/custom.js"],
        alphaConfig: ["zed/custom.js"],
        gammaRender: ["zed/custom.js"],
        deltaFormat: ["zed/custom.js"],
        gammaState: ["zed/custom.js"]
      },
      order: []
    };
    const result = stableSplitFromCode(FIXTURE, {
      budgets: BUDGETS,
      prior
    });
    assert.ok(result);
    // Everything matched -> everything lands in the prior file; the final
    // no-binding console.log follows its neighbor (locality).
    assert.deepStrictEqual([...result.fileContents.keys()], ["zed/custom.js"]);
    assert.strictEqual(result.stats.inherited, 6);
    // console.log + the padding fillers all follow their neighbor.
    assert.strictEqual(result.stats.residueLocality, 1 + PAD_COUNT);
  });

  it("maps redeclared names by ordinal on equal counts, abstains on mismatch", () => {
    const redeclared = wrap([
      "var sharedFlag = 1;",
      "function useOne(x) {",
      "  return sharedFlag + x;",
      "}",
      "var sharedFlag = 2;",
      "function useTwo(x) {",
      "  return sharedFlag * x;",
      "}"
    ]);
    const prior: StableSplitLedger = {
      version: 1,
      files: ["one/first.js", "two/second.js"],
      nameToFiles: {
        sharedFlag: ["one/first.js", "two/second.js"],
        useOne: ["one/first.js"],
        useTwo: ["two/second.js"]
      },
      order: []
    };
    const result = stableSplitFromCode(redeclared, {
      budgets: BUDGETS,
      prior
    });
    assert.ok(result);
    const first = result.fileContents.get("one/first.js") ?? "";
    const second = result.fileContents.get("two/second.js") ?? "";
    assert.match(first, /sharedFlag = 1/);
    assert.match(second, /sharedFlag = 2/);
    assert.strictEqual(result.stats.inheritedViaOrdinal, 2);

    // Count mismatch (prior saw it 3 times) -> both abstain -> locality.
    const mismatch: StableSplitLedger = {
      ...prior,
      nameToFiles: {
        ...prior.nameToFiles,
        sharedFlag: ["one/first.js", "two/second.js", "one/first.js"]
      }
    };
    const fallback = stableSplitFromCode(redeclared, {
      budgets: BUDGETS,
      prior: mismatch
    });
    assert.ok(fallback);
    assert.strictEqual(fallback.stats.inheritedViaOrdinal, 0);
  });

  it("sends disagreeing multi-name statements to their neighbor's file", () => {
    const multi = wrap([
      "function anchorFn(x) {",
      "  return x;",
      "}",
      "var { pFlag, qFlag } = anchorFn(1);",
      "console.log(pFlag, qFlag);"
    ]);
    const prior: StableSplitLedger = {
      version: 1,
      files: ["a/a.js", "b/b.js"],
      nameToFiles: {
        anchorFn: ["a/a.js"],
        pFlag: ["a/a.js"],
        qFlag: ["b/b.js"]
      },
      order: []
    };
    const result = stableSplitFromCode(multi, { budgets: BUDGETS, prior });
    assert.ok(result);
    assert.strictEqual(result.stats.conflictDisagree, 1);
    // The destructuring follows anchorFn's file (its preceding neighbor).
    assert.match(result.fileContents.get("a/a.js") ?? "", /pFlag, qFlag/);
  });
});
