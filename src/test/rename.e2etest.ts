/**
 * E2E tests for the rename plugin with mocked LLM providers.
 *
 * These tests verify the full rename pipeline works correctly without
 * requiring actual LLM calls.
 */

import assert from "node:assert";
import { describe, it } from "node:test";
import type { LLMContext } from "../analysis/types.js";
import type { LLMProvider, NameSuggestion } from "../llm/types.js";
import { createRenamePlugin } from "../rename/plugin.js";

describe("Rename E2E", () => {
  it("transforms minified code to readable code", async () => {
    const minified = `function a(b,c){return b+c}function d(){return a(1,2)}`;

    const mockProvider: LLMProvider = {
      async suggestName(name: string): Promise<NameSuggestion> {
        const renames: Record<string, string> = {
          a: "addNumbers",
          b: "firstNumber",
          c: "secondNumber",
          d: "calculateSum"
        };
        return { name: renames[name] || name };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
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

  it("retries on name conflicts", async () => {
    const code = `function a(b) { return b; }`;

    let attempts = 0;
    const mockProvider: LLMProvider = {
      async suggestName(name: string): Promise<NameSuggestion> {
        if (name === "b") {
          attempts++;
          // First attempt conflicts with function name
          if (attempts === 1) return { name: "a" };
          return { name: "input" };
        }
        return { name: "myFunction" };
      },
      async retrySuggestName(
        _name: string,
        _rejected: string,
        _reason: string,
        _context: LLMContext
      ): Promise<NameSuggestion> {
        return { name: "inputValue" };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
      concurrency: 1
    });

    const result = await plugin(code);

    // Should have resolved the conflict - function shouldn't have conflicting param name
    assert.ok(
      !result.code.includes("function myFunction(a)"),
      "Should not have conflicting names"
    );
    assert.ok(
      !result.code.includes("function myFunction(myFunction)"),
      "Should not have self-referential param"
    );
  });

  it("handles arrow functions", async () => {
    // Note: For `const a = (b) => ...`, 'a' is a binding in the module/parent scope,
    // while 'b' is a parameter binding in the arrow function's own scope.
    // The processor renames bindings within each function's scope.
    const code = `const double = (x) => x * 2;`;

    const mockProvider: LLMProvider = {
      async suggestName(name: string): Promise<NameSuggestion> {
        const renames: Record<string, string> = {
          x: "value"
        };
        return { name: renames[name] || name };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
      concurrency: 1
    });

    const result = await plugin(code);

    // The arrow function's parameter should be renamed
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

    const mockProvider: LLMProvider = {
      async suggestName(name: string): Promise<NameSuggestion> {
        // Return the same names to verify structure is preserved
        return { name: `${name}Renamed` };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
      concurrency: 1
    });

    const result = await plugin(code);

    // Verify the code still has proper structure
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

  it("handles code with no functions", async () => {
    const code = `const x = 1; const y = 2;`;

    const mockProvider: LLMProvider = {
      async suggestName(name: string): Promise<NameSuggestion> {
        return { name: `${name}Renamed` };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
      concurrency: 1
    });

    const result = await plugin(code);

    // Should return valid code without errors
    assert.ok(
      result.code.includes("const"),
      "Should preserve const declarations"
    );
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

    const mockProvider: LLMProvider = {
      async suggestName(name: string): Promise<NameSuggestion> {
        concurrentCalls++;
        maxConcurrent = Math.max(maxConcurrent, concurrentCalls);

        // Simulate async work
        await new Promise((resolve) => setTimeout(resolve, 10));

        concurrentCalls--;
        return { name: `${name}Fn` };
      }
    };

    const plugin = createRenamePlugin({
      provider: mockProvider,
      concurrency: 4
    });

    await plugin(code);

    // With 4 independent functions and concurrency 4, we should see parallel execution
    assert.ok(
      maxConcurrent > 1,
      `Should process functions in parallel (max concurrent: ${maxConcurrent})`
    );
  });
});
