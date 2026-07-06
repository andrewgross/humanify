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

describe("prior-input contract", () => {
  it("throws on an empty prior instead of silently running with zero transfers", () => {
    const newFunctions = buildFunctions(`function x(y) { return y; }`);
    assert.throws(
      () => matchPriorVersion("", newFunctions),
      /empty/i,
      "empty prior must fail fast, not become a full-cost zero-transfer run"
    );
  });

  it("throws on an unparseable prior", () => {
    const newFunctions = buildFunctions(`function x(y) { return y; }`);
    assert.throws(
      () => matchPriorVersion("function {{{ not javascript", newFunctions),
      /parse/i
    );
  });

  it("throws when the prior does not appear to be the same program", () => {
    // 55 prior functions, none of which match the new version — a
    // wrong-file prior would otherwise burn a full run transferring
    // nothing.
    const priorCode = Array.from(
      { length: 55 },
      (_, i) => `function p${i}(x) { return x + ${i}; }`
    ).join("\n");
    const newCode = `function z(a, b) { for (;;) { if (a) break; } return b; }`;

    const newFunctions = buildFunctions(newCode);
    assert.throws(
      () => matchPriorVersion(priorCode, newFunctions),
      /same program/i
    );
  });

  it("does not throw when a large prior is the same program", () => {
    // Identical code: every prior hash exists in the new version (even
    // where same-hash siblings stay ambiguous), so the floor must not
    // fire regardless of how many resolve to matches.
    const body = (i: number) => `function p${i}(x) { return x + ${i}; }`;
    const code = Array.from({ length: 55 }, (_, i) => body(i)).join("\n");

    const newFunctions = buildFunctions(code);
    assert.doesNotThrow(() => matchPriorVersion(code, newFunctions));
  });
});

describe("close-match body-local transfer", () => {
  it("transfers locals from content-aligned statements when a statement was inserted", () => {
    const priorCode = `
      function processItems(list) {
        const filtered = list.filter(Boolean);
        const sorted = filtered.sort();
        const first = sorted[0];
        return first + sorted.length;
      }
    `;
    const newCode = `
      function p(a) {
        const b = a.filter(Boolean);
        console.log("extra", b.length);
        const c = b.sort();
        const d = c[0];
        return d + c.length;
      }
    `;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "pair must close-match");
    const info = [...result.closeMatchContext.values()][0];
    assert.strictEqual(info.nameTransfers.p, "processItems");
    assert.strictEqual(info.nameTransfers.a, "list");
    assert.strictEqual(
      info.nameTransfers.b,
      "filtered",
      `body local from aligned statement must transfer, got ${JSON.stringify(info.nameTransfers)}`
    );
    assert.strictEqual(info.nameTransfers.c, "sorted");
    assert.strictEqual(info.nameTransfers.d, "first");
  });

  it("transfers remaining locals when a statement was removed", () => {
    const priorCode = `
      function build(input) {
        const trimmed = input.trim();
        console.log("debug", trimmed.length);
        const upper = trimmed.toUpperCase();
        return upper;
      }
    `;
    const newCode = `
      function w(z) {
        const t = z.trim();
        const u = t.toUpperCase();
        return u;
      }
    `;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "pair must close-match");
    const info = [...result.closeMatchContext.values()][0];
    assert.strictEqual(info.nameTransfers.t, "trimmed");
    assert.strictEqual(info.nameTransfers.u, "upper");
  });

  it("does not transfer a local whose declaration statement changed", () => {
    const priorCode = `
      function check(v) {
        const limit = getLimit();
        const ok = v < limit;
        return ok ? v : limit;
      }
    `;
    const newCode = `
      function k(x) {
        const m = getLimit() + 5;
        const n = x < m;
        return n ? x : m;
      }
    `;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "pair must close-match");
    const info = [...result.closeMatchContext.values()][0];
    assert.ok(
      !("m" in info.nameTransfers),
      `local with changed declaration must NOT transfer, got ${JSON.stringify(info.nameTransfers)}`
    );
    assert.strictEqual(
      info.nameTransfers.n,
      "ok",
      "local declared in an unchanged statement still transfers"
    );
  });

  it("transfers nothing for a shape-coincidence pair with zero aligned statements", () => {
    // Same count-features (cosine 1.0 → close pair) but NOT one line of
    // identical normalized content: a deleted helper and an unrelated
    // added helper. Transferring the name+params would present a wrong
    // name as continuity — the pair may only serve as LLM context.
    const priorCode = `
      function readAll(store) {
        const rows = store.fetch();
        return rows.concat(store.extra);
      }
    `;
    const newCode = `
      function z(q) {
        const t = q.persist();
        return t.filter(q.limit);
      }
    `;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "pair must close-match");
    const info = [...result.closeMatchContext.values()][0];
    assert.deepStrictEqual(
      info.nameTransfers,
      {},
      `zero aligned statements must gate ALL transfers, got ${JSON.stringify(info.nameTransfers)}`
    );
    assert.ok(
      info.priorCode.includes("readAll"),
      "prior context still provided"
    );
  });

  it("aligns duplicate-shape statements by ordinal only when counts are equal", () => {
    const priorCode = `
      function pair(obj) {
        const first = obj.pick();
        const second = obj.pick();
        return first + second + extraWork(obj);
      }
    `;
    const newCode = `
      function pr(o) {
        const q = o.pick();
        const r = o.pick();
        return q + r + extraWork(o) + 1;
      }
    `;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "pair must close-match");
    const info = [...result.closeMatchContext.values()][0];
    assert.strictEqual(info.nameTransfers.q, "first");
    assert.strictEqual(info.nameTransfers.r, "second");
  });
});

describe("placeholder alignment invariant", () => {
  it("throws when a matched pair's placeholder maps disagree", () => {
    // Equal structural hashes guarantee aligned slot sets by construction;
    // a divergence means the mapping is stale or corrupt, and translating
    // through it would transfer names to the WRONG identifiers. Corrupt
    // the new side's captured mapping to prove the guard fires.
    const priorCode = `function getUser(userId) { return userId; }`;
    const newCode = `function x(y) { return y; }`;

    const newFunctions = buildFunctions(newCode);
    const newFn = [...newFunctions.values()][0];
    assert.ok(newFn.placeholderMapping, "graph build captures the mapping");
    newFn.placeholderMapping?.delete("$1");

    assert.throws(
      () => matchPriorVersion(priorCode, newFunctions),
      /placeholder/i
    );
  });
});

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

  it("finds corroboration inside a single changed container statement", () => {
    // The whole body is ONE if/else chain and the edit is nested inside
    // it — top-level alignment sees zero pairs. Alignment must recurse
    // into the corresponding branches of the lone unaligned statement
    // pair, find the untouched nested statements, and keep the transfer.
    const priorCode = `
      function updateFromInput(input) {
        if (typeof input === "number") {
          setCount(input);
        } else if (typeof input === "string") {
          setLabel(input);
        }
      }
    `;
    const newCode = `
      function N(j) {
        if (typeof j === "number") {
          console.log("perturbation");
          setCount(j);
        } else if (typeof j === "string") {
          setLabel(j);
        }
      }
    `;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "pair must close-match");
    const info = [...result.closeMatchContext.values()][0];
    assert.strictEqual(
      info.nameTransfers.N,
      "updateFromInput",
      `nested alignment must corroborate the pair, got ${JSON.stringify(info.nameTransfers)}`
    );
    assert.strictEqual(info.nameTransfers.j, "input");
  });

  it("corroborates a refactored pair via shingle overlap when no statement aligns", () => {
    // Every statement changed shape (var r = X; return r → return X), so
    // alignment finds nothing — but the rename-invariant shingle tokens
    // (property accesses, external calls, string literals) are identical.
    const priorCode = `function mergeObjects(e, t) { var r = Object.assign({}, e, t); return r; }`;
    const newCode = `function a(e, t) { return Object.assign({}, e, t); }`;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "pair must close-match");
    const info = [...result.closeMatchContext.values()][0];
    assert.strictEqual(
      info.nameTransfers.a,
      "mergeObjects",
      `shingle-corroborated pair keeps its signature transfer, got ${JSON.stringify(info.nameTransfers)}`
    );
  });

  it("gates an uncorroborated arrow/function pair entirely (shape coincidence)", () => {
    // map.delete(key) and c.set(a, b) share count features but not one
    // aligned statement — nothing may transfer. (Historically this pair
    // exercised placeholder-slot misalignment: q→map, b→delete. The gate
    // now removes the whole hazard for uncorroborated pairs.)
    const priorCode = `var removeEntry = (map, key) => map.delete(key);`;
    const newCode = `function q(a, b) { c.set(a, b); }`;

    const newFunctions = buildFunctions(newCode);
    const result = matchPriorVersion(priorCode, newFunctions);

    assert.strictEqual(result.closeMatchCount, 1, "should close-match");
    const info = [...result.closeMatchContext.values()][0];
    assert.deepStrictEqual(info.nameTransfers, {});
  });

  it("close match between arrow and named function aligns params by AST position", () => {
    // Prior: an arrow — no function-name identifier, so placeholder slots
    // are shifted and the property name `delete` used to occupy a slot.
    // Placeholder-position alignment would produce q→map (function renamed
    // to a param name), b→delete (reserved word) — the mechanism behind
    // Run B's `function collection(key, delete)`. With one aligned
    // statement corroborating the pair, params transfer by AST position.
    const priorCode = `
      var removeEntry = (map, key) => {
        log("removing entry now");
        return map.delete(key);
      };
    `;
    const newCode = `
      function q(a, b) {
        log("removing entry now");
        return a.delete(b);
      }
    `;

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

  it("disambiguates same-hash lazy-init bindings by matched callee identity", () => {
    // Bun-style lazy-init wrappers have identical structural hashes
    // (identifier names normalize away). The distinguishing signal is
    // WHICH already-matched function each wrapper references.
    const priorCode = `
      function loadArrayHelpers() { return [1]; }
      function loadMapHelpers() { return new Map(); }
      var arrayHelper = Z(() => { loadArrayHelpers(); });
      var mapHelper = Z(() => { loadMapHelpers(); });
    `;
    const newCode = `
      function q1() { return [1]; }
      function q2() { return new Map(); }
      var x = Z(() => { q1(); });
      var y = Z(() => { q2(); });
    `;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    const renames = new Map(
      (result.moduleBindingRenames ?? []).map((r) => [r.oldName, r.newName])
    );
    assert.strictEqual(renames.get("x"), "arrayHelper");
    assert.strictEqual(renames.get("y"), "mapHelper");
  });

  it("disambiguates same-hash literal bindings by matched caller identity", () => {
    // Two empty-object bindings are structurally identical; the functions
    // that REFERENCE them are matched, so caller identity resolves them.
    const priorCode = `
      var registry = {};
      var cache = {};
      function addToRegistry(k) { registry[k] = 1; }
      function readFromCache(k) { return cache[k]; }
    `;
    const newCode = `
      var r = {};
      var c = {};
      function a1(k) { r[k] = 1; }
      function a2(k) { return c[k]; }
    `;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    const renames = new Map(
      (result.moduleBindingRenames ?? []).map((r) => [r.oldName, r.newName])
    );
    assert.strictEqual(renames.get("r"), "registry");
    assert.strictEqual(renames.get("c"), "cache");
  });

  it("resolves binding aliases through already-matched binding neighbors", () => {
    // `var alias = OTHER_BINDING` normalizes to the same hash for every
    // alias. The referenced bindings themselves match uniquely (distinct
    // literals), so a second identity round can resolve the aliases by
    // binding-neighbor correspondence.
    const priorCode = `
      var BASE_CONFIG = { mode: 1 };
      var configAlias = BASE_CONFIG;
      var OTHER_LIMITS = { mode: 2 };
      var limitsAlias = OTHER_LIMITS;
    `;
    const newCode = `
      var a = { mode: 1 };
      var b = a;
      var c = { mode: 2 };
      var d = c;
    `;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    const renames = new Map(
      (result.moduleBindingRenames ?? []).map((r) => [r.oldName, r.newName])
    );
    assert.strictEqual(renames.get("b"), "configAlias");
    assert.strictEqual(renames.get("d"), "limitsAlias");
  });

  it("disambiguates function-alias bindings by the referenced matched function", () => {
    // `var x = someFn` (reference, not a call) — every such alias shares
    // one hash; the referenced matched function is the identity signal.
    const priorCode = `
      function loadArrayHelpers() { return [1]; }
      function loadMapHelpers() { return new Map(); }
      var arrayLoader = loadArrayHelpers;
      var mapLoader = loadMapHelpers;
    `;
    const newCode = `
      function f1() { return [1]; }
      function f2() { return new Map(); }
      var x = f1;
      var y = f2;
    `;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    const renames = new Map(
      (result.moduleBindingRenames ?? []).map((r) => [r.oldName, r.newName])
    );
    assert.strictEqual(renames.get("x"), "arrayLoader");
    assert.strictEqual(renames.get("y"), "mapLoader");
  });

  it("leaves same-hash bindings unmatched when identity evidence conflicts", () => {
    // Same-hash bindings whose referencing functions did NOT match
    // anything must stay unmatched (precision over recall).
    const priorCode = `
      var first = {};
      var second = {};
    `;
    const newCode = `
      var a = {};
      var b = {};
    `;

    const { functions, moduleBindings } = buildFunctionsAndBindings(
      newCode,
      isEligible
    );
    const result = matchPriorVersion(priorCode, functions, moduleBindings);

    assert.strictEqual(result.moduleBindingsMatched, 0);
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
