import assert from "node:assert";
import { describe, it } from "node:test";
import { canonicalizePlan } from "./determinism.js";
import type { Cluster, SplitPlan, SplitStats } from "./types.js";

function makePlan(overrides?: Partial<SplitPlan>): SplitPlan {
  const stats: SplitStats = {
    totalFunctions: 2,
    totalClusters: 1,
    avgClusterSize: 2,
    sharedFunctions: 0,
    sharedRatio: 0,
    orphanFunctions: 0,
    mqScore: 1
  };

  const cluster: Cluster = {
    id: "abc123",
    rootFunctions: ["fn1"],
    members: new Set(["fn1", "fn2"]),
    memberHashes: ["hash1", "hash2"]
  };

  return {
    clusters: [cluster],
    shared: new Set<string>(),
    orphans: new Set<string>(),
    ledger: { entries: new Map(), duplicated: new Map() },
    stats,
    ...overrides
  };
}

describe("canonicalizePlan", () => {
  it("serializes Sets as sorted arrays", () => {
    const plan = makePlan({
      shared: new Set(["c", "a", "b"]),
      orphans: new Set(["z", "x"])
    });

    const canonical = canonicalizePlan(plan);
    const parsed = JSON.parse(canonical);

    assert.deepStrictEqual(parsed.shared, ["a", "b", "c"]);
    assert.deepStrictEqual(parsed.orphans, ["x", "z"]);
  });

  it("serializes Maps with sorted keys", () => {
    const plan = makePlan();
    plan.ledger.entries.set("z:1:Fn", {
      id: "z:1:Fn",
      node: {} as any,
      type: "Fn",
      source: "z"
    });
    plan.ledger.entries.set("a:1:Fn", {
      id: "a:1:Fn",
      node: {} as any,
      type: "Fn",
      source: "a"
    });

    const canonical = canonicalizePlan(plan);
    const parsed = JSON.parse(canonical);

    const keys = Object.keys(parsed.ledger.entries);
    assert.deepStrictEqual(keys, ["a:1:Fn", "z:1:Fn"]);
  });

  it("produces identical output for same input", () => {
    const plan1 = makePlan({ shared: new Set(["b", "a"]) });
    const plan2 = makePlan({ shared: new Set(["a", "b"]) });

    assert.strictEqual(canonicalizePlan(plan1), canonicalizePlan(plan2));
  });

  it("produces different output for different input", () => {
    const plan1 = makePlan({ shared: new Set(["a"]) });
    const plan2 = makePlan({ shared: new Set(["b"]) });

    assert.notStrictEqual(canonicalizePlan(plan1), canonicalizePlan(plan2));
  });

  it("handles cluster members as sorted arrays", () => {
    const plan = makePlan();
    plan.clusters[0].members = new Set(["z", "m", "a"]);

    const canonical = canonicalizePlan(plan);
    const parsed = JSON.parse(canonical);

    assert.deepStrictEqual(parsed.clusters[0].members, ["a", "m", "z"]);
  });
});
