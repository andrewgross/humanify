import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import { selectSplitAdapter } from "./index.js";
import { EsbuildESMAdapter } from "./esbuild-esm.js";
import { EsbuildCJSAdapter } from "./esbuild-cjs.js";
import { CallGraphAdapter } from "./call-graph.js";
import type { DetectionResult } from "../module-detect.js";

function makeDetection(
  bundler: DetectionResult["bundler"],
  moduleCount: number
): DetectionResult {
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
    assert.ok(adapter instanceof EsbuildESMAdapter);
    assert.equal(adapter.name, "esbuild-esm");
  });

  it("selects esbuild-cjs adapter for CJS detection", () => {
    const detection = makeDetection("esbuild-cjs", 3);
    const adapter = selectSplitAdapter(detection);
    assert.ok(adapter instanceof EsbuildCJSAdapter);
    assert.equal(adapter.name, "esbuild-cjs");
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
    assert.ok(adapter instanceof EsbuildESMAdapter);
  });
});

describe("EsbuildESMAdapter", () => {
  const adapter = new EsbuildESMAdapter();

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

describe("EsbuildCJSAdapter", () => {
  const adapter = new EsbuildCJSAdapter();

  it("supports esbuild-cjs with >= 2 modules", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-cjs", 2)), true);
  });

  it("does not support other bundler types", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-esm", 3)), false);
    assert.equal(adapter.supports(makeDetection("unknown", 0)), false);
  });
});

describe("CallGraphAdapter", () => {
  const adapter = new CallGraphAdapter();

  it("supports any detection result", () => {
    assert.equal(adapter.supports(makeDetection("esbuild-esm", 5)), true);
    assert.equal(adapter.supports(makeDetection("esbuild-cjs", 3)), true);
    assert.equal(adapter.supports(makeDetection("unknown", 0)), true);
  });
});
