import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { assignFossil } from "./fossil-assign.js";
import type { StableSplitLedger } from "./stable-split.js";
import { STATEMENT_HASH_VERSION, statementHash } from "./statement-hash.js";

function bodyOf(code: string): t.Statement[] {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File;
  return ast.program.body;
}

const BUNDLE = [
  "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);",
  "function alphaCore(x) { return x + 1; }",
  "var alphaState;",
  "var init_alpha = __esm(() => { alphaState = alphaCore(1); });",
  "function betaRender(y) { return alphaState + y; }",
  "var betaCache;",
  "var init_beta = __esm(() => { init_alpha(); betaCache = betaRender(2); });",
  "console.log(init_beta);"
].join("\n");

describe("assignFossil", () => {
  const body = bodyOf(BUNDLE);
  const hashes = body.map(statementHash);

  it("assigns each module's statements to one file, eager tail to bootstrap", () => {
    const out = assignFossil(body, hashes, undefined);
    assert.strictEqual(out.assignment.length, body.length);
    // one file per module
    const fileA = out.assignment[1];
    assert.strictEqual(out.assignment[2], fileA);
    assert.strictEqual(out.assignment[3], fileA);
    const fileB = out.assignment[4];
    assert.notStrictEqual(fileA, fileB);
    assert.strictEqual(out.assignment[6], fileB);
    // eager tail
    assert.strictEqual(out.assignment[7], "src/bootstrap.js");
    // names derive from module content, kebab-cased, under src/
    assert.match(fileA, /^src\/.*alpha-core\.js$/);
    // the ledger record carries every fresh module for the next hop
    assert.strictEqual(out.fossilModules.length, 2);
    assert.strictEqual(out.fossilModules[0].file, fileA);
  });

  it("matched modules inherit their prior file path verbatim", () => {
    const first = assignFossil(body, hashes, undefined);
    const prior: StableSplitLedger = {
      version: 1,
      files: [],
      nameToFiles: {},
      order: [],
      hashVersion: STATEMENT_HASH_VERSION,
      fossilModules: [
        { ...first.fossilModules[0], file: "src/legacy/kept-name.js" },
        first.fossilModules[1]
      ]
    };
    const second = assignFossil(body, hashes, prior);
    assert.strictEqual(second.assignment[1], "src/legacy/kept-name.js");
    // and the new ledger re-records the inherited path
    assert.strictEqual(second.fossilModules[0].file, "src/legacy/kept-name.js");
  });

  it("throws loudly when the bundle records no fossils at all", () => {
    const eager = bodyOf("var a = 1;\nconsole.log(a);");
    assert.throws(
      () => assignFossil(eager, eager.map(statementHash), undefined),
      /no module fossils/
    );
  });
});
