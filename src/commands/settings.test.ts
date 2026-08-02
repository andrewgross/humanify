import assert from "node:assert";
import { describe, it } from "node:test";
import { resolveSettings } from "./settings.js";
import { DEFAULT_LLM_TIMEOUT_MS } from "./default-args.js";

/**
 * Settings resolve ONCE, here, so downstream code reads a field instead of
 * re-deriving one. The failures this prevents are all ones the codebase has
 * already had: the same default written in three places with two different
 * values, `reasoningEffort` parsed twice in one function, and `skipLibraries`
 * defaulted independently in `unminify.ts` and `rename/plugin.ts`.
 */

/** Run `fn` with `vars` applied to the environment, then restore exactly. */
function withEnv<T>(vars: Record<string, string | undefined>, fn: () => T): T {
  const prev = new Map<string, string | undefined>();
  for (const [k, v] of Object.entries(vars)) {
    prev.set(k, Object.hasOwn(process.env, k) ? process.env[k] : undefined);
    if (v === undefined) delete process.env[k];
    else process.env[k] = v;
  }
  try {
    return fn();
  } finally {
    for (const [k, v] of prev) {
      if (v === undefined) delete process.env[k];
      else process.env[k] = v;
    }
  }
}

/** The environment a commander-parsed run actually arrives with. */
const CLI = {
  endpoint: "http://cli/v1",
  model: "cli-model",
  apiKey: "cli-key"
};

const CLEAR = {
  HUMANIFY_API_KEY: undefined,
  OPENAI_API_KEY: undefined,
  HUMANIFY_MAX_TOKENS: undefined,
  HUMANIFY_REASONING_EFFORT: undefined,
  HUMANIFY_LLM_CACHE: undefined,
  HUMANIFY_MODULE_CONCURRENCY: undefined
};

describe("resolveSettings", () => {
  it("prefers a CLI value over the environment", () => {
    const s = withEnv({ ...CLEAR, HUMANIFY_API_KEY: "env-key" }, () =>
      resolveSettings(CLI)
    );
    assert.strictEqual(s.apiKey, "cli-key");
  });

  it("falls back to HUMANIFY_ then OPENAI_ for the api key", () => {
    const humanify = withEnv(
      { ...CLEAR, HUMANIFY_API_KEY: "h", OPENAI_API_KEY: "o" },
      () => resolveSettings({ ...CLI, apiKey: undefined })
    );
    assert.strictEqual(humanify.apiKey, "h", "HUMANIFY_ wins over OPENAI_");
    const openai = withEnv({ ...CLEAR, OPENAI_API_KEY: "o" }, () =>
      resolveSettings({ ...CLI, apiKey: undefined })
    );
    assert.strictEqual(openai.apiKey, "o");
  });

  it("resolves the LLM timeout to the single-sourced default", () => {
    const s = withEnv(CLEAR, () => resolveSettings(CLI));
    assert.strictEqual(s.timeout, DEFAULT_LLM_TIMEOUT_MS);
  });

  it("parses numbers once, into numbers", () => {
    // Every one of these used to be `parseNumber(opts.x)` at its point of use,
    // several of them far from where opts was built.
    const s = withEnv(CLEAR, () =>
      resolveSettings({
        ...CLI,
        concurrency: "7",
        timeout: "1234",
        retries: "5",
        batchSize: "9",
        maxRetries: "3",
        maxFreeRetries: "2",
        laneThreshold: "11"
      })
    );
    assert.strictEqual(s.concurrency, 7);
    assert.strictEqual(s.timeout, 1234);
    assert.strictEqual(s.retryAttempts, 5);
    assert.strictEqual(s.batchSize, 9);
    assert.strictEqual(s.maxRetriesPerIdentifier, 3);
    assert.strictEqual(s.maxFreeRetries, 2);
    assert.strictEqual(s.laneThreshold, 11);
  });

  it("leaves module concurrency undefined so the bundler-aware default applies", () => {
    // Resolving it to a number here would defeat defaultModuleConcurrency(),
    // which needs the detected bundler that is not known at this point.
    const unset = withEnv(CLEAR, () => resolveSettings(CLI));
    assert.strictEqual(unset.moduleConcurrency, undefined);
    const set = withEnv({ ...CLEAR, HUMANIFY_MODULE_CONCURRENCY: "33" }, () =>
      resolveSettings(CLI)
    );
    assert.strictEqual(set.moduleConcurrency, 33);
  });

  it("defaults the levers ON, and makes the sweep depend on the floor", () => {
    const on = withEnv(CLEAR, () => resolveSettings(CLI));
    assert.strictEqual(on.levers.namingFloor, true);
    assert.strictEqual(on.levers.namingFloorSweep, true);

    // Disabling the floor implicitly disables the sweep — the sweep cannot run
    // without it, and expressing that here stops a caller from combining them
    // wrongly.
    const off = withEnv(CLEAR, () =>
      resolveSettings({ ...CLI, namingFloor: false })
    );
    assert.strictEqual(off.levers.namingFloorSweep, false);
  });

  it("gates the prior-diff reconcile on there actually being a prior", () => {
    const noPrior = withEnv(CLEAR, () => resolveSettings(CLI));
    assert.strictEqual(noPrior.levers.reconcilePriorDiff, false);
    const withPrior = withEnv(CLEAR, () =>
      resolveSettings({ ...CLI, priorVersion: "/some/prior.js" })
    );
    assert.strictEqual(withPrior.levers.reconcilePriorDiff, true);
  });

  it("defaults skipLibraries ON — it was defaulted in two other places", () => {
    assert.strictEqual(
      withEnv(CLEAR, () => resolveSettings(CLI)).skipLibraries,
      true
    );
    assert.strictEqual(
      withEnv(CLEAR, () => resolveSettings({ ...CLI, skipLibraries: false }))
        .skipLibraries,
      false
    );
  });

  it("is frozen, so nothing downstream can re-decide a setting", () => {
    const s = withEnv(CLEAR, () => resolveSettings(CLI));
    assert.throws(() => {
      (s as { concurrency: number }).concurrency = 999;
    }, /read only|readonly|not extensible|Cannot assign/i);
  });

  it("reports a missing api key instead of resolving to undefined", () => {
    assert.throws(
      () =>
        withEnv(CLEAR, () => resolveSettings({ ...CLI, apiKey: undefined })),
      /api key/i
    );
  });
});
