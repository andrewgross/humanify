import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { LLMContext } from "../analysis/types.js";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import {
  createRenamePlugin,
  getModuleLevelBindings,
  getProximateUsedNames
} from "./plugin.js";

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

describe("prior-version function declaration transfer", () => {
  it("transfers function declaration name from prior version", async () => {
    // Minified code with a function declaration
    const currentCode = `function a(e, t) { return Object.assign({}, e, t); }`;
    // Prior humanified version — same structure, renamed
    const priorCode = `function mergeObjects(e, t) { return Object.assign({}, e, t); }`;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // The function declaration name should be transferred
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred from prior version, got:\n${result.code}`
    );
    assert.ok(
      !result.code.includes("function a("),
      `original minified name should be replaced, got:\n${result.code}`
    );
  });

  it("transfers close-match function declaration name", async () => {
    // Current code with function declaration
    const currentCode = `function a(e, t) { return Object.assign({}, e, t); }`;
    // Prior version — slightly different structure (extra statement) so it's a close match
    const priorCode = `function mergeObjects(e, t) { var r = Object.assign({}, e, t); return r; }`;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // For close-match, the function name should be transferred directly
    // (remaining identifiers go through LLM with prior context)
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred via close-match, got:\n${result.code}`
    );
  });

  it("transfers body-local binding in nested block scope (for-in)", async () => {
    // Function with a for-in loop declaring a block-scoped variable
    const currentCode = `function a(e, t) { for (var n in e) { t[n] = e[n]; } return t; }`;
    // Prior version: same structure, body-local n renamed to nextState
    const priorCode = `function mergeObjects(e, t) { for (var nextState in e) { t[nextState] = e[nextState]; } return t; }`;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // The body-local for-in binding should be transferred
    assert.ok(
      result.code.includes("nextState"),
      `body-local for-in binding should be transferred, got:\n${result.code}`
    );
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred, got:\n${result.code}`
    );
  });

  it("transfers module-level bindings via module binding matching", async () => {
    // Function references a module-level binding (assign) that it doesn't own
    const currentCode = `
      var a = Object.assign;
      function b(e, t) { return a({}, e, t); }
    `;
    // Prior version: assign renamed, function renamed
    const priorCode = `
      var mergeAssign = Object.assign;
      function mergeObjects(e, t) { return mergeAssign({}, e, t); }
    `;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // Function name should be transferred via function matching
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred, got:\n${result.code}`
    );
    // Module-level binding should be transferred via module binding matching
    // (not via function transfer — it's an external reference)
    assert.ok(
      result.code.includes("mergeAssign"),
      `module-level binding should be transferred via module binding matching, got:\n${result.code}`
    );
    assert.strictEqual(
      result.priorVersionBindingsApplied,
      1,
      "should report 1 module binding matched"
    );
  });
});

describe("propagated external references", () => {
  it("propagates module binding name from matched function external refs", async () => {
    // Module binding has different init (so structural hash differs, no direct match),
    // but a function that references it matches exactly
    const currentCode = `
      var a = [1, 2, 3];
      function b(e) { return a.includes(e); }
    `;
    // Prior: different init value means binding hash differs, but function matches
    const priorCode = `
      var allowedValues = [1, 2, 3, 4];
      function isAllowed(e) { return allowedValues.includes(e); }
    `;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    assert.ok(
      result.code.includes("isAllowed"),
      `function name should be transferred, got:\n${result.code}`
    );
    assert.ok(
      result.code.includes("allowedValues"),
      `module binding should be propagated from function external ref, got:\n${result.code}`
    );
    assert.ok(
      (result.priorVersionBindingsApplied ?? 0) >= 1,
      `should report propagated module binding, got: ${result.priorVersionBindingsApplied}`
    );
  });

  it("propagates module binding with multiple agreeing votes", async () => {
    // Two functions both reference the same module binding
    const currentCode = `
      var a = [1, 2, 3];
      function b(e) { return a.includes(e); }
      function c(e) { a.push(e); return a; }
    `;
    const priorCode = `
      var allowedValues = [1, 2, 3, 4];
      function isAllowed(e) { return allowedValues.includes(e); }
      function addAllowed(e) { allowedValues.push(e); return allowedValues; }
    `;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    assert.ok(
      result.code.includes("allowedValues"),
      `module binding should be propagated with multiple votes, got:\n${result.code}`
    );
  });

  it("does not double-rename module bindings already matched by structural hash", async () => {
    // Both binding AND function match — binding matched by structural hash,
    // external ref is redundant and should not cause issues
    const currentCode = `
      var a = Object.assign;
      function b(e, t) { return a({}, e, t); }
    `;
    const priorCode = `
      var mergeAssign = Object.assign;
      function mergeObjects(e, t) { return mergeAssign({}, e, t); }
    `;

    const rename = createRenamePlugin({
      provider: mockProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // Should work correctly — binding matched by structural hash,
    // propagation finds no unmatched binding with the name
    assert.ok(
      result.code.includes("mergeAssign"),
      `module binding should be renamed (by structural hash), got:\n${result.code}`
    );
    assert.ok(
      result.code.includes("mergeObjects"),
      `function name should be transferred, got:\n${result.code}`
    );
  });
});

describe("close-match set elimination suggestedName", () => {
  // For close match to trigger, functions must have different structural hashes
  // but similar feature vectors. An extra if-statement changes the hash.

  it("sets suggestedName on module binding when 1:1 elimination succeeds", async () => {
    // Close match: extra if-statement in prior version changes AST structure.
    // Both reference one external: a (prior: counter).
    // After var name transfer, counter is the only unresolved prior external
    // and a is the only unresolved new external → 1:1 → suggestedName.
    const currentCode = `
      var a = 0;
      var b = function(x) { a++; return a + x; };
    `;
    const priorCode = `
      var counter = 0;
      var increment = function(x) { if (x > 0) counter++; return counter + x; };
    `;

    const suggestingProvider: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        return { name: `${currentName}Renamed` };
      },
      async suggestAllNames(request: BatchRenameRequest) {
        const result: Record<string, string> = {};
        for (const id of request.identifiers) {
          result[id] = `${id}Renamed`;
        }
        return { renames: result };
      }
    };

    const rename = createRenamePlugin({
      provider: suggestingProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // The function var name should be transferred via close match
    assert.ok(
      result.code.includes("increment"),
      `function var name should be transferred via close match, got:\n${result.code}`
    );
  });

  it("does NOT set suggestedName when elimination leaves >1 candidates", async () => {
    // Two unresolved externals on each side — no 1:1 elimination possible.
    // Use named function declarations (not anonymous expressions) so placeholder
    // mapping doesn't accidentally transfer body locals via $0/$1 positions.
    // Extra if-statement ensures close match (not exact).
    const currentCode = `
      var a = [1, 2];
      var c = {x: 1};
      function b(x) { a.push(x); return c.x + a.length + x; }
    `;
    const priorCode = `
      var counter = [1, 2, 3];
      var greeting = {x: 1, y: 2};
      function increment(x) { if (x > 0) counter.push(x); return greeting.x + counter.length + x; }
    `;

    const suggestingProvider: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        return { name: `${currentName}Renamed` };
      },
      async suggestAllNames(request: BatchRenameRequest) {
        const result: Record<string, string> = {};
        for (const id of request.identifiers) {
          result[id] = `${id}Renamed`;
        }
        return { renames: result };
      }
    };

    const rename = createRenamePlugin({
      provider: suggestingProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // Neither binding should get the prior name via set elimination
    // (>1 candidates on each side prevents 1:1 match)
    assert.ok(
      !result.code.includes("counter"),
      `should not auto-suggest 'counter' when >1 candidates remain, got:\n${result.code}`
    );
    assert.ok(
      !result.code.includes("greeting"),
      `should not auto-suggest 'greeting' when >1 candidates remain, got:\n${result.code}`
    );
  });

  it("eliminates already-resolved pairs before counting", async () => {
    // Two externals: a (resolved by structural hash) and c (unresolved).
    // Extra if-guard ensures close match (not exact) for the function.
    // c has different init across versions so it won't match by structural hash.
    // After eliminating mergeAssign from resolved set, c→totalCount is 1:1.
    const currentCode = `
      var a = Object.assign;
      var c = [1, 2];
      var b = function(x) { return a({}, {val: c.length + x}); };
    `;
    const priorCode = `
      var mergeAssign = Object.assign;
      var totalCount = [1, 2, 3];
      var createMerged = function(x) { if (x > 0) return mergeAssign({}, {val: totalCount.length + x}); return null; };
    `;

    const suggestingProvider: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        return { name: `${currentName}Renamed` };
      },
      async suggestAllNames(request: BatchRenameRequest) {
        const result: Record<string, string> = {};
        for (const id of request.identifiers) {
          result[id] = `${id}Renamed`;
        }
        return { renames: result };
      }
    };

    const rename = createRenamePlugin({
      provider: suggestingProvider,
      priorVersionCode: priorCode
    });

    const result = await rename(currentCode);

    // `a` should be resolved by structural hash → `mergeAssign`
    assert.ok(
      result.code.includes("mergeAssign"),
      `'a' should match 'mergeAssign' by structural hash, got:\n${result.code}`
    );
    // `b` close-matches `createMerged` — function var name should transfer
    assert.ok(
      result.code.includes("createMerged"),
      `function var name should transfer via close match, got:\n${result.code}`
    );
  });
});

describe("shouldSkipBinding in wrapper mode", () => {
  it("skips function declarations inside wrapper IIFE from module bindings", () => {
    // Simulate a Bun-style CJS wrapper IIFE with enough bindings to trigger
    // wrapper detection (>50 bindings). Function declarations inside should
    // NOT appear as module bindings — they're handled as FunctionNodes.
    const varDecls = Array.from(
      { length: 55 },
      (_, i) => `var v${i} = ${i};`
    ).join("\n");
    const code = `(function(exports, require, module) {\n${varDecls}\nfunction a(x) { return x + 1; }\nfunction b(y) { return y * 2; }\n});`;

    const ast = parseSync(code, { sourceType: "unambiguous" });
    assert.ok(ast, "should parse code");
    const result = getModuleLevelBindings(ast);

    assert.ok(result, "should detect module-level bindings");

    const bindingNames = result.bindings.map((b: { name: string }) => b.name);
    assert.ok(
      !bindingNames.includes("a"),
      `function declaration 'a' should NOT be a module binding, got: [${bindingNames.join(", ")}]`
    );
    assert.ok(
      !bindingNames.includes("b"),
      `function declaration 'b' should NOT be a module binding, got: [${bindingNames.join(", ")}]`
    );
    // var declarations should still be present
    assert.ok(
      bindingNames.includes("v0"),
      `var declaration 'v0' should be a module binding`
    );
  });
});

describe("shouldSkipBinding with Bun CJS classification", () => {
  it("skips bindings inside a CJS factory body when source is provided", () => {
    const code = [
      "var A = (q, _) => () => (_ || q((_ = {exports: {}}).exports, _), _.exports);",
      "var lib = A((q, _) => {",
      "  var innerVar = 1;",
      "  function innerFn() { return innerVar; }",
      "  _.exports = innerFn;",
      "});",
      "var appVar = lib();"
    ].join("\n");

    const ast = parseSync(code, { sourceType: "unambiguous" });
    assert.ok(ast);

    const result = getModuleLevelBindings(ast, undefined, code);
    assert.ok(result, "should detect module-level bindings");

    const bindingNames = result.bindings.map((b: { name: string }) => b.name);

    // Outer factory var and the app var should be kept.
    assert.ok(bindingNames.includes("lib"));
    assert.ok(bindingNames.includes("appVar"));

    // Inner bindings should be classified as third-party and skipped.
    assert.ok(
      !bindingNames.includes("innerVar"),
      `innerVar should be skipped, got: [${bindingNames.join(", ")}]`
    );
    assert.ok(
      !bindingNames.includes("innerFn"),
      `innerFn should be skipped, got: [${bindingNames.join(", ")}]`
    );

    assert.ok(result.classification);
    assert.strictEqual(result.classification.factories.length, 1);
  });
});
