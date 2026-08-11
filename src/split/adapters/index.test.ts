import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import { selectSplitAdapter, SPLIT_STRATEGY_NAMES } from "./index.js";
import { PositionalSplitAdapter } from "./positional-assignment.js";
import { CallGraphAdapter } from "./call-graph.js";
import type { ModuleDetectionResult } from "../module-detect.js";

function makeDetection(
  bundler: ModuleDetectionResult["bundler"],
  moduleCount: number
): ModuleDetectionResult {
  const modules = Array.from({ length: moduleCount }, (_, i) => ({
    id: `mod_${i}`,
    startLine: i * 10 + 1,
    endLine: (i + 1) * 10
  }));
  return { bundler, modules, uncoveredRanges: [] };
}

describe("selectSplitAdapter", () => {
  it("selects esbuild-esm adapter for ESM detection", () => {
    const detection = makeDetection("esbuild-esm", 3);
    const adapter = selectSplitAdapter(detection);
    assert.ok(adapter instanceof PositionalSplitAdapter);
    assert.equal(adapter.name, "esbuild-esm");
  });

  it("selects esbuild-cjs adapter for CJS detection", () => {
    const detection = makeDetection("esbuild-cjs", 3);
    const adapter = selectSplitAdapter(detection);
    assert.ok(adapter instanceof PositionalSplitAdapter);
    assert.equal(adapter.name, "esbuild-cjs");
  });

  it("selects bun-cjs adapter for Bun CJS detection", () => {
    const detection = makeDetection("bun-cjs", 3);
    const adapter = selectSplitAdapter(detection);
    assert.ok(adapter instanceof PositionalSplitAdapter);
    assert.equal(adapter.name, "bun-cjs");
  });

  it("falls back to call-graph for unknown bundler", () => {
    const detection = makeDetection("unknown", 0);
    const adapter = selectSplitAdapter(detection);
    assert.ok(adapter instanceof CallGraphAdapter);
    assert.equal(adapter.name, "call-graph");
  });

  it("falls back to call-graph when fewer than 2 modules detected", () => {
    const detection = makeDetection("esbuild-esm", 1);
    const adapter = selectSplitAdapter(detection);
    assert.ok(adapter instanceof CallGraphAdapter);
  });

  it("honors forced strategy override", () => {
    const detection = makeDetection("esbuild-esm", 5);
    const adapter = selectSplitAdapter(detection, "call-graph");
    assert.ok(adapter instanceof CallGraphAdapter);
  });

  it("honors forced esbuild-esm override even for unknown detection", () => {
    const detection = makeDetection("unknown", 0);
    const adapter = selectSplitAdapter(detection, "esbuild-esm");
    assert.equal(adapter.name, "esbuild-esm");
  });
});

describe("positional adapter: esbuild-esm", () => {
  const adapter = new PositionalSplitAdapter("esbuild-esm");

  it("supports esbuild-esm with >= 2 modules", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-esm", 2)), true);
    assert.equal(adapter.supports(makeDetection("esbuild-esm", 5)), true);
  });

  it("does not support esbuild-esm with < 2 modules", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-esm", 0)), false);
    assert.equal(adapter.supports(makeDetection("esbuild-esm", 1)), false);
  });

  it("does not support other bundler types", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-cjs", 3)), false);
    assert.equal(adapter.supports(makeDetection("unknown", 0)), false);
  });
});

describe("positional adapter: esbuild-cjs", () => {
  const adapter = new PositionalSplitAdapter("esbuild-cjs");

  it("supports esbuild-cjs with >= 2 modules", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-cjs", 2)), true);
  });

  it("does not support other bundler types", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-esm", 3)), false);
    assert.equal(adapter.supports(makeDetection("unknown", 0)), false);
  });
});

describe("positional adapter: bun-cjs", () => {
  const adapter = new PositionalSplitAdapter("bun-cjs");

  it("supports bun-cjs with >= 2 modules", () => {
    assert.equal(adapter.supports(makeDetection("bun-cjs", 2)), true);
  });

  it("does not support bun-cjs with < 2 modules", () => {
    assert.equal(adapter.supports(makeDetection("bun-cjs", 1)), false);
  });

  it("does not support other bundler types", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-cjs", 3)), false);
    assert.equal(adapter.supports(makeDetection("unknown", 0)), false);
  });
});

describe("CallGraphAdapter", () => {
  const adapter = new CallGraphAdapter();

  it("supports any detection result", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-esm", 5)), true);
    assert.equal(adapter.supports(makeDetection("esbuild-cjs", 3)), true);
    assert.equal(adapter.supports(makeDetection("bun-cjs", 3)), true);
    assert.equal(adapter.supports(makeDetection("unknown", 0)), true);
  });
});

/**
 * The registry is the only place that knows which split strategies exist.
 * Two other places restated it and BOTH drifted:
 *
 *  - `VALID_STRATEGIES` in the standalone `split` command (deleted 2026-08-11;
 *    it was the only path that ever passed a strategy override) was a
 *    hand-written Set that omitted `bun-cjs`, so `--split-strategy bun-cjs`
 *    exited 1 while the command's own help text advertised it.
 *  - `SplitStrategyType` (types.ts) listed `webpack`, for which no adapter
 *    exists. `selectSplitAdapter` has no else-branch, so that override was a
 *    SILENT no-op: it looked accepted and changed nothing.
 *
 * A name list that must be updated by hand when the registry changes will drift
 * again, so both now derive from `SPLIT_STRATEGY_NAMES`, and an unknown
 * override fails loudly instead of being ignored.
 */
describe("split strategy names have exactly one source", () => {
  it("every declared strategy name has an adapter", () => {
    for (const name of SPLIT_STRATEGY_NAMES) {
      const forced = selectSplitAdapter(makeDetection("unknown", 0), name);
      assert.strictEqual(
        forced.name,
        name,
        `"${name}" is a declared strategy with no adapter that answers to it`
      );
    }
  });

  it("an unknown override throws instead of silently doing nothing", () => {
    assert.throws(
      () =>
        selectSplitAdapter(
          makeDetection("unknown", 0),
          "webpack" as (typeof SPLIT_STRATEGY_NAMES)[number]
        ),
      /unknown split strategy/i,
      "a typo'd or removed strategy must fail loudly — silently falling through " +
        "to detection is indistinguishable from the override having worked"
    );
  });
});
