import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { Identifier } from "@babel/types";
import {
  buildPriorMatchMap,
  type MatchedBindingRef
} from "./prior-match-map.js";

/** A matched binding whose declaration identifier now carries `finalName`. */
function ref(finalName: string, priorName: string): MatchedBindingRef {
  return { identifier: { name: finalName } as Identifier, priorName };
}

describe("buildPriorMatchMap", () => {
  it("keeps a flipped binding (final !== prior) as {final -> prior}", () => {
    const map = buildPriorMatchMap([ref("noop4", "serializeTask")]);
    assert.strictEqual(map.get("noop4"), "serializeTask");
    assert.strictEqual(map.size, 1);
  });

  it("drops a pinned binding (final === prior) — no relocation to inherit", () => {
    const map = buildPriorMatchMap([
      ref("serializeTask", "serializeTask"),
      ref("emptyFn", "processDataVal")
    ]);
    assert.strictEqual(map.has("serializeTask"), false);
    assert.strictEqual(map.get("emptyFn"), "processDataVal");
    assert.strictEqual(map.size, 1);
  });

  it("drops a final name that maps to conflicting prior names (ambiguous)", () => {
    // Two matched bindings both flipped to the same minted name but were
    // matched to different priors — the key is untrustworthy, drop it.
    const map = buildPriorMatchMap([
      ref("noop4", "serializeTask"),
      ref("noop4", "authManager")
    ]);
    assert.strictEqual(map.has("noop4"), false);
    assert.strictEqual(map.size, 0);
  });

  it("keeps a final name that maps to the SAME prior name from two refs", () => {
    const map = buildPriorMatchMap([
      ref("noop4", "serializeTask"),
      ref("noop4", "serializeTask")
    ]);
    assert.strictEqual(map.get("noop4"), "serializeTask");
  });

  it("returns an empty map for no refs", () => {
    assert.strictEqual(buildPriorMatchMap([]).size, 0);
  });
});
