import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { after, describe, it } from "node:test";
import { changedLines, changedLinesInTree, treeFileDelta } from "./diff.js";
import { bundleStatements, fileStatements, treeFiles } from "./trees.js";

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "explib-"));
after(() => fs.rmSync(tmp, { recursive: true, force: true }));

function tree(name: string, files: Record<string, string>): string {
  const root = path.join(tmp, name);
  for (const [rel, content] of Object.entries(files)) {
    const p = path.join(root, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, content);
  }
  return root;
}

describe("experiments/lib diff", () => {
  it("counts a modified line TWICE — once removed, once added", () => {
    // This is what every published "git lines" figure in this repo means. A
    // counter that reported 1 here would halve every ceiling.
    assert.strictEqual(changedLines("a\nb\nc\n", "a\nCHANGED\nc\n"), 2);
  });

  it("counts pure additions and deletions once each", () => {
    assert.strictEqual(changedLines("a\n", "a\nb\n"), 1);
    assert.strictEqual(changedLines("a\nb\n", "a\n"), 1);
    assert.strictEqual(changedLines("same\n", "same\n"), 0);
  });

  it("counts files that exist on only ONE side", () => {
    // Without `-N`, diff reports "Only in ..." and contributes zero, silently
    // excluding every added and deleted file — the largest single category of
    // churn in a release diff.
    const a = tree("only-a", { "src/keep.js": "x\n" });
    const b = tree("only-b", {
      "src/keep.js": "x\n",
      "src/added.js": "1\n2\n"
    });
    const d = changedLinesInTree(a, b);
    assert.strictEqual(d.total, 2, "both lines of the added file must count");
    assert.strictEqual(d.byFile.get("src/added.js"), 2);
  });

  it("reports added and removed files separately from line counts", () => {
    const a = tree("delta-a", { "src/gone.js": "x\n", "src/keep.js": "k\n" });
    const b = tree("delta-b", { "src/keep.js": "k\n", "src/new.js": "n\n" });
    assert.deepStrictEqual(treeFileDelta(a, b), {
      added: ["src/new.js"],
      removed: ["src/gone.js"]
    });
  });

  it("attributes tree churn to the right file", () => {
    const a = tree("attr-a", { "x.js": "1\n", "sub/y.js": "1\n" });
    const b = tree("attr-b", { "x.js": "1\n", "sub/y.js": "CHANGED\n" });
    const d = changedLinesInTree(a, b);
    assert.strictEqual(d.total, 2);
    assert.strictEqual(d.byFile.get("sub/y.js"), 2);
    assert.strictEqual(d.byFile.get("x.js"), undefined);
  });
});

describe("experiments/lib trees", () => {
  // A bundle wraps everything in an IIFE; a split file does not. Reading the
  // wrong one returns a plausible number instead of an error, which is the
  // trap that made ~7 experiment scripts look wrong when they were fine.
  const BUNDLE = `(function (exports, require, module, __filename, __dirname) {
  var a = 1;
  var b = 2;
  function c() { return a + b; }
${Array.from({ length: 60 }, (_, i) => `  var pad${i} = ${i};`).join("\n")}
});`;
  const SPLIT_FILE = "var a = 1;\nvar b = 2;\nfunction c() { return a + b; }\n";

  it("reads a bundle's statements from inside the wrapper", () => {
    const stmts = bundleStatements(BUNDLE);
    assert.ok(
      stmts.length > 3,
      `expected the wrapper body, got ${stmts.length} statements`
    );
  });

  it("REFUSES a split file, instead of returning the one wrapper statement", () => {
    assert.throws(
      () => bundleStatements(SPLIT_FILE, "a-split-file"),
      /no wrapper IIFE found/
    );
  });

  it("reads a split file's statements from the program body", () => {
    assert.strictEqual(fileStatements(SPLIT_FILE).length, 3);
  });

  it("REFUSES a bundle where a split file was expected", () => {
    assert.throws(
      () => fileStatements(BUNDLE, "a-bundle"),
      /looks like a BUNDLE/
    );
  });

  it("excludes .humanify/ when listing a tree", () => {
    const t = tree("meta", {
      "src/real.js": "x\n",
      ".humanify/humanified.js": "y\n"
    });
    assert.deepStrictEqual(treeFiles(t), ["src/real.js"]);
  });
});
