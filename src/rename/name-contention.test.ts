import assert from "node:assert";
import { beforeEach, describe, it } from "node:test";
import { nameContention } from "./name-contention.js";

describe("nameContention recorder", () => {
  beforeEach(() => nameContention.reset(true));

  it("records nothing while disabled", () => {
    nameContention.reset(false);
    nameContention.record({
      requested: "writeConfig",
      resolvedTo: "writeConfigVal",
      oldName: "q7",
      site: "wave"
    });
    assert.deepStrictEqual(nameContention.report().events, []);
  });

  it("captures requested vs resolved with the claimant's old name", () => {
    nameContention.record({
      requested: "writeConfig",
      resolvedTo: "writeConfigVal",
      oldName: "q7",
      site: "wave"
    });
    nameContention.record({
      requested: "parseArgs",
      resolvedTo: "parseArgs2",
      oldName: "Zx",
      site: "remaining"
    });
    const { events } = nameContention.report();
    assert.strictEqual(events.length, 2);
    assert.deepStrictEqual(events[0], {
      requested: "writeConfig",
      resolvedTo: "writeConfigVal",
      oldName: "q7",
      site: "wave"
    });
    assert.strictEqual(events[1].site, "remaining");
  });

  it("reset clears prior events — one run's contention never leaks into the next", () => {
    nameContention.record({
      requested: "a",
      resolvedTo: "aVal",
      oldName: "x",
      site: "wave"
    });
    nameContention.reset(true);
    assert.deepStrictEqual(nameContention.report().events, []);
  });
});
