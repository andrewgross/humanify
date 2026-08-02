import assert from "node:assert";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import { emitRunnableCjs } from "./cjs-emit.js";
import { stableSplitFromCode } from "./stable-split.js";
import {
  RUNNER_FILENAME,
  detectExternalPackages,
  writeRunnableScaffold
} from "./runnable-scaffold.js";

/**
 * Does a tree the SPLITTER produced actually run?
 *
 * Every fast check in the gate verifies that output PARSES. Nothing in the fast
 * path executed a split tree, so a regression that parses fine and throws at
 * require time — a wrong `require` path, an export accessor bound to a name
 * that moved file, a statement emitted before the value it reads — was
 * invisible to all of `npm run check` and only surfaced in the ~1h eval or the
 * multi-hop walk, both of which are run by hand.
 *
 * `runnable-scaffold.test.ts` already boots a tree with node, but it writes the
 * entry and its module BY HAND: it proves the scaffold works, not that the
 * splitter emits something loadable. This starts from a bundle and runs the
 * real `stableSplitFromCode` → `emitRunnableCjs` → `writeRunnableScaffold`
 * chain, so the require headers, export accessors, alias allocation, emission
 * order and load-order barriers are all exercised by actually executing them.
 *
 * Deliberately node, not bun: a check gated on an optional binary is a check
 * that skips (this repo has already been bitten by exactly that).
 */

/** Filler so the input clears the wrapper's binding threshold. */
const PAD = Array.from({ length: 60 }, (_, i) => `  var padFiller${i} = ${i};`);

/**
 * A bundle with real cross-statement structure, so splitting it has to emit
 * working imports and exports rather than one self-contained file:
 *
 *  - `formatLabel` is defined in one place and CALLED from two others, so
 *    whichever files they land in must require each other correctly;
 *  - `sharedCounter` is a mutable module binding WRITTEN by one function and
 *    READ by another — an export accessor with a setter, the case a plain
 *    `module.exports = x` snapshot would get wrong;
 *  - `bootReport` runs at load time and prints, so the process exits non-zero
 *    if any of it is wired up wrong.
 */
const BUNDLE = [
  "(function (exports, require, module, __filename, __dirname) {",
  "  var sharedCounter = 0;",
  '  var LABEL_PREFIX = "item-";',
  "  function formatLabel(n) {",
  "    return LABEL_PREFIX + n;",
  "  }",
  "  function bumpCounter(by) {",
  "    sharedCounter = sharedCounter + by;",
  "    return sharedCounter;",
  "  }",
  "  function describeFirst() {",
  "    return formatLabel(bumpCounter(1));",
  "  }",
  "  function describeSecond() {",
  "    return formatLabel(bumpCounter(2));",
  "  }",
  "  function bootReport() {",
  "    var a = describeFirst();",
  "    var b = describeSecond();",
  "    return { first: a, second: b, counter: sharedCounter };",
  "  }",
  "  console.log(JSON.stringify(bootReport()));",
  ...PAD,
  "});"
].join("\n");

const SMALL = {
  targetFiles: 8,
  maxLines: 3,
  maxSeg: 2,
  maxTop: 3,
  maxSub: 2,
  window: 4,
  minGap: 1
};

describe("a tree the splitter emitted actually boots", () => {
  it("splits, emits, and runs under node with correct cross-file wiring", async () => {
    const result = await stableSplitFromCode(BUNDLE, { clusterConfig: SMALL });
    assert.ok(result, "split produced nothing");

    const files = emitRunnableCjs(BUNDLE, result.ledger, result.wrapper);
    // Fixture guard: one file would make cross-file wiring untestable, which is
    // the entire point of booting rather than parsing.
    const srcFiles = [...files.keys()].filter((f) => f.startsWith("src/"));
    assert.ok(
      srcFiles.length >= 2,
      `expected the fixture to split across files, got ${srcFiles.length}: ${srcFiles.join(", ")}`
    );

    const dir = mkdtempSync(path.join(tmpdir(), "split-boots-"));
    try {
      for (const [rel, text] of files) {
        const p = path.join(dir, rel);
        mkdirSync(path.dirname(p), { recursive: true });
        writeFileSync(p, text);
      }
      assert.ok(
        files.has("index.js"),
        `emit produced no entry point: ${[...files.keys()].slice(0, 5).join(", ")}`
      );

      const externals = await detectExternalPackages(dir);
      await writeRunnableScaffold(dir, "index.js", externals);

      // The assertion that matters: it RUNS, and computes the right answer.
      // `counter: 3` proves the mutable binding was shared across files rather
      // than each importer getting its own copy — the failure a snapshot
      // export would produce and a parse check could never see.
      const out = execFileSync("node", [path.join(dir, RUNNER_FILENAME)], {
        encoding: "utf-8",
        timeout: 60_000
      });
      const report = JSON.parse(out.trim().split("\n").pop() ?? "{}");
      assert.deepStrictEqual(
        report,
        { first: "item-1", second: "item-3", counter: 3 },
        `emitted tree booted but computed the wrong result.\nfiles: ${srcFiles.join(", ")}\noutput: ${out}`
      );
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("emits an entry that requires the split files, not a copy of them", async () => {
    // A tree whose index.js inlined its sources would boot AND pass the check
    // above while being no split at all.
    const result = await stableSplitFromCode(BUNDLE, { clusterConfig: SMALL });
    assert.ok(result);
    const files = emitRunnableCjs(BUNDLE, result.ledger, result.wrapper);
    const entry = files.get("index.js") ?? "";
    assert.match(
      entry,
      /require\(/,
      "index.js must load the split files rather than contain them"
    );
    assert.ok(
      !entry.includes("function bootReport"),
      "index.js contains a source function body — the split did not actually split"
    );
  });
});
