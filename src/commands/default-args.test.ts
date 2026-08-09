import assert from "node:assert";
import fs from "node:fs";
import { describe, it } from "node:test";
import {
  DEFAULT_LLM_TIMEOUT_MS,
  MAX_DEFAULT_MODULE_CONCURRENCY,
  defaultModuleConcurrency,
  resolveWaveScheduling
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

/**
 * Wave scheduling became the default in 4343b22 (2026-07-22) on a 4-pair eval
 * showing "noise HALVED vs the free loop". But the default was written only in
 * `resolveSettings`, i.e. on the CLI path. `ProcessorOptions.waveScheduling` is
 * optional, and `processor.ts` branches on it directly — so ANY caller that
 * builds the plugin without the CLI silently got the OLD free-running loop.
 *
 * That is not hypothetical: all six `createRenamePlugin` calls in
 * `src/test/rename.e2etest.ts` omit it, so the e2e suite inside `npm run check`
 * has been validating a scheduler production does not use.
 *
 * Same failure this file's own header documents for the LLM timeout: a default
 * stated in more than one place, where the divergent copy is unreachable from
 * the CLI and therefore invisible, but live for every direct caller.
 */
describe("wave scheduling default", () => {
  it("resolves to the production default when the caller omits it", () => {
    assert.strictEqual(
      resolveWaveScheduling(undefined),
      true,
      "omitting the option must mean what production does, not the opposite"
    );
  });

  it("still honours an explicit choice in both directions", () => {
    assert.strictEqual(resolveWaveScheduling(true), true);
    assert.strictEqual(
      resolveWaveScheduling(false),
      false,
      "--no-wave-scheduling must remain a real escape hatch"
    );
  });

  it("plugin.ts never reads the RAW option — every read goes through the resolver", () => {
    // The exact recurrence this file's header warns about: the processor
    // site was fixed, then two sweep sites passed `deterministicApply:
    // options.waveScheduling` raw — a non-CLI caller got a deterministic
    // processor and a completion-order-nondeterministic sweep. Any read
    // of `options.waveScheduling` outside a `resolveWaveScheduling(...)`
    // argument is a defaulting bug.
    const source = fs.readFileSync(
      new URL("../rename/plugin.ts", import.meta.url),
      "utf-8"
    );
    const raw = [...source.matchAll(/^.*options\.waveScheduling.*$/gm)]
      .map((m) => m[0])
      .filter((line) => !line.includes("resolveWaveScheduling("));
    assert.deepStrictEqual(
      raw,
      [],
      `raw options.waveScheduling reads (wrap in resolveWaveScheduling):\n${raw.join("\n")}`
    );
  });
});
