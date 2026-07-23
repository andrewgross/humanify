import assert from "node:assert";
import { describe, it } from "node:test";
import type { FunctionNode, MatchResult } from "../analysis/types.js";
import { buildAmbiguityProbe } from "./ambiguity-probe.js";

function stubFn(
  id: string,
  callers: FunctionNode[],
  callees: FunctionNode[]
): FunctionNode {
  const fn = {
    sessionId: id,
    position: { line: 1, column: 0 },
    path: { node: { type: "FunctionDeclaration", id: { name: id } } },
    callers: new Set(callers),
    internalCallees: new Set(callees)
  } as unknown as FunctionNode;
  return fn;
}

function matchResultOf(
  matches: [string, string][],
  ambiguous: [string, string[]][]
): MatchResult {
  return {
    matches: new Map(matches),
    ambiguous: new Map(ambiguous),
    unmatched: [],
    resolutionStats: {} as MatchResult["resolutionStats"]
  };
}

describe("buildAmbiguityProbe", () => {
  it("dumps evidence for every participant of an ambiguous bucket", () => {
    const helper = stubFn("prior:helper", [], []);
    const pA = stubFn("prior:a", [helper], []);
    const fHelper = stubFn("fresh:helper", [], []);
    const f1 = stubFn("fresh:1", [fHelper], []);
    const f2 = stubFn("fresh:2", [], [fHelper]);

    const probe = buildAmbiguityProbe(
      matchResultOf(
        [["prior:helper", "fresh:helper"]],
        [["prior:a", ["fresh:1", "fresh:2"]]]
      ),
      new Map([
        ["prior:a", pA],
        ["prior:helper", helper]
      ]),
      new Map([
        ["fresh:1", f1],
        ["fresh:2", f2],
        ["fresh:helper", fHelper]
      ])
    );

    assert.deepStrictEqual(probe.ambiguous["prior:a"], ["fresh:1", "fresh:2"]);
    assert.deepStrictEqual(probe.prior["prior:a"].callers, ["prior:helper"]);
    assert.deepStrictEqual(probe.fresh["fresh:1"].callers, ["fresh:helper"]);
    assert.deepStrictEqual(probe.fresh["fresh:2"].callees, ["fresh:helper"]);
    assert.strictEqual(probe.matches["prior:helper"], "fresh:helper");
    assert.ok(probe.prior["prior:a"].head.startsWith("prior:a"));
    // Only bucket participants are dumped — no unrelated bloat.
    assert.strictEqual(probe.fresh["fresh:helper"], undefined);
  });
});
