import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import {
  collectMintedBindings,
  isBunToken,
  summarizeCensus
} from "./minted-census.js";
import { createIsEligible } from "./rename-eligibility.js";

const IS_ELIGIBLE = createIsEligible("bun", "bun");

function parse(code: string): t.File {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  assert.ok(ast);
  return ast as t.File;
}

function census(code: string) {
  return collectMintedBindings(parse(code), IS_ELIGIBLE);
}

describe("isBunToken (loose census shape — over-counts by design)", () => {
  it("flags Bun mint shapes", () => {
    for (const name of ["uq", "M2_", "FH3", "j2_", "$2", "H", "Tj_", "wP_"]) {
      assert.strictEqual(isBunToken(name), true, `${name} should be a token`);
    }
  });

  it("does not flag ordinary descriptive names", () => {
    for (const name of ["BaseError", "completionState", "sessionStartTime"]) {
      assert.strictEqual(isBunToken(name), false, `${name} is descriptive`);
    }
  });

  it("does not flag common short words", () => {
    for (const name of ["fs", "os", "id", "url", "map"]) {
      assert.strictEqual(isBunToken(name), false, `${name} is a real word`);
    }
  });
});

describe("collectMintedBindings — family classification", () => {
  it("classifies a class-expression inner id and its derivation source", () => {
    const bindings = census(`var BaseError = class uq extends Error {};`);
    const entry = bindings.find((b) => b.name === "uq");
    assert.ok(entry, "uq must be collected");
    assert.strictEqual(entry.family, "classExprId");
    assert.strictEqual(entry.derivedFrom, "BaseError");
  });

  it("derives from an assignment target and a property key", () => {
    const assigned = census(`Registry.Foo = class q2 {};`);
    assert.strictEqual(
      assigned.find((b) => b.name === "q2")?.derivedFrom,
      "Foo"
    );
    const prop = census(`var obj = { Widget: class w3 {} };`);
    assert.strictEqual(
      prop.find((b) => b.name === "w3")?.derivedFrom,
      "Widget"
    );
  });

  it("returns null derivation when the source is itself minted", () => {
    const bindings = census(`var Z9 = class q2 {};`);
    assert.strictEqual(
      bindings.find((b) => b.name === "q2")?.derivedFrom,
      null
    );
  });

  it("classifies params, whole declarations, and var/other", () => {
    const bindings = census(`
      function updateState(H) { return H; }
      function j2_() {}
      var Kq_ = load();
    `);
    assert.strictEqual(bindings.find((b) => b.name === "H")?.family, "param");
    assert.strictEqual(
      bindings.find((b) => b.name === "j2_")?.family,
      "fnDecl"
    );
    assert.strictEqual(
      bindings.find((b) => b.name === "Kq_")?.family,
      "varOther"
    );
  });

  it("skips eligible descriptive names and skip-listed names", () => {
    const bindings = census(`
      var completionState = load();
      function __helper() {}
    `);
    assert.deepStrictEqual(
      bindings.map((b) => b.name),
      []
    );
  });

  it("records reference counts", () => {
    const bindings = census(`
      function f() {
        var Tj_ = load();
        return Tj_ + Tj_;
      }
    `);
    assert.strictEqual(bindings.find((b) => b.name === "Tj_")?.refCount, 2);
  });
});

describe("summarizeCensus", () => {
  it("totals per family and reports derivable/zero-ref expression ids", () => {
    const bindings = census(`
      var BaseError = class uq extends Error {};
      register(class w7 {});
      function useIt(H) { return H; }
      function j2_() {}
      var Kq_ = load();
      var Wm$ = other();
    `);
    const summary = summarizeCensus(bindings);
    assert.strictEqual(summary.total, 6);
    assert.strictEqual(summary.byFamily.classExprId, 2);
    assert.strictEqual(summary.byFamily.param, 1);
    assert.strictEqual(summary.byFamily.fnDecl, 1);
    assert.strictEqual(summary.byFamily.varOther, 2);
    // uq derives from BaseError; w7 is in argument position → no source.
    // Both class ids have zero references here.
    assert.strictEqual(summary.derivableExprIds, 1);
    assert.strictEqual(summary.zeroRefExprIds, 2);
  });

  it("is all-zero for an output with no minted leftovers", () => {
    const summary = summarizeCensus(census(`var completionState = load();`));
    assert.strictEqual(summary.total, 0);
    assert.strictEqual(summary.byFamily.classExprId, 0);
  });
});
