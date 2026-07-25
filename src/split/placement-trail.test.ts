import assert from "node:assert";
import { beforeEach, describe, it } from "node:test";
import { placementTrail } from "./placement-trail.js";

function entry(index: number, placedBy: string, file = "core/main.js") {
  return {
    index,
    names: ["alpha", "beta"],
    placedBy,
    file,
    evidence: {}
  };
}

describe("placementTrail", () => {
  beforeEach(() => placementTrail.reset(false));

  it("records nothing at all when disabled", () => {
    placementTrail.record(entry(0, "novote"));
    const report = placementTrail.report();
    assert.deepStrictEqual(report.tiers, {});
    assert.strictEqual(report.trails.length, 0);
  });

  it("counts every tier but details only the diagnosable ones", () => {
    placementTrail.reset(true);
    placementTrail.record(entry(0, "hash"));
    placementTrail.record(entry(1, "hash"));
    placementTrail.record(entry(2, "name"));
    placementTrail.record(entry(3, "novote"));
    placementTrail.record(entry(4, "anchor"));
    const report = placementTrail.report();
    assert.deepStrictEqual(report.tiers, {
      hash: 2,
      name: 1,
      novote: 1,
      anchor: 1
    });
    assert.deepStrictEqual(
      report.trails.map((t) => t.placedBy),
      ["novote", "anchor"],
      "the bulk tiers are counted, never described — the file is already ~100 MB"
    );
  });

  it("keeps the evidence that explains a locality placement", () => {
    placementTrail.reset(true);
    placementTrail.record({
      index: 7,
      names: ["generateContextUsageMarkdown", "inputData"],
      placedBy: "conflict",
      file: "src/lsp/skill-hook-registry.js",
      evidence: {
        votes: ["src/query-input/context-usage.js", "src/logging/socket.js"],
        allSame: ["src/query-input/context-usage.js"]
      }
    });
    const [only] = placementTrail.report().trails;
    assert.deepStrictEqual(only.evidence.votes, [
      "src/query-input/context-usage.js",
      "src/logging/socket.js"
    ]);
    assert.deepStrictEqual(only.evidence.allSame, [
      "src/query-input/context-usage.js"
    ]);
  });

  it("truncates the declared-name list", () => {
    placementTrail.reset(true);
    placementTrail.record({
      ...entry(0, "novote"),
      names: Array.from({ length: 40 }, (_, i) => `name${i}`)
    });
    assert.strictEqual(placementTrail.report().trails[0].names.length, 8);
  });

  it("reset clears prior state", () => {
    placementTrail.reset(true);
    placementTrail.record(entry(0, "novote"));
    placementTrail.reset(true);
    assert.deepStrictEqual(placementTrail.report(), { tiers: {}, trails: [] });
  });
});
