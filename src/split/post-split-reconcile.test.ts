import assert from "node:assert";
import {
  configureKillSwitches,
  resetKillSwitchesForTests
} from "../kill-switches.js";
import { describe, it } from "node:test";
import { createIsEligible } from "../rename/rename-eligibility.js";
import { postSplitReconcile } from "./post-split-reconcile.js";
import type { StableSplitLedger } from "./stable-split.js";

const isEligible = createIsEligible(undefined, undefined);

function ledgerOf(
  files: string[],
  order: string[],
  emitNames: (string | null)[],
  nameToFiles: Record<string, string[]> = {}
): StableSplitLedger {
  return {
    version: 1,
    files,
    nameToFiles,
    order,
    emitHashes: order.map((_, i) => `h${i}`),
    emitNames
  };
}

/** Same statement shape either side, one local named differently. */
const PRIOR_LOCAL = `function renderRow(entry) {
  const rowLabel = entry.label;
  return rowLabel + entry.id;
}`;
const FRESH_LOCAL = `function renderRow(entry) {
  const labelText = entry.label;
  return labelText + entry.id;
}`;

function run(
  files: Map<string, string>,
  prior: Map<string, string>,
  ledger: StableSplitLedger
) {
  const result = postSplitReconcile({
    ledger,
    readFresh: (f) => files.get(f),
    readPrior: (f) => prior.get(f),
    isEligible
  });
  // The caller writes `changed` back over the tree; model that here so the
  // assertions read against the text that would ship.
  return {
    ...result,
    shipped: new Map([...files, ...result.changed])
  };
}

describe("postSplitReconcile", () => {
  it("snaps a drifted local back to the prior release's name and reports it", () => {
    const ledger = ledgerOf(["a.js"], ["a.js"], ["renderRow"]);
    const out = run(
      new Map([["a.js", FRESH_LOCAL]]),
      new Map([["a.js", PRIOR_LOCAL]]),
      ledger
    );
    assert.deepStrictEqual(
      out.renames.map((r) => [r.file, r.fromName, r.toName]),
      [["a.js", "labelText", "rowLabel"]]
    );
    assert.strictEqual(out.shipped.get("a.js"), PRIOR_LOCAL);
  });

  it("rewrites the text without re-generating: only the renamed tokens move", () => {
    // Deliberately odd spacing the babel generator would normalize away. A
    // re-generate would reflow it and churn the whole file; the textual
    // rewrite must not.
    const prior = `function f() {\n  const  aaa   = 1;\n  return  aaa;\n}`;
    const fresh = `function f() {\n  const  bbb   = 1;\n  return  bbb;\n}`;
    const out = run(
      new Map([["a.js", fresh]]),
      new Map([["a.js", prior]]),
      ledgerOf(["a.js"], ["a.js"], ["f"])
    );
    assert.strictEqual(out.shipped.get("a.js"), prior);
  });

  it("expands a shorthand property instead of rewriting its key", () => {
    // `{ count }` holds TWO identifier nodes at one loc — key and value —
    // and the rename moves only the value. Substituting the bare new name
    // at that loc rewrites the PROPERTY KEY, and the reparse guard then
    // throws away the WHOLE file's reconciliation. The occurrence must
    // expand to `count: tally` instead.
    const prior = `function makeCounter() {
  const runningTally = 1;
  log(runningTally);
  send(runningTally);
  return { runningTally };
}`;
    const fresh = `function makeCounter() {
  const countValue = 1;
  log(countValue);
  send(countValue);
  return { countValue };
}`;
    const out = run(
      new Map([["a.js", fresh]]),
      new Map([["a.js", prior]]),
      ledgerOf(["a.js"], ["a.js"], ["makeCounter"])
    );
    assert.deepStrictEqual(
      out.renames.map((r) => [r.fromName, r.toName]),
      [["countValue", "runningTally"]],
      "the shorthand file must reconcile, not silently discard"
    );
    const shipped = out.shipped.get("a.js") ?? "";
    assert.match(shipped, /const runningTally = 1/);
    assert.match(
      shipped,
      /return \{\s*countValue: runningTally\s*\}/,
      `the key must survive the rename, got:\n${shipped}`
    );
  });

  it("refuses same-named sibling declarations — the locator depends on it", () => {
    // One statement declares `attemptNum` TWICE (two inner functions).
    // If BOTH were renamed, `locateRenames` would give each ordinal 0
    // (computed on the reparsed file, where co-renamed siblings no longer
    // hold the old name) and the bundle carry would rename the FIRST
    // declaration for both. That corruption is unreachable ONLY because
    // diff-reconcile's clean-declaration proof refuses a group with two
    // declarations of one name — this test pins that refusal. If it ever
    // fails because the tier learned to emit such pairs, nameOrdinal must
    // learn to count co-renamed siblings in the same change.
    const shared = `  log("phase one start");
  log("phase two start");
  log("phase three start");
  log("phase four start");
  log("phase five start");
  log("phase six start");
  log("phase seven start");
  log("phase eight start");`;
    const prior = `function runJobs(items) {
${shared}
  function first(list) { let retryCount = list.length; return retryCount + 1; }
  function second(list) { let retryCount = list.length; return retryCount + 2; }
  return [first(items), second(items)];
}`;
    const fresh = `function runJobs(items) {
${shared}
  function first(list) { let attemptNum = list.length; return attemptNum + 1; }
  function second(list) { let attemptNum = list.length; return attemptNum + 2; }
  return [first(items), second(items)];
}`;
    const out = run(
      new Map([["a.js", fresh]]),
      new Map([["a.js", prior]]),
      ledgerOf(["a.js"], ["a.js"], ["runJobs"])
    );
    assert.deepStrictEqual(
      out.renames.map((r) => [r.fromName, r.toName]),
      [],
      "an ambiguous same-name declaration group must abstain, not guess"
    );
    assert.strictEqual(out.shipped.get("a.js"), fresh, "file untouched");
  });

  it("leaves a file with no prior counterpart untouched", () => {
    const out = run(
      new Map([["new.js", FRESH_LOCAL]]),
      new Map(),
      ledgerOf(["new.js"], ["new.js"], ["renderRow"])
    );
    assert.deepStrictEqual(out.renames, []);
    assert.strictEqual(out.shipped.get("new.js"), FRESH_LOCAL);
  });

  it("only visits the ledger's own files — vendor and metadata are not named", () => {
    const files = new Map([
      ["a.js", FRESH_LOCAL],
      ["vendor/lib.js", FRESH_LOCAL]
    ]);
    const out = run(
      files,
      new Map([
        ["a.js", PRIOR_LOCAL],
        ["vendor/lib.js", PRIOR_LOCAL]
      ]),
      ledgerOf(["a.js"], ["a.js"], ["renderRow"])
    );
    assert.deepStrictEqual(
      out.renames.map((r) => r.file),
      ["a.js"]
    );
    assert.strictEqual(out.shipped.get("vendor/lib.js"), FRESH_LOCAL);
  });

  it("is off under --disable post-split-reconcile", () => {
    configureKillSwitches({ disable: ["post-split-reconcile"] });
    try {
      const out = run(
        new Map([["a.js", FRESH_LOCAL]]),
        new Map([["a.js", PRIOR_LOCAL]]),
        ledgerOf(["a.js"], ["a.js"], ["renderRow"])
      );
      assert.deepStrictEqual(out.renames, []);
      assert.strictEqual(out.shipped.get("a.js"), FRESH_LOCAL);
    } finally {
      resetKillSwitchesForTests();
    }
  });

  it("never renames an import alias (the cross-module misfire 054 read)", () => {
    const prior = `const bunDetection = require("./bun-detection.js");
function readOne(ctx) {
  return bunDetection.configFilePath(ctx.path);
}
function readTwo(list) {
  return list.map(e => bunDetection.configFilePath(e));
}`;
    const fresh = `const cwdManager = require("./cwd-manager.js");
function readOne(ctx) {
  return cwdManager.configFilePath(ctx.path);
}
function readTwo(list) {
  return list.map(e => cwdManager.configFilePath(e));
}`;
    const out = run(
      new Map([["a.js", fresh]]),
      new Map([["a.js", prior]]),
      ledgerOf(["a.js"], ["a.js", "a.js", "a.js"], [null, "readOne", "readTwo"])
    );
    assert.deepStrictEqual(out.renames, []);
    assert.strictEqual(out.shipped.get("a.js"), fresh);
  });

  describe("ledger coherence", () => {
    const priorTop = `const widgetCount = compute();
function useIt() {
  return widgetCount + 1;
}`;
    const freshTop = `const gadgetTally = compute();
function useIt() {
  return gadgetTally + 1;
}`;

    it("patches emitNames for a renamed TOP-LEVEL declaration", () => {
      const ledger = ledgerOf(
        ["a.js"],
        ["a.js", "a.js"],
        ["gadgetTally", "useIt"],
        { gadgetTally: ["a.js"], useIt: ["a.js"] }
      );
      const before = [...(ledger.emitHashes ?? [])];
      const out = run(
        new Map([["a.js", freshTop]]),
        new Map([["a.js", priorTop]]),
        ledger
      );
      assert.strictEqual(out.renames.length, 1);
      assert.deepStrictEqual(ledger.emitNames, ["widgetCount", "useIt"]);
      assert.deepStrictEqual(
        ledger.emitHashes,
        before,
        "statementHash masks names — emitHashes must not move"
      );
      assert.deepStrictEqual(ledger.nameToFiles, {
        widgetCount: ["a.js"],
        useIt: ["a.js"]
      });
    });

    it("leaves emitNames alone for an inner local that shares a top-level name", () => {
      // `labelText` is an inner local here; a name-keyed patch that did not
      // check top-level-ness would rewrite the unrelated slot of the same
      // spelling and point the next release's aligner at a phantom.
      const ledger = ledgerOf(
        ["a.js"],
        ["a.js", "b.js"],
        ["renderRow", "labelText"],
        { labelText: ["b.js"] }
      );
      const out = run(
        new Map([["a.js", FRESH_LOCAL]]),
        new Map([["a.js", PRIOR_LOCAL]]),
        ledger
      );
      assert.strictEqual(out.renames.length, 1);
      assert.deepStrictEqual(ledger.emitNames, ["renderRow", "labelText"]);
      assert.deepStrictEqual(ledger.nameToFiles, { labelText: ["b.js"] });
    });

    it("patches a MULTI-declarator slot, whose key joins every declared name", () => {
      // `statementAlignName` records ALL of a statement's declared names,
      // sorted and comma-joined ("alpha,beta,gamma") — `var a, b, c;` and
      // `var d, e, f;` share a statement hash and have to key apart. A patch
      // comparing the slot against a bare name never matches one of these, so
      // the next release would align on a name that is no longer in the tree.
      const priorMulti = `var alpha, beta, gamma;
function useThem() {
  return alpha + beta + gamma;
}`;
      const freshMulti = `var alpha, bravo, gamma;
function useThem() {
  return alpha + bravo + gamma;
}`;
      const ledger = ledgerOf(
        ["a.js"],
        ["a.js", "a.js"],
        ["alpha,bravo,gamma", "useThem"],
        { alpha: ["a.js"], bravo: ["a.js"], gamma: ["a.js"] }
      );
      const out = run(
        new Map([["a.js", freshMulti]]),
        new Map([["a.js", priorMulti]]),
        ledger
      );
      assert.deepStrictEqual(
        out.renames.map((r) => [r.fromName, r.toName, r.topLevel]),
        [["bravo", "beta", true]]
      );
      assert.deepStrictEqual(ledger.emitNames, ["alpha,beta,gamma", "useThem"]);
      assert.deepStrictEqual(ledger.nameToFiles, {
        alpha: ["a.js"],
        beta: ["a.js"],
        gamma: ["a.js"]
      });
      assert.strictEqual(out.stats.incoherent, 0);
    });

    it("survives a rename CHAIN, where one rename's target is another's source", () => {
      // Real, and found on the first live run: `oversize-report.js` shipped
      // `readSessionTemplate -> loadSessionTemplate` AND
      // `fetchSessionNotesPrompt -> readSessionTemplate`. The reconcile rounds
      // produce these — the second is blocked until the first frees the name.
      // Two things have to hold: the freed name must be substituted
      // SIMULTANEOUSLY (not name-by-name, where the later removal would delete
      // the home the earlier one just wrote), and the coherence check must not
      // mistake a name that came BACK as a target for one left behind.
      const prior = `const alphaLoad = one();
const alphaRead = two();
function useThem() {
  return alphaLoad + alphaRead;
}`;
      const fresh = `const alphaRead = one();
const alphaFetch = two();
function useThem() {
  return alphaRead + alphaFetch;
}`;
      const ledger = ledgerOf(
        ["a.js"],
        ["a.js", "a.js", "a.js"],
        ["alphaRead", "alphaFetch", "useThem"],
        { alphaRead: ["a.js"], alphaFetch: ["a.js"], useThem: ["a.js"] }
      );
      const out = run(
        new Map([["a.js", fresh]]),
        new Map([["a.js", prior]]),
        ledger
      );
      assert.deepStrictEqual(
        out.renames.map((r) => `${r.fromName}->${r.toName}`).sort(),
        ["alphaFetch->alphaRead", "alphaRead->alphaLoad"]
      );
      assert.strictEqual(
        out.stats.incoherent,
        0,
        "a name that came back as a TARGET is not an entry left behind"
      );
      assert.deepStrictEqual(ledger.emitNames, [
        "alphaLoad",
        "alphaRead",
        "useThem"
      ]);
      assert.deepStrictEqual(ledger.nameToFiles, {
        alphaLoad: ["a.js"],
        alphaRead: ["a.js"],
        useThem: ["a.js"]
      });
    });

    it("reports incoherence rather than leaving it silent", () => {
      // The patch is name-keyed, so `incoherent` is the check that it landed.
      // Here the same top-level name is ALSO recorded against a second slot of
      // the same file — a shape the patch has to catch, and if it ever stopped
      // catching it the next release would align on a dead name.
      const ledger = ledgerOf(
        ["a.js"],
        ["a.js", "a.js"],
        ["gadgetTally", "gadgetTally,other"],
        { gadgetTally: ["a.js", "a.js"] }
      );
      const out = run(
        new Map([
          [
            "a.js",
            `const gadgetTally = compute();\nfunction useIt() {\n  return gadgetTally + 1;\n}`
          ]
        ]),
        new Map([
          [
            "a.js",
            `const widgetCount = compute();\nfunction useIt() {\n  return widgetCount + 1;\n}`
          ]
        ]),
        ledger
      );
      assert.strictEqual(out.renames.length, 1);
      assert.strictEqual(
        out.stats.incoherent,
        0,
        `no slot may still name gadgetTally: ${JSON.stringify(ledger.emitNames)}`
      );
      assert.deepStrictEqual(ledger.emitNames, [
        "widgetCount",
        "other,widgetCount"
      ]);
    });
  });

  it("abstains when the prior file is too dissimilar (corpus gate)", () => {
    const prior = Array.from({ length: 40 }, (_, i) => `const v${i} = ${i};`)
      .join("\n")
      .concat(
        "\nfunction renderRow(entry) {\n  const rowLabel = entry.label;\n  return rowLabel;\n}"
      );
    const out = run(
      new Map([["a.js", FRESH_LOCAL]]),
      new Map([["a.js", prior]]),
      ledgerOf(["a.js"], ["a.js"], ["renderRow"])
    );
    assert.deepStrictEqual(out.renames, []);
    assert.strictEqual(out.stats.corpusGated, 1);
  });
});
