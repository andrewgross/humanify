import assert from "node:assert";
import { describe, it } from "node:test";
import { NULL_PROFILER } from "../profiling/profiler.js";
import { resolveRunConfig } from "./run-config.js";

describe("resolveRunConfig", () => {
  it("defaults to NULL_PROFILER and the standard eligibility rules", () => {
    const config = resolveRunConfig();
    assert.strictEqual(config.profiler, NULL_PROFILER);
    assert.strictEqual(config.isEligible("a"), true);
    assert.strictEqual(config.isEligible("__toESM"), false);
  });

  it("resolves eligibility from bundler/minifier types", () => {
    // _inherits is skipped only by the swc-specific skip set — the generic
    // helper regex needs two underscore-separated segments.
    assert.strictEqual(resolveRunConfig().isEligible("_inherits"), true);
    assert.strictEqual(
      resolveRunConfig({ minifierType: "swc" }).isEligible("_inherits"),
      false
    );
  });

  it("an explicit isEligible wins over bundler/minifier resolution", () => {
    const custom = (name: string) => name === "only";
    const config = resolveRunConfig({
      isEligible: custom,
      minifierType: "swc"
    });
    assert.strictEqual(config.isEligible, custom);
  });

  it("passes an explicit profiler through", () => {
    const profiler = Object.create(NULL_PROFILER) as typeof NULL_PROFILER;
    const config = resolveRunConfig({ profiler });
    assert.strictEqual(config.profiler, profiler);
  });
});
