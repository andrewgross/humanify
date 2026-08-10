import assert from "node:assert";
import fs from "node:fs";
import path from "node:path";
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

/**
 * The wave-barrier scheduler is the ONLY scheduler. Its free-running twin
 * (and the scheduler-toggle option selecting between them) was deleted on
 * 2026-08-10: two complete orchestrations of the LLM stage had hand-maintained
 * parity, and every divergence between them was a bug class (report corruption
 * on one path, mid-flight AST mutation, callbacks that must never run with
 * nothing enforcing it).
 *
 * This guard is the INVERSE of the resolver test that used to live here
 * ("every read goes through resolveWaveScheduling"): the toggle must not
 * quietly return. A legitimate scheduler escape hatch, should one ever be
 * needed, is a degenerate wave (waves of size 1) — not a second
 * implementation, and not this option name.
 */
describe("wave scheduling toggle stays deleted", () => {
  // Built by concatenation so this file does not match its own guard.
  const TOKENS = ["wave" + "Scheduling", "wave-" + "scheduling"];

  function srcFiles(): string[] {
    const root = path.resolve(
      path.dirname(new URL(import.meta.url).pathname),
      ".."
    );
    const out: string[] = [];
    for (const f of fs.readdirSync(root, {
      recursive: true
    }) as string[]) {
      if (/\.(ts|js)$/.test(f)) out.push(path.join(root, f));
    }
    return out;
  }

  it("no file under src/ mentions the deleted toggle", () => {
    const offenders: string[] = [];
    for (const file of srcFiles()) {
      const source = fs.readFileSync(file, "utf-8");
      if (TOKENS.some((token) => source.includes(token))) {
        offenders.push(path.relative(process.cwd(), file));
      }
    }
    assert.deepStrictEqual(
      offenders,
      [],
      `the deleted ${TOKENS[0]} toggle resurfaced in:\n${offenders.join("\n")}`
    );
  });
});
