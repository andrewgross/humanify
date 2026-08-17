import assert from "node:assert";
import { describe, it } from "node:test";
import { FOSSIL_BOOTSTRAP_FILE } from "./fossil-assign.js";
import { stableSplitFromCode } from "./stable-split.js";

/** Filler so the fixture clears the 50-binding wrapper threshold. */
const PADDING = Array.from(
  { length: 60 },
  (_, i) => `var padFiller${i} = ${i};`
);

function wrap(bodyLines: string[]): string {
  return [
    "(function (exports, require, module, __filename, __dirname) {",
    ...bodyLines.map((l) => `  ${l}`),
    "});"
  ].join("\n");
}

const FOSSIL_FIXTURE = wrap([
  "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);",
  "function alphaCore(x) { return x + 1; }",
  "var alphaState;",
  "var init_alpha = __esm(() => { alphaState = alphaCore(1); });",
  "function betaRender(y) { return alphaState + y; }",
  "var betaCache;",
  "var init_beta = __esm(() => { init_alpha(); betaCache = betaRender(2); });",
  ...PADDING,
  "console.log(init_beta, padFiller0);"
]);

describe("stableSplitFromCode — fossil path (exp070)", () => {
  it("assigns by module fossils and records them in the ledger", async () => {
    const result = await stableSplitFromCode(FOSSIL_FIXTURE, { fossil: true });
    assert.ok(result, "fixture must be splittable");
    assert.ok(result.ledger.fossilModules);
    assert.strictEqual(result.ledger.fossilModules.length, 2);
    // Module files exist and the eager tail (padding + entry) is the
    // counted bootstrap residue.
    const files = result.ledger.files;
    assert.ok(files.includes(FOSSIL_BOOTSTRAP_FILE));
    assert.ok(
      files.some((f) => f.endsWith("alpha-core.js")),
      `expected a module file named from content, got ${files.join(", ")}`
    );
  });

  it("inherits matched module paths from the prior ledger", async () => {
    const first = await stableSplitFromCode(FOSSIL_FIXTURE, { fossil: true });
    assert.ok(first?.ledger.fossilModules);
    const renamed = structuredClone(first.ledger);
    assert.ok(renamed.fossilModules);
    renamed.fossilModules[0] = {
      ...renamed.fossilModules[0],
      file: "src/carried/alpha-legacy.js"
    };
    const second = await stableSplitFromCode(FOSSIL_FIXTURE, {
      fossil: true,
      prior: renamed
    });
    assert.ok(second?.ledger.fossilModules);
    assert.strictEqual(
      second.ledger.fossilModules[0].file,
      "src/carried/alpha-legacy.js"
    );
    assert.ok(second.ledger.files.includes("src/carried/alpha-legacy.js"));
  });

  it("fails loudly when fossil is requested but the bundle has none", async () => {
    const eagerOnly = wrap([
      "var mainConfig = 1;",
      "function runMain() { return mainConfig; }",
      ...PADDING,
      "console.log(runMain());"
    ]);
    await assert.rejects(
      () => stableSplitFromCode(eagerOnly, { fossil: true }),
      /no module fossils/
    );
  });
});
