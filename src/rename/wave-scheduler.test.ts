import assert from "node:assert";
import { describe, it } from "node:test";
import {
  applyWaveBarrier,
  computeWaveMembers,
  sortWaveEntries,
  WaveCollector,
  WaveGate,
  type WaveEntry,
  type WaveMembershipGraph
} from "./wave-scheduler.js";

/** Build a membership graph from adjacency lists. */
function makeGraph(
  ids: string[],
  deps: Record<string, string[]> = {},
  scopeParentEdges: string[] = []
): WaveMembershipGraph {
  const nodes = new Map<string, unknown>();
  for (const id of ids) nodes.set(id, {});
  const dependencies = new Map<string, Set<string>>();
  for (const [id, ds] of Object.entries(deps)) {
    dependencies.set(id, new Set(ds));
  }
  return { nodes, dependencies, scopeParentEdges: new Set(scopeParentEdges) };
}

describe("computeWaveMembers", () => {
  it("returns dependency-free pending nodes in graph iteration order", () => {
    const graph = makeGraph(["a", "b", "c"], { b: ["a"] });
    const members = computeWaveMembers(
      graph,
      new Set(["a", "b", "c"]),
      new Set()
    );
    assert.deepStrictEqual(members, { ids: ["a", "c"], tier: 0 });
  });

  it("treats done dependencies as settled and skips non-pending nodes", () => {
    const graph = makeGraph(["a", "b"], { b: ["a"] });
    const members = computeWaveMembers(graph, new Set(["b"]), new Set(["a"]));
    assert.deepStrictEqual(members, { ids: ["b"], tier: 0 });
  });

  it("promotes via tier 1 by ignoring scopeParent edges", () => {
    // a -> b is a scopeParent edge; b -> a is a regular edge. Nothing is
    // ready normally; relaxing scopeParent edges frees exactly a.
    const graph = makeGraph(["a", "b"], { a: ["b"], b: ["a"] }, ["a->b"]);
    const members = computeWaveMembers(graph, new Set(["a", "b"]), new Set());
    assert.deepStrictEqual(members, { ids: ["a"], tier: 1 });
  });

  it("force-breaks pure callee cycles via tier 2", () => {
    const graph = makeGraph(["a", "b"], { a: ["b"], b: ["a"] });
    const members = computeWaveMembers(graph, new Set(["a", "b"]), new Set());
    assert.deepStrictEqual(members, { ids: ["a", "b"], tier: 2 });
  });

  it("returns an empty tier-0 wave when nothing is pending", () => {
    const graph = makeGraph(["a"]);
    const members = computeWaveMembers(graph, new Set(), new Set(["a"]));
    assert.deepStrictEqual(members, { ids: [], tier: 0 });
  });
});

describe("WaveGate", () => {
  it("settle resolves false when every task finishes without arriving", async () => {
    const gate = new WaveGate(2);
    gate.finish();
    gate.finish();
    assert.strictEqual(await gate.settle(), false);
  });

  it("settle waits for the last task, then reports waiters", async () => {
    const gate = new WaveGate(2);
    const arrival = gate.arrive(0, () => "resumed");
    let settled: boolean | undefined;
    const settleP = gate.settle().then((v) => {
      settled = v;
      return v;
    });
    // Not yet: one task is still running.
    await new Promise((r) => setImmediate(r));
    assert.strictEqual(settled, undefined);
    gate.finish();
    assert.strictEqual(await settleP, true);
    gate.release();
    assert.strictEqual(await arrival, "resumed");
    gate.finish();
    assert.strictEqual(await gate.settle(), false);
  });

  it("release runs barrier computes in order-key order and delivers values", async () => {
    const gate = new WaveGate(3);
    const log: number[] = [];
    // Arrive out of order relative to the order keys.
    const p2 = gate.arrive(2, () => {
      log.push(2);
      return "two";
    });
    const p0 = gate.arrive(0, () => {
      log.push(0);
      return "zero";
    });
    const p1 = gate.arrive(1, () => {
      log.push(1);
      return "one";
    });
    assert.strictEqual(await gate.settle(), true);
    gate.release();
    assert.deepStrictEqual(await Promise.all([p0, p1, p2]), [
      "zero",
      "one",
      "two"
    ]);
    assert.deepStrictEqual(log, [0, 1, 2]);
  });

  it("supports multiple barrier rounds from the same task", async () => {
    const gate = new WaveGate(1);
    const task = (async () => {
      await gate.arrive(0, () => "first");
      await gate.arrive(0, () => "second");
      gate.finish();
    })();
    assert.strictEqual(await gate.settle(), true);
    gate.release();
    assert.strictEqual(await gate.settle(), true);
    gate.release();
    assert.strictEqual(await gate.settle(), false);
    await task;
  });

  it("rejects a waiter whose barrier compute throws", async () => {
    const gate = new WaveGate(1);
    const arrival = gate.arrive(0, () => {
      throw new Error("compute failed");
    });
    assert.strictEqual(await gate.settle(), true);
    gate.release();
    await assert.rejects(arrival, /compute failed/);
  });
});

/** Test entry with scripted apply results and call recording. */
function makeEntry(
  overrides: Partial<WaveEntry> & {
    applyResults?: Array<{ applied: boolean; reason?: string }>;
    used?: Set<string>;
  }
): WaveEntry & {
  applied: string[];
  rejectedWith: Array<{ reason: string; winner?: string }>;
  resolvedNames: string[];
} {
  const applyResults = overrides.applyResults ?? [{ applied: true }];
  const applied: string[] = [];
  const rejectedWith: Array<{ reason: string; winner?: string }> = [];
  const resolvedNames: string[] = [];
  let call = 0;
  const entry = {
    nodeIndex: 0,
    phase: 0,
    bindingIndex: 0,
    seq: 0,
    oldName: "old",
    newName: "next",
    ...overrides,
    apply(name: string) {
      applied.push(name);
      const result = applyResults[Math.min(call, applyResults.length - 1)];
      call++;
      return result;
    },
    liveUsedNames: () => overrides.used ?? new Set<string>(),
    onApplied(finalName: string) {
      resolvedNames.push(finalName);
    },
    onRejected(reason: string, winner?: string) {
      rejectedWith.push({ reason, winner });
    }
  };
  return Object.assign(entry, { applied, rejectedWith, resolvedNames });
}

describe("sortWaveEntries", () => {
  it("orders by nodeIndex, then phase, then bindingIndex, then seq", () => {
    const entries = [
      makeEntry({ nodeIndex: 1, phase: 0, bindingIndex: 0, seq: 3 }),
      makeEntry({ nodeIndex: 0, phase: 1, bindingIndex: 0, seq: 2 }),
      makeEntry({ nodeIndex: 0, phase: 0, bindingIndex: 1, seq: 1 }),
      makeEntry({ nodeIndex: 0, phase: 0, bindingIndex: 0, seq: 4 }),
      makeEntry({ nodeIndex: 0, phase: 0, bindingIndex: 0, seq: 0 })
    ];
    const sorted = sortWaveEntries([...entries]);
    assert.deepStrictEqual(
      sorted.map((e) => [e.nodeIndex, e.phase, e.bindingIndex, e.seq]),
      [
        [0, 0, 0, 0],
        [0, 0, 0, 4],
        [0, 0, 1, 1],
        [0, 1, 0, 2],
        [1, 0, 0, 3]
      ]
    );
  });
});

describe("WaveCollector", () => {
  it("drains sorted entries and clears", () => {
    const collector = new WaveCollector();
    const a = makeEntry({ nodeIndex: 1, seq: collector.nextSeq() });
    const b = makeEntry({ nodeIndex: 0, seq: collector.nextSeq() });
    collector.add(a);
    collector.add(b);
    const drained = collector.drain();
    assert.deepStrictEqual(
      drained.map((e) => e.nodeIndex),
      [0, 1]
    );
    assert.deepStrictEqual(collector.drain(), []);
  });
});

describe("applyWaveBarrier", () => {
  const identityConflict = (name: string) => `${name}2`;

  it("applies in deterministic order; graph-order winner keeps the name", () => {
    const winners = new Map<string, string>();
    const winner = makeEntry({
      nodeIndex: 0,
      oldName: "a",
      newName: "shared"
    });
    const loser = makeEntry({
      nodeIndex: 1,
      oldName: "b",
      newName: "shared",
      applyResults: [{ applied: false, reason: "target-in-scope" }]
    });
    // Feed out of order: sorting must decide the winner, not arrival.
    const rejections = applyWaveBarrier(
      [loser, winner],
      winners,
      identityConflict
    );
    assert.deepStrictEqual(winner.applied, ["shared"]);
    assert.strictEqual(rejections.length, 1);
    assert.strictEqual(rejections[0].entry.oldName, "b");
    assert.strictEqual(rejections[0].winnerOldName, "a");
    assert.deepStrictEqual(loser.rejectedWith, [
      { reason: "target-in-scope", winner: "a" }
    ]);
    assert.strictEqual(winners.get("shared"), "a");
  });

  it("rejects on the live used-name check without touching the AST", () => {
    const winners = new Map<string, string>();
    const entry = makeEntry({
      oldName: "b",
      newName: "taken",
      used: new Set(["taken"])
    });
    const rejections = applyWaveBarrier([entry], winners, identityConflict);
    assert.deepStrictEqual(entry.applied, []);
    assert.strictEqual(rejections.length, 1);
    assert.deepStrictEqual(entry.rejectedWith, [
      { reason: "duplicate", winner: undefined }
    ]);
  });

  it("identity entries record and never reject", () => {
    const entry = makeEntry({
      oldName: "kept",
      newName: "kept",
      identity: true,
      used: new Set(["kept"])
    });
    const rejections = applyWaveBarrier([entry], new Map(), identityConflict);
    assert.deepStrictEqual(entry.applied, ["kept"]);
    assert.strictEqual(rejections.length, 0);
  });

  it("suffixOnReject resolves a collision with a deterministic variant", () => {
    const winners = new Map<string, string>([["shared", "a"]]);
    const entry = makeEntry({
      oldName: "b",
      newName: "shared",
      suffixOnReject: true,
      used: new Set(["shared"])
    });
    const rejections = applyWaveBarrier([entry], winners, identityConflict);
    assert.strictEqual(rejections.length, 0);
    assert.deepStrictEqual(entry.applied, ["shared2"]);
    assert.deepStrictEqual(entry.resolvedNames, ["shared2"]);
    assert.strictEqual(winners.get("shared2"), "b");
  });

  it("suffixOnReject gives up terminally when the variant also fails", () => {
    const winners = new Map<string, string>([["shared", "a"]]);
    const entry = makeEntry({
      oldName: "b",
      newName: "shared",
      suffixOnReject: true,
      used: new Set(["shared"]),
      applyResults: [{ applied: false, reason: "shadows-child" }]
    });
    const rejections = applyWaveBarrier([entry], winners, identityConflict);
    // Terminal: resolved at this barrier (unrenamed), never seeds a retry.
    assert.strictEqual(rejections.length, 0);
    assert.deepStrictEqual(entry.rejectedWith, [
      { reason: "shadows-child", winner: "a" }
    ]);
  });
});
