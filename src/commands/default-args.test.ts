import assert from "node:assert";
import { describe, it } from "node:test";
import {
  DEFAULT_LLM_TIMEOUT_MS,
  MAX_DEFAULT_MODULE_CONCURRENCY,
  defaultModuleConcurrency
} from "./default-args.js";

/**
 * These pin the two invariants that made the duplicated defaults safe, so the
 * next edit that breaks one fails here instead of in a run.
 *
 * Neither duplication was a live bug when found (exp058 follow-up): the CLI
 * always supplied its timeout so the provider's smaller fallback was
 * unreachable, and the rate limiter's flat ceiling was an OUTER bound that
 * always exceeded what the processor scheduled. Both were one edit away from
 * mattering, which is the whole reason the values moved here.
 */
describe("single-sourced defaults", () => {
  it("keeps the rate-limiter ceiling at or above every module lane", () => {
    // buildProvider runs before detectBundle, so the LLM rate limiter is sized
    // against this constant rather than the detected bundler. If a lane ever
    // exceeds it, the ceiling starts throttling the processor silently.
    for (const bundler of [
      "esbuild",
      "webpack",
      "rollup",
      "browserify",
      "unknown"
    ] as const) {
      assert.ok(
        defaultModuleConcurrency(bundler) <= MAX_DEFAULT_MODULE_CONCURRENCY,
        `lane for ${bundler} (${defaultModuleConcurrency(bundler)}) exceeds the ceiling ${MAX_DEFAULT_MODULE_CONCURRENCY}`
      );
    }
    assert.ok(
      defaultModuleConcurrency(undefined) <= MAX_DEFAULT_MODULE_CONCURRENCY
    );
  });

  it("widens the lane for esbuild and not for others", () => {
    assert.strictEqual(defaultModuleConcurrency("esbuild"), 40);
    assert.strictEqual(defaultModuleConcurrency("webpack"), 20);
    assert.strictEqual(defaultModuleConcurrency(undefined), 20);
  });

  it("states the LLM timeout in one place", () => {
    // A direct provider construction that omits `timeout` must get what the
    // CLI would have passed — two experiment scripts do exactly that and were
    // silently running on a 10x smaller budget.
    assert.strictEqual(DEFAULT_LLM_TIMEOUT_MS, 300_000);
  });
});
