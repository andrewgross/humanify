import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { collectEvalWithTaint } from "../analysis/soundness.js";
import { generate } from "../babel-utils.js";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import {
  collectSweepTargets,
  isSweepTarget,
  sweepMintedNames
} from "./coverage-sweep.js";
import { createIsEligible } from "./rename-eligibility.js";
import { strategyTrail } from "./strategy-trail.js";

const IS_ELIGIBLE = createIsEligible("bun", "bun");

/** Provider that names each requested identifier via `mapping[id]`. */
function mapProvider(mapping: Record<string, string>): {
  provider: LLMProvider;
  requests: BatchRenameRequest[];
} {
  const requests: BatchRenameRequest[] = [];
  const provider: LLMProvider = {
    async suggestAllNames(request) {
      requests.push(request);
      const renames: Record<string, string> = {};
      for (const id of request.identifiers) {
        if (mapping[id]) renames[id] = mapping[id];
      }
      return { renames };
    }
  };
  return { provider, requests };
}

describe("isSweepTarget (stricter than the census token shape)", () => {
  it("selects genuine short minted survivors", () => {
    for (const name of [
      "H",
      "uq",
      "Q8",
      "j2_",
      "FH3",
      "Kq_",
      "Tj_",
      "wP_",
      "$2_",
      "Wm$"
    ]) {
      assert.strictEqual(isSweepTarget(name), true, `${name} should sweep`);
    }
  });

  it("rejects the census false positives (a real word or too long)", () => {
    for (const name of [
      "LZ77Compressor", // embedded word "Compressor"
      "is2017Api", // too long, recognizable fragments
      "OS_MODULE", // SCREAMING_CASE constant
      "$context", // embedded word "context"
      "ec2MetadataServiceEndpointSelector",
      "initializeApp_", // decorated descriptive — decoration-retry's job
      "RP_ConstructorKey" // underscore-joined tail — separate population
    ]) {
      assert.strictEqual(isSweepTarget(name), false, `${name} must NOT sweep`);
    }
  });

  it("selects camel half-mints: a mint stem wearing a derived kind word", () => {
    // Archive fossils like do7Function/T7Class (exp035 task C): a
    // deterministic pass of an old pipeline glued a kind word onto the
    // minted stem. The tail carries no meaning the LLM could not beat, and
    // the reconcile pass re-inherits them every hop — sweeping is the only
    // path that ever names them properly.
    for (const name of ["do7Function", "T7Class", "sm6Factory", "u4Function"]) {
      assert.strictEqual(isSweepTarget(name), true, `${name} should sweep`);
    }
    // Heading/domain carve-outs and no-tail mints stay out.
    for (const name of ["h1Regex", "v8Engine", "iIn"]) {
      assert.strictEqual(isSweepTarget(name), false, `${name} must NOT sweep`);
    }
  });

  it("rejects anything the census would not even flag", () => {
    for (const name of ["completionState", "MAX", "URL", "fs"]) {
      assert.strictEqual(isSweepTarget(name), false, `${name}`);
    }
  });
});

describe("collectSweepTargets", () => {
  function collect(code: string) {
    const ast = parseSync(code, {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    }) as t.File;
    assert.ok(ast);
    return collectSweepTargets(ast, IS_ELIGIBLE, collectEvalWithTaint(ast));
  }

  it("collects short minted bindings and skips descriptive/false-positive shapes", () => {
    const targets = collect(`
      function outer(H) {
        var Kq_ = load();
        var completionState = other();
        var LZ77Compressor = compress();
        return H + Kq_;
      }
    `);
    assert.deepStrictEqual(targets.map((tgt) => tgt.name).sort(), ["H", "Kq_"]);
  });

  it("excludes eval-tainted scopes", () => {
    const targets = collect(`
      var Kq_ = load();
      eval("x");
    `);
    assert.deepStrictEqual(targets, []);
  });
});

describe("sweepMintedNames", () => {
  function parse(code: string): t.File {
    const ast = parseSync(code, {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    });
    assert.ok(ast);
    return ast as t.File;
  }

  async function run(code: string, mapping: Record<string, string>) {
    const ast = parse(code);
    const { provider, requests } = mapProvider(mapping);
    const result = await sweepMintedNames(
      ast,
      provider,
      IS_ELIGIBLE,
      collectEvalWithTaint(ast)
    );
    return { result, requests, output: generate(ast, { compact: false }).code };
  }

  it("names a minted param and local together, grouped by function", async () => {
    const { result, requests, output } = await run(
      `
        function outer(H) {
          var Kq_ = load(H);
          return Kq_;
        }
      `,
      { H: "inputHandle", Kq_: "loadedState" }
    );
    assert.strictEqual(result.named, 2);
    // One request for the whole function, not one per identifier.
    assert.strictEqual(requests.length, 1);
    assert.deepStrictEqual(requests[0].identifiers.sort(), ["H", "Kq_"]);
    assert.match(output, /function outer\(inputHandle\)/);
    assert.match(output, /var loadedState = load\(inputHandle\)/);
  });

  it("names a module-level minted binding from its statement window", async () => {
    const { result, output } = await run(`var Kq_ = requireShim("fs");`, {
      Kq_: "fileSystem"
    });
    assert.strictEqual(result.named, 1);
    assert.match(output, /var fileSystem = requireShim/);
  });

  it("skips a suggestion that collides with a name already in scope", async () => {
    const { result, output } = await run(
      `
        function f() {
          var used = one();
          var Kq_ = two();
          return used + Kq_;
        }
      `,
      { Kq_: "used" }
    );
    assert.strictEqual(result.named, 0);
    assert.ok(result.skipped >= 1);
    assert.match(output, /var Kq_ = two/);
  });

  it("skips when the LLM returns no name or the original", async () => {
    const { result, output } = await run(
      `function f() { var Kq_ = one(); return Kq_; }`,
      { Kq_: "Kq_" }
    );
    assert.strictEqual(result.named, 0);
    assert.match(output, /var Kq_ = one/);
  });

  it("does nothing when there are no sweep targets", async () => {
    const { result, requests } = await run(
      `function process() { var completionState = one(); return completionState; }`,
      {}
    );
    assert.strictEqual(result.named, 0);
    assert.strictEqual(requests.length, 0);
  });

  /**
   * deterministicApply (wave scheduling): two top-level statements form two
   * groups whose targets share the module scope; both suggest the same
   * name. In the default free-running mode the completion order decides the
   * winner; with deterministicApply all responses are collected first and
   * applied in group-build order, so the output is completion-order-free.
   * Resolution is deferred to a macrotask so BOTH awaits are attached —
   * the permutation then really controls continuation order.
   */
  function resolveAllWithSharedName(
    pending: Array<{
      request: BatchRenameRequest;
      resolve: (r: { renames: Record<string, string> }) => void;
    }>,
    reverseCompletions: boolean
  ): void {
    const order = reverseCompletions ? [...pending].reverse() : pending;
    for (const call of order) {
      const renames: Record<string, string> = {};
      for (const id of call.request.identifiers) {
        renames[id] = "sharedState";
      }
      call.resolve({ renames });
    }
  }

  async function runPermutedSharedName(
    reverseCompletions: boolean,
    deterministicApply: boolean
  ) {
    const ast = parse(`var uq = one();\nvar Q8 = two();\nlog(uq, Q8);`);
    const pending: Array<{
      request: BatchRenameRequest;
      resolve: (r: { renames: Record<string, string> }) => void;
    }> = [];
    const provider: LLMProvider = {
      suggestAllNames(request) {
        return new Promise((resolve) => {
          pending.push({ request, resolve });
          if (pending.length < 2) return;
          setImmediate(() =>
            resolveAllWithSharedName(pending, reverseCompletions)
          );
        });
      }
    };
    const result = await sweepMintedNames(
      ast,
      provider,
      IS_ELIGIBLE,
      collectEvalWithTaint(ast),
      { deterministicApply }
    );
    return { result, output: generate(ast, { compact: false }).code };
  }

  it("free-running default: cross-group conflict winner follows completion order (baseline)", async () => {
    const forward = await runPermutedSharedName(false, false);
    const reversed = await runPermutedSharedName(true, false);
    // The contested name lands on whichever group's response applied
    // first — the order-dependence deterministicApply exists to remove.
    assert.match(forward.output, /var sharedState = one\(\);/);
    assert.match(reversed.output, /var sharedState = two\(\);/);
    assert.notStrictEqual(forward.output, reversed.output);
  });

  it("deterministicApply resolves cross-group conflicts by group order, not completion order", async () => {
    const forward = await runPermutedSharedName(false, true);
    const reversed = await runPermutedSharedName(true, true);

    assert.strictEqual(forward.output, reversed.output);
    // The first group's target wins the contested module-scope name.
    assert.match(forward.output, /var sharedState = one\(\);/);
    assert.match(forward.output, /var Q8 = two\(\);/);
    assert.strictEqual(forward.result.named, 1);
    assert.strictEqual(forward.result.skipped, 1);
  });
});

describe("sweepMintedNames — strategy trail", () => {
  function parse(code: string): t.File {
    const ast = parseSync(code, {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    });
    assert.ok(ast);
    return ast as t.File;
  }

  it("records the half-mint sweep as a coverage-sweep apply", async () => {
    strategyTrail.reset(true);
    try {
      const ast = parse(`
        var T7Class;
        T7Class = makeBuilder();
        export { T7Class };
      `);
      const { provider } = mapProvider({ T7Class: "hashMapBuilder" });
      const result = await sweepMintedNames(
        ast,
        provider,
        IS_ELIGIBLE,
        collectEvalWithTaint(ast)
      );
      assert.strictEqual(result.named, 1);
      const { funnel, trails } = strategyTrail.report();
      assert.strictEqual(funnel["coverage-sweep"].applied, 1);
      const entry = trails.find((e) => e.terminalBy === "coverage-sweep");
      assert.ok(entry, "swept binding carries a trail entry");
      assert.strictEqual(entry.oldName, "T7Class");
    } finally {
      strategyTrail.reset(false);
    }
  });
});

describe("sweepMintedNames — below-floor suggestion refusal", () => {
  function parse(code: string): t.File {
    const ast = parseSync(code, {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    });
    assert.ok(ast);
    return ast as t.File;
  }

  it("refuses a suggestion that still fails the floor (stem echo)", async () => {
    // The LLM sometimes treats the mint stem as meaningful and returns
    // h06Result -> h06CommandResult: still a half-mint, which would
    // re-flag and re-roll every hop. Refuse it; the binding stays as-is
    // for this run (honest census row, no churn loop).
    const ast = parse("var h06Result = run();\nconsole.log(h06Result);");
    const { provider } = mapProvider({ h06Result: "h06CommandResult" });
    const result = await sweepMintedNames(
      ast,
      provider,
      IS_ELIGIBLE,
      collectEvalWithTaint(ast)
    );
    assert.strictEqual(result.named, 0);
    assert.strictEqual(result.skipped, 1);
    const out = generate(ast, { compact: false }).code;
    assert.match(out, /h06Result/, "binding must keep its current name");
  });
});
