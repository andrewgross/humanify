/**
 * The ask-trace harness's pure logic: the stub's answer policies, the
 * runner's arg parsing/summary, and the comparator's diff semantics — the
 * exit-nonzero-empty-diff contract use (a) (a no-op change) depends on.
 */
import assert from "node:assert";
import { describe, it } from "node:test";

import type { AskRow } from "../scripts/ask-trace.js";
import { parseArgs, summarize } from "../scripts/ask-trace.js";
import { diffAsks, formatDelta } from "../scripts/diff-asks.js";
import {
  askedIdentifiers,
  collideAnswer,
  stubAnswer
} from "../scripts/lib/stub-llm.js";

const body = (prompt: string) =>
  JSON.stringify({ messages: [{ role: "user", content: prompt }] });

const PROMPT =
  "some code\n\nIdentifiers to rename: Ka, Mb_2, not-an-id!\n\nmap";

const row = (over: Partial<AskRow>): AskRow => ({
  seq: 0,
  site: "naming",
  scope: "fn-a",
  scopeKind: "fn",
  reason: "fresh",
  isRetry: false,
  priorContext: false,
  round: 1,
  identifiers: ["Ka"],
  usedNamesCount: 3,
  promptVariant: "batch",
  ...over
});

describe("the stub's answer policies", () => {
  it("names every asked identifier after itself, skipping non-identifiers", () => {
    assert.deepEqual(JSON.parse(stubAnswer(body(PROMPT))), {
      Ka: "KaRenamed",
      Mb_2: "Mb_2Renamed"
    });
  });

  it("collide names every identifier the same, to force collisions", () => {
    const answer = collideAnswer("takenName");
    assert.deepEqual(JSON.parse(answer(body(PROMPT))), {
      Ka: "takenName",
      Mb_2: "takenName"
    });
  });

  it("reads the identifiers line only", () => {
    assert.deepEqual(askedIdentifiers(body(PROMPT)), [
      "Ka",
      "Mb_2",
      "not-an-id!"
    ]);
    assert.deepEqual(askedIdentifiers("not json"), []);
  });
});

describe("the runner's arg parsing and summary", () => {
  it("parses its own flags and fails loud on unknowns by convention", () => {
    const a = parseArgs([
      "in.js",
      "--out",
      "o",
      "--collide",
      "boom",
      "--sequential",
      "--",
      "--split",
      "--batch-size",
      "10"
    ]);
    assert.equal(a.input, "in.js");
    assert.equal(a.out, "o");
    assert.equal(a.collide, "boom");
    assert.equal(a.sequential, true);
    assert.deepEqual(a.passthrough, ["--split", "--batch-size", "10"]);
    const bare = parseArgs(["in.js"]);
    assert.equal(bare.out, undefined);
    assert.deepEqual(bare.passthrough, []);
  });

  it("summarizes totals-first with a per-reason table", () => {
    const text = summarize([
      row({ reason: "retry", isRetry: true, retryCause: "NameTaken" }),
      row({ reason: "retry", isRetry: true, retryCause: "NameTaken" }),
      row({ reason: "fresh" }),
      row({ reason: "module-lane", scope: "module-binding-batch:e0" })
    ]);
    assert.match(text, /^TOTAL 4 ask\(s\)/);
    assert.match(text, /\bfresh\s+1\b/);
    assert.match(text, /\bretry\s+2\b/);
    assert.match(text, /REMAINING\s+2 re-ask\(s\)/);
  });
});

describe("the ask diff", () => {
  const same = [row({ seq: 0 })];

  it("flags nothing when the logs are identical, even re-ordered duplicates pair by multiset", () => {
    const a = [
      row({ seq: 0 }),
      row({
        seq: 1,
        round: 2,
        reason: "retry",
        isRetry: true,
        retryCause: "NameTaken"
      })
    ];
    const b = [
      row({ seq: 5 }),
      row({
        seq: 9,
        round: 2,
        reason: "retry",
        isRetry: true,
        retryCause: "NameTaken"
      })
    ];
    const d = diffAsks(a, b);
    assert.equal(d.identical, true);
    assert.deepEqual(d.onlyA, []);
    assert.deepEqual(d.onlyB, []);
    assert.deepEqual(d.changed, []);
    assert.equal(diffAsks(same, same).identical, true);
  });

  it("reports asks only in A / only in B by scope", () => {
    const a = [row({}), row({ scope: "fn-b", identifiers: ["Zz"] })];
    const b = [row({}), row({ scope: "fn-c", identifiers: ["Yy"] })];
    const d = diffAsks(a, b);
    assert.equal(d.identical, false);
    assert.deepEqual(
      d.onlyA.map((r) => r.scope),
      ["fn-b"]
    );
    assert.deepEqual(
      d.onlyB.map((r) => r.scope),
      ["fn-c"]
    );
    const text = formatDelta({ a: "A", b: "B" }, a, b, d);
    assert.match(text, /^A: A — 2 ask\(s\)/);
    assert.match(text, /IDENTICAL: no — only-in-A 1, only-in-B 1, changed 0/);
    assert.match(text, /ONLY IN A \(1\)/);
    assert.match(text, /\[naming\] fn-b \(round 1, fresh\) \[Zz\]/);
  });

  it("pairs identical asks that changed a compared field, naming the field", () => {
    const a = [row({ wave: 1, priorContext: false })];
    const b = [row({ wave: 2, priorContext: true })];
    const d = diffAsks(a, b);
    assert.equal(d.identical, false);
    assert.deepEqual(d.onlyA, []);
    assert.equal(d.changed.length, 1);
    assert.deepEqual(d.changed[0].fields.sort(), ["priorContext", "wave"]);
    const text = formatDelta({ a: "A", b: "B" }, a, b, d);
    assert.match(text, /CHANGED \(1\)/);
    assert.match(text, /wave 1 → 2/);
    assert.match(text, /priorContext false → true/);
  });

  it("treats a different retryCause as a different ask, not a changed one", () => {
    const a = [
      row({ reason: "retry", isRetry: true, retryCause: "NameTaken" })
    ];
    const b = [
      row({ reason: "retry", isRetry: true, retryCause: "InvalidSuggestion" })
    ];
    const d = diffAsks(a, b);
    assert.equal(d.identical, false);
    assert.deepEqual(d.changed, []);
    assert.equal(d.onlyA.length, 1);
    assert.equal(d.onlyB.length, 1);
  });
});
