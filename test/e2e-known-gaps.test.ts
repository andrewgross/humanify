/**
 * The e2e stage's KNOWN-GAP mechanism (scripts/e2e.ts): a fixture that
 * records a gap the pipeline has today stays green only while it fails
 * EXACTLY the declared way — a different failure, or a pass, fails the
 * stage, so a fix promotes the fixture instead of hiding behind the entry.
 */
import assert from "node:assert";
import { describe, it } from "node:test";

import {
  expectationFailures,
  judgeKnownGap,
  KNOWN_GAPS,
  type KnownGap,
  staleKnownGaps
} from "../scripts/e2e.js";

const gap: KnownGap = {
  fixture: "some-esm",
  spec: "plugin-spec I25",
  reason: "no wrapper",
  error: "no recognizable bundle wrapper"
};

describe("judgeKnownGap", () => {
  it("passes a fixture with no gap and no failure", () => {
    assert.equal(judgeKnownGap(undefined, null).verdict, "pass");
  });

  it("fails a fixture with no gap that failed, carrying the failure", () => {
    const j = judgeKnownGap(undefined, "boom");
    assert.equal(j.verdict, "fail");
    assert.match(j.message, /boom/);
  });

  it("records the declared failure as a known gap, quoting it", () => {
    const failure =
      "fresh: the binary exited 1\nError: ... has no recognizable bundle wrapper (...)";
    const j = judgeKnownGap(gap, failure);
    assert.equal(j.verdict, "known-gap");
    assert.match(j.message, /plugin-spec I25/);
    assert.match(j.message, /no recognizable bundle wrapper/);
  });

  it("fails a known gap that now fails DIFFERENTLY", () => {
    const j = judgeKnownGap(gap, "oxc failed to parse the input bundle");
    assert.equal(j.verdict, "fail");
    assert.match(j.message, /changed/);
    assert.match(j.message, /oxc failed to parse/);
  });

  it("fails a known gap that now PASSES, asking for promotion", () => {
    const j = judgeKnownGap(gap, null);
    assert.equal(j.verdict, "fail");
    assert.match(j.message, /now PASSES/);
    assert.match(j.message, /KNOWN_GAPS/);
  });
});

describe("staleKnownGaps", () => {
  it("names an entry whose fixture does not exist", () => {
    assert.deepEqual(staleKnownGaps([gap], ["other"]), [
      'KNOWN_GAPS entry "some-esm" names no fixture'
    ]);
  });

  it("names a fixture declared twice", () => {
    assert.deepEqual(staleKnownGaps([gap, gap], ["some-esm"]), [
      'KNOWN_GAPS declares "some-esm" twice'
    ]);
  });

  it("accepts the committed list against the committed fixtures", () => {
    assert.deepEqual(
      staleKnownGaps(KNOWN_GAPS, [
        "bun-esm-minified",
        "esbuild-cjs",
        "esbuild-esm",
        "esbuild-minified"
      ]),
      []
    );
  });
});

describe("expectationFailures", () => {
  const observed = { bundler: "unknown", unpackAdapter: "passthrough" };

  it("is empty when nothing is expected", () => {
    assert.deepEqual(expectationFailures(undefined, observed, 0), []);
  });

  it("names each expectation the run missed", () => {
    assert.deepEqual(
      expectationFailures(
        { bundler: "esbuild", unpackAdapter: "esbuild", vendor: true },
        observed,
        0
      ),
      [
        "detection: bundler is unknown, expected esbuild",
        "toolchain: unpack adapter is passthrough, expected esbuild",
        "unpack: the split tree's vendor/ is empty, expected the extracted dependencies"
      ]
    );
  });

  it("is empty when every expectation holds", () => {
    assert.deepEqual(
      expectationFailures(
        { bundler: "bun", unpackAdapter: "bun", vendor: true },
        { bundler: "bun", unpackAdapter: "bun" },
        26
      ),
      []
    );
  });
});
