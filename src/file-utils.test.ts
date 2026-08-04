import assert from "node:assert";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import { listJsFilesRecursive } from "./file-utils.js";

describe("listJsFilesRecursive", () => {
  it("lists nested JS files relative to rootDir, skipping node_modules", () => {
    const root = mkdtempSync(path.join(tmpdir(), "listjs-"));
    try {
      writeFileSync(path.join(root, "a.js"), "");
      mkdirSync(path.join(root, "sub"), { recursive: true });
      writeFileSync(path.join(root, "sub", "b.js"), "");
      writeFileSync(path.join(root, "sub", "notes.txt"), "");
      mkdirSync(path.join(root, "node_modules", "dep"), { recursive: true });
      writeFileSync(path.join(root, "node_modules", "dep", "index.js"), "");

      assert.deepStrictEqual(listJsFilesRecursive(root).sort(), [
        "a.js",
        path.join("sub", "b.js")
      ]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  /**
   * The three tree walkers must agree on `.humanify/`.
   *
   * `jsFilesUnder` (runnable-scaffold.ts) and `treeFiles`
   * (experiments/lib/trees.ts) both skip it; this one did not, and
   * docs/responsibility.md ranked the divergence #3 of what is still
   * duplicated. The live consequence: `env-reads` pointed at a split output dir
   * walks BOTH the emitted source files and `.humanify/humanified.js`, which
   * contains all of the same code — so every env read is counted twice.
   *
   * `.humanify/` is pipeline metadata (the ledger, the carried bundle, stats),
   * not emitted source. No walker wants it, and three answers to "what is in
   * this tree" is exactly the duplication this repo keeps paying for.
   */
  it("skips .humanify/ — it is metadata, and counting it double-counts", () => {
    const root = mkdtempSync(path.join(tmpdir(), "listjs-humanify-"));
    try {
      writeFileSync(path.join(root, "a.js"), "");
      mkdirSync(path.join(root, ".humanify"), { recursive: true });
      // The carried bundle: every emitted file's code, all over again.
      writeFileSync(path.join(root, ".humanify", "humanified.js"), "");
      assert.deepStrictEqual(listJsFilesRecursive(root).sort(), ["a.js"]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("broadens to the given extensions", () => {
    const root = mkdtempSync(path.join(tmpdir(), "listjs-ext-"));
    try {
      writeFileSync(path.join(root, "a.js"), "");
      writeFileSync(path.join(root, "b.cjs"), "");
      writeFileSync(path.join(root, "c.mjs"), "");
      assert.deepStrictEqual(
        listJsFilesRecursive(root, root, [".js", ".cjs", ".mjs"]).sort(),
        ["a.js", "b.cjs", "c.mjs"]
      );
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
