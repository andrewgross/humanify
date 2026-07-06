import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { collectEvalWithTaint } from "./soundness.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("failed to parse");
  return ast as t.File;
}

function taintedNames(code: string): {
  names: Set<string>;
  moduleTainted: boolean;
  siteCount: number;
} {
  const taint = collectEvalWithTaint(parse(code));
  const names = new Set<string>();
  for (const node of taint.taintedFunctions) {
    const fn = node as t.FunctionDeclaration;
    if (fn.id?.name) names.add(fn.id.name);
  }
  return {
    names,
    moduleTainted: taint.moduleTainted,
    siteCount: taint.siteCount
  };
}

describe("collectEvalWithTaint", () => {
  it("taints every function on the scope chain of a with statement", () => {
    const { names, moduleTainted } = taintedNames(`
      function outer(cfg) {
        function safe(x) { return x * 2; }
        function risky(obj) {
          with (obj) { doThing(); }
        }
        return safe(1) + risky(cfg);
      }
    `);
    assert.ok(names.has("risky"), "function containing with is tainted");
    assert.ok(names.has("outer"), "enclosing function is tainted");
    assert.ok(!names.has("safe"), "sibling off the scope chain is not");
    assert.strictEqual(moduleTainted, true);
  });

  it("taints the scope chain of a direct eval call", () => {
    const { names } = taintedNames(`
      function runner(code) {
        return eval(code);
      }
      function clean(y) { return y + 1; }
    `);
    assert.ok(names.has("runner"));
    assert.ok(!names.has("clean"));
  });

  it("does not taint calls to a locally bound eval", () => {
    const { siteCount } = taintedNames(`
      function shim(input) {
        const eval = (s) => s.length;
        return eval(input);
      }
    `);
    assert.strictEqual(siteCount, 0, "indirect eval via local binding is safe");
  });

  it("reports nothing for clean code", () => {
    const { siteCount, moduleTainted } = taintedNames(`
      function pure(a, b) { return a + b; }
    `);
    assert.strictEqual(siteCount, 0);
    assert.strictEqual(moduleTainted, false);
  });
});
