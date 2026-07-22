import assert from "node:assert";
import { describe, it } from "node:test";
import { computeWaveProfile } from "./wave-profile.js";

function graphOf(deps: Record<string, string[]>): {
  nodes: Map<string, unknown>;
  dependencies: Map<string, Set<string>>;
} {
  const nodes = new Map<string, unknown>();
  const dependencies = new Map<string, Set<string>>();
  for (const [id, d] of Object.entries(deps)) {
    nodes.set(id, {});
    dependencies.set(id, new Set(d));
    for (const dep of d) if (!nodes.has(dep)) nodes.set(dep, {});
  }
  return { nodes, dependencies };
}

describe("computeWaveProfile", () => {
  it("computes waves of a chain plus an isolated node", () => {
    const g = graphOf({ c: ["b"], b: ["a"], a: [], d: [] });
    const profile = computeWaveProfile(g, new Set());
    assert.deepStrictEqual(profile.sizes, [2, 1, 1]); // {a,d}, {b}, {c}
    assert.strictEqual(profile.waves, 3);
    assert.strictEqual(profile.deadlocked, 0);
  });

  it("seeds from pre-settled nodes and skips them", () => {
    const g = graphOf({ c: ["b"], b: ["a"], a: [] });
    const profile = computeWaveProfile(g, new Set(["a"]));
    assert.deepStrictEqual(profile.sizes, [1, 1]); // {b}, {c}
  });

  it("reports cycle members as deadlocked instead of looping", () => {
    const g = graphOf({ a: ["b"], b: ["a"], c: [] });
    const profile = computeWaveProfile(g, new Set());
    assert.deepStrictEqual(profile.sizes, [1]); // {c}
    assert.strictEqual(profile.deadlocked, 2);
  });
});
