/**
 * The e2e stage's KNOWN-GAP mechanism (scripts/e2e.ts): a fixture that
 * records a gap the pipeline has today stays green only while it fails
 * EXACTLY the declared way — a different failure, or a pass, fails the
 * stage, so a fix promotes the fixture instead of hiding behind the entry.
 */
import assert from "node:assert";
import { describe, it } from "node:test";

import {
  apartFailures,
  assetFailures,
  expectationFailures,
  judgeKnownGap,
  KNOWN_GAPS,
  type KnownGap,
  staleKnownGaps,
  staysFailures
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
    assert.deepEqual(staleKnownGaps(KNOWN_GAPS, ["esbuild-minified"]), []);
  });
});

describe("expectationFailures", () => {
  const observed = { bundler: "unknown", unpackAdapter: "passthrough" };

  it("is empty when nothing is expected", () => {
    assert.deepEqual(
      expectationFailures(undefined, observed, { vendorFiles: 0 }),
      []
    );
  });

  it("names each expectation the run missed", () => {
    assert.deepEqual(
      expectationFailures(
        {
          bundler: "esbuild",
          unpackAdapter: "esbuild",
          vendor: true,
          splitMethod: "not-split"
        },
        observed,
        { vendorFiles: 0, splitMethod: "fresh-grouping" }
      ),
      [
        "detection: bundler is unknown, expected esbuild",
        "toolchain: unpack adapter is passthrough, expected esbuild",
        "unpack: the split tree's vendor/ is empty, expected the extracted dependencies",
        "split: the fresh run's split method is fresh-grouping, expected not-split"
      ]
    );
  });

  it("is empty when every expectation holds", () => {
    assert.deepEqual(
      expectationFailures(
        {
          bundler: "bun",
          unpackAdapter: "bun",
          vendor: true,
          splitMethod: "module-markers"
        },
        { bundler: "bun", unpackAdapter: "bun" },
        { vendorFiles: 26, splitMethod: "module-markers" }
      ),
      []
    );
  });
});

describe("apartFailures", () => {
  // Which files of a split tree hold each marker string.
  const files = new Map([
    ["src/index.js", "LEDGER# PALETTE#"],
    ["src/archive.js", "ARCHIVE#"]
  ]);

  it("names a pair that shares a file (the marker method's pile-up)", () => {
    assert.deepEqual(apartFailures([["PALETTE#", "LEDGER#"]], files), [
      'split: "PALETTE#" and "LEDGER#" share src/index.js, expected separate files'
    ]);
  });

  it("names a marker no file holds", () => {
    assert.deepEqual(apartFailures([["ARCHIVE#", "CODEC#"]], files), [
      'split: no file holds "CODEC#"'
    ]);
  });

  it("is empty when every pair sits in separate files", () => {
    assert.deepEqual(apartFailures([["ARCHIVE#", "PALETTE#"]], files), []);
  });
});

describe("assetFailures", () => {
  it("names an expected app text asset the tree lacks", () => {
    assert.deepEqual(
      assetFailures(
        ["src/_assets/a.js", "src/_assets/b.js"],
        ["src/_assets/a.js", "src/index.js"]
      ),
      ["unpack: no app text asset src/_assets/b.js"]
    );
  });
});

describe("staysFailures", () => {
  const fresh = new Map([["vendor/lib_aaaaaaaa.js", "YAML-MARK"]]);

  it("passes a vendor file that kept its path", () => {
    const prior = new Map([["vendor/lib_aaaaaaaa.js", "YAML-MARK v2"]]);
    assert.deepEqual(staysFailures(["YAML-MARK"], fresh, prior), []);
  });

  it("names a vendor file that moved across the release", () => {
    const prior = new Map([["vendor/lib_bbbbbbbb.js", "YAML-MARK v2"]]);
    assert.deepEqual(staysFailures(["YAML-MARK"], fresh, prior), [
      'unpack: "YAML-MARK" moved from vendor/lib_aaaaaaaa.js to vendor/lib_bbbbbbbb.js across the release'
    ]);
  });

  it("names a marker one release lacks", () => {
    assert.deepEqual(staysFailures(["YAML-MARK"], fresh, new Map()), [
      'unpack: no vendor file holds "YAML-MARK" in both releases'
    ]);
  });
});
