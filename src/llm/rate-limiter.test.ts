import assert from "node:assert";
import { describe, it } from "node:test";
import type { LLMContext } from "../analysis/types.js";
import { MetricsTracker } from "./metrics.js";
import { withRateLimit } from "./rate-limiter.js";
import type { LLMProvider, NameSuggestion } from "./types.js";

const makeContext = (): LLMContext => ({
  functionCode: "function test() {}",
  calleeSignatures: [],
  callsites: [],
  usedIdentifiers: new Set()
});

describe("RateLimitedProvider", () => {
  describe("concurrency limiting", () => {
    it("respects maxConcurrent limit", async () => {
      let currentConcurrent = 0;
      let maxConcurrent = 0;

      const mockProvider: LLMProvider = {
        async suggestName(name: string): Promise<NameSuggestion> {
          currentConcurrent++;
          maxConcurrent = Math.max(maxConcurrent, currentConcurrent);
          await new Promise((r) => setTimeout(r, 50));
          currentConcurrent--;
          return { name: name + "Renamed" };
        }
      };

      const limited = withRateLimit(mockProvider, { maxConcurrent: 3 });
      const context = makeContext();

      // Fire 10 requests simultaneously
      const promises = Array.from({ length: 10 }, (_, i) =>
        limited.suggestName(`var${i}`, context)
      );

      await Promise.all(promises);

      assert.ok(
        maxConcurrent <= 3,
        `Max concurrent was ${maxConcurrent}, expected <= 3`
      );
      assert.ok(maxConcurrent >= 1, "Should have had at least 1 concurrent");
    });

    it("allows full concurrency when under limit", async () => {
      let maxConcurrent = 0;
      let currentConcurrent = 0;

      const mockProvider: LLMProvider = {
        async suggestName(name: string): Promise<NameSuggestion> {
          currentConcurrent++;
          maxConcurrent = Math.max(maxConcurrent, currentConcurrent);
          await new Promise((r) => setTimeout(r, 20));
          currentConcurrent--;
          return { name: name + "Renamed" };
        }
      };

      const limited = withRateLimit(mockProvider, { maxConcurrent: 10 });
      const context = makeContext();

      // Fire 5 requests - should all run concurrently
      const promises = Array.from({ length: 5 }, (_, i) =>
        limited.suggestName(`var${i}`, context)
      );

      await Promise.all(promises);

      assert.strictEqual(
        maxConcurrent,
        5,
        "All 5 should have run concurrently"
      );
    });
  });

  describe("retry behavior", () => {
    it("retries on retryable errors", async () => {
      let attempts = 0;

      const mockProvider: LLMProvider = {
        async suggestName(name: string): Promise<NameSuggestion> {
          attempts++;
          if (attempts < 3) {
            const error = new Error("rate limit exceeded 429");
            throw error;
          }
          return { name: name + "Renamed" };
        }
      };

      const limited = withRateLimit(mockProvider, {
        retryAttempts: 3,
        retryDelayMs: 10 // Short delay for tests
      });

      const result = await limited.suggestName("test", makeContext());

      assert.strictEqual(attempts, 3, "Should have tried 3 times");
      assert.strictEqual(result.name, "testRenamed");
    });

    it("does not retry on non-retryable errors", async () => {
      let attempts = 0;

      const mockProvider: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          attempts++;
          throw new Error("Invalid API key");
        }
      };

      const limited = withRateLimit(mockProvider, {
        retryAttempts: 3,
        retryDelayMs: 10
      });

      await assert.rejects(
        () => limited.suggestName("test", makeContext()),
        /Invalid API key/
      );

      assert.strictEqual(attempts, 1, "Should not retry non-retryable errors");
    });

    it("fails after exhausting retries", async () => {
      let attempts = 0;

      const mockProvider: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          attempts++;
          throw new Error("network timeout");
        }
      };

      const limited = withRateLimit(mockProvider, {
        retryAttempts: 2,
        retryDelayMs: 10
      });

      await assert.rejects(
        () => limited.suggestName("test", makeContext()),
        /network timeout/
      );

      assert.strictEqual(attempts, 3, "Should try initial + 2 retries");
    });

    it("recognizes various retryable error patterns", async () => {
      const retryableErrors = [
        "network error",
        "timeout exceeded",
        "ECONNRESET",
        "ECONNREFUSED",
        "rate limit hit",
        "429 Too Many Requests",
        "500 Internal Server Error",
        "502 Bad Gateway",
        "503 Service Unavailable",
        "504 Gateway Timeout"
      ];

      for (const errorMsg of retryableErrors) {
        let attempts = 0;

        const mockProvider: LLMProvider = {
          async suggestName(name: string): Promise<NameSuggestion> {
            attempts++;
            if (attempts === 1) {
              throw new Error(errorMsg);
            }
            return { name: name + "Renamed" };
          }
        };

        const limited = withRateLimit(mockProvider, {
          retryAttempts: 1,
          retryDelayMs: 1
        });

        const result = await limited.suggestName("test", makeContext());
        assert.strictEqual(
          result.name,
          "testRenamed",
          `Should retry on: ${errorMsg}`
        );
      }
    });
  });

  describe("metrics integration", () => {
    it("tracks successful calls", async () => {
      const metrics = new MetricsTracker();

      const mockProvider: LLMProvider = {
        async suggestName(name: string): Promise<NameSuggestion> {
          return { name: name + "Renamed" };
        }
      };

      const limited = withRateLimit(mockProvider, {}, metrics);

      await limited.suggestName("test1", makeContext());
      await limited.suggestName("test2", makeContext());

      const stats = metrics.getMetrics();
      assert.strictEqual(stats.llm.completedCalls, 2);
      assert.strictEqual(stats.llm.failedCalls, 0);
      assert.strictEqual(stats.llm.inFlightCalls, 0);
    });

    it("tracks failed calls", async () => {
      const metrics = new MetricsTracker();

      const mockProvider: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          throw new Error("API error");
        }
      };

      const limited = withRateLimit(
        mockProvider,
        { retryAttempts: 0 },
        metrics
      );

      await assert.rejects(() => limited.suggestName("test", makeContext()));

      const stats = metrics.getMetrics();
      assert.strictEqual(stats.llm.failedCalls, 1);
      assert.strictEqual(stats.llm.completedCalls, 0);
    });

    it("tracks in-flight calls during execution", async () => {
      const metrics = new MetricsTracker();
      let capturedInFlight = 0;

      const mockProvider: LLMProvider = {
        async suggestName(name: string): Promise<NameSuggestion> {
          capturedInFlight = metrics.getMetrics().llm.inFlightCalls;
          await new Promise((r) => setTimeout(r, 10));
          return { name: name + "Renamed" };
        }
      };

      const limited = withRateLimit(
        mockProvider,
        { maxConcurrent: 5 },
        metrics
      );

      // Start 3 concurrent requests
      const promises = [
        limited.suggestName("a", makeContext()),
        limited.suggestName("b", makeContext()),
        limited.suggestName("c", makeContext())
      ];

      await Promise.all(promises);

      assert.ok(capturedInFlight >= 1, "Should have captured in-flight calls");
    });
  });

  describe("suggestFunctionName", () => {
    it("delegates to inner provider if available", async () => {
      const mockProvider: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          return { name: "variable" };
        },
        async suggestFunctionName(): Promise<NameSuggestion> {
          return { name: "functionName" };
        }
      };

      const limited = withRateLimit(mockProvider);
      const result = await limited.suggestFunctionName("fn", makeContext());

      assert.strictEqual(result.name, "functionName");
    });

    it("falls back to suggestName if suggestFunctionName not available", async () => {
      const mockProvider: LLMProvider = {
        async suggestName(): Promise<NameSuggestion> {
          return { name: "fallbackName" };
        }
      };

      const limited = withRateLimit(mockProvider);
      const result = await limited.suggestFunctionName("fn", makeContext());

      assert.strictEqual(result.name, "fallbackName");
    });
  });
});
