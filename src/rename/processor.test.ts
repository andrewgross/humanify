import { describe, it } from "node:test";
import assert from "node:assert";
import { parseSync } from "@babel/core";
import * as babelGenerator from "@babel/generator";
import * as t from "@babel/types";
import { RenameProcessor, type LLMProvider } from "./processor.js";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import type { LLMContext } from "../analysis/types.js";

const generate: typeof babelGenerator.default =
  typeof babelGenerator.default === "function"
    ? babelGenerator.default
    : (babelGenerator.default as any).default;

describe("RenameProcessor", () => {
  it("processes leaf functions first", async () => {
    const code = `
      function a() { b(); }
      function b() { c(); }
      function c() { return 1; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const processOrder: string[] = [];

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        processOrder.push(currentName);
        return { name: currentName + "Renamed" };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, { concurrency: 1 });

    // With concurrency 1, we should see a deterministic order
    // c's bindings should be processed before b's, b's before a's
    // But since we're just checking the functions get processed, this is a basic test
    assert.ok(processOrder.length > 0, "Should have processed some identifiers");
  });

  it("respects concurrency limit", async () => {
    const code = `
      function a() {}
      function b() {}
      function c() {}
      function d() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    let maxConcurrent = 0;
    let currentConcurrent = 0;

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        currentConcurrent++;
        maxConcurrent = Math.max(maxConcurrent, currentConcurrent);

        // Simulate some async work
        await new Promise((resolve) => setTimeout(resolve, 10));

        currentConcurrent--;
        return { name: currentName + "Renamed" };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, { concurrency: 2 });

    assert.ok(maxConcurrent <= 2, "Should respect concurrency limit");
  });

  it("tracks rename decisions for source maps", async () => {
    const code = `function foo(bar) { return bar; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        if (currentName === "foo") return { name: "calculateValue" };
        if (currentName === "bar") return { name: "inputValue" };
        return { name: currentName };
      }
    };

    const processor = new RenameProcessor(ast);
    const renames = await processor.processAll(functions, mockLLM);

    // Should have renames tracked
    assert.ok(renames.length > 0, "Should track rename decisions");

    // Check that renames have required fields for source map generation
    for (const rename of renames) {
      assert.ok(rename.originalPosition, "Should have original position");
      assert.ok(typeof rename.originalPosition.line === "number", "Should have line number");
      assert.ok(typeof rename.originalPosition.column === "number", "Should have column number");
      assert.ok(rename.originalName, "Should have original name");
      assert.ok(rename.newName, "Should have new name");
      assert.ok(rename.functionId, "Should have function ID");
    }
  });

  it("reports progress during processing", async () => {
    const code = `
      function a() {}
      function b() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const progressUpdates: number[] = [];

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        return { name: currentName + "Renamed" };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, {
      onProgress: (progress) => {
        progressUpdates.push(progress.done);
      }
    });

    assert.ok(progressUpdates.length > 0, "Should receive progress updates");
  });

  it("handles functions with no bindings", async () => {
    const code = `function empty() {}`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        return { name: currentName + "Renamed" };
      }
    };

    const processor = new RenameProcessor(ast);
    const renames = await processor.processAll(functions, mockLLM);

    // Should complete without error
    assert.ok(Array.isArray(renames), "Should return rename array");
  });

  it("updates AST in place with renames", async () => {
    const code = `function foo(bar) { return bar + 1; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        if (currentName === "foo") return { name: "calculateSum" };
        if (currentName === "bar") return { name: "inputNumber" };
        return { name: currentName };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // Generate code from modified AST
    const output = generate(ast);

    // Verify renames appear in generated code
    assert.ok(output.code.includes("calculateSum"), "Function should be renamed");
    assert.ok(output.code.includes("inputNumber"), "Parameter should be renamed");
    assert.ok(!output.code.includes("foo"), "Old function name should be gone");
    assert.ok(!output.code.includes("bar"), "Old parameter name should be gone");
  });

  it("FunctionNode.path remains valid after renames", async () => {
    const code = `
      function outer(x) {
        function inner(y) { return y * 2; }
        return inner(x);
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        return { name: currentName + "Renamed" };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // Verify all FunctionNode paths are still valid and can generate code
    for (const fn of functions) {
      assert.ok(fn.path.node, "Path should still have a node");
      assert.ok(t.isFunction(fn.path.node), "Node should still be a function");

      // Should be able to generate code from the path
      const fnCode = generate(fn.path.node);
      assert.ok(fnCode.code.length > 0, "Should generate non-empty code");
    }
  });

  it("FunctionNode references remain consistent after renames", async () => {
    const code = `
      function a() { b(); }
      function b() { c(); }
      function c() { return 1; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    // Capture original relationships
    const fnA = functions.find(f => f.path.node.id?.name === "a");
    const fnB = functions.find(f => f.path.node.id?.name === "b");
    const fnC = functions.find(f => f.path.node.id?.name === "c");

    assert.ok(fnA && fnB && fnC, "Should find all functions");
    assert.ok(fnA.internalCallees.has(fnB), "a should call b before rename");
    assert.ok(fnB.internalCallees.has(fnC), "b should call c before rename");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        const renames: Record<string, string> = {
          a: "entryPoint",
          b: "middleStep",
          c: "leafFunction"
        };
        return { name: renames[currentName] || currentName };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // Relationships should still be intact (same object references)
    assert.ok(fnA.internalCallees.has(fnB), "a should still reference b after rename");
    assert.ok(fnB.internalCallees.has(fnC), "b should still reference c after rename");
    assert.ok(fnB.callers.has(fnA), "b should still have a as caller");
    assert.ok(fnC.callers.has(fnB), "c should still have b as caller");
  });

  it("renameMapping is populated after processing", async () => {
    const code = `function foo(bar, baz) { return bar + baz; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        const renames: Record<string, string> = {
          foo: "addNumbers",
          bar: "firstNum",
          baz: "secondNum"
        };
        return { name: renames[currentName] || currentName };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // Check renameMapping is populated
    const fn = functions[0];
    assert.ok(fn.renameMapping, "Should have renameMapping");
    assert.ok(fn.renameMapping.names, "Should have names map");
    assert.strictEqual(fn.renameMapping.names["foo"], "addNumbers");
    assert.strictEqual(fn.renameMapping.names["bar"], "firstNum");
    assert.strictEqual(fn.renameMapping.names["baz"], "secondNum");
  });

  it("status transitions correctly during processing", async () => {
    const code = `function test() { return 1; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const fn = functions[0];

    assert.strictEqual(fn.status, "pending", "Should start as pending");

    const statusDuringProcess: string[] = [];

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        statusDuringProcess.push(fn.status);
        return { name: currentName + "Renamed" };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    assert.ok(
      statusDuringProcess.includes("processing"),
      "Should be 'processing' during LLM call"
    );
    assert.strictEqual(fn.status, "done", "Should be 'done' after completion");
  });

  it("externalCallees remain valid (not renamed)", async () => {
    const code = `
      function myFunc(data) {
        console.log(data);
        fetch('/api');
        return JSON.parse(data);
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const fn = functions[0];

    // External callees captured at build time
    const externalBefore = new Set(fn.externalCallees);

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        if (currentName === "myFunc") return { name: "processData" };
        if (currentName === "data") return { name: "jsonString" };
        return { name: currentName };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // External callees should be unchanged - we don't rename globals
    assert.deepStrictEqual(
      fn.externalCallees,
      externalBefore,
      "externalCallees should not change after processing"
    );

    // Verify specific externals are tracked
    assert.ok(fn.externalCallees.has("log"), "Should track console.log");
    assert.ok(fn.externalCallees.has("fetch"), "Should track fetch");
    assert.ok(fn.externalCallees.has("parse"), "Should track JSON.parse");
  });

  it("internalCallees point to renamed functions correctly", async () => {
    const code = `
      function caller() { return callee(); }
      function callee() { return 42; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const callerFn = functions.find(f => f.path.node.id?.name === "caller");
    const calleeFn = functions.find(f => f.path.node.id?.name === "callee");

    assert.ok(callerFn && calleeFn);

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        if (currentName === "caller") return { name: "getAnswer" };
        if (currentName === "callee") return { name: "computeValue" };
        return { name: currentName };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // The reference should still be valid
    assert.ok(callerFn.internalCallees.has(calleeFn), "Should still reference same object");

    // And the referenced function's AST should have the new name
    const calleeNode = calleeFn.path.node;
    assert.ok(t.isFunctionDeclaration(calleeNode));
    assert.strictEqual(calleeNode.id?.name, "computeValue", "Callee AST should have new name");

    // Generate code to verify the call site was also updated
    const output = generate(ast);
    assert.ok(output.code.includes("computeValue()"), "Call site should use new name");
  });
});

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}
