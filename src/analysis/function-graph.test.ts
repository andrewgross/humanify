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

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}
