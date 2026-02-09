import { describe, it } from "node:test";
import assert from "node:assert";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  buildFunctionGraph,
  findLeafFunctions,
  detectCycles,
  getProcessingOrder
} from "./function-graph.js";

describe("buildFunctionGraph", () => {
  it("finds all functions in a file", () => {
    const code = `
      function a() {}
      function b() {}
      const c = () => {};
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    assert.strictEqual(functions.length, 3, "Should find 3 functions");
  });

  it("identifies internal callees", () => {
    const code = `
      function a() {
        b();
      }
      function b() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const fnA = functions.find((f) => f.sessionId.includes(":2:"));
    const fnB = functions.find((f) => f.sessionId.includes(":5:"));

    assert.ok(fnA, "Should find function a");
    assert.ok(fnB, "Should find function b");
    assert.strictEqual(fnA.internalCallees.size, 1, "a should have 1 internal callee");
    assert.ok(fnA.internalCallees.has(fnB!), "a should call b");
  });

  it("identifies external callees", () => {
    const code = `
      function a() {
        console.log("test");
        fetch("/api");
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    assert.strictEqual(functions.length, 1);
    const fn = functions[0];

    assert.ok(fn.externalCallees.has("log"), "Should track log as external");
    assert.ok(fn.externalCallees.has("fetch"), "Should track fetch as external");
  });

  it("tracks reverse dependencies (callers)", () => {
    const code = `
      function a() { b(); }
      function b() {}
      function c() { b(); }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const fnB = functions.find((f) => f.sessionId.includes(":3:"));
    assert.ok(fnB, "Should find function b");
    assert.strictEqual(fnB.callers.size, 2, "b should have 2 callers");
  });

  it("handles nested functions", () => {
    const code = `
      function outer() {
        function inner() {}
        inner();
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    assert.strictEqual(functions.length, 2, "Should find 2 functions");

    const outer = functions.find((f) => f.sessionId.includes(":2:"));
    const inner = functions.find((f) => f.sessionId.includes(":3:"));

    assert.ok(outer, "Should find outer function");
    assert.ok(inner, "Should find inner function");
    assert.ok(outer.internalCallees.has(inner!), "outer should call inner");
  });
});

describe("findLeafFunctions", () => {
  it("returns functions with no internal dependencies", () => {
    const code = `
      function a() { b(); }
      function b() { c(); }
      function c() { console.log("leaf"); }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const leaves = findLeafFunctions(functions);

    assert.strictEqual(leaves.length, 1, "Should have 1 leaf function");
    assert.ok(
      leaves[0].sessionId.includes(":4:"),
      "The leaf should be function c"
    );
  });

  it("returns all functions when none have internal dependencies", () => {
    const code = `
      function a() { console.log(1); }
      function b() { console.log(2); }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const leaves = findLeafFunctions(functions);

    assert.strictEqual(leaves.length, 2, "Both functions should be leaves");
  });
});

describe("detectCycles", () => {
  it("returns empty array when no cycles exist", () => {
    const code = `
      function a() { b(); }
      function b() { c(); }
      function c() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const cycles = detectCycles(functions);

    assert.strictEqual(cycles.length, 0, "Should have no cycles");
  });

  it("detects simple cycles", () => {
    const code = `
      function a() { b(); }
      function b() { a(); }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const cycles = detectCycles(functions);

    assert.strictEqual(cycles.length, 1, "Should detect 1 cycle");
    assert.strictEqual(cycles[0].length, 2, "Cycle should have 2 functions");
  });

  it("detects self-referential functions", () => {
    const code = `
      function recursive() { recursive(); }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const cycles = detectCycles(functions);

    assert.strictEqual(cycles.length, 1, "Should detect self-loop as cycle");
  });
});

describe("nested function scope dependencies", () => {
  it("child functions should depend on parent scope even without calls", () => {
    // This test verifies that nested functions are processed AFTER their parent
    // even when there's no call relationship, because children may reference
    // variables from the parent scope that need to be renamed first.
    const code = `
      function parent() {
        var sharedVar = 1;
        function child() {
          return sharedVar;  // references parent's var, but doesn't CALL parent
        }
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const parent = functions.find((f) => f.sessionId.includes(":2:"));
    const child = functions.find((f) => f.sessionId.includes(":4:"));

    assert.ok(parent, "Should find parent function");
    assert.ok(child, "Should find child function");

    // Child should have parent as scopeParent (not in internalCallees)
    // This ensures parent's scope is processed first without polluting fingerprints
    assert.ok(
      child.scopeParent === parent,
      "child should have parent as scopeParent"
    );
    assert.ok(
      !child.internalCallees.has(parent!),
      "child should NOT have parent in internalCallees (scope nesting is separate)"
    );
  });

  it("sibling nested functions should both depend on parent", () => {
    const code = `
      function parent() {
        var x = 1;
        function child1() { return x + 1; }
        function child2() { return x + 2; }
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const parent = functions.find((f) => f.sessionId.includes(":2:"));
    const child1 = functions.find((f) => f.sessionId.includes(":4:"));
    const child2 = functions.find((f) => f.sessionId.includes(":5:"));

    assert.ok(parent, "Should find parent");
    assert.ok(child1, "Should find child1");
    assert.ok(child2, "Should find child2");

    // Both children should have parent as scopeParent
    assert.ok(
      child1.scopeParent === parent,
      "child1 should have parent as scopeParent"
    );
    assert.ok(
      child2.scopeParent === parent,
      "child2 should have parent as scopeParent"
    );
  });

  it("deeply nested functions should depend on all ancestors", () => {
    const code = `
      function grandparent() {
        function parent() {
          function child() {}
        }
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const grandparent = functions.find((f) => f.sessionId.includes(":2:"));
    const parent = functions.find((f) => f.sessionId.includes(":3:"));
    const child = functions.find((f) => f.sessionId.includes(":4:"));

    assert.ok(grandparent, "Should find grandparent");
    assert.ok(parent, "Should find parent");
    assert.ok(child, "Should find child");

    // parent has grandparent as scopeParent
    assert.ok(
      parent.scopeParent === grandparent,
      "parent should have grandparent as scopeParent"
    );

    // child has parent as scopeParent (and transitively grandparent through processing order)
    assert.ok(
      child.scopeParent === parent,
      "child should have parent as scopeParent"
    );
  });

  it("parent should be processed before nested children", () => {
    const code = `
      function parent() {
        function child() {}
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const order = getProcessingOrder(functions);

    const parent = functions.find((f) => f.sessionId.includes(":2:"));
    const child = functions.find((f) => f.sessionId.includes(":3:"));

    const parentIndex = order.indexOf(parent!);
    const childIndex = order.indexOf(child!);

    assert.ok(
      parentIndex < childIndex,
      `parent (index ${parentIndex}) should be processed before child (index ${childIndex})`
    );
  });
});

describe("getProcessingOrder", () => {
  it("returns leaves first in processing order", () => {
    const code = `
      function a() { b(); }
      function b() { c(); }
      function c() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const order = getProcessingOrder(functions);

    assert.strictEqual(order.length, 3);

    // c should be first (no dependencies)
    // b should be second (depends on c)
    // a should be third (depends on b)
    const fnC = functions.find((f) => f.sessionId.includes(":4:"));
    const fnB = functions.find((f) => f.sessionId.includes(":3:"));
    const fnA = functions.find((f) => f.sessionId.includes(":2:"));

    assert.strictEqual(order[0], fnC, "c should be processed first");
    assert.strictEqual(order[1], fnB, "b should be processed second");
    assert.strictEqual(order[2], fnA, "a should be processed third");
  });

  it("handles cycles by adding them at the end", () => {
    const code = `
      function a() { b(); }
      function b() { a(); }
      function c() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const order = getProcessingOrder(functions);

    assert.strictEqual(order.length, 3);

    // c should be first (no cycle, no dependencies)
    const fnC = functions.find((f) => f.sessionId.includes(":4:"));
    assert.strictEqual(order[0], fnC, "c should be processed first");

    // a and b should be at the end (in the cycle)
    const fnA = functions.find((f) => f.sessionId.includes(":2:"));
    const fnB = functions.find((f) => f.sessionId.includes(":3:"));
    assert.ok(
      (order[1] === fnA && order[2] === fnB) ||
        (order[1] === fnB && order[2] === fnA),
      "Cycle members should be at the end"
    );
  });
});

describe("call site indexing", () => {
  it("populates callSites on function nodes", () => {
    const code = `
      function target() { return 1; }
      function caller1() { return target(); }
      function caller2() { return target() + target(); }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const target = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(target, "Should find target function");
    // Called from 2 statements: once in caller1, once in caller2 (deduped within statement)
    assert.strictEqual(target.callSites.length, 2, "target should have 2 call sites");
  });

  it("limits call sites to 5", () => {
    const code = `
      function target() {}
      function caller() {
        var a = target();
        var b = target();
        var c = target();
        var d = target();
        var e = target();
        var f = target();
        var g = target();
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const target = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(target, "Should find target function");
    assert.strictEqual(target.callSites.length, 5, "call sites should be limited to 5");
  });

  it("includes call site code", () => {
    const code = `
      function add(a, b) { return a + b; }
      function test() { return add(1, 2); }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const add = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(add, "Should find add function");
    assert.strictEqual(add.callSites.length, 1, "add should have 1 call site");
    assert.ok(
      add.callSites[0].code.includes("add(1, 2)") || add.callSites[0].code.includes("add(1,2)"),
      `Call site code should include the call expression, got: ${add.callSites[0].code}`
    );
  });

  it("includes surrounding context for short call site statements", () => {
    const code = `
      function target() { return 1; }
      function caller() {
        var x = 10;
        var y = 20;
        return target();
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const target = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(target, "Should find target function");
    assert.strictEqual(target.callSites.length, 1, "target should have 1 call site");

    const callSiteCode = target.callSites[0].code;
    // The short "return target();" should be expanded with preceding siblings
    assert.ok(
      callSiteCode.includes("x") && callSiteCode.includes("y"),
      `Call site should include surrounding context, got: ${callSiteCode}`
    );
  });

  it("does not expand context for already-long statements", () => {
    const code = `
      function target() { return 1; }
      function caller() {
        var result = target() + someOtherFunction() + yetAnotherFunction() + moreStuff() + evenMore();
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const target = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(target, "Should find target function");
    assert.strictEqual(target.callSites.length, 1, "target should have 1 call site");

    const callSiteCode = target.callSites[0].code;
    // Long statement should NOT include surrounding context
    assert.ok(
      callSiteCode.length >= 80 || !callSiteCode.includes("\n"),
      `Long statement should not be expanded with siblings`
    );
  });

  it("functions with no callers have empty callSites", () => {
    const code = `
      function unused() { return 42; }
      function main() { console.log("hello"); }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const unused = functions.find((f) => f.sessionId.includes(":2:"));
    const main = functions.find((f) => f.sessionId.includes(":3:"));

    assert.ok(unused, "Should find unused function");
    assert.ok(main, "Should find main function");
    assert.strictEqual(unused.callSites.length, 0, "unused should have 0 call sites");
    assert.strictEqual(main.callSites.length, 0, "main should have 0 call sites");
  });
});

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}
