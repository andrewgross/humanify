import assert from "node:assert";
import { describe, it } from "node:test";
import { MetricsTracker } from "./metrics.js";
import { withRateLimit } from "./rate-limiter.js";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMProvider
} from "./types.js";

const makeRequest = (identifiers: string[] = ["a"]): BatchRenameRequest => ({
  code: "function test() {}",
  identifiers,
  usedNames: new Set(),
  calleeSignatures: [],
  callsites: []
});

const renameAll = (request: BatchRenameRequest): BatchRenameResponse => {
  const renames: Record<string, string> = {};
  for (const id of request.identifiers) {
    renames[id] = `${id}Renamed`;
  }
  return { renames };
};

describe("RateLimitedProvider", () => {
  describe("concurrency limiting", () => {
    it("respects maxConcurrent limit", async () => {
      let currentConcurrent = 0;
      let maxConcurrent = 0;

      const mockProvider: LLMProvider = {
        async suggestAllNames(request) {
          currentConcurrent++;
          maxConcurrent = Math.max(maxConcurrent, currentConcurrent);
          await new Promise((r) => setTimeout(r, 50));
          currentConcurrent--;
          return renameAll(request);
        }
      };

      const limited = withRateLimit(mockProvider, { maxConcurrent: 3 });

      // Fire 10 requests simultaneously
      const promises = Array.from({ length: 10 }, (_, i) =>
        limited.suggestAllNames(makeRequest([`var${i}`]))
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
        async suggestAllNames(request) {
          currentConcurrent++;
          maxConcurrent = Math.max(maxConcurrent, currentConcurrent);
          await new Promise((r) => setTimeout(r, 20));
          currentConcurrent--;
          return renameAll(request);
        }
      };

      const limited = withRateLimit(mockProvider, { maxConcurrent: 10 });

      // Fire 5 requests - should all run concurrently
      const promises = Array.from({ length: 5 }, (_, i) =>
        limited.suggestAllNames(makeRequest([`var${i}`]))
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
        async suggestAllNames(request) {
          attempts++;
          if (attempts < 3) {
            const error = new Error("rate limit exceeded 429");
            throw error;
          }
          return renameAll(request);
        }
      };

      const limited = withRateLimit(mockProvider, {
        retryAttempts: 3,
        retryDelayMs: 10 // Short delay for tests
      });

      const result = await limited.suggestAllNames(makeRequest(["test"]));

      assert.strictEqual(attempts, 3, "Should have tried 3 times");
      assert.strictEqual(result.renames.test, "testRenamed");
    });

    it("does not retry on non-retryable errors", async () => {
      let attempts = 0;

      const mockProvider: LLMProvider = {
        async suggestAllNames() {
          attempts++;
          throw new Error("Invalid API key");
        }
      };

      const limited = withRateLimit(mockProvider, {
        retryAttempts: 3,
        retryDelayMs: 10
      });

      await assert.rejects(
        () => limited.suggestAllNames(makeRequest()),
        /Invalid API key/
      );

      assert.strictEqual(attempts, 1, "Should not retry non-retryable errors");
    });

    it("fails after exhausting retries", async () => {
      let attempts = 0;

      const mockProvider: LLMProvider = {
        async suggestAllNames() {
          attempts++;
          throw new Error("network timeout");
        }
      };

      const limited = withRateLimit(mockProvider, {
        retryAttempts: 2,
        retryDelayMs: 10
      });

      await assert.rejects(
        () => limited.suggestAllNames(makeRequest()),
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
          async suggestAllNames(request) {
            attempts++;
            if (attempts === 1) {
              throw new Error(errorMsg);
            }
            return renameAll(request);
          }
        };

        const limited = withRateLimit(mockProvider, {
          retryAttempts: 1,
          retryDelayMs: 1
        });

        const result = await limited.suggestAllNames(makeRequest(["test"]));
        assert.strictEqual(
          result.renames.test,
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
        async suggestAllNames(request) {
          return renameAll(request);
        }
      };

      const limited = withRateLimit(mockProvider, {}, metrics);

      await limited.suggestAllNames(makeRequest(["test1"]));
      await limited.suggestAllNames(makeRequest(["test2"]));

      const stats = metrics.getMetrics();
      assert.strictEqual(stats.llm.completedCalls, 2);
      assert.strictEqual(stats.llm.failedCalls, 0);
      assert.strictEqual(stats.llm.inFlightCalls, 0);
    });

    it("tracks failed calls", async () => {
      const metrics = new MetricsTracker();

      const mockProvider: LLMProvider = {
        async suggestAllNames() {
          throw new Error("API error");
        }
      };

      const limited = withRateLimit(
        mockProvider,
        { retryAttempts: 0 },
        metrics
      );

      await assert.rejects(() => limited.suggestAllNames(makeRequest()));

      const stats = metrics.getMetrics();
      assert.strictEqual(stats.llm.failedCalls, 1);
      assert.strictEqual(stats.llm.completedCalls, 0);
    });

    it("tracks in-flight calls during execution", async () => {
      const metrics = new MetricsTracker();
      let capturedInFlight = 0;

      const mockProvider: LLMProvider = {
        async suggestAllNames(request) {
          capturedInFlight = metrics.getMetrics().llm.inFlightCalls;
          await new Promise((r) => setTimeout(r, 10));
          return renameAll(request);
        }
      };

      const limited = withRateLimit(
        mockProvider,
        { maxConcurrent: 5 },
        metrics
      );

      // Start 3 concurrent requests
      const promises = [
        limited.suggestAllNames(makeRequest(["a"])),
        limited.suggestAllNames(makeRequest(["b"])),
        limited.suggestAllNames(makeRequest(["c"]))
      ];

      await Promise.all(promises);

      assert.ok(capturedInFlight >= 1, "Should have captured in-flight calls");
    });
  });
});
