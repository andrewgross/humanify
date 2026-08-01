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

  it("describes EVERY statement, not only the tiers that lost", () => {
    // The trail used to detail 7 tiers of 10 and count the rest, which left
    // 1,192 of 35,903 statements described on a real bundle — and zero for the
    // `hash` tier that places most of them. "Why is this here?" was then
    // unanswerable for 97% of the tree (exp057).
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
      ["hash", "hash", "name", "novote", "anchor"]
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

  it("drops the bulky evidence for an uneventful placement", () => {
    // Now that every statement is described, the vote arrays are what would
    // grow the file — and for a statement that landed where its prior self
    // lived, with nothing dissenting, they explain nothing.
    placementTrail.reset(true);
    placementTrail.record({
      ...entry(0, "hash"),
      priorFile: "core/main.js",
      evidence: { votes: ["core/main.js"], allSame: ["core/main.js"] }
    });
    const [only] = placementTrail.report().trails;
    assert.deepStrictEqual(only.evidence, {});
    assert.strictEqual(only.priorFile, "core/main.js");
  });

  it("keeps the evidence when the statement MOVED", () => {
    // The exp057 case: 26 declarations changed file between releases and every
    // consumer's import alias followed. A moved statement is the one thing a
    // reviewer always wants explained, whichever tier placed it.
    placementTrail.reset(true);
    placementTrail.record({
      ...entry(0, "name", "storage/error-messages/auth-manager.js"),
      priorFile: "floor/cli-interaction/task-serializer.js",
      evidence: { votes: ["storage/error-messages/auth-manager.js"] }
    });
    const [only] = placementTrail.report().trails;
    assert.deepStrictEqual(only.evidence.votes, [
      "storage/error-messages/auth-manager.js"
    ]);
  });

  it("records why the hash tier — the strongest evidence — abstained", () => {
    placementTrail.reset(true);
    placementTrail.record({ ...entry(0, "name"), hashMiss: "absent" });
    assert.strictEqual(placementTrail.report().trails[0].hashMiss, "absent");
  });

  it("records the losing tiers only when one DISAGREES with the winner", () => {
    placementTrail.reset(true);
    placementTrail.record({
      ...entry(0, "name", "a.js"),
      alternatives: { name: "a.js", allsame: "a.js" }
    });
    placementTrail.record({
      ...entry(1, "name", "a.js"),
      alternatives: { name: "a.js", anchor: "b.js" }
    });
    const [agreed, disagreed] = placementTrail.report().trails;
    assert.strictEqual(
      agreed.alternatives,
      undefined,
      "unanimous evidence needs no explanation"
    );
    assert.deepStrictEqual(disagreed.alternatives, { anchor: "b.js" });
  });

  it("truncates the declared-name list but says how many there were", () => {
    // The trail is now the index a reader searches by name, so truncating at 8
    // made entries unfindable: the 2.1.215->216 statement that dragged 32
    // declarations into the wrong file recorded 8 of them, and the other 24
    // names — `localPendingTasks`, `taskStatuses`, … — matched nothing.
    placementTrail.reset(true);
    placementTrail.record({
      ...entry(0, "novote"),
      names: Array.from({ length: 40 }, (_, i) => `name${i}`)
    });
    const [only] = placementTrail.report().trails;
    assert.strictEqual(only.names.length, 32);
    assert.strictEqual(
      only.nameCount,
      40,
      "a truncated list must not read as the whole list"
    );
  });

  it("does not report a full name list as truncated", () => {
    placementTrail.reset(true);
    placementTrail.record(entry(0, "hash"));
    assert.strictEqual(placementTrail.report().trails[0].nameCount, undefined);
  });

  it("reset clears prior state", () => {
    placementTrail.reset(true);
    placementTrail.record(entry(0, "novote"));
    placementTrail.reset(true);
    assert.deepStrictEqual(placementTrail.report(), { tiers: {}, trails: [] });
  });
});
