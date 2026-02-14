import { describe, it } from "node:test";
import assert from "node:assert";
import { createRenamePlugin } from "./rename.js";
import type { LLMProvider } from "../llm/types.js";
import type { LLMContext } from "../analysis/types.js";

const mockProvider: LLMProvider = {
  async suggestName(currentName: string, _context: LLMContext) {
    return { name: currentName + "Renamed" };
  }
};

describe("createRenamePlugin sourceMap", () => {
  it("sourceMap is null when not requested", async () => {
    const rename = createRenamePlugin({ provider: mockProvider });
    const result = await rename("function a() { return 1; }");

    assert.strictEqual(result.sourceMap, null);
  });

  it("sourceMap is produced when sourceMap: true", async () => {
    const rename = createRenamePlugin({ provider: mockProvider, sourceMap: true });
    const result = await rename("function a() { return 1; }");

    assert.ok(result.sourceMap, "sourceMap should not be null");
    assert.strictEqual(result.sourceMap.version, 3);
    assert.ok(result.sourceMap.mappings, "mappings should be non-empty");
    assert.ok(Array.isArray(result.sourceMap.sources), "sources should be an array");
  });

  it("sourceMap sources uses sourceFileName", async () => {
    const rename = createRenamePlugin({ provider: mockProvider, sourceMap: true });
    const result = await rename("function a() { return 1; }");

    assert.ok(result.sourceMap);
    assert.ok(result.sourceMap.sources.includes("input.js"));
  });

  it("sourceMap is null for empty function list with sourceMap: false", async () => {
    const rename = createRenamePlugin({ provider: mockProvider });
    // Code with no functions
    const result = await rename("var x = 1;");

    assert.strictEqual(result.sourceMap, null);
  });

  it("sourceMap is produced for empty function list with sourceMap: true", async () => {
    const rename = createRenamePlugin({ provider: mockProvider, sourceMap: true });
    // Code with no functions — hits the early return path
    const result = await rename("var x = 1;");

    assert.ok(result.sourceMap, "sourceMap should be produced even with no functions");
    assert.strictEqual(result.sourceMap.version, 3);
  });
});
