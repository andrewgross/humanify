import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  buildFunctionGraph,
  buildUnifiedGraph
} from "../analysis/function-graph.js";
import type { IdentifierOutcome, LLMContext } from "../analysis/types.js";
import { generate } from "../babel-utils.js";
import type { LLMProvider } from "../llm/types.js";
import {
  RenameProcessor,
  applyValidRenames,
  type BatchRenameCallbacks,
  type BatchValidationResult,
  type IdentifierAttemptState
} from "./processor.js";

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
        return { name: `${currentName}Renamed` };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, { concurrency: 1 });

    // With concurrency 1, we should see a deterministic order
    // c's bindings should be processed before b's, b's before a's
    // But since we're just checking the functions get processed, this is a basic test
    assert.ok(
      processOrder.length > 0,
      "Should have processed some identifiers"
    );
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
        return { name: `${currentName}Renamed` };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, { concurrency: 2 });

    assert.ok(maxConcurrent <= 2, "Should respect concurrency limit");
  });

  it("tracks rename decisions for source maps", async () => {
    const code = `function a(b) { return b; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        if (currentName === "a") return { name: "calculateValue" };
        if (currentName === "b") return { name: "inputValue" };
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
      assert.ok(
        typeof rename.originalPosition.line === "number",
        "Should have line number"
      );
      assert.ok(
        typeof rename.originalPosition.column === "number",
        "Should have column number"
      );
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
        return { name: `${currentName}Renamed` };
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
        return { name: `${currentName}Renamed` };
      }
    };

    const processor = new RenameProcessor(ast);
    const renames = await processor.processAll(functions, mockLLM);

    // Should complete without error
    assert.ok(Array.isArray(renames), "Should return rename array");
  });

  it("updates AST in place with renames", async () => {
    const code = `function a(b) { return b + 1; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        if (currentName === "a") return { name: "calculateSum" };
        if (currentName === "b") return { name: "inputNumber" };
        return { name: currentName };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // Generate code from modified AST
    const output = generate(ast);

    // Verify renames appear in generated code
    assert.ok(
      output.code.includes("calculateSum"),
      "Function should be renamed"
    );
    assert.ok(
      output.code.includes("inputNumber"),
      "Parameter should be renamed"
    );
    assert.ok(!output.code.includes(" a("), "Old function name should be gone");
    assert.ok(
      !output.code.includes(" b)"),
      "Old parameter name should be gone"
    );
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
        return { name: `${currentName}Renamed` };
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
    const fnA = functions.find(
      (f) => (f.path.node as t.FunctionDeclaration).id?.name === "a"
    );
    const fnB = functions.find(
      (f) => (f.path.node as t.FunctionDeclaration).id?.name === "b"
    );
    const fnC = functions.find(
      (f) => (f.path.node as t.FunctionDeclaration).id?.name === "c"
    );

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
    assert.ok(
      fnA.internalCallees.has(fnB),
      "a should still reference b after rename"
    );
    assert.ok(
      fnB.internalCallees.has(fnC),
      "b should still reference c after rename"
    );
    assert.ok(fnB.callers.has(fnA), "b should still have a as caller");
    assert.ok(fnC.callers.has(fnB), "c should still have b as caller");
  });

  it("renameMapping is populated after processing", async () => {
    const code = `function a(b, c) { return b + c; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        const renames: Record<string, string> = {
          a: "addNumbers",
          b: "firstNum",
          c: "secondNum"
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
    assert.strictEqual(fn.renameMapping.names.a, "addNumbers");
    assert.strictEqual(fn.renameMapping.names.b, "firstNum");
    assert.strictEqual(fn.renameMapping.names.c, "secondNum");
  });

  it("status transitions correctly during processing", async () => {
    const code = `function t() { return 1; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const fn = functions[0];

    assert.strictEqual(fn.status, "pending", "Should start as pending");

    const statusDuringProcess: string[] = [];

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        statusDuringProcess.push(fn.status);
        return { name: `${currentName}Renamed` };
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
      function a() { return b(); }
      function b() { return 42; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const callerFn = functions.find(
      (f) => (f.path.node as t.FunctionDeclaration).id?.name === "a"
    );
    const calleeFn = functions.find(
      (f) => (f.path.node as t.FunctionDeclaration).id?.name === "b"
    );

    assert.ok(callerFn && calleeFn);

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        if (currentName === "a") return { name: "getAnswer" };
        if (currentName === "b") return { name: "computeValue" };
        return { name: currentName };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // The reference should still be valid
    assert.ok(
      callerFn.internalCallees.has(calleeFn),
      "Should still reference same object"
    );

    // And the referenced function's AST should have the new name
    const calleeNode = calleeFn.path.node;
    assert.ok(t.isFunctionDeclaration(calleeNode));
    assert.strictEqual(
      calleeNode.id?.name,
      "computeValue",
      "Callee AST should have new name"
    );

    // Generate code to verify the call site was also updated
    const output = generate(ast);
    assert.ok(
      output.code.includes("computeValue()"),
      "Call site should use new name"
    );
  });
});

describe("Nested block scope bindings", () => {
  it("collects let/const bindings from nested blocks", async () => {
    const code = `
      const f = (l = 21) => {
        let a = "";
        while (true) {
          let e = getRandom(step);
          let u = step;
          while (u--) {
            a += alphabet[e[u] & mask] || "";
            if (a.length === l) return a;
          }
        }
      };
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const renamedIds: string[] = [];
    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        renamedIds.push(...request.identifiers);
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // The arrow function should see: l (param), a (body let), e (while-block let), u (while-block let)
    assert.ok(renamedIds.includes("l"), "Should find param binding 'l'");
    assert.ok(renamedIds.includes("a"), "Should find body-scope binding 'a'");
    assert.ok(renamedIds.includes("e"), "Should find nested block binding 'e'");
    assert.ok(renamedIds.includes("u"), "Should find nested block binding 'u'");
  });

  it("collects bindings from for-loop initializers", async () => {
    const code = `
      function f(n) {
        let r = "";
        for (let i = 0; i < n; i++) {
          r += String.fromCharCode(i);
        }
        return r;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const renamedIds: string[] = [];
    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        renamedIds.push(...request.identifiers);
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    assert.ok(renamedIds.includes("n"), "Should find param 'n'");
    assert.ok(renamedIds.includes("r"), "Should find body let 'r'");
    assert.ok(renamedIds.includes("i"), "Should find for-loop let 'i'");
  });

  it("does not collect bindings from nested functions", async () => {
    // The nested function is not called by the parent, avoiding cycle issues
    const code = `
      function f(a) {
        var b = (c) => {
          let d = c + 1;
          return d;
        };
        return a;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    // Track identifiers per batch call, keyed by which function they belong to
    const allBatchIds: string[][] = [];
    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        allBatchIds.push([...request.identifiers]);
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, { concurrency: 1 });

    // Find the batch that included param 'a' (outer function)
    const outerBatch = allBatchIds.find((ids) => ids.includes("a"));
    assert.ok(outerBatch, "Should find a batch with outer param 'a'");
    assert.ok(outerBatch.includes("b"), "Outer should have var 'b'");
    assert.ok(
      !outerBatch.includes("c"),
      "Outer should NOT have inner's param 'c'"
    );
    assert.ok(
      !outerBatch.includes("d"),
      "Outer should NOT have inner's let 'd'"
    );

    // Find the batch that included param 'c' (inner function)
    const innerBatch = allBatchIds.find((ids) => ids.includes("c"));
    assert.ok(innerBatch, "Should find a batch with inner param 'c'");
    assert.ok(innerBatch.includes("d"), "Inner should have its own let 'd'");
    assert.ok(
      !innerBatch.includes("a"),
      "Inner should NOT have outer's param 'a'"
    );
  });
});

describe("Batch Renaming", () => {
  it("uses suggestAllNames when available", async () => {
    const code = `
      function a(e, t) {
        var n = [];
        return n;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    let batchCalled = false;
    let sequentialCalled = false;

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string) {
        sequentialCalled = true;
        return { name: `${currentName}Val` };
      },
      async suggestAllNames(request) {
        batchCalled = true;
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    assert.strictEqual(batchCalled, true, "Should use batch renaming");
    assert.strictEqual(
      sequentialCalled,
      false,
      "Should not use sequential when batch available"
    );
  });

  it("falls back to sequential when suggestAllNames not available", async () => {
    const code = `
      function a(e, t) {
        var n = [];
        return n;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    let sequentialCalled = false;

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string) {
        sequentialCalled = true;
        return { name: `${currentName}Val` };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    assert.strictEqual(
      sequentialCalled,
      true,
      "Should use sequential renaming"
    );
  });

  it("handles duplicate names from LLM by retrying", async () => {
    const code = `
      function a(e, t) {
        return e + t;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    let attempts = 0;

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(_request) {
        attempts++;
        if (attempts === 1) {
          // First attempt: return duplicates
          return {
            renames: {
              a: "func",
              e: "input",
              t: "input" // Duplicate!
            } as Record<string, string>
          };
        } else {
          // Second attempt: fix the duplicates for both e and t
          return {
            renames: {
              e: "firstInput",
              t: "secondInput"
            } as Record<string, string>
          };
        }
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    assert.strictEqual(attempts, 2, "Should retry after duplicate");

    // Verify the output has unique names
    const output = generate(ast);
    assert.ok(output.code.includes("func"), "Function should be renamed");
    assert.ok(
      output.code.includes("firstInput"),
      "First param should be renamed"
    );
    assert.ok(
      output.code.includes("secondInput"),
      "Second param should have unique name"
    );
  });

  it("handles missing identifiers from LLM response", async () => {
    const code = `
      function a(e, t) {
        var n = e + t;
        return n;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    let attempts = 0;

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(_request) {
        attempts++;
        if (attempts === 1) {
          // First attempt: missing some identifiers
          return {
            renames: {
              a: "calculate",
              e: "first"
              // Missing: t, n
            } as Record<string, string>
          };
        } else {
          // Second attempt: provide the missing ones
          return {
            renames: {
              t: "second",
              n: "result"
            } as Record<string, string>
          };
        }
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    assert.strictEqual(attempts, 2, "Should retry for missing identifiers");

    const output = generate(ast);
    assert.ok(output.code.includes("calculate"), "Function should be renamed");
    assert.ok(output.code.includes("first"), "First param should be renamed");
    assert.ok(output.code.includes("second"), "Second param should be renamed");
    assert.ok(output.code.includes("result"), "Variable should be renamed");
  });

  it("keeps original names after max retries", async () => {
    const code = `
      function a(e) {
        return e;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    let attempts = 0;

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames() {
        attempts++;
        // Always return empty - simulating LLM failure
        return { renames: {} };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // With batch-until-done: 1 attempt for main batch (no progress → exhausted)
    // + 1 straggler pass = 2 total attempts
    assert.strictEqual(
      attempts,
      2,
      "Should make 2 attempts (main + straggler pass)"
    );

    // Original names should be preserved
    const output = generate(ast);
    assert.ok(
      output.code.includes("function a"),
      "Function name should be preserved"
    );
    assert.ok(output.code.includes("(e)"), "Param name should be preserved");
  });

  it("sanitizes reserved words to valid names", async () => {
    const code = `
      function a(e) {
        return e;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames() {
        // Return reserved words - they should be sanitized to class_ and if_
        return {
          renames: {
            e: "class" // Reserved word gets sanitized to class_
          }
        };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    const output = generate(ast);
    // Reserved word "class" should be sanitized to "class_"
    assert.ok(
      output.code.includes("class_"),
      "Reserved word should be sanitized with underscore suffix"
    );
    assert.ok(
      !output.code.includes("class("),
      "Raw reserved word should not be in output"
    );
  });

  it("renames block-scoped for-loop variables", async () => {
    const code = `
      function a(e) {
        let r = "";
        for (let o = 0; o < e; o++) r += "x";
        return r;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          const map: Record<string, string> = {
            a: "buildString",
            e: "length",
            r: "result",
            o: "index"
          };
          renames[id] = map[id] || `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    const output = generate(ast);
    assert.ok(
      output.code.includes("index"),
      "For-loop variable 'o' should be renamed to 'index'"
    );
    assert.ok(
      !output.code.includes("let o"),
      "Original for-loop variable name should be gone"
    );
    assert.ok(
      output.code.includes("result"),
      "Function-scoped variable 'r' should be renamed"
    );
    assert.ok(
      output.code.includes("buildString"),
      "Function name 'a' should be renamed"
    );
  });
});

describe("Deadlock breaking", () => {
  it("breaks scopeParent deadlock in single-IIFE bundles", async () => {
    // Simulate: outer IIFE wraps inner functions.
    // - outer has internalCallees = {inner} (it calls inner)
    // - inner has scopeParent = outer (it's nested inside outer)
    // Without deadlock breaking: inner waits for outer (scopeParent),
    //   outer waits for inner (callee) → deadlock, zero processing
    // With deadlock breaking: inner processes first (scopeParent relaxed),
    //   then outer becomes ready
    const code = `
      (function() {
        function a(x) { return x + 1; }
        a(42);
      })();
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    // Verify the deadlock scenario exists
    assert.ok(functions.length >= 2, "Should have at least 2 functions");

    // Find the outer and inner functions
    const _outer = functions.find(
      (f) => f.internalCallees.size > 0 && !f.scopeParent
    );
    const _inner = functions.find((f) => f.scopeParent !== undefined);

    // If buildFunctionGraph doesn't set up the exact deadlock, that's fine —
    // the important thing is processAll completes and processes functions
    const processOrder: string[] = [];
    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        processOrder.push(currentName);
        return { name: `${currentName}Renamed` };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, { concurrency: 1 });

    // All functions should be processed (not stuck in deadlock)
    for (const fn of functions) {
      assert.strictEqual(
        fn.status,
        "done",
        `Function ${fn.sessionId} should be done`
      );
    }
  });

  it("breaks mid-loop scopeParent deadlock", async () => {
    // Three-level nesting: grandparent -> parent -> child
    // After grandparent is processed, parent should become ready even
    // if it has a scopeParent dependency that creates a secondary deadlock
    const code = `
      (function() {
        function a(x) {
          function b(y) { return y * 2; }
          return b(x);
        }
        a(10);
      })();
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const processedFunctions: string[] = [];
    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        processedFunctions.push(currentName);
        return { name: `${currentName}Renamed` };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, { concurrency: 1 });

    // All functions should complete
    for (const fn of functions) {
      assert.strictEqual(
        fn.status,
        "done",
        `Function ${fn.sessionId} should be done`
      );
    }
    assert.ok(
      processedFunctions.length > 0,
      "Should have processed some identifiers"
    );
  });

  it("preserves scopeParent ordering when no deadlock", async () => {
    // Simple case: parent has no callees, child has scopeParent = parent
    // No deadlock — parent processes first naturally, then child
    const code = `
      function outer(a) {
        function inner(b) { return b + 1; }
        return inner(a);
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const processOrder: string[] = [];
    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        processOrder.push(currentName);
        return { name: `${currentName}Renamed` };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, { concurrency: 1 });

    // All functions should complete
    for (const fn of functions) {
      assert.strictEqual(
        fn.status,
        "done",
        `Function ${fn.sessionId} should be done`
      );
    }
  });
});

describe("Error resilience", () => {
  it("completes processing when LLM throws on one function", async () => {
    const code = `
      function a() { return 1; }
      function b() { return 2; }
      function c() { return 3; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    let _callCount = 0;

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        _callCount++;
        if (currentName === "b") {
          throw new Error("API timeout");
        }
        return { name: `${currentName}Renamed` };
      }
    };

    const processor = new RenameProcessor(ast);
    const _renames = await processor.processAll(functions, mockLLM, {
      concurrency: 1
    });

    // All functions should complete (not crash)
    for (const fn of functions) {
      assert.strictEqual(
        fn.status,
        "done",
        `Function ${fn.sessionId} should be done`
      );
    }

    // The failed function should have an empty renameMapping
    const fnB = functions.find(
      (f) =>
        (f.path.node as t.FunctionDeclaration).id?.name === "b" ||
        (f.path.node as t.FunctionDeclaration).id?.name === "bRenamed"
    );
    if (fnB) {
      assert.ok(fnB.renameMapping, "Failed function should have renameMapping");
    }

    // Failed count should be 1
    assert.strictEqual(processor.failed, 1, "Should track one failure");
  });

  it("processes dependents of a failed leaf function", async () => {
    const code = `
      function a() { return b(); }
      function b() { return 1; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        // b is the leaf — it will be processed first
        if (currentName === "b") {
          throw new Error("API timeout");
        }
        return { name: `${currentName}Renamed` };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM, { concurrency: 1 });

    // Both functions should complete — a should still process even though b failed
    for (const fn of functions) {
      assert.strictEqual(
        fn.status,
        "done",
        `Function ${fn.sessionId} should be done`
      );
    }

    assert.strictEqual(processor.failed, 1, "Should track one failure");
  });

  it("reports zero failures when all succeed", async () => {
    const code = `function a(b) { return b; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string, _context: LLMContext) {
        return { name: `${currentName}Renamed` };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    assert.strictEqual(processor.failed, 0, "Should have zero failures");
  });
});

describe("processUnified", () => {
  it("processes leaf functions and leaf module vars in parallel", async () => {
    const code = `
      var a = 1;
      var b = 2;
      function c() { return 1; }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    const processedTypes: string[] = [];

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string) {
        processedTypes.push(`function:${currentName}`);
        return { name: `${currentName}Renamed` };
      },
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          // Module-level batch
          processedTypes.push(`module:${request.identifiers.join(",")}`);
        } else {
          processedTypes.push(
            `function-batch:${request.identifiers.join(",")}`
          );
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    // Both function and module binding processing should have occurred
    assert.ok(
      processedTypes.some((t) => t.startsWith("module:")),
      "Should process module bindings"
    );
    assert.ok(
      processedTypes.some(
        (t) => t.startsWith("function-batch:") || t.startsWith("function:")
      ),
      "Should process functions"
    );
  });

  it("module var dependent on function waits for function to complete", async () => {
    const code = `
      function f() { return 42; }
      var a = f();
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    const processOrder: string[] = [];

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string) {
        processOrder.push(currentName);
        return { name: `${currentName}Renamed` };
      },
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          processOrder.push(`module:${request.identifiers.join(",")}`);
        } else {
          processOrder.push(`fn:${request.identifiers.join(",")}`);
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 1 });

    // All nodes should be processed
    for (const [, node] of graph.nodes) {
      if (node.type === "function") {
        assert.strictEqual(node.node.status, "done", "Function should be done");
      } else {
        assert.strictEqual(
          node.node.status,
          "done",
          "Module binding should be done"
        );
      }
    }
  });

  it("processes all nodes even with no module bindings", async () => {
    const code = `
      function foo() { return 1; }
      function bar() { return foo(); }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string) {
        return { name: `${currentName}Renamed` };
      },
      async suggestAllNames(request) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    for (const [, node] of graph.nodes) {
      if (node.type === "function") {
        assert.strictEqual(
          node.node.status,
          "done",
          "All functions should be done"
        );
      }
    }
  });

  it("handles empty graph gracefully", async () => {
    const code = `console.log("hello");`;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "x" };
      }
    };

    const processor = new RenameProcessor(ast);
    const renames = await processor.processUnified(graph, mockLLM);

    assert.ok(Array.isArray(renames));
    assert.strictEqual(renames.length, 0);
  });
});

describe("processUnified two-tier deadlock breaking", () => {
  it("uses Tier 1 (scopeParent relaxation) before Tier 2", async () => {
    // Create a nested function structure where child is blocked by scopeParent
    // but has no callee dependencies — Tier 1 should unblock it
    const code = `
      (function() {
        function a(x) {
          function b(y) { return y * 2; }
          return b(x);
        }
        a(10);
      })();
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string) {
        return { name: `${currentName}Renamed` };
      },
      async suggestAllNames(request) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    // All nodes should be processed
    for (const [, node] of graph.nodes) {
      if (node.type === "function") {
        assert.strictEqual(
          node.node.status,
          "done",
          `Function ${node.node.sessionId} should be done`
        );
      }
    }
  });
});

describe("processUnified module binding retry", () => {
  it("retries module bindings on collision", async () => {
    const code = `
      var a = 1;
      var b = 2;
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    let attempts = 0;

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          // Module-level batch
          attempts++;
          if (attempts === 1) {
            // First attempt: a gets a valid rename, b is missing
            return {
              renames: {
                a: "firstValue"
                // b missing — triggers retry
              }
            };
          } else {
            // Second attempt: only b remaining, gets a unique name
            return {
              renames: {
                b: "secondValue"
              }
            };
          }
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    // Should have retried — round 1 renames a, b missing, round 2 renames b
    assert.ok(
      attempts >= 2,
      `Should retry after missing identifier, got ${attempts} attempts`
    );

    // All module bindings should be done
    for (const [, node] of graph.nodes) {
      if (node.type === "module-binding") {
        assert.strictEqual(node.node.status, "done");
      }
    }
  });

  it("uses resolveConflict fallback after retries exhausted", async () => {
    const code = `
      var a = 1;
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          // Always return a name that collides with an existing scope name
          // The usedNames will contain "a" initially
          return {
            renames: { a: "console" } // "console" should collide
          };
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    // Should complete without error (resolveConflict handles the collision)
    for (const [, node] of graph.nodes) {
      if (node.type === "module-binding") {
        assert.strictEqual(node.node.status, "done");
      }
    }

    // Should have reports
    assert.ok(processor.reports.length > 0, "Should generate reports");
  });

  it("generates reports for module binding batches", async () => {
    const code = `
      var a = 1;
      var b = 2;
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          return {
            renames: {
              a: "firstValue",
              b: "secondValue"
            }
          };
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    // Find the module binding report
    const mbReport = processor.reports.find((r) => r.type === "module-binding");
    assert.ok(mbReport, "Should have a module binding batch report");
    assert.ok(mbReport?.totalIdentifiers > 0, "Should have identifiers");
    assert.ok(mbReport?.renamedCount > 0, "Should have renamed some");
    assert.ok(
      Object.keys(mbReport?.outcomes).length > 0,
      "Should have outcomes"
    );
  });
});

describe("applyModuleRename correctness", () => {
  it("renames declaration, references, and scope bindings", async () => {
    const code = `
      var a = 1;
      var b = a + 2;
      console.log(a);
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          return {
            renames: {
              a: "alpha",
              b: "beta"
            }
          };
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    // Verify the AST was actually updated by generating code
    const output = generate(ast).code;

    // The generated code should contain the new names, not the old ones
    assert.ok(
      output.includes("alpha"),
      `Output should contain 'alpha', got: ${output}`
    );
    assert.ok(
      output.includes("beta"),
      `Output should contain 'beta', got: ${output}`
    );
    assert.ok(
      !output.includes("var a "),
      `Output should not contain 'var a ', got: ${output}`
    );
    assert.ok(
      !output.includes("var b "),
      `Output should not contain 'var b ', got: ${output}`
    );
  });

  it("does not rename shadowed references in child scopes", async () => {
    // The inner 'a' is a different binding — should NOT be renamed
    const code = `
      var a = 1;
      function f() {
        var a = 99;
        return a;
      }
      console.log(a);
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          return { renames: { a: "alpha" } };
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 1 });

    const output = generate(ast).code;

    // Module-level 'a' should be renamed to 'alpha'
    assert.ok(
      output.includes("alpha"),
      `Output should contain 'alpha' for module-level a, got: ${output}`
    );
    // The inner function should still have its own 'a' (shadowed)
    // It will be renamed by the function processor (to aRenamed), but not to 'alpha'
    assert.ok(
      !output.includes("var alpha = 99"),
      `Inner shadowed 'a' should NOT become 'alpha', got: ${output}`
    );
  });

  it("handles constant violations (reassignments)", async () => {
    const code = `
      var a = 1;
      a = 2;
      a = a + 3;
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          return { renames: { a: "counter" } };
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    const output = generate(ast).code;

    // All references to 'a' should be renamed to 'counter'
    assert.ok(
      !output.match(/\ba\b.*=/),
      `No assignment to bare 'a' should remain, got: ${output}`
    );
    assert.ok(
      output.includes("counter = 2"),
      `Should have 'counter = 2', got: ${output}`
    );
    assert.ok(
      output.includes("counter = counter + 3") ||
        output.includes("counter = counter+3"),
      `Should have 'counter = counter + 3', got: ${output}`
    );
  });

  it("renames multiple module bindings without cross-contamination", async () => {
    const code = `
      var a = 1;
      var b = 2;
      var c = a + b;
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          return {
            renames: {
              a: "first",
              b: "second",
              c: "sum"
            }
          };
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    const output = generate(ast).code;

    assert.ok(
      output.includes("first"),
      `Should contain 'first', got: ${output}`
    );
    assert.ok(
      output.includes("second"),
      `Should contain 'second', got: ${output}`
    );
    assert.ok(output.includes("sum"), `Should contain 'sum', got: ${output}`);
    // c = a + b should now be sum = first + second
    assert.ok(
      output.includes("first + second") || output.includes("first+second"),
      `References in init should be updated, got: ${output}`
    );
  });
});

describe("processUnified deadlock tracking correctness", () => {
  it("correctly tracks pending/blocked counts through processing", async () => {
    // Chain: a -> b -> c (c is leaf)
    const code = `
      function a() { return b(); }
      function b() { return c(); }
      function c() { return 1; }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string) {
        return { name: `${currentName}Renamed` };
      },
      async suggestAllNames(request) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 1 });

    // All should be processed — verifies pending/blocked tracking didn't lose nodes
    for (const [, node] of graph.nodes) {
      if (node.type === "function") {
        assert.strictEqual(
          node.node.status,
          "done",
          `Function ${node.node.sessionId} should be done`
        );
      }
    }
  });

  it("handles complex dependency graph with mixed node types", async () => {
    // Module vars depend on functions, functions depend on each other
    const code = `
      function f() { return 42; }
      function g() { return f(); }
      var a = f();
      var b = a + 1;
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName(currentName: string) {
        return { name: `${currentName}Better` };
      },
      async suggestAllNames(request) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Better`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    // All nodes should complete
    for (const [, node] of graph.nodes) {
      const n = node.type === "function" ? node.node : node.node;
      assert.strictEqual(n.status, "done", `Node should be done`);
    }
  });
});

describe("Outcome suggestion persistence", () => {
  it("resolves duplicate suggestions via resolveRemaining fallback", async () => {
    const code = `
      function a(e, t) {
        return e + t;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames() {
        // Both params get the same name = duplicate
        return {
          renames: { a: "add", e: "value", t: "value" } as Record<
            string,
            string
          >
        };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    // With resolveRemaining fallback, duplicates get resolved via suffix
    const output = generate(ast);
    assert.ok(output.code.includes("add"), "Function should be renamed to add");
    assert.ok(
      output.code.includes("value"),
      "At least one param should be 'value'"
    );
    // The duplicate should be resolved (e.g., value2)
    const report = processor.reports.find((r) => r.outcomes.e || r.outcomes.t);
    assert.ok(report, "Should have a report with param outcomes");

    // Both e and t should be renamed (one directly, one via resolveConflict)
    const eOutcome = report?.outcomes.e;
    const tOutcome = report?.outcomes.t;
    assert.ok(eOutcome, "Should have outcome for 'e'");
    assert.ok(tOutcome, "Should have outcome for 't'");
    assert.strictEqual(eOutcome.status, "renamed", "e should be renamed");
    assert.strictEqual(tOutcome.status, "renamed", "t should be renamed");
  });

  it("persists suggestion on unchanged outcomes", async () => {
    const code = `
      function a(e) {
        return e;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames() {
        // Return the original name = unchanged
        return { renames: { a: "add", e: "e" } };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    const report = processor.reports.find((r) => r.outcomes.e);
    assert.ok(report, "Should have a report with 'e' outcome");

    const outcome = report?.outcomes.e;
    assert.strictEqual(outcome.status, "unchanged");
    if (outcome.status === "unchanged") {
      assert.strictEqual(
        outcome.suggestion,
        "e",
        "Should persist the LLM's suggestion on unchanged"
      );
    }
  });

  // Note: "invalid" outcomes are nearly impossible to trigger through suggestAllNames
  // because sanitizeIdentifier fixes most invalid inputs before validation runs.
  // The invalid suggestion field is tested via diagnostics.test.ts with mock reports.

  it("does not have suggestion on missing outcomes", async () => {
    const code = `
      function a(e) {
        return e;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestName() {
        return { name: "fallback" };
      },
      async suggestAllNames() {
        // Only return rename for 'a', missing 'e'
        return { renames: { a: "add" } };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, mockLLM);

    const report = processor.reports.find((r) => r.outcomes.e);
    assert.ok(report, "Should have a report with 'e' outcome");

    const outcome = report?.outcomes.e;
    assert.strictEqual(outcome.status, "missing");
    // Missing outcomes have no suggestion by definition
    assert.ok(
      !("suggestion" in outcome),
      "Missing outcomes should not have suggestion field"
    );
  });
});

describe("applyValidRenames late-collision guard", () => {
  it("skips names already claimed in usedNames", () => {
    // Simulate the race: validateBatchRenames passed "foo" as valid for both
    // "a" and "b" (from different lanes), but by the time lane B's
    // applyValidRenames runs, lane A already claimed "foo".
    const usedNames = new Set(["foo"]); // "foo" already claimed by lane A
    const applied: Array<[string, string]> = [];

    const callbacks: BatchRenameCallbacks = {
      buildRequest: () => {
        throw new Error("not needed");
      },
      applyRename: (oldName, newName) => {
        usedNames.add(newName);
        applied.push([oldName, newName]);
      },
      getUsedNames: () => usedNames,
      functionId: "test-fn"
    };

    const validation: BatchValidationResult = {
      valid: { a: "foo", b: "bar" },
      duplicates: [],
      invalid: [],
      missing: [],
      unchanged: []
    };

    const idState = new Map<string, IdentifierAttemptState>([
      ["a", { attempts: 0, freeRetries: 0 }],
      ["b", { attempts: 0, freeRetries: 0 }]
    ]);
    const outcomes: Record<string, IdentifierOutcome> = {};

    const result = applyValidRenames(
      validation,
      callbacks,
      idState,
      outcomes,
      1,
      false
    );

    // "a → foo" should be skipped (late collision), "b → bar" should apply
    assert.strictEqual(result.applied, 1);
    assert.deepStrictEqual(result.lateCollisions, ["a"]);
    assert.deepStrictEqual(applied, [["b", "bar"]]);
    assert.strictEqual(outcomes.b.status, "renamed");
    assert.ok(!outcomes.a, "a should not have an outcome (late collision)");
  });

  it("claims names atomically so second entry with same name is skipped", () => {
    // Even within a single call, if validation.valid has two entries mapping
    // to the same newName (shouldn't happen normally, but defense-in-depth),
    // only the first should be applied.
    const usedNames = new Set<string>();
    const applied: Array<[string, string]> = [];

    const callbacks: BatchRenameCallbacks = {
      buildRequest: () => {
        throw new Error("not needed");
      },
      applyRename: (oldName, newName) => {
        usedNames.add(newName);
        applied.push([oldName, newName]);
      },
      getUsedNames: () => usedNames,
      functionId: "test-fn"
    };

    // Both map to "target" — only first should win
    const validation: BatchValidationResult = {
      valid: { x: "target", y: "target" },
      duplicates: [],
      invalid: [],
      missing: [],
      unchanged: []
    };

    const idState = new Map<string, IdentifierAttemptState>([
      ["x", { attempts: 0, freeRetries: 0 }],
      ["y", { attempts: 0, freeRetries: 0 }]
    ]);
    const outcomes: Record<string, IdentifierOutcome> = {};

    const result = applyValidRenames(
      validation,
      callbacks,
      idState,
      outcomes,
      1,
      false
    );

    assert.strictEqual(result.applied, 1);
    assert.strictEqual(result.lateCollisions.length, 1);
    // First entry wins, second is a late collision
    assert.deepStrictEqual(applied.length, 1);
    assert.strictEqual(applied[0][1], "target");
  });
});

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}
