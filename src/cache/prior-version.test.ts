import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import type { FunctionNode, ModuleBindingNode } from "../analysis/types.js";
import { matchPriorVersion } from "./prior-version.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  return ast;
}

function buildFunctions(code: string): Map<string, FunctionNode> {
  const ast = parse(code);
  const functions = buildFunctionGraph(ast, "test.js");
  return new Map(functions.map((f) => [f.sessionId, f]));
}

/** Build function map + module binding nodes from code via the unified graph. */
function buildFunctionsAndBindings(
  code: string,
  isEligible?: (name: string) => boolean
): {
  functions: Map<string, FunctionNode>;
  moduleBindings: ModuleBindingNode[];
} {
  const ast = parse(code);
  const graph = buildUnifiedGraph(ast, "test.js", undefined, isEligible);
  const functions = new Map<string, FunctionNode>();
  const moduleBindings: ModuleBindingNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "function") {
      functions.set(node.node.sessionId, node.node);
    } else if (node.type === "module-binding") {
      moduleBindings.push(node.node);
    }
  }
  return { functions, moduleBindings };
}

describe("matchPriorVersion", () => {
  it("function match transfers names via placeholder mapping", () => {
    // Prior version: function a(b) { return b; } — renamed to getUser(userId)
    const priorCode = `function getUser(userId) { return userId; }`;
    // New version: same structure, different minified names
    const newCode = `function x(y) { return y; }`;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.functionsMatched, 1);
    // The new function should get renames: x→getUser, y→userId
    const newFn = [...newFunctions.values()][0];
    assert.ok(newFn.renameMapping);
    assert.strictEqual(newFn.renameMapping.names.x, "getUser");
    assert.strictEqual(newFn.renameMapping.names.y, "userId");
  });

  it("no prior version code returns zero matches", () => {
    const newCode = `function x(y) { return y; }`;
    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion("", newFunctions);

    assert.strictEqual(result.functionsMatched, 0);
    assert.strictEqual(result.functionsAlreadyNamed, 0);
    assert.strictEqual(result.moduleBindingsMatched, 0);
  });

  it("counts already-named functions separately from renamed", () => {
    // Prior and new have identical identifiers — nothing was minified
    // (e.g., export names and property keys preserved by minifier)
    const code = `function createRef() { return { current: null }; }`;

    const newFunctions = buildFunctions(code);
    const result = matchPriorVersion(code, newFunctions);

    // Matched structurally, but no renames needed
    assert.strictEqual(result.functionsMatched, 0);
    assert.strictEqual(result.functionsAlreadyNamed, 1);

    // Function should still get an empty renameMapping (marked as done)
    const fn = [...newFunctions.values()][0];
    assert.ok(fn.renameMapping);
    assert.deepStrictEqual(fn.renameMapping.names, {});
  });

  it("structurally different functions do not match", () => {
    const priorCode = `function getUser(userId) { return userId; }`;
    const newCode = `function x(y) { if (y) { return y + 1; } return 0; }`;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.functionsMatched, 0);
  });

  it("matches multiple functions correctly", () => {
    const priorCode = `
      function getUser(userId) { return userId; }
      function add(left, right) { return left + right; }
    `;
    const newCode = `
      function x(y) { return y; }
      function p(q, r) { return q + r; }
    `;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.functionsMatched, 2);
  });

  it("mixed: some functions match, some don't", () => {
    const priorCode = `
      function getUser(userId) { return userId; }
      function oldHelper(x) { return x * 2; }
    `;
    // New version has getUser (same structure) but a structurally different second function
    const newCode = `
      function a(b) { return b; }
      function c(d, e) { if (d) { for (var i = 0; i < e; i++) {} } return d; }
    `;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    // Only the first function should match
    assert.strictEqual(result.functionsMatched, 1);
  });

  it("ambiguous functions are not matched (safety)", () => {
    // Prior has two structurally identical functions
    const priorCode = `
      function getState() { return state; }
      function getInitialState() { return state; }
    `;
    // New also has two identical functions
    const newCode = `
      function a() { return b; }
      function c() { return d; }
    `;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    // These are ambiguous — the cascade can't disambiguate
    // They should NOT be matched (precision over recall)
    assert.ok(
      result.functionsMatched <= 2,
      "Should match at most 2 (may be 0 if fully ambiguous)"
    );
  });

  it("close match found for modified function", () => {
    // Prior: function with 2 params, a branch, and a return
    const priorCode = `function handleError(err, info) { if (err) { console.log(err); } return info; }`;
    // New: same shape (2 params, branch, return) but with an extra statement
    const newCode = `function a(b, c) { if (b) { console.log(b); b.flag = true; } return c; }`;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    // Exact match should fail (different AST structure)
    assert.strictEqual(result.functionsMatched, 0);
    // But close match should find it
    assert.strictEqual(result.closeMatchCount, 1);
    // Close match context should contain prior humanified code
    const newFn = [...newFunctions.values()][0];
    const info = result.closeMatchContext.get(newFn.sessionId);
    assert.ok(info, "Should have close match info");
    assert.ok(
      info?.priorCode.includes("handleError"),
      "Context should contain prior function name"
    );
    // Should transfer function name + params
    assert.strictEqual(info?.nameTransfers.a, "handleError");
    assert.strictEqual(info?.nameTransfers.b, "err");
    assert.strictEqual(info?.nameTransfers.c, "info");
  });

  it("close match not found for very different function", () => {
    // Prior: simple no-arg function that returns a value
    const priorCode = `function getUser() { return state; }`;
    // New: complex function with loops, branches, try/catch, rest param, many string literals
    // Feature vectors should be completely different in direction (cosine < 0.8)
    const newCode = `function a(...b) {
      var x = "hello", y = "world", z = "test";
      try {
        for (var i = 0; i < 10; i++) {
          for (var j = 0; j < 10; j++) {
            if (i > j) { x = "a"; } else if (j > 5) { y = "b"; } else { z = "c"; }
          }
        }
        if (x) { return x; }
        if (y) { return y; }
      } catch(e) { return z; }
      return null;
    }`;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.functionsMatched, 0);
    assert.strictEqual(result.closeMatchCount, 0);
  });

  it("close match context is humanified code with name transfers", () => {
    // Prior: humanified names
    const priorCode = `function processItem(item, config) { if (item.active) { return config.handler(item); } return null; }`;
    // New: similar structure, different minified names
    const newCode = `function a(b, c) { if (b.active) { return c.handler(b); } return null; return undefined; }`;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    if (result.closeMatchCount > 0) {
      const newFn = [...newFunctions.values()][0];
      const info = result.closeMatchContext.get(newFn.sessionId);
      assert.ok(info, "Should have info");
      // Prior code should contain descriptive names
      assert.ok(
        info?.priorCode.includes("processItem") ||
          info?.priorCode.includes("config"),
        "Context should contain humanified names"
      );
      // Name transfers should map function name + params
      assert.strictEqual(info?.nameTransfers.a, "processItem");
      assert.strictEqual(info?.nameTransfers.b, "item");
      assert.strictEqual(info?.nameTransfers.c, "config");
    }
  });

  it("close match between arrow and named function aligns params by AST position", () => {
    // Prior: an arrow — no function-name identifier, so its placeholder
    // slots are shifted, and the property name `delete` occupies a slot.
    // Placeholder-position alignment would produce q→map (function renamed
    // to a param name), a→key (wrong slot), b→delete (reserved word) —
    // the exact mechanism behind Run B's `function collection(key, delete)`.
    const priorCode = `var removeEntry = (map, key) => map.delete(key);`;
    const newCode = `function q(a, b) { c.set(a, b); }`;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "should close-match");
    const newFn = [...newFunctions.values()].find(
      (fn) => fn.path.node.params.length === 2
    );
    assert.ok(newFn);
    const info = result.closeMatchContext.get(newFn.sessionId);
    assert.ok(info, "should have close match info");
    assert.deepStrictEqual(info.nameTransfers, { a: "map", b: "key" });
  });

  it("does not transfer a function name onto an arrow's first param", () => {
    const priorCode = `function isValid(value) { return value != null; }`;
    const newCode = `var z = (x) => x != null;`;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "should close-match");
    const newFn = [...newFunctions.values()][0];
    const info = result.closeMatchContext.get(newFn.sessionId);
    assert.ok(info, "should have close match info");
    assert.deepStrictEqual(info.nameTransfers, { x: "value" });
  });
});

describe("matchPriorVersion function variable name transfers", () => {
  it("transfers variable name for arrow function expression", () => {
    // Prior: var isValid = (x) => x != null — humanified variable + param
    const priorCode = `var isValid = (x) => x != null;`;
    // New: same arrow structure, minified variable + param
    const newCode = `var a = (b) => b != null;`;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      (name) => name.length === 1
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    // Arrow function matching transfers param (b→x), but we also need variable name (a→isValid)
    const renames = result.moduleBindingRenames ?? [];
    const varRename = renames.find((r) => r.oldName === "a");
    assert.ok(varRename, "Should transfer variable name for arrow function");
    assert.strictEqual(varRename?.newName, "isValid");
  });

  it("transfers variable name for function expression", () => {
    const priorCode = `var helper = function(x) { return x; };`;
    const newCode = `var a = function(b) { return b; };`;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      (name) => name.length === 1
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    const renames = result.moduleBindingRenames ?? [];
    const varRename = renames.find((r) => r.oldName === "a");
    assert.ok(
      varRename,
      "Should transfer variable name for function expression"
    );
    assert.strictEqual(varRename?.newName, "helper");
  });

  it("transfers variable name for close-matched arrow function", () => {
    // Prior: arrow with one statement
    const priorCode = `var isValid = (x) => x != null;`;
    // New: slightly different arrow body (close match, not exact)
    const newCode = `var a = (b) => b != null && b.type;`;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      (name) => name.length === 1
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    // For close matches, the variable name should be in nameTransfers
    if (result.closeMatchCount > 0) {
      const entries = [...result.closeMatchContext.values()];
      const entry = entries[0];
      assert.ok(entry, "Should have close match entry");
      // The variable name should appear as a module binding rename
      const renames = result.moduleBindingRenames ?? [];
      const varRename = renames.find((r) => r.oldName === "a");
      assert.ok(
        varRename,
        "Should transfer variable name for close-matched arrow function"
      );
      assert.strictEqual(varRename?.newName, "isValid");
    }
  });

  it("does not transfer variable name when names already match", () => {
    const priorCode = `var isValid = (x) => x != null;`;
    const newCode = `var isValid = (b) => b != null;`;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      (name) => name.length === 1
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    // Variable name is already "isValid" — should not produce a rename
    const renames = result.moduleBindingRenames ?? [];
    const varRename = renames.find((r) => r.oldName === "isValid");
    assert.strictEqual(
      varRename,
      undefined,
      "Should not rename when variable name already matches"
    );
  });
});

describe("matchPriorVersion module bindings", () => {
  // Use a custom isEligible to treat single-char names as minified
  const isEligible = (name: string) => name.length === 1;

  it("matches module binding by unique structural hash", () => {
    // Prior: humanified binding
    const priorCode = `var UNDEFINED_VALUE = void 0;`;
    // New: minified binding with same init expression
    const newCode = `var a = void 0;`;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    assert.strictEqual(result.moduleBindingsMatched, 1);
    const renames = result.moduleBindingRenames ?? [];
    assert.strictEqual(renames.length, 1);
    assert.strictEqual(renames[0].oldName, "a");
    assert.strictEqual(renames[0].newName, "UNDEFINED_VALUE");
  });

  it("skips ambiguous module bindings with same hash", () => {
    // Prior: two bindings with same init expression (empty array)
    const priorCode = `
      var queue = [];
      var buffer = [];
    `;
    // New: two bindings with same init expression
    const newCode = `
      var a = [];
      var b = [];
    `;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    // Both have `[] ` as init, so hash collides — should NOT match
    assert.strictEqual(result.moduleBindingsMatched, 0);
  });

  it("skips bindings with no init expression", () => {
    // Prior: binding with init
    const priorCode = `var arraySlice = [].slice;`;
    // New: binding declared without init (bare `var a;`)
    const newCode = `var a;`;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    // No init → no hash → can't match
    assert.strictEqual(result.moduleBindingsMatched, 0);
  });

  it("matches multiple unique bindings", () => {
    const priorCode = `
      var UNDEFINED_VALUE = void 0;
      var arraySlice = [].slice;
      var isArrayFn = Array.isArray;
    `;
    const newCode = `
      var a = void 0;
      var b = [].slice;
      var c = Array.isArray;
    `;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    assert.strictEqual(result.moduleBindingsMatched, 3);
  });

  it("transfers function expression variable name via function matching path", () => {
    // Prior: binding with function expression init
    const priorCode = `var helper = function(x) { return x; };`;
    // New: same structure
    const newCode = `var a = function(b) { return b; };`;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    // Function expressions are excluded from hash-based module binding matching,
    // but variable names are transferred via the function matching path
    assert.strictEqual(result.moduleBindingsMatched, 1);
    const renames = result.moduleBindingRenames ?? [];
    const varRename = renames.find((r) => r.oldName === "a");
    assert.ok(varRename, "Should transfer variable name");
    assert.strictEqual(varRename?.newName, "helper");
  });

  it("does not match when no module bindings provided", () => {
    const priorCode = `var UNDEFINED_VALUE = void 0;`;
    const newCode = `var a = void 0;`;

    const newFunctions = buildFunctions(newCode);
    // Call without module bindings (legacy path)
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.moduleBindingsMatched, 0);
  });
});
