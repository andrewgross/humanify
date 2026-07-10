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
      "u4Function", // embedded word "Function"
      "initializeApp_", // decorated descriptive — decoration-retry's job
      "RP_ConstructorKey" // minified stem + descriptive tail
    ]) {
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
});
