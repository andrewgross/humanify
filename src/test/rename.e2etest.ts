/**
 * E2E tests for the rename plugin with mocked LLM providers.
 *
 * These tests verify the full rename pipeline works correctly without
 * requiring actual LLM calls. Mocks implement suggestAllNames — the
 * batch pipeline production runs.
 */

import assert from "node:assert";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import { createRenamePlugin } from "../rename/plugin.js";

/** Batch provider that renames via a fixed map (identity for unknowns). */
function mapProvider(renames: Record<string, string>): LLMProvider {
  return {
    async suggestAllNames(request: BatchRenameRequest) {
      const out: Record<string, string> = {};
      for (const id of request.identifiers) {
        out[id] = renames[id] || id;
      }
      return { renames: out };
    }
  };
}

describe("Rename E2E", () => {
  it("transforms minified code to readable code", async () => {
    const minified = `function a(b,c){return b+c}function d(){return a(1,2)}`;

    const plugin = createRenamePlugin({
      provider: mapProvider({
        a: "addNumbers",
        b: "firstNumber",
        c: "secondNumber",
        d: "calculateSum"
      }),
      concurrency: 1
    });

    const result = await plugin(minified);

    assert.ok(result.code.includes("addNumbers"), "Should rename function a");
    assert.ok(result.code.includes("firstNumber"), "Should rename param b");
    assert.ok(result.code.includes("secondNumber"), "Should rename param c");
    assert.ok(result.code.includes("calculateSum"), "Should rename function d");
    assert.ok(
      !result.code.includes("function a("),
      "Original name should be gone"
    );
  });

  it("keeps output valid when the provider proposes colliding names", async () => {
    const code = `function a(b) { return b; }`;

    // The provider proposes the SAME name for every identifier. Legal
    // shadowing may result (param shadowing the function name is
    // behavior-preserving), but the output must parse and hold the
    // rename invariants — same-scope duplicates would fail both gates.
    const provider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const out: Record<string, string> = {};
        for (const id of request.identifiers) {
          out[id] = "clashing";
        }
        return { renames: out };
      }
    };

    const plugin = createRenamePlugin({ provider, concurrency: 1 });
    const result = await plugin(code);

    assert.strictEqual(
      result.parseFailure,
      undefined,
      `output must parse:\n${result.code}`
    );
    assert.strictEqual(
      result.semanticFailure,
      undefined,
      `rename invariants must hold:\n${result.code}`
    );
  });

  it("handles arrow functions", async () => {
    const code = `const double = (x) => x * 2;`;

    const plugin = createRenamePlugin({
      provider: mapProvider({ x: "value" }),
      concurrency: 1
    });

    const result = await plugin(code);

    assert.ok(
      result.code.includes("value"),
      "Should rename arrow function param"
    );
    assert.ok(
      !result.code.includes("(x)"),
      "Original param name should be gone"
    );
  });

  it("preserves code structure", async () => {
    const code = `
      function calculate(x, y) {
        const sum = x + y;
        const product = x * y;
        return { sum, product };
      }
    `;

    const provider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        const out: Record<string, string> = {};
        for (const id of request.identifiers) {
          out[id] = `${id}Renamed`;
        }
        return { renames: out };
      }
    };

    const plugin = createRenamePlugin({ provider, concurrency: 1 });
    const result = await plugin(code);

    assert.ok(
      result.code.includes("return"),
      "Should preserve return statement"
    );
    assert.ok(result.code.includes("+"), "Should preserve addition operator");
    assert.ok(
      result.code.includes("*"),
      "Should preserve multiplication operator"
    );
  });

  it("renames module-level bindings in code with no functions", async () => {
    // The legacy single-name path silently skipped module bindings; the
    // batch pipeline renames them.
    const code = `const x = 1; const y = 2;`;

    const plugin = createRenamePlugin({
      provider: mapProvider({ x: "firstValue", y: "secondValue" }),
      concurrency: 1
    });

    const result = await plugin(code);

    assert.ok(
      result.code.includes("firstValue"),
      `Module bindings should be renamed:\n${result.code}`
    );
    assert.ok(result.code.includes("secondValue"));
  });

  it("handles parallel processing with higher concurrency", async () => {
    const code = `
      function a() { return 1; }
      function b() { return 2; }
      function c() { return 3; }
      function d() { return 4; }
    `;

    let concurrentCalls = 0;
    let maxConcurrent = 0;

    const provider: LLMProvider = {
      async suggestAllNames(request: BatchRenameRequest) {
        concurrentCalls++;
        maxConcurrent = Math.max(maxConcurrent, concurrentCalls);

        // Simulate async work
        await new Promise((resolve) => setTimeout(resolve, 10));

        concurrentCalls--;
        const out: Record<string, string> = {};
        for (const id of request.identifiers) {
          out[id] = `${id}Fn`;
        }
        return { renames: out };
      }
    };

    const plugin = createRenamePlugin({ provider, concurrency: 4 });
    await plugin(code);

    assert.ok(
      maxConcurrent > 1,
      `Should process batches in parallel (max concurrent: ${maxConcurrent})`
    );
  });
});
