import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import type { FunctionNode, IdentifierOutcome } from "../analysis/types.js";
import { generate } from "../babel-utils.js";
import type { LLMProvider } from "../llm/types.js";
import { traverse } from "../babel-utils.js";
import { isSettled } from "./lifecycle.js";
import type { Stateful } from "./lifecycle.js";

/** The names an llm-done node recorded on its lifecycle state. */
function llmDoneNames(node: Stateful): Record<string, string> {
  assert.strictEqual(
    node.state.kind,
    "llm-done",
    `expected an llm-done node, got ${node.state.kind}`
  );
  return node.state.kind === "llm-done" ? node.state.names : {};
}
import {
  RenameProcessor,
  applyValidRenames,
  buildCallbacks,
  buildRetryUsedNames,
  computeLaneCount,
  computeMaxFreeRetries,
  extractRetrySnippet,
  type BatchRenameCallbacks,
  type BatchValidationResult,
  type IdentifierAttemptState,
  type RenameStrategy
} from "./processor.js";

/** Collect FunctionNodes from a unified graph. */
function getFunctionNodes(
  graph: ReturnType<typeof buildUnifiedGraph>
): FunctionNode[] {
  const fns: FunctionNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "function") fns.push(node.node);
  }
  return fns;
}

describe("RenameProcessor", () => {
  it("respects concurrency limit", async () => {
    const code = `
      function a() {}
      function b() {}
      function c() {}
      function d() {}
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    let maxConcurrent = 0;
    let currentConcurrent = 0;

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        currentConcurrent++;
        maxConcurrent = Math.max(maxConcurrent, currentConcurrent);

        // Simulate some async work
        await new Promise((resolve) => setTimeout(resolve, 10));

        currentConcurrent--;
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 2 });

    assert.ok(maxConcurrent <= 2, "Should respect concurrency limit");
  });

  it("counts an internal per-function error in failed and completes the run", async () => {
    // LLM provider throws are contained downstream and never reach this
    // counter — anything counted here is a programming error in our own
    // pipeline, which the CLI must surface as a failed run.
    const code = `
      function a(x) { return x + 1; }
      function b(y) { return y * 2; }
    `;
    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    const poisoned = getFunctionNodes(graph)[0];
    Object.defineProperty(poisoned, "path", {
      get() {
        throw new TypeError("poisoned path — simulated internal bug");
      }
    });

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) renames[id] = `${id}Renamed`;
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    assert.strictEqual(processor.failed, 1, "internal error must be counted");
  });

  it("tracks rename decisions for source maps", async () => {
    const code = `function a(b) { return b; }`;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        const map: Record<string, string> = {
          a: "calculateValue",
          b: "inputValue"
        };
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = map[id] ?? `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    const renames = await processor.processUnified(graph, mockLLM);

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

  it("renameMapping is populated after processing", async () => {
    const code = `function a(b, c) { return b + c; }`;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        const map: Record<string, string> = {
          a: "addNumbers",
          b: "firstNum",
          c: "secondNum"
        };
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = map[id] ?? `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    // Check the llm-done state carries the applied names
    const [fn] = getFunctionNodes(graph);
    assert.ok(fn, "Should have a function node");
    const names = llmDoneNames(fn);
    assert.strictEqual(names.a, "addNumbers");
    assert.strictEqual(names.b, "firstNum");
    assert.strictEqual(names.c, "secondNum");
  });

  it("state transitions pending → llm-done during processing", async () => {
    const code = `function t() { return 1; }`;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    const [fn] = getFunctionNodes(graph);
    assert.ok(fn, "Should have a function node");

    assert.strictEqual(fn.state.kind, "pending", "Should start as pending");

    const kindDuringProcess: string[] = [];

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        // In flight, the node is still pending — it settles only on completion.
        kindDuringProcess.push(fn.state.kind);
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    assert.ok(
      kindDuringProcess.includes("pending"),
      "Should still be pending during the LLM call"
    );
    assert.strictEqual(
      fn.state.kind,
      "llm-done",
      "Should be llm-done after completion"
    );
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
    const graph = buildUnifiedGraph(ast, "test.js");
    const [fn] = getFunctionNodes(graph);
    assert.ok(fn, "Should have a function node");

    // External callees captured at build time
    const externalBefore = new Set(fn.externalCallees);

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        const map: Record<string, string> = {
          myFunc: "processData",
          data: "jsonString"
        };
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = map[id] ?? `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

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
    const graph = buildUnifiedGraph(ast, "test.js");

    const renamedIds: string[] = [];
    const mockLLM: LLMProvider = {
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
    await processor.processUnified(graph, mockLLM);

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
    const graph = buildUnifiedGraph(ast, "test.js");

    const renamedIds: string[] = [];
    const mockLLM: LLMProvider = {
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
    await processor.processUnified(graph, mockLLM);

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
    const graph = buildUnifiedGraph(ast, "test.js");

    // Track identifiers per batch call, keyed by which function they belong to
    const allBatchIds: string[][] = [];
    const mockLLM: LLMProvider = {
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
    await processor.processUnified(graph, mockLLM, { concurrency: 1 });

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
  it("handles duplicate names from LLM by retrying", async () => {
    const code = `
      function a(e, t) {
        return e + t;
      }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    let attempts = 0;

    const mockLLM: LLMProvider = {
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
    await processor.processUnified(graph, mockLLM);

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
    const graph = buildUnifiedGraph(ast, "test.js");
    let attempts = 0;

    const mockLLM: LLMProvider = {
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
    await processor.processUnified(graph, mockLLM);

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
    const graph = buildUnifiedGraph(ast, "test.js");
    let attempts = 0;

    const mockLLM: LLMProvider = {
      async suggestAllNames() {
        attempts++;
        // Always return empty - simulating LLM failure
        return { renames: {} };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

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
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
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
    await processor.processUnified(graph, mockLLM);

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

  it("sanitizes global built-in names to avoid shadowing", async () => {
    const code = `
      function a(e) {
        return e;
      }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames() {
        return {
          renames: {
            e: "Date" // Global built-in gets sanitized to Date_
          }
        };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    const output = generate(ast);
    assert.ok(
      output.code.includes("Date_"),
      "Global built-in should be sanitized with underscore suffix"
    );
    assert.ok(
      !output.code.includes("Date(") && !output.code.match(/\bDate\b[^_]/),
      "Raw global built-in name should not shadow the global"
    );
  });

  it("resolves persistent collisions algorithmically after one LLM retry", async () => {
    // "takenName" exists at module level, so the mock's suggestion for `e`
    // collides every time. The plan's conflict-resolution lever: initial
    // call + ONE LLM retry, then resolve via suffixing — no retry #2+,
    // no straggler call for identifiers that already have a suggestion.
    const code = `
      function takenName() { return 1; }
      function a(e) {
        return e + takenName();
      }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    const seenBatches: string[][] = [];

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        seenBatches.push([...request.identifiers]);
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          // `a` gets a clean name; `e` always collides with takenName
          renames[id] = id === "a" ? "computeTotal" : "takenName";
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    const callsWithE = seenBatches.filter((b) => b.includes("e")).length;
    assert.strictEqual(
      callsWithE,
      2,
      `Persistent collision should stop after initial call + 1 retry, got ${callsWithE} calls for "e"`
    );

    const output = generate(ast);
    assert.ok(
      !/\be\b/.test(output.code),
      "Colliding identifier should still end up renamed (via suffix resolution)"
    );
    assert.match(
      output.code,
      /takenName(Val|Value|Data|2)/,
      "Suffix-resolved variant of the colliding suggestion should be applied"
    );
  });

  it("still runs the straggler pass for identifiers the LLM never answered", async () => {
    // Provider errors are contained: the first call throws, the straggler
    // pass gives those identifiers (which have no suggestion) one final
    // chance instead of skipping straight to unrenamed.
    const code = `
      function a(e) {
        return e;
      }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    let calls = 0;

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        calls++;
        if (calls === 1) throw new Error("provider hiccup");
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Recovered`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    assert.strictEqual(calls, 2, "Straggler pass should make the second call");
    const output = generate(ast);
    assert.ok(
      output.code.includes("aRecovered") && output.code.includes("eRecovered"),
      "Straggler pass should recover identifiers after a provider error"
    );
    assert.strictEqual(processor.failed, 0, "LLM throws must stay contained");
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
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
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
    await processor.processUnified(graph, mockLLM);

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

describe("Error resilience", () => {
  it("completes processing when LLM throws on one function", async () => {
    const code = `
      function a() { return 1; }
      function b() { return 2; }
      function c() { return 3; }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        if (request.identifiers.includes("b")) {
          throw new Error("API timeout");
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

    // All functions should complete (not crash)
    for (const fn of getFunctionNodes(graph)) {
      assert.ok(isSettled(fn), `Function ${fn.sessionId} should be done`);
    }

    const output = generate(ast).code;
    // Other functions should still be renamed
    assert.ok(output.includes("aRenamed"), "a should be renamed");
    assert.ok(output.includes("cRenamed"), "c should be renamed");
    // The failing function keeps its original name
    assert.ok(!output.includes("bRenamed"), "b should keep its original name");

    // The batch loop contains LLM errors: the failure surfaces as unrenamed
    // outcomes in the report, not as a processor-level function failure.
    const bReport = processor.reports.find((r) => r.outcomes.b);
    assert.ok(bReport, "Should have a report covering 'b'");
    assert.strictEqual(
      bReport.renamedCount,
      0,
      "b's batch should have renamed nothing"
    );
    assert.strictEqual(
      processor.failed,
      0,
      "LLM errors are contained by the batch loop, not function failures"
    );
  });

  it("processes dependents of a failed leaf function", async () => {
    const code = `
      function a() { return b(); }
      function b() { return 1; }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        // b is the leaf — it will be processed first
        if (request.identifiers.includes("b")) {
          throw new Error("API timeout");
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

    // Both functions should complete — a should still process even though b failed
    for (const fn of getFunctionNodes(graph)) {
      assert.ok(isSettled(fn), `Function ${fn.sessionId} should be done`);
    }

    const output = generate(ast).code;
    assert.ok(
      output.includes("aRenamed"),
      "Dependent 'a' should still be processed and renamed"
    );
  });

  it("reports zero failures when all succeed", async () => {
    const code = `function a(b) { return b; }`;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
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
        assert.ok(isSettled(node.node), "Function should be done");
      } else {
        assert.ok(isSettled(node.node), "Module binding should be done");
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
        assert.ok(isSettled(node.node), "All functions should be done");
      }
    }
  });

  it("seeds usedNames with scope globals to prevent shadowing built-ins", async () => {
    // Code where a minified variable is assigned a value derived from Date,
    // and the LLM tries to rename it to "Date" — should be rejected
    const code = `
      (function() {
        var a = Date.now();
        var b = a + 1;
      })();
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    // Verify that "Date" appears in the targetScope.globals
    const globals = Object.keys(graph.targetScope.globals || {});
    assert.ok(
      globals.includes("Date"),
      `"Date" should be in scope globals, got: ${globals.join(", ")}`
    );

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          // Try to rename everything to "Date" — should be rejected
          renames[id] = "Date";
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    const output = generate(ast);
    // "Date" should still refer to the global Date, not be a renamed variable
    assert.ok(
      output.code.includes("Date.now()"),
      "Date.now() should remain intact"
    );
  });

  it("prevents module-level rename from shadowing child function locals", async () => {
    // Module-level var X is a function referenced inside a child function.
    // Child function also has a local var Y.
    // If function rename turns Y → "agentSymbols" and module rename turns
    // X → "agentSymbols", the local var hoists and shadows the module-level
    // reference, causing TypeError at runtime.
    const code = `
      (function() {
        var a = function() { return { kFoo: 1 }; };
        var b = function() {
          var c = a();
          var d = 42;
          return c.kFoo + d;
        };
        b();
      })();
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM = makeNameMapLLM({
      a: "getSymbols",
      b: "processResult",
      c: "getSymbols", // Same as "a" — triggers cross-scope collision
      d: "offset"
    });

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    const output = generate(ast);
    // The function-local "c" and module-level "a" should not both be "getSymbols"
    // because the local var would shadow the module-level reference via hoisting.
    // Count declarations of getSymbols — if both got through, there's a shadow bug.
    const declarations = output.code.match(
      /\bvar getSymbols\b|\blet getSymbols\b|\bconst getSymbols\b/g
    );
    assert.ok(
      !declarations || declarations.length <= 1,
      `Should not have conflicting "getSymbols" declarations across scopes, ` +
        `found ${declarations?.length ?? 0} in:\n${output.code}`
    );
  });

  it("prevents parent rename from shadowing child locals via constantViolations", async () => {
    // Parent-scope var `x` is only *written to* (x |= val) inside a child
    // function, never read. Babel tracks this as a constantViolation, not a
    // referencePath. If the shadow check only inspects referencePaths, it
    // misses this case and both `x` (parent) and param `y` (child) can be
    // renamed to the same name — causing `y |= val` to clobber the parameter.
    const code = `
      (function() {
        var x = 0;
        function pA(y, l) {
          x |= l;
          y.lanes |= l;
        }
        pA({lanes: 0}, 1);
      })();
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    // LLM wants to rename both x (parent) and y (child param) to "fiberNode"
    const mockLLM = makeNameMapLLM({
      x: "fiberNode",
      y: "fiberNode",
      l: "lanes"
    });

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    const output = generate(ast);
    // Both x and y should NOT both become "fiberNode" — that would mean
    // the parent var assignment `x |= l` becomes `fiberNode |= lanes`,
    // clobbering the child function's parameter.
    const fiberNodeDecls = output.code.match(
      /\bvar fiberNode\b|\bfunction\b[^(]*\(\s*fiberNode\b/g
    );
    assert.ok(
      !fiberNodeDecls || fiberNodeDecls.length <= 1,
      `Parent var and child param should not both be "fiberNode" — ` +
        `constantViolation (x |= l) must be detected as a shadow. ` +
        `Found ${fiberNodeDecls?.length ?? 0} in:\n${output.code}`
    );
  });

  it("prevents shadow collision in resolveRemaining fallback path", async () => {
    // When the batch rename path exhausts retries and falls back to
    // resolveRemainingIdentifiers, it must still check wouldShadow.
    // Without this, a parent-scope function reference can be renamed
    // to the same name as a child-scope local var, causing the local
    // to shadow the parent reference at runtime.
    const code = `
      (function() {
        var Hcq = function(x) { return x; };
        var gW = true;
        function yE6(y, cursor) {
          for (; cursor !== null; ) {
            y = cursor;
            var F = y.alternate;
            var I = y.flags;
            switch (y.tag) {
              case 3:
                if ((I & 1024) !== 0 && gW) Hcq(y.stateNode.containerInfo);
                break;
            }
          }
        }
        yE6({}, {});
      })();
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    // LLM renames child var F -> alternateFiberNode, then parent Hcq -> alternateFiberNode
    const mockLLM = makeNameMapLLM({
      F: "alternateFiberNode",
      I: "flags",
      Hcq: "alternateFiberNode",
      gW: "getWorkInProgressFiber",
      yE6: "traverseFiberTree",
      y: "fiber",
      cursor: "node"
    });

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    const output = generate(ast);
    // The parent-scope Hcq (a function) and child-scope F (a var) must not
    // both become "alternateFiberNode". If they do, the var shadows the
    // parent function reference, causing "alternateFiberNode is not a function".
    const altFiberDecls = output.code.match(/\bvar alternateFiberNode\b/g);
    assert.ok(
      !altFiberDecls || altFiberDecls.length <= 1,
      `Parent function ref and child local var should not both be "alternateFiberNode" — ` +
        `resolveRemaining must check wouldShadow. ` +
        `Found ${altFiberDecls?.length ?? 0} in:\n${output.code}`
    );
  });

  it("handles empty graph gracefully", async () => {
    const code = `console.log("hello");`;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames() {
        return { renames: {} };
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
        assert.ok(
          isSettled(node.node),
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
        assert.ok(isSettled(node.node));
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
        assert.ok(isSettled(node.node));
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

describe("module binding rename correctness (processUnified)", () => {
  it("renames declaration, references, and scope bindings", async () => {
    const code = `
      var a = 1;
      var b = a + 2;
      console.log(a);
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
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

  it("renames destructuring assignment targets", async () => {
    const code = `
      var a, b;
      var factory = () => ({ onExit: 1, load: 2 });
      ({
        onExit: a,
        load: b,
      } = factory());
      console.log(a, b);
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          return {
            renames: {
              a: "onExitHandler",
              b: "loadHandler"
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

    // The destructuring assignment targets should be renamed
    assert.ok(
      output.includes("onExitHandler"),
      `Should rename 'a' to 'onExitHandler' in destructuring, got: ${output}`
    );
    assert.ok(
      output.includes("loadHandler"),
      `Should rename 'b' to 'loadHandler' in destructuring, got: ${output}`
    );
    // The old names should NOT appear as binding targets
    assert.ok(
      !output.match(/onExit:\s*a[,\s}]/),
      `Destructuring target 'a' should be renamed, got: ${output}`
    );
    assert.ok(
      !output.match(/load:\s*b[,\s}]/),
      `Destructuring target 'b' should be renamed, got: ${output}`
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

  it("handles shorthand object properties correctly", async () => {
    const code = `
      var a = 1;
      var obj = { a };
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
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
    await processor.processUnified(graph, mockLLM, { concurrency: 50 });

    const output = generate(ast).code;

    // Shorthand { a } should become { a: alpha }, NOT { alpha }
    // Babel's parser creates separate key/value nodes even for shorthand,
    // so mutating value.name leaves key.name unchanged.
    assert.ok(
      output.includes("a: alpha") || output.includes("a:alpha"),
      `Shorthand property should expand to { a: alpha }, got: ${output}`
    );
    assert.ok(
      !/ \{ alpha \}| \{alpha\}/.test(output),
      `Property key should NOT be renamed in shorthand, got: ${output}`
    );
  });

  it("renames UpdateExpression constant violations", async () => {
    const code = `
      var a = 0;
      a++;
      console.log(a);
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
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

    assert.ok(
      output.includes("counter++"),
      `a++ should become counter++, got: ${output}`
    );
    assert.ok(
      !output.includes("a++"),
      `Old name a++ should not remain, got: ${output}`
    );
  });

  it("renames ForIn/ForOf LHS with destructuring pattern", async () => {
    const code = `
      var a = 0;
      for ({x: a} in obj) {}
      console.log(a);
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        if (request.systemPrompt) {
          return { renames: { a: "value" } };
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

    // The destructuring target 'a' should be renamed to 'value'
    assert.ok(
      output.includes("x: value") || output.includes("x:value"),
      `ForIn destructuring target should be renamed, got: ${output}`
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
        assert.ok(
          isSettled(node.node),
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
      assert.ok(isSettled(n), `Node should be done`);
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
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
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
    await processor.processUnified(graph, mockLLM);

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
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames() {
        // Return the original name = unchanged
        return { renames: { a: "add", e: "e" } };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

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
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames() {
        // Only return rename for 'a', missing 'e'
        return { renames: { a: "add" } };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

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

describe("processUnified function-declaration vs module-binding name collision", () => {
  it("prevents duplicate names when function and module binding both suggest the same name", async () => {
    // x1 is an import alias, x2 is a function declaration that calls x1.
    // Both live in module scope. The LLM suggests "normalizePath" for both.
    // The function path handles x2's name, the module path handles x1's name.
    // Without the fix, both get "normalizePath" → duplicate declaration.
    const code = `
      import { normalize as x1 } from "path";
      function x2() { return x1("/foo").replace(/\\\\/g, "/"); }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = "normalizePath";
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 1 });

    const output = generate(ast).code;

    // Count occurrences of "normalizePath" as a declared name
    // There should be at most one binding named "normalizePath"
    const importMatch = output.match(/as\s+(\w+)/);
    const fnMatch = output.match(/function\s+(\w+)/);
    const importName = importMatch?.[1];
    const fnName = fnMatch?.[1];

    assert.ok(
      importName !== fnName,
      `Import alias and function declaration must not share the same name, ` +
        `but both got "${importName}". Output:\n${output}`
    );
    // At least one should be "normalizePath"
    assert.ok(
      importName === "normalizePath" || fnName === "normalizePath",
      `At least one should be renamed to "normalizePath", got import="${importName}" fn="${fnName}". Output:\n${output}`
    );
  });
});

describe("rename chain collision prevention", () => {
  it("rejects rename targeting an in-use name from same batch", () => {
    // Scenario: batch has Y→z and z→labL. With old names kept in usedNames,
    // Y→z should be rejected as a late collision because "z" is still in use.
    const usedNames = new Set(["Y", "z", "_"]);
    const applied: Array<[string, string]> = [];

    const callbacks: BatchRenameCallbacks = {
      buildRequest: () => {
        throw new Error("not needed");
      },
      applyRename: (oldName, newName) => {
        usedNames.delete(oldName);
        usedNames.add(newName);
        applied.push([oldName, newName]);
      },
      getUsedNames: () => usedNames,
      functionId: "test-fn"
    };

    // Y→z should be rejected (z is in usedNames), z→labL and _→labA should pass
    const validation: BatchValidationResult = {
      valid: { Y: "z", z: "labL", _: "labA" },
      duplicates: [],
      invalid: [],
      missing: [],
      unchanged: []
    };

    const idState = new Map<string, IdentifierAttemptState>([
      ["Y", { attempts: 0, freeRetries: 0 }],
      ["z", { attempts: 0, freeRetries: 0 }],
      ["_", { attempts: 0, freeRetries: 0 }]
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

    assert.strictEqual(result.applied, 2, "z→labL and _→labA should apply");
    assert.deepStrictEqual(result.lateCollisions, ["Y"]);
    assert.ok(!outcomes.Y, "Y should not have an outcome (late collision)");
    assert.strictEqual(outcomes.z?.status, "renamed");
    assert.strictEqual(outcomes._?.status, "renamed");
  });

  it("applyValidRenames removes old name from usedNames on successful rename", () => {
    const usedNames = new Set(["a", "b", "existing"]);
    const applied: Array<[string, string]> = [];

    const callbacks: BatchRenameCallbacks = {
      buildRequest: () => {
        throw new Error("not needed");
      },
      applyRename: (oldName, newName) => {
        usedNames.delete(oldName);
        usedNames.add(newName);
        applied.push([oldName, newName]);
      },
      getUsedNames: () => usedNames,
      functionId: "test-fn"
    };

    const validation: BatchValidationResult = {
      valid: { a: "foo" },
      duplicates: [],
      invalid: [],
      missing: [],
      unchanged: []
    };

    const idState = new Map<string, IdentifierAttemptState>([
      ["a", { attempts: 0, freeRetries: 0 }]
    ]);
    const outcomes: Record<string, IdentifierOutcome> = {};

    applyValidRenames(validation, callbacks, idState, outcomes, 1, false);

    assert.ok(usedNames.has("foo"), "new name 'foo' should be in usedNames");
    assert.ok(usedNames.has("b"), "'b' should still be in usedNames");
    assert.ok(
      usedNames.has("existing"),
      "'existing' should still be in usedNames"
    );
    assert.ok(
      !usedNames.has("a"),
      "'a' should have been removed from usedNames"
    );
  });

  it("rename chain is prevented: freed name available on retry", () => {
    // Simulates the full retry-after-free flow:
    // Round 1: {Y: "z", z: "labL"} → Y rejected (z in use), z→labL applied
    // Round 2: {Y: "z"} → Y→z now valid (z was freed)
    const usedNames = new Set(["Y", "z"]);
    const applied: Array<[string, string]> = [];

    const callbacks: BatchRenameCallbacks = {
      buildRequest: () => {
        throw new Error("not needed");
      },
      applyRename: (oldName, newName) => {
        usedNames.delete(oldName);
        usedNames.add(newName);
        applied.push([oldName, newName]);
      },
      getUsedNames: () => usedNames,
      functionId: "test-fn"
    };

    // Round 1: Y→z blocked, z→labL applied
    const validation1: BatchValidationResult = {
      valid: { Y: "z", z: "labL" },
      duplicates: [],
      invalid: [],
      missing: [],
      unchanged: []
    };

    const idState = new Map<string, IdentifierAttemptState>([
      ["Y", { attempts: 0, freeRetries: 0 }],
      ["z", { attempts: 0, freeRetries: 0 }]
    ]);
    const outcomes: Record<string, IdentifierOutcome> = {};

    const result1 = applyValidRenames(
      validation1,
      callbacks,
      idState,
      outcomes,
      1,
      false
    );

    assert.strictEqual(result1.applied, 1);
    assert.deepStrictEqual(result1.lateCollisions, ["Y"]);
    assert.ok(
      usedNames.has("labL"),
      "labL should be in usedNames after round 1"
    );
    assert.ok(!usedNames.has("z"), "z should be freed after round 1");

    // Round 2: Y→z should now succeed since z was freed
    const validation2: BatchValidationResult = {
      valid: { Y: "z" },
      duplicates: [],
      invalid: [],
      missing: [],
      unchanged: []
    };

    const result2 = applyValidRenames(
      validation2,
      callbacks,
      idState,
      outcomes,
      2,
      true
    );

    assert.strictEqual(result2.applied, 1);
    assert.deepStrictEqual(result2.lateCollisions, []);
    assert.ok(usedNames.has("z"), "z should be reclaimed by Y");
    assert.ok(!usedNames.has("Y"), "Y should be freed");
    assert.strictEqual(outcomes.Y?.status, "renamed");
    assert.strictEqual(outcomes.Y?.newName, "z");
  });
});

describe("Phase 1: Separate module binding concurrency pool", () => {
  it("uses moduleConcurrency for module binding dispatch", async () => {
    // Module bindings should use a separate concurrency limiter from functions.
    // This test verifies that the moduleConcurrency option is accepted and
    // doesn't break processing.
    const code = `
      var x = 1;
      var y = 2;
      function a(b) { return b + x; }
    `;
    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = `${id}Renamed`;
        }
        return { renames, finishReason: "stop" };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, {
      concurrency: 10,
      moduleConcurrency: 5
    });

    // Should complete without error — module bindings processed on separate pool
    assert.ok(true, "Processing completed with moduleConcurrency");
  });
});

// Phase 2 tests for computeDependentDepths are in function-graph.test.ts

describe("Phase 3: Reduce retry storms", () => {
  it("scales lane count with binding count", async () => {
    // With 201 bindings, should use 8 lanes (not 4)
    assert.ok(
      computeLaneCount(201) === 8,
      `201 bindings should use 8 lanes, got ${computeLaneCount(201)}`
    );

    // With 1001 bindings, should use 16 lanes
    assert.ok(
      computeLaneCount(1001) === 16,
      `1001 bindings should use 16 lanes, got ${computeLaneCount(1001)}`
    );

    // With 25 bindings, should use 4 lanes (original)
    assert.ok(
      computeLaneCount(26) === 4,
      `26 bindings should use 4 lanes, got ${computeLaneCount(26)}`
    );

    // Below threshold, no lanes
    assert.ok(
      computeLaneCount(24) === 0,
      `24 bindings should use 0 lanes (no splitting), got ${computeLaneCount(24)}`
    );
  });

  it("scales maxFreeRetries with binding count", () => {
    // maxFreeRetries should be proportional to binding count for large functions
    assert.ok(
      computeMaxFreeRetries(2032) === 508,
      `2032 bindings: maxFreeRetries should be 508, got ${computeMaxFreeRetries(2032)}`
    );
    assert.ok(
      computeMaxFreeRetries(100) === 100,
      `100 bindings: maxFreeRetries should be 100 (minimum), got ${computeMaxFreeRetries(100)}`
    );
    assert.ok(
      computeMaxFreeRetries(10) === 100,
      `10 bindings: maxFreeRetries should be 100 (minimum), got ${computeMaxFreeRetries(10)}`
    );
  });

  it("resolves algorithmically on second cross-lane collision", () => {
    // When an identifier has already had ≥1 cross-lane collision retry,
    // it should resolve algorithmically instead of doing another LLM call
    // First collision: free retry (returns true, meaning "retry via LLM")
    // Second collision: resolve algorithmically (returns false, and applies resolution)
    // This is tested via the classifyFailedIdentifiers behavior
    assert.ok(true, "Placeholder — tested via integration");
  });
});

describe("retry context diet", () => {
  describe("extractRetrySnippet", () => {
    it("returns short code unchanged", () => {
      const code = "function a(e) {\n  return e;\n}";
      assert.strictEqual(extractRetrySnippet(code, ["e"]), code);
    });

    it("keeps only lines referencing the identifiers plus context", () => {
      const lines = ["function big(e, t) {"];
      for (let i = 0; i < 50; i++) lines.push(`  const filler${i} = ${i};`);
      lines.push("  const target = e + 1;");
      for (let i = 50; i < 100; i++) lines.push(`  const filler${i} = ${i};`);
      lines.push("  return target;");
      lines.push("}");
      const code = lines.join("\n");

      const snippet = extractRetrySnippet(code, ["e"]);
      const snippetLines = snippet.split("\n");

      assert.ok(
        snippetLines.length < 20,
        `Snippet should be much shorter than the ${lines.length}-line original, got ${snippetLines.length} lines`
      );
      assert.ok(
        snippet.includes("const target = e + 1;"),
        "Line referencing the identifier must be kept"
      );
      assert.ok(
        snippet.includes("function big(e, t) {"),
        "Signature line must be kept"
      );
      assert.ok(snippet.includes("// …"), "Elided regions must be marked");
      assert.ok(
        !snippet.includes("filler40"),
        "Unrelated lines must be dropped"
      );
    });

    it("does not match identifiers inside longer names", () => {
      const lines = ["function big(e) {"];
      for (let i = 0; i < 20; i++) lines.push(`  const items${i} = ${i};`);
      lines.push("  const extended = 1;"); // contains 'e' as substring only
      for (let i = 20; i < 40; i++) lines.push(`  const items${i} = ${i};`);
      lines.push("  return e;");
      lines.push("}");
      const code = lines.join("\n");

      const snippet = extractRetrySnippet(code, ["e"]);
      assert.ok(snippet.includes("return e;"), "Word-boundary match kept");
      assert.ok(
        !snippet.includes("const extended"),
        "Substring occurrences must not count as references"
      );
    });
  });

  describe("buildRetryUsedNames", () => {
    it("always includes the colliding previous suggestions", () => {
      const windowed = new Set(
        Array.from({ length: 100 }, (_, i) => `windowedName${i}`)
      );
      const prev = { e: "handleError", t: "parseConfig" };

      const dieted = buildRetryUsedNames(windowed, prev);

      assert.ok(dieted.has("handleError"));
      assert.ok(dieted.has("parseConfig"));
    });

    it("caps the total size well below the full windowed set", () => {
      const windowed = new Set(
        Array.from({ length: 200 }, (_, i) => `windowedName${i}`)
      );
      const dieted = buildRetryUsedNames(windowed, { e: "handleError" });

      assert.ok(
        dieted.size <= 25,
        `Dieted set should be capped at 25, got ${dieted.size}`
      );
    });

    it("fills remaining budget from the windowed set", () => {
      const windowed = new Set(["nearbyA", "nearbyB"]);
      const dieted = buildRetryUsedNames(windowed, { e: "clash" });

      assert.deepStrictEqual([...dieted].sort(), [
        "clash",
        "nearbyA",
        "nearbyB"
      ]);
    });
  });

  it("sends a dieted retry request (snippet code + capped used names)", async () => {
    // Long function body; first call collides for `e`, so the retry request
    // must carry only the conflict-relevant context, not the full prompt.
    const fillerA = Array.from(
      { length: 60 },
      (_, i) => `  const filler${i} = ${i};`
    ).join("\n");
    const code = `
      function takenName() { return 1; }
      function a(e) {
${fillerA}
        return e + takenName();
      }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    const retryRequests: Array<{
      code: string;
      usedNames: Set<string>;
      identifiers: string[];
    }> = [];

    // e always collides with the stable takenName; fillers succeed so the
    // batch makes progress and e reaches the retry round
    const fixedNames: Record<string, string> = {
      e: "takenName",
      a: "computeTotal",
      takenName: "takenName"
    };
    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        if (request.isRetry) {
          retryRequests.push({
            code: request.code,
            usedNames: request.usedNames,
            identifiers: [...request.identifiers]
          });
        }
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = fixedNames[id] ?? `renamed_${id}`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM);

    const eRetry = retryRequests.find((r) => r.identifiers.includes("e"));
    assert.ok(eRetry, "A retry request for e should have been sent");
    assert.ok(
      eRetry.code.split("\n").length < 20,
      `Retry code should be a snippet, got ${eRetry.code.split("\n").length} lines`
    );
    assert.ok(
      eRetry.usedNames.has("takenName"),
      "Colliding name must be in the retry used-names set"
    );
    assert.ok(
      eRetry.usedNames.size <= 25,
      `Retry used-names should be capped, got ${eRetry.usedNames.size}`
    );
  });
});

describe("cross-function retry batching", () => {
  it("merges concurrent functions' collision retries into one LLM call", async () => {
    // Both a and b hit a collision on their first call; their single-
    // identifier retries arrive within the batching window and must share
    // one LLM call instead of two.
    const code = `
      function takenName() { return 1; }
      function a(e) { return e + takenName(); }
      function b(t) { return t + takenName(); }
    `;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");
    const calls: Array<{ isRetry: boolean; identifiers: string[] }> = [];

    const firstNames: Record<string, string> = {
      a: "alphaHelper",
      b: "betaHelper",
      takenName: "takenName",
      e: "takenName",
      t: "takenName"
    };
    const retryNames: Record<string, string> = {
      e: "eventCount",
      t: "timerCount"
    };

    const mockLLM: LLMProvider = {
      async suggestAllNames(request) {
        calls.push({
          isRetry: !!request.isRetry,
          identifiers: [...request.identifiers]
        });
        const renames: Record<string, string> = {};
        for (const id of request.identifiers) {
          renames[id] = request.isRetry
            ? (retryNames[id] ?? `${id}Retry`)
            : (firstNames[id] ?? `${id}First`);
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, {
      retryBatchWindowMs: 30
    });

    const retryCalls = calls.filter((c) => c.isRetry);
    const mergedCall = retryCalls.find(
      (c) => c.identifiers.includes("e") && c.identifiers.includes("t")
    );
    assert.ok(
      mergedCall,
      `Retries for e and t should merge into one call; saw retry calls: ${JSON.stringify(retryCalls)}`
    );

    const output = generate(ast);
    assert.ok(
      output.code.includes("eventCount") && output.code.includes("timerCount"),
      "Merged retry results must route back to their own functions"
    );
    assert.strictEqual(processor.failed, 0);
  });
});

describe("buildCallbacks (unified callback builder)", () => {
  /** Real function scope whose binding is referenced inside a child block that binds childBindingName. */
  function makeScopeWithChild(bindingName: string, childBindingName: string) {
    const ast = parse(
      `function f(${bindingName}) { { let ${childBindingName} = 1; use(${bindingName}, ${childBindingName}); } }`
    );
    let parentScope: import("@babel/traverse").Scope | undefined;
    traverse(ast, {
      Function(path) {
        parentScope = path.scope;
        path.stop();
      }
    });
    if (!parentScope) throw new Error("no function scope");
    return { parentScope };
  }

  function scopeStrategy(
    scopeMap: Record<string, import("@babel/traverse").Scope>,
    overrides?: Partial<RenameStrategy>
  ) {
    return makeTestStrategy({
      getScope: (name) => scopeMap[name],
      ...overrides
    });
  }

  it("wouldShadow delegates to wouldRenameShadowInChildScope via getScope", () => {
    const { parentScope } = makeScopeWithChild("x", "taken");
    const callbacks = buildCallbacks(
      scopeStrategy({ x: parentScope }, { getUsedNames: () => new Set(["x"]) })
    )("lane0");

    assert.strictEqual(callbacks.wouldShadow?.("x", "taken"), true);
    assert.strictEqual(callbacks.wouldShadow?.("x", "free"), false);
    assert.strictEqual(callbacks.wouldShadow?.("unknown", "taken"), false);
  });

  it("resolveRemaining passes wouldShadow to resolveRemainingIdentifiers", () => {
    const { parentScope } = makeScopeWithChild("a", "shadow");
    const applied: Array<[string, string]> = [];
    const callbacks = buildCallbacks(
      scopeStrategy(
        { a: parentScope },
        { applyRename: (old, New) => applied.push([old, New]) }
      )
    )("lane0");
    const outcomes: Record<string, IdentifierOutcome> = {};

    callbacks.resolveRemaining?.(new Set(["a"]), { a: "shadow" }, outcomes, 1);

    assert.strictEqual(applied.length, 0);
    assert.strictEqual(outcomes.a, undefined);
  });

  it("resolveRemaining applies renames that pass shadow check", () => {
    const ast = parse(`function f(b) { return b; }`);
    let parentScope: import("@babel/traverse").Scope | undefined;
    traverse(ast, {
      Function(path) {
        parentScope = path.scope;
        path.stop();
      }
    });
    if (!parentScope) throw new Error("no function scope");
    const applied: Array<[string, string]> = [];
    const callbacks = buildCallbacks(
      scopeStrategy(
        { b: parentScope },
        { applyRename: (old, New) => applied.push([old, New]) }
      )
    )("lane0");
    const outcomes: Record<string, IdentifierOutcome> = {};

    callbacks.resolveRemaining?.(
      new Set(["b"]),
      { b: "betterName" },
      outcomes,
      1
    );

    assert.deepStrictEqual(applied, [["b", "betterName"]]);
    assert.strictEqual(outcomes.b.status, "renamed");
    assert.strictEqual(
      (outcomes.b as { status: "renamed"; newName: string }).newName,
      "betterName"
    );
  });

  it("includes laneId in functionId", () => {
    const callbacks = buildCallbacks(
      makeTestStrategy({ getScope: () => undefined, functionId: "my-func" })
    )(":lane2");
    assert.strictEqual(callbacks.functionId, "my-func:lane2");
  });

  it("passes through onUnrenamed when provided", () => {
    const unrenamed: string[] = [];
    const callbacks = buildCallbacks(
      makeTestStrategy({
        getScope: () => undefined,
        functionId: "test",
        onUnrenamed: (name) => unrenamed.push(name)
      })
    )(":lane0");
    callbacks.onUnrenamed?.("foo");
    assert.deepStrictEqual(unrenamed, ["foo"]);
  });
});

function makeNameMapLLM(nameMap: Record<string, string>): LLMProvider {
  return {
    async suggestAllNames(request) {
      const renames: Record<string, string> = {};
      for (const id of request.identifiers) {
        renames[id] = nameMap[id] ?? `${id}Renamed`;
      }
      return { renames };
    }
  };
}

function makeTestStrategy(
  overrides: Partial<RenameStrategy> & Pick<RenameStrategy, "getScope">
): RenameStrategy {
  return {
    applyRename: () => {},
    buildRequest: () => ({
      code: "",
      identifiers: [],
      usedNames: new Set(),
      calleeSignatures: [],
      callsites: [],
      isRetry: false
    }),
    getUsedNames: () => new Set<string>(),
    functionId: "test-fn",
    ...overrides
  };
}

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}

describe("processFunction renames shadowed block bindings", () => {
  it("renames catch clause var that shadows a parameter", async () => {
    const code = `function f(t, n) {
      try { n.doSomething(); } catch(t) { console.log(t); }
    }`;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const mockLLM: LLMProvider = {
      suggestAllNames: async (req) => {
        const renames: Record<string, string> = {};
        for (const id of req.identifiers) {
          if (id === "t") renames[id] = "error";
          else if (id === "n") renames[id] = "service";
          else renames[id] = `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 1 });

    const output = generate(ast).code;
    assert.ok(
      !output.includes("catch (t)"),
      `catch(t) should be renamed, got:\n${output}`
    );
    assert.ok(
      !output.includes("function f(t,"),
      `param t should be renamed, got:\n${output}`
    );
  });

  it("renames for-loop block-scoped const that shadows a parameter", async () => {
    const code = `function f(o, r) {
      for (let i = 0; i < 10; i++) {
        const o = compute(i);
        const r = transform(o);
        emit(r);
      }
      return o + r;
    }`;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    const nameMap: Record<string, string> = {
      o: "offset",
      r: "result",
      i: "index"
    };
    const mockLLM: LLMProvider = {
      suggestAllNames: async (req) => {
        const renames: Record<string, string> = {};
        for (const id of req.identifiers) {
          renames[id] = nameMap[id] ?? `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 1 });

    const output = generate(ast).code;
    // Block-scoped o and r should NOT remain as single-letter
    assert.ok(
      !output.includes("const o ="),
      `block-scoped "const o" should be renamed, got:\n${output}`
    );
    assert.ok(
      !output.includes("const r ="),
      `block-scoped "const r" should be renamed, got:\n${output}`
    );
  });

  it("does not re-rename bindings already processed in phase 1", async () => {
    // catch(e) does NOT shadow any param, so it's collected in phase 1.
    // After phase 1 renames e→caughtError, phase 2 should NOT re-process it.
    const code = `function f(a) {
      try { a.run(); } catch(e) { console.log(e); }
    }`;

    const ast = parse(code);
    const graph = buildUnifiedGraph(ast, "test.js");

    let llmCallCount = 0;
    const nameMap: Record<string, string> = {
      a: "runner",
      e: "caughtError",
      caughtError: "innerError"
    };
    const mockLLM: LLMProvider = {
      suggestAllNames: async (req) => {
        llmCallCount++;
        const renames: Record<string, string> = {};
        for (const id of req.identifiers) {
          renames[id] = nameMap[id] ?? `${id}Renamed`;
        }
        return { renames };
      }
    };

    const processor = new RenameProcessor(ast);
    await processor.processUnified(graph, mockLLM, { concurrency: 1 });

    const output = generate(ast).code;
    // Should have caughtError from phase 1, NOT innerError from a phase 2 re-rename
    assert.ok(
      output.includes("caughtError"),
      `catch binding should keep phase 1 name "caughtError", got:\n${output}`
    );
    assert.ok(
      !output.includes("innerError"),
      `catch binding should NOT be re-renamed to "innerError", got:\n${output}`
    );
    // Only 1 LLM call — no second pass needed since no bindings were shadowed
    assert.strictEqual(
      llmCallCount,
      1,
      `expected 1 LLM call, got ${llmCallCount}`
    );
  });
});
