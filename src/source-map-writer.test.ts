import { describe, it } from "node:test";
import assert from "node:assert";
import fs from "fs/promises";
import path from "path";
import os from "os";
import { createSourceMapWriter } from "./source-map-writer.js";

async function withTempDir(fn: (dir: string) => Promise<void>) {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "smwriter-"));
  try {
    await fn(dir);
  } finally {
    await fs.rm(dir, { recursive: true, force: true });
  }
}

describe("createSourceMapWriter", () => {
  it("write() creates .map file and appends sourceMappingURL", async () => {
    await withTempDir(async (dir) => {
      const jsPath = path.join(dir, "output.js");
      await fs.writeFile(jsPath, "var x = 1;\n");

      const writer = createSourceMapWriter();
      writer.capture({ version: 3, sources: ["input.js"], mappings: "AAAA" });
      await writer.write(jsPath);

      // Check .map file exists and is valid
      const mapContent = JSON.parse(await fs.readFile(jsPath + ".map", "utf-8"));
      assert.strictEqual(mapContent.version, 3);
      assert.strictEqual(mapContent.file, "output.js");
      assert.deepStrictEqual(mapContent.sources, ["input.js"]);

      // Check sourceMappingURL was appended
      const jsContent = await fs.readFile(jsPath, "utf-8");
      assert.ok(jsContent.includes("//# sourceMappingURL=output.js.map"));
    });
  });

  it("write() is a no-op when no map captured", async () => {
    await withTempDir(async (dir) => {
      const jsPath = path.join(dir, "output.js");
      await fs.writeFile(jsPath, "var x = 1;\n");

      const writer = createSourceMapWriter();
      await writer.write(jsPath);

      // No .map file should exist
      await assert.rejects(fs.access(jsPath + ".map"));

      // JS file should be unchanged
      const jsContent = await fs.readFile(jsPath, "utf-8");
      assert.strictEqual(jsContent, "var x = 1;\n");
    });
  });

  it("capture(null) means write() is a no-op", async () => {
    await withTempDir(async (dir) => {
      const jsPath = path.join(dir, "output.js");
      await fs.writeFile(jsPath, "var x = 1;\n");

      const writer = createSourceMapWriter();
      writer.capture(null);
      await writer.write(jsPath);

      await assert.rejects(fs.access(jsPath + ".map"));
    });
  });

  it("pending is cleared after write()", async () => {
    await withTempDir(async (dir) => {
      const jsPath1 = path.join(dir, "a.js");
      const jsPath2 = path.join(dir, "b.js");
      await fs.writeFile(jsPath1, "var a = 1;\n");
      await fs.writeFile(jsPath2, "var b = 2;\n");

      const writer = createSourceMapWriter();
      writer.capture({ version: 3, sources: ["input.js"], mappings: "AAAA" });
      await writer.write(jsPath1);

      // Second write without new capture should be no-op
      await writer.write(jsPath2);
      await assert.rejects(fs.access(jsPath2 + ".map"));
    });
  });
});
