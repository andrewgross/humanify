import assert from "node:assert";
import { beforeEach, describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { Binding, Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import { clearBabelTraverseCache, traverse } from "../babel-utils.js";
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
    strategyTrail.record(binding, "q7", {
      strategy: "exact-match",
      outcome: "vote",
      reason: "external-reference",
      newName: "packItem"
    });
    const entry = strategyTrail.report().trails[0];
    assert.strictEqual(entry.trail.length, 1);
    assert.strictEqual(entry.postSettleAttempts, 1);
    assert.strictEqual(
      entry.postSettleVotes,
      1,
      "post-settle votes are expected testimony, tracked separately"
    );
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
    // A second bare declaration breaks the statement-twin tier's 1:1
    // hash uniqueness — with exp066's content-free elimination the twin
    // tier would otherwise settle `t` deterministically before any vote
    // could route (a better outcome, but not the path this test guards).
    const v2Code = `
      var t;
      var u9;
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

  it("post-pass applies append past settling and set terminalBy", () => {
    // Floor/reconcile passes legitimately re-rename SETTLED bindings —
    // recording them must not trip the clobber counter, and the entry's
    // terminal namer must follow the last applied strategy.
    const binding = bindingOf("function q7(v) { return v; }", "q7");
    strategyTrail.record(binding, "q7", {
      strategy: "exact-match",
      outcome: "applied",
      newName: "initializeApp_"
    });
    strategyTrail.recordPostPass(binding, "initializeApp_", {
      strategy: "decoration-retry",
      outcome: "applied",
      newName: "initializeApp"
    });
    const entry = strategyTrail.report().trails[0];
    assert.strictEqual(entry.trail.length, 2);
    assert.strictEqual(entry.settledBy, "exact-match");
    assert.strictEqual(entry.terminalBy, "decoration-retry");
    assert.strictEqual(entry.postSettleAttempts, 0, "not a clobber");
  });

  it("post-pass entries for unseen bindings create their own trail", () => {
    // The reconcile and deferred sweep act on privately re-parsed ASTs —
    // their bindings were never seen by the transfer tiers.
    const binding = bindingOf("var iIn = 1; console.log(iIn);", "iIn");
    strategyTrail.recordPostPass(binding, "iIn", {
      strategy: "reconcile-asymmetric",
      outcome: "applied",
      newName: "T7Class"
    });
    const entry = strategyTrail.report().trails[0];
    assert.strictEqual(entry.oldName, "iIn");
    assert.strictEqual(entry.settledBy, undefined, "no transfer tier settled");
    assert.strictEqual(entry.terminalBy, "reconcile-asymmetric");
  });

  it("post-pass abstains are recorded without touching terminal state", () => {
    const binding = bindingOf("var q8 = 1; console.log(q8);", "q8");
    strategyTrail.recordPostPass(binding, "q8", {
      strategy: "coverage-sweep",
      outcome: "abstained",
      reason: "llm-declined"
    });
    const entry = strategyTrail.report().trails[0];
    assert.strictEqual(entry.terminalBy, undefined);
    assert.strictEqual(entry.trail.length, 1);
    const { funnel } = strategyTrail.report();
    assert.strictEqual(funnel["coverage-sweep"].abstained, 1);
  });

  it("record() sets terminalBy on the settling apply", () => {
    const binding = bindingOf("function q7(v) { return v; }", "q7");
    strategyTrail.record(binding, "q7", {
      strategy: "exact-match",
      outcome: "applied",
      newName: "packItem"
    });
    const entry = strategyTrail.report().trails[0];
    assert.strictEqual(entry.terminalBy, "exact-match");
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

/**
 * How many references did the binding have WHEN IT WAS RENAMED?
 *
 * Both capture guards depend entirely on `binding.referencePaths`, and neither
 * has a fallback:
 *
 *   wouldRenameShadowInChildScope — iterates referencePaths; an empty list
 *     means the loop body never runs and it returns false.
 *   wouldCaptureOuterReference    — `outer.referencePaths.some(...)`; an empty
 *     list is false.
 *
 * So an empty or incomplete list makes BOTH guards pass in either order, which
 * is the leading explanation for exp059: two bindings in nested scopes were
 * both renamed to `dirPath`, producing `dirPath !== dirPath` (always false) and
 * `dirPath = dirPath` (a no-op) in shipped output.
 *
 * Recording the count makes that checkable from a run instead of arguable: a
 * rename applied to a binding the guards believed had ZERO references, in a
 * file whose source plainly references it, is the smoking gun.
 */
describe("strategy trail records the reference count at rename time", () => {
  beforeEach(() => {
    strategyTrail.reset(true);
  });

  it("carries refCount through to the recorded attempt", () => {
    const binding = bindingOf("let outerDir = 1; outerDir = 2;", "outerDir");
    strategyTrail.record(binding, "outerDir", {
      strategy: "llm",
      outcome: "applied",
      newName: "dirPath",
      refCount: 0
    });
    const entry = strategyTrail
      .report()
      .trails.find((t) => t.oldName === "outerDir");
    assert.ok(entry, "the binding must be in the report");
    assert.strictEqual(
      entry.trail[0].refCount,
      0,
      "a zero reference count is the finding, so it must survive to the report"
    );
  });

  it("distinguishes zero references from an unrecorded count", () => {
    // `undefined` means the caller did not measure; 0 means it measured and
    // found none. Collapsing them would make the smoking gun unreadable.
    const binding = bindingOf("let a = 1; a = 2;", "a");
    strategyTrail.record(binding, "a", {
      strategy: "exact-match",
      outcome: "applied",
      newName: "x"
    });
    const entry = strategyTrail.report().trails.find((t) => t.oldName === "a");
    assert.strictEqual(entry?.trail[0].refCount, undefined);
  });
});

/**
 * The trail keyed entries by the Babel `Binding` OBJECT. Babel mints a NEW
 * Binding (and Scope) for the same lexical binding whenever a fresh NodePath is
 * created for an already-scoped node — its `Scope` constructor returns the
 * cached scope only when `cached.path === path`
 * (@babel/traverse/lib/scope/index.js:320-323). The pipeline does exactly that:
 * the unified graph retains handles across `clearBabelCacheAfterPriorMatch`,
 * so two scope epochs coexist over one AST (exp059, `scope-era.test.ts`).
 *
 * The consequence is not cosmetic. `postSettleAttempts` is the CLOBBER
 * DETECTOR — it counts a second strategy renaming an already-settled binding.
 * Split the entry across epochs and the second apply lands on a FRESH entry
 * with no `settledBy`, so the counter cannot fire, and it reports a clean zero
 * while doing so. The diagnostic under-reports precisely the phenomenon it
 * exists to catch, which is how exp059 stayed invisible.
 *
 * Keyed by the declaration IDENTIFIER NODE it is epoch-stable — the same choice
 * `structural-hash.ts` (slotByDeclId) and `validated-rename.ts` (renameClaims)
 * each arrived at independently.
 */
describe("strategy trail survives a scope epoch", () => {
  beforeEach(() => {
    strategyTrail.reset(true);
  });

  it("keeps ONE entry for one lexical binding, and still counts the clobber", () => {
    const code = "var target = 1; use(target);";
    const ast = parseSync(code, { sourceType: "module" });
    if (!ast) throw new Error("parse failed");

    const bindingIn = (): Binding => {
      let b: Binding | undefined;
      traverse(ast as t.File, {
        Program(p) {
          b = p.scope.getBinding("target");
        }
      });
      if (!b) throw new Error("no binding");
      return b;
    };

    const eraA = bindingIn();
    clearBabelTraverseCache();
    const eraB = bindingIn();

    assert.notStrictEqual(
      eraA,
      eraB,
      "precondition: a cache clear must yield a DIFFERENT Binding object — " +
        "if this ever fails, the epoch hazard is gone and this test is obsolete"
    );
    assert.strictEqual(
      eraA.identifier,
      eraB.identifier,
      "but both wrap the SAME declaration identifier node, which is why it is a usable key"
    );

    strategyTrail.record(eraA, "target", {
      strategy: "exact-match",
      outcome: "applied",
      newName: "firstName"
    });
    // A second strategy renames the SAME lexical binding through the other
    // epoch's object. This is a clobber and must be reported as one.
    strategyTrail.record(eraB, "target", {
      strategy: "llm",
      outcome: "applied",
      newName: "secondName"
    });

    const report = strategyTrail.report();
    assert.strictEqual(
      report.trails.length,
      1,
      "one lexical binding must produce ONE trail entry, not one per scope epoch"
    );
    assert.strictEqual(
      report.trails[0].settledBy,
      "exact-match",
      "the first apply settled it"
    );
    assert.strictEqual(
      report.trails[0].postSettleAttempts,
      1,
      "the second apply is a CLOBBER — this counter existing but never firing is " +
        "worse than not having it, because a zero reads as 'no clobbers happened'"
    );
  });
});
