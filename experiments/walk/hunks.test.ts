import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  classifyHunk,
  type Hunk,
  parseHunks,
  pickExamples,
  tallyKinds
} from "./hunks.js";

const DIFF = [
  "diff --git a/src/a.js b/src/a.js",
  "index 1..2 100644",
  "--- a/src/a.js",
  "+++ b/src/a.js",
  "@@ -1,3 +1,3 @@",
  " const x = 1;",
  "-const fooBar = load(x);",
  "+const loadedValue = load(x);",
  "@@ -10,2 +10,3 @@",
  " f();",
  '+console.log("new feature");',
  "diff --git a/src/b.js b/src/b.js",
  "--- a/src/b.js",
  "+++ b/src/b.js",
  "@@ -1,4 +1,4 @@",
  "-a();",
  "-b();",
  "+b();",
  "+a();",
  "@@ -9,2 +9,2 @@",
  "-if (user.name) run(1);",
  "+if (user.id) run(2);",
  "\\ No newline at end of file"
].join("\n");

const hunk = (lines: string[]): Hunk => ({ file: "f.js", header: "@@", lines });

describe("parseHunks", () => {
  it("splits hunks and tags each with its b/ path", () => {
    const hs = parseHunks(DIFF);
    assert.equal(hs.length, 4);
    assert.deepEqual(
      hs.map((h) => h.file),
      ["src/a.js", "src/a.js", "src/b.js", "src/b.js"]
    );
    assert.equal(hs[3].lines.length, 2, "the no-newline marker is not a line");
  });
});

describe("classifyHunk", () => {
  it("a local rename is name-only", () => {
    assert.equal(classifyHunk(parseHunks(DIFF)[0]), "name-only");
  });
  it("pure additions are added, a swap is moved", () => {
    const hs = parseHunks(DIFF);
    assert.equal(classifyHunk(hs[1]), "added");
    assert.equal(classifyHunk(hs[2]), "moved");
  });
  it("a changed PROPERTY or literal is real, not name-only", () => {
    // maskIdentifiers keeps `.name` vs `.id` distinct — a property is semantic.
    assert.equal(classifyHunk(parseHunks(DIFF)[3]), "real");
  });
  it("pure deletions are removed", () => {
    assert.equal(classifyHunk(hunk(["-gone();", " kept();"])), "removed");
  });
});

describe("pickExamples", () => {
  it("never shows two hunks from one file for a kind, and skips walls", () => {
    const wall = hunk(Array.from({ length: 60 }, (_, i) => `+x${i}();`));
    const a = { ...hunk(["-p(1);", "+p(2);", "+q();"]), file: "a.js" };
    const a2 = { ...hunk(["-r(1);", "+r(3);"]), file: "a.js" };
    const b = { ...hunk(["-s(1);", "+s(9);"]), file: "b.js" };
    const picked = pickExamples([wall, a, a2, b], ["real", "added"], 5);
    assert.deepEqual(
      picked.map((p) => `${p.kind}:${p.hunk.file}`),
      ["real:a.js", "real:b.js"]
    );
  });
});

describe("tallyKinds", () => {
  it("counts changed lines per kind", () => {
    const t = tallyKinds(parseHunks(DIFF));
    assert.deepEqual(t, {
      "name-only": 2,
      moved: 4,
      added: 1,
      removed: 0,
      real: 2
    });
  });
});
