import { describe, it } from "node:test";
import assert from "node:assert";
import { FallbackProvider, withFallback } from "./fallback.js";
import type { LLMProvider, NameSuggestion } from "./types.js";
import type { LLMContext } from "../analysis/types.js";

const makeContext = (): LLMContext => ({
  functionCode: "function test() {}",
  calleeSignatures: [],
  callsites: [],
  usedIdentifiers: new Set()
});

describe("FallbackProvider", () => {
  describe("constructor", () => {
    it("requires at least one provider", () => {
      assert.throws(
        () => new FallbackProvider([]),
        /at least one provider/
      );
    });
  });

  describe("suggestName", () => {
    it("uses first provider when successful", async () => {
      const provider1: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          return { name: "fromProvider1" };
        }
      };
      const provider2: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          return { name: "fromProvider2" };
        }
      };

      const fallback = withFallback([provider1, provider2]);
      const result = await fallback.suggestName("test", makeContext());

      assert.strictEqual(result.name, "fromProvider1");
    });

    it("falls back to second provider when first fails", async () => {
      const provider1: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          throw new Error("Provider 1 failed");
        }
      };
      const provider2: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          return { name: "fromProvider2" };
        }
      };

      const fallback = withFallback([provider1, provider2]);
      const result = await fallback.suggestName("test", makeContext());

      assert.strictEqual(result.name, "fromProvider2");
    });

    it("tries all providers in order", async () => {
      const callOrder: number[] = [];

      const provider1: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          callOrder.push(1);
          throw new Error("Provider 1 failed");
        }
      };
      const provider2: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          callOrder.push(2);
          throw new Error("Provider 2 failed");
        }
      };
      const provider3: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          callOrder.push(3);
          return { name: "fromProvider3" };
        }
      };

      const fallback = withFallback([provider1, provider2, provider3]);
      const result = await fallback.suggestName("test", makeContext());

      assert.deepStrictEqual(callOrder, [1, 2, 3]);
      assert.strictEqual(result.name, "fromProvider3");
    });

    it("returns original name when all providers fail", async () => {
      const provider1: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          throw new Error("Provider 1 failed");
        }
      };
      const provider2: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          throw new Error("Provider 2 failed");
        }
      };

      const fallback = withFallback([provider1, provider2]);
      const result = await fallback.suggestName("originalName", makeContext());

      assert.strictEqual(result.name, "originalName");
      assert.ok(result.reasoning?.includes("failed"));
    });

    it("logs warnings when configured", async () => {
      const warnings: string[] = [];
      const originalWarn = console.warn;
      console.warn = (msg: string) => warnings.push(msg);

      try {
        const provider1: LLMProvider = {
          async suggestName(): Promise<NameSuggestion> {
            throw new Error("Test error message");
          }
        };
        const provider2: LLMProvider = {
          async suggestName(): Promise<NameSuggestion> {
            return { name: "success" };
          }
        };

        const fallback = withFallback([provider1, provider2], { logWarnings: true });
        await fallback.suggestName("test", makeContext());

        assert.ok(
          warnings.some((w) => w.includes("Test error message")),
          "Should have logged warning"
        );
      } finally {
        console.warn = originalWarn;
      }
    });

    it("does not log warnings when disabled", async () => {
      const warnings: string[] = [];
      const originalWarn = console.warn;
      console.warn = (msg: string) => warnings.push(msg);

      try {
        const provider1: LLMProvider = {
          async suggestName(): Promise<NameSuggestion> {
            throw new Error("Test error");
          }
        };
        const provider2: LLMProvider = {
          async suggestName(): Promise<NameSuggestion> {
            return { name: "success" };
          }
        };

        const fallback = withFallback([provider1, provider2], { logWarnings: false });
        await fallback.suggestName("test", makeContext());

        assert.strictEqual(warnings.length, 0, "Should not have logged warnings");
      } finally {
        console.warn = originalWarn;
      }
    });
  });

  describe("suggestFunctionName", () => {
    it("uses suggestFunctionName when available", async () => {
      const provider: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          return { name: "variable" };
        },
        async suggestFunctionName(): Promise<NameSuggestion> {
          return { name: "functionName" };
        }
      };

      const fallback = withFallback([provider]);
      const result = await fallback.suggestFunctionName("fn", makeContext());

      assert.strictEqual(result.name, "functionName");
    });

    it("falls back to suggestName when suggestFunctionName not available", async () => {
      const provider: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          return { name: "fromSuggestName" };
        }
      };

      const fallback = withFallback([provider]);
      const result = await fallback.suggestFunctionName("fn", makeContext());

      assert.strictEqual(result.name, "fromSuggestName");
    });

    it("falls back between providers for suggestFunctionName", async () => {
      const provider1: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          throw new Error("Failed");
        },
        async suggestFunctionName(): Promise<NameSuggestion> {
          throw new Error("Failed");
        }
      };
      const provider2: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          return { name: "fallbackName" };
        }
      };

      const fallback = withFallback([provider1, provider2]);
      const result = await fallback.suggestFunctionName("fn", makeContext());

      assert.strictEqual(result.name, "fallbackName");
    });
  });

  describe("suggestNames (batch)", () => {
    it("uses batch method when available", async () => {
      const provider: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          return { name: "individual" };
        },
        async suggestNames(
          requests: Array<{ name: string; context: LLMContext }>
        ): Promise<NameSuggestion[]> {
          return requests.map((r) => ({ name: r.name + "Batched" }));
        }
      };

      const fallback = withFallback([provider]);
      const results = await fallback.suggestNames([
        { name: "a", context: makeContext() },
        { name: "b", context: makeContext() }
      ]);

      assert.strictEqual(results[0].name, "aBatched");
      assert.strictEqual(results[1].name, "bBatched");
    });

    it("falls back to individual calls when batch not available", async () => {
      let callCount = 0;
      const provider: LLMProvider = {
        async suggestName(name: string): Promise<NameSuggestion> {
          callCount++;
          return { name: name + "Individual" };
        }
      };

      const fallback = withFallback([provider]);
      const results = await fallback.suggestNames([
        { name: "a", context: makeContext() },
        { name: "b", context: makeContext() }
      ]);

      assert.strictEqual(callCount, 2);
      assert.strictEqual(results[0].name, "aIndividual");
      assert.strictEqual(results[1].name, "bIndividual");
    });
  });
});
