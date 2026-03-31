import assert from "node:assert";
import { describe, it } from "node:test";
import type { LLMContext } from "../analysis/types.js";
import type { LLMProvider } from "../llm/types.js";
import { createRenamePlugin, getProximateUsedNames } from "./plugin.js";

const mockProvider: LLMProvider = {
  async suggestName(currentName: string, _context: LLMContext) {
    return { name: `${currentName}Renamed` };
  }
};

describe("createRenamePlugin sourceMap", () => {
  it("sourceMap is null when not requested", async () => {
    const rename = createRenamePlugin({ provider: mockProvider });
    const result = await rename("function a() { return 1; }");

    assert.strictEqual(result.sourceMap, null);
  });

  it("sourceMap is produced when sourceMap: true", async () => {
    const rename = createRenamePlugin({
      provider: mockProvider,
      sourceMap: true
    });
    const result = await rename("function a() { return 1; }");

    assert.ok(result.sourceMap, "sourceMap should not be null");
    assert.strictEqual(result.sourceMap.version, 3);
    assert.ok(result.sourceMap.mappings, "mappings should be non-empty");
    assert.ok(
      Array.isArray(result.sourceMap.sources),
      "sources should be an array"
    );
  });

  it("sourceMap sources uses sourceFileName", async () => {
    const rename = createRenamePlugin({
      provider: mockProvider,
      sourceMap: true
    });
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
    const rename = createRenamePlugin({
      provider: mockProvider,
      sourceMap: true
    });
    // Code with no functions — hits the early return path
    const result = await rename("var x = 1;");

    assert.ok(
      result.sourceMap,
      "sourceMap should be produced even with no functions"
    );
    assert.strictEqual(result.sourceMap.version, 3);
  });
});

describe("getProximateUsedNames", () => {
  function makeBinding(line: number, refLines: number[] = []) {
    return {
      identifier: { loc: { start: { line } } },
      referencePaths: refLines.map((l) => ({
        node: { loc: { start: { line: l } } }
      }))
    };
  }

  it("always includes well-known names", () => {
    const allNames = new Set(["exports", "require", "console", "a", "b"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      exports: makeBinding(1),
      require: makeBinding(2),
      console: makeBinding(3),
      a: makeBinding(1000), // far away
      b: makeBinding(1001) // far away
    };

    const result = getProximateUsedNames(allNames, [50], scopeBindings, 200);

    assert.ok(result.has("exports"), "should include well-known 'exports'");
    assert.ok(result.has("require"), "should include well-known 'require'");
    assert.ok(result.has("console"), "should include well-known 'console'");
  });

  it("excludes eligible names", () => {
    // With the default isEligible, single-char names and descriptive names
    // are all eligible (everything is a rename candidate). Use an override
    // that treats only single-char names as eligible.
    const isEligible = (name: string) => name.length === 1;
    const allNames = new Set(["a", "b", "c", "myVar"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      a: makeBinding(50),
      b: makeBinding(50),
      c: makeBinding(50),
      myVar: makeBinding(50)
    };

    const result = getProximateUsedNames(
      allNames,
      [50],
      scopeBindings,
      200,
      isEligible
    );

    assert.ok(!result.has("a"), "should exclude eligible 'a'");
    assert.ok(!result.has("b"), "should exclude eligible 'b'");
    assert.ok(!result.has("c"), "should exclude eligible 'c'");
    assert.ok(result.has("myVar"), "should include non-eligible 'myVar'");
  });

  it("includes names within +-100 lines, excludes those outside", () => {
    // Use an override that treats only single-char names as eligible,
    // so nearVar/farVar are preserved and subject to windowing
    const isEligible = (name: string) => name.length === 1;
    const allNames = new Set(["nearVar", "farVar"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      nearVar: makeBinding(55), // within +-100 of line 50
      farVar: makeBinding(500) // far away from line 50
    };

    const result = getProximateUsedNames(
      allNames,
      [50],
      scopeBindings,
      200,
      isEligible
    );

    assert.ok(result.has("nearVar"), "should include name within proximity");
    assert.ok(!result.has("farVar"), "should exclude name outside proximity");
  });

  it("includes name if any reference is within proximity", () => {
    const isEligible = (name: string) => name.length === 1;
    const allNames = new Set(["refVar"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      refVar: makeBinding(500, [45]) // declaration far, but reference near line 50
    };

    const result = getProximateUsedNames(
      allNames,
      [50],
      scopeBindings,
      200,
      isEligible
    );

    assert.ok(
      result.has("refVar"),
      "should include name whose reference is within proximity"
    );
  });

  it("returns all preserved names when below threshold", () => {
    const isEligible = (name: string) => name.length === 1;
    const allNames = new Set(["nearVar", "farVar", "a"]);
    const scopeBindings: Record<string, ReturnType<typeof makeBinding>> = {
      nearVar: makeBinding(50),
      farVar: makeBinding(500),
      a: makeBinding(50)
    };

    // totalBindings < 100 -> no windowing
    const result = getProximateUsedNames(
      allNames,
      [50],
      scopeBindings,
      50,
      isEligible
    );

    assert.ok(result.has("nearVar"), "should include nearVar");
    assert.ok(
      result.has("farVar"),
      "should include farVar (no windowing below threshold)"
    );
    assert.ok(!result.has("a"), "should still exclude eligible names");
  });
});
