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
