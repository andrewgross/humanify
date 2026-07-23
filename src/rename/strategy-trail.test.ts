import assert from "node:assert";
import { beforeEach, describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { Binding, Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import { strategyTrail } from "./strategy-trail.js";

function bindingOf(code: string, name: string): Binding {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse fixture");
  let scope: Scope | undefined;
  traverse(ast as t.File, {
    Program(path) {
      scope = path.scope;
    }
  });
  const binding = scope?.getBinding(name);
  if (!binding) throw new Error(`no binding ${name}`);
  return binding;
}

describe("strategyTrail recorder", () => {
  beforeEach(() => {
    strategyTrail.reset(true);
  });

  it("records attempts in order and marks the settling strategy", () => {
    const binding = bindingOf("function q7(v) { return v; } q7(1);", "q7");
    strategyTrail.record(binding, "q7", {
      strategy: "statement-twin",
      outcome: "abstained",
      reason: "family-bucket"
    });
    strategyTrail.record(binding, "q7", {
      strategy: "fn-name-pin",
      outcome: "applied",
      newName: "packItem"
    });
    const report = strategyTrail.report();
    assert.strictEqual(report.trails.length, 1);
    const entry = report.trails[0];
    assert.strictEqual(entry.oldName, "q7");
    assert.strictEqual(entry.settledBy, "fn-name-pin");
    assert.deepStrictEqual(
      entry.trail.map((a) => `${a.strategy}:${a.outcome}`),
      ["statement-twin:abstained", "fn-name-pin:applied"]
    );
  });

  it("stops recording once settled, counting post-settle attempts", () => {
    const binding = bindingOf("function q7(v) { return v; }", "q7");
    strategyTrail.record(binding, "q7", {
      strategy: "exact-match",
      outcome: "applied",
      newName: "packItem"
    });
    strategyTrail.record(binding, "q7", {
      strategy: "module-vote",
      outcome: "applied",
      newName: "packOther"
    });
    const entry = strategyTrail.report().trails[0];
    assert.strictEqual(entry.trail.length, 1);
    assert.strictEqual(entry.postSettleAttempts, 1);
  });

  it("keeps shadowed same-name bindings apart", () => {
    const code = "var e = 1; function f() { var e = 2; return e; } f();";
    const ast = parseSync(code, { sourceType: "module" });
    if (!ast) throw new Error("Failed to parse fixture");
    let outer: Binding | undefined;
    let inner: Binding | undefined;
    traverse(ast as t.File, {
      Program(path) {
        outer = path.scope.getBinding("e");
      },
      Function(path) {
        inner = path.scope.getBinding("e");
      }
    });
    if (!outer || !inner) throw new Error("bindings not found");
    strategyTrail.record(outer, "e", {
      strategy: "binding-cascade",
      outcome: "applied",
      newName: "outerE"
    });
    strategyTrail.record(inner, "e", {
      strategy: "exact-match",
      outcome: "rejected",
      reason: "target-in-scope"
    });
    const report = strategyTrail.report();
    assert.strictEqual(report.trails.length, 2);
  });

  it("is a no-op when disabled", () => {
    strategyTrail.reset(false);
    const binding = bindingOf("function q7(v) { return v; }", "q7");
    strategyTrail.record(binding, "q7", {
      strategy: "exact-match",
      outcome: "applied",
      newName: "x"
    });
    assert.strictEqual(strategyTrail.report().trails.length, 0);
  });

  it("captures transfer-tier attempts through the real pipeline", async () => {
    const { createRenamePlugin } = await import("./plugin.js");
    const priorCode = `
      var appConfig;
      function readA() {
        for (let i = 0; i < 3; i++) { if (appConfig > i) console.log(i); }
        return appConfig;
      }
      function readB(x) {
        return x + appConfig;
      }
    `;
    const v2Code = `
      var t;
      function rA() {
        for (let i = 0; i < 3; i++) { if (t > i) console.log(i); }
        return t;
      }
      function rB(x) {
        return x + t;
      }
    `;
    const provider = {
      async suggestAllNames(request: {
        identifiers: readonly string[];
      }): Promise<{ renames: Record<string, string> }> {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) renames[id] = `${id}Fresh`;
        return { renames };
      }
    };
    strategyTrail.reset(true);
    const rename = createRenamePlugin({
      provider,
      priorVersionCode: priorCode
    });
    const result = await rename(v2Code);
    assert.strictEqual(result.parseFailure, undefined);

    const { funnel, trails } = strategyTrail.report();
    // The two exact-matched readers vote `t` into appConfig — the trail
    // must show the vote-routing attempts and the module-vote apply.
    assert.ok(
      (funnel["module-vote"]?.applied ?? 0) >= 1,
      `expected a module-vote apply in the funnel, got ${JSON.stringify(funnel)}`
    );
    const voted = trails.find((e) => e.settledBy === "module-vote");
    assert.ok(voted, "the voted binding carries a settled trail");
    assert.ok(
      voted.trail.some(
        (a) => a.outcome === "vote" && a.reason === "external-reference"
      ),
      `trail should show the vote routing before the apply, got ${JSON.stringify(voted.trail)}`
    );
  });

  it("rolls attempts up into a per-strategy funnel", () => {
    const a = bindingOf("function q7(v) { return v; }", "q7");
    const b = bindingOf("function w3(v) { return v; }", "w3");
    strategyTrail.record(a, "q7", {
      strategy: "statement-twin",
      outcome: "applied",
      newName: "packItem"
    });
    strategyTrail.record(b, "w3", {
      strategy: "statement-twin",
      outcome: "rejected",
      reason: "target-in-scope"
    });
    strategyTrail.record(b, "w3", {
      strategy: "module-pin",
      outcome: "abstained",
      reason: "non-exact-source"
    });
    const { funnel } = strategyTrail.report();
    assert.strictEqual(funnel["statement-twin"].applied, 1);
    assert.strictEqual(funnel["statement-twin"].rejected, 1);
    assert.strictEqual(funnel["module-pin"].abstained, 1);
  });
});
