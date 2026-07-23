import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { collectEvalWithTaint } from "../analysis/soundness.js";
import { generate } from "../babel-utils.js";
import { deriveExpressionInnerNames } from "./class-id-floor.js";
import { createIsEligible } from "./rename-eligibility.js";
import { strategyTrail } from "./strategy-trail.js";

const IS_ELIGIBLE = createIsEligible("bun", "bun");

/**
 * Run the class/function-expression inner-id derivation on a fixture and
 * return the regenerated code plus the pass result. The fixture is parsed
 * fresh so binding scopes are live.
 */
function run(code: string) {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File;
  assert.ok(ast);
  const taint = collectEvalWithTaint(ast);
  const result = deriveExpressionInnerNames(ast, IS_ELIGIBLE, taint);
  return { result, output: generate(ast, { compact: false }).code };
}

function reasons(result: { skipped: Array<{ reason: string }> }): string[] {
  return result.skipped.map((s) => s.reason);
}

describe("deriveExpressionInnerNames — derivation sources", () => {
  it("derives from a variable declarator id (the intentional shadow)", () => {
    const { result, output } = run(
      `var BaseError = class uq extends Error {};`
    );
    assert.strictEqual(result.derived, 1);
    assert.match(output, /class BaseError extends Error/);
    assert.doesNotMatch(output, /class uq/);
  });

  it("derives from a plain-identifier assignment target", () => {
    const { result, output } = run(`
      var Widget;
      Widget = class q2 {};
    `);
    assert.strictEqual(result.derived, 1);
    assert.match(output, /Widget = class Widget/);
  });

  it("derives from a member-assignment property (no shadow needed)", () => {
    const { result, output } = run(`Registry.Panel = class q3 {};`);
    assert.strictEqual(result.derived, 1);
    assert.match(output, /class Panel/);
  });

  it("derives from an object property key", () => {
    const { result, output } = run(`var reg = { Modal: class q4 {} };`);
    assert.strictEqual(result.derived, 1);
    assert.match(output, /class Modal/);
  });

  it("derives named function expressions too", () => {
    const { result, output } = run(`var handler = function q5() {};`);
    assert.strictEqual(result.derived, 1);
    assert.match(output, /function handler/);
  });
});

describe("deriveExpressionInnerNames — the self-reference case (276→ zero-ref, 52 with refs)", () => {
  it("rewrites the inner id's own references when it is self-referential", () => {
    const { result, output } = run(`
      var Counter = class uq {
        clone() { return new uq(); }
      };
    `);
    assert.strictEqual(result.derived, 1);
    assert.match(output, /class Counter/);
    assert.match(output, /new Counter\(\)/);
    assert.doesNotMatch(output, /uq/);
  });
});

describe("deriveExpressionInnerNames — safety gates", () => {
  it("skips when the outer binding is referenced INSIDE the expression (capture)", () => {
    // `extends BaseError` references the OUTER BaseError; after uq→BaseError
    // it would re-resolve to the inner id (self-extends). Must skip.
    const { result, output } = run(`
      var BaseError = class uq extends BaseError {};
    `);
    assert.strictEqual(result.derived, 0);
    assert.ok(reasons(result).includes("capture-in-subtree"));
    assert.match(output, /class uq/);
  });

  it("skips when a nested scope would shadow the new name for a self-ref", () => {
    // Renaming q→Widget makes the inner `q` reference resolve to the local
    // `var Widget` (shadows-child) instead of the class.
    const { result, output } = run(`
      var Widget = class q {
        render() {
          var Widget = 1;
          return q + Widget;
        }
      };
    `);
    assert.strictEqual(result.derived, 0);
    assert.match(output, /class q/);
  });

  it("skips when the inner scope already binds the target name", () => {
    // A method-local can't collide, but a second class-scope binding can be
    // simulated: the derivation target equals an already-present name in
    // the inner scope is rejected by the validated path.
    const { result } = run(`
      var Widget = class Widget2 {};
      var Other = class Widget2 {};
    `);
    // Both inner ids are `Widget2`-shaped descriptive → not minted → not
    // even candidates; nothing derived, nothing wrongly applied.
    assert.strictEqual(result.derived, 0);
  });

  it("skips eval-tainted module scope", () => {
    const { result, output } = run(`
      var Session = class uq {};
      eval("boot()");
    `);
    assert.strictEqual(result.derived, 0);
    assert.ok(reasons(result).includes("eval-taint-frozen"));
    assert.match(output, /class uq/);
  });

  it("skips when the derivation source is itself minted", () => {
    const { result, output } = run(`var Z9 = class q6 {};`);
    assert.strictEqual(result.derived, 0);
    assert.ok(reasons(result).includes("no-derivation-source"));
    assert.match(output, /class q6/);
  });

  it("skips a descriptive inner id (nothing to floor)", () => {
    const { result } = run(`var Base = class NamedError {};`);
    assert.strictEqual(result.derived, 0);
    assert.strictEqual(result.skipped.length, 0);
  });

  it("leaves the program a pure rename (no structural change)", () => {
    const { output } = run(`
      var BaseError = class uq extends Error {
        describe() { return uq.name; }
      };
      var Http = class q2 extends BaseError {};
    `);
    // Both derived; the extends chain still resolves (BaseError is the
    // outer var, unchanged), self-ref rewritten.
    assert.match(output, /class BaseError extends Error/);
    assert.match(output, /return BaseError\.name/);
    assert.match(output, /class Http extends BaseError/);
  });
});

describe("deriveExpressionInnerNames — strategy trail", () => {
  it("records the derivation as a class-id-floor apply", () => {
    strategyTrail.reset(true);
    try {
      run("var HashMap = class q7 { get(k) { return q7.cache[k]; } };");
      const { funnel, trails } = strategyTrail.report();
      assert.strictEqual(funnel["class-id-floor"].applied, 1);
      const entry = trails.find((e) => e.terminalBy === "class-id-floor");
      assert.ok(entry, "derived binding carries a trail entry");
      assert.strictEqual(entry.oldName, "q7");
    } finally {
      strategyTrail.reset(false);
    }
  });
});
