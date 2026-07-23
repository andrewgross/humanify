import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { collectEvalWithTaint } from "../analysis/soundness.js";
import { generate } from "../babel-utils.js";
import { retryDecoratedNames } from "./decoration-retry.js";
import { createIsEligible } from "./rename-eligibility.js";
import { strategyTrail } from "./strategy-trail.js";

const IS_ELIGIBLE = createIsEligible("bun", "bun");

function run(code: string) {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File;
  assert.ok(ast);
  const result = retryDecoratedNames(
    ast,
    IS_ELIGIBLE,
    collectEvalWithTaint(ast)
  );
  return { result, output: generate(ast, { compact: false }).code };
}

describe("retryDecoratedNames", () => {
  it("undecorates a trailing-underscore name when the blocker is gone", () => {
    const { result, output } = run(`
      function f() {
        var initializeApp_ = boot();
        return initializeApp_;
      }
    `);
    assert.strictEqual(result.undecorated, 1);
    assert.match(output, /var initializeApp =/);
    assert.doesNotMatch(output, /initializeApp_/);
  });

  it("leaves the decorated name when the undecorated stem is still bound", () => {
    const { result, output } = run(`
      function f() {
        var initializeApp = realOne();
        var initializeApp_ = other();
        return initializeApp + initializeApp_;
      }
    `);
    assert.strictEqual(result.undecorated, 0);
    assert.match(output, /initializeApp_/);
  });

  it("does not touch genuine minified tokens (stem is itself minted)", () => {
    // `Tj_` strips to `Tj`, which is still a minted token — not a decorated
    // descriptive name, so decoration-retry ignores it (the derivation /
    // sweep own that binding).
    const { result, output } = run(`
      function f() {
        var Tj_ = load();
        return Tj_;
      }
    `);
    assert.strictEqual(result.undecorated, 0);
    assert.match(output, /Tj_/);
  });

  it("skips eval-tainted scopes", () => {
    const { result, output } = run(`
      var sessionManager_ = make();
      eval("x");
    `);
    assert.strictEqual(result.undecorated, 0);
    assert.match(output, /sessionManager_/);
  });

  it("rewrites references when it undecorates", () => {
    const { result, output } = run(`
      function f() {
        var renderTree_ = build();
        use(renderTree_);
        return renderTree_.root;
      }
    `);
    assert.strictEqual(result.undecorated, 1);
    assert.doesNotMatch(output, /renderTree_/);
    assert.match(output, /use\(renderTree\)/);
    assert.match(output, /renderTree\.root/);
  });
});

describe("retryDecoratedNames — strategy trail", () => {
  it("records applies and blocked retries as decoration-retry attempts", () => {
    strategyTrail.reset(true);
    try {
      run(`
        function f() {
          var initializeApp_ = boot();
          return initializeApp_;
        }
        function g() {
          var loadConfig = realOne();
          var loadConfig_ = other();
          return loadConfig + loadConfig_;
        }
      `);
      const { funnel } = strategyTrail.report();
      assert.strictEqual(funnel["decoration-retry"].applied, 1);
      assert.strictEqual(funnel["decoration-retry"].abstained, 1);
    } finally {
      strategyTrail.reset(false);
    }
  });
});
