import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  assignEntry,
  collectLedger,
  summarize,
  verifyComplete
} from "./ledger.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  return ast;
}

describe("collectLedger", () => {
  it("collects all top-level nodes from a simple file", () => {
    const code = `
      function a() {}
      function b() {}
      const x = 1;
    `;
    const ast = parse(code);
    const ledger = collectLedger(ast, "test.js");

    assert.strictEqual(
      ledger.entries.size,
      3,
      "Should collect 3 top-level nodes"
    );
  });

  it("collects function declarations, variable declarations, class declarations, expression statements", () => {
    const code = `
      function foo() {}
      const bar = 42;
      class Baz {}
      console.log("hello");
    `;
    const ast = parse(code);
    const ledger = collectLedger(ast, "test.js");

    assert.strictEqual(ledger.entries.size, 4);

    const types = Array.from(ledger.entries.values())
      .map((e) => e.type)
      .sort();
    assert.deepStrictEqual(types, [
      "ClassDeclaration",
      "ExpressionStatement",
      "FunctionDeclaration",
      "VariableDeclaration"
    ]);
  });

  it("assigns entries and tracks them", () => {
    const code = `
      function a() {}
      function b() {}
    `;
    const ast = parse(code);
    const ledger = collectLedger(ast, "test.js");

    const entries = Array.from(ledger.entries.keys());
    assignEntry(ledger, entries[0], "cluster-a.js");
    assignEntry(ledger, entries[1], "cluster-b.js");

    assert.strictEqual(
      ledger.entries.get(entries[0])!.outputFile,
      "cluster-a.js"
    );
    assert.strictEqual(
      ledger.entries.get(entries[1])!.outputFile,
      "cluster-b.js"
    );
  });

  it("verifyComplete passes when all assigned", () => {
    const code = `
      function a() {}
      const b = 1;
    `;
    const ast = parse(code);
    const ledger = collectLedger(ast, "test.js");

    for (const id of ledger.entries.keys()) {
      assignEntry(ledger, id, "output.js");
    }

    // Should not throw
    verifyComplete(ledger);
  });

  it("verifyComplete throws with details when entries are unassigned", () => {
    const code = `
      function a() {}
      const b = 1;
      class C {}
    `;
    const ast = parse(code);
    const ledger = collectLedger(ast, "test.js");

    // Only assign the first entry
    const firstId = Array.from(ledger.entries.keys())[0];
    assignEntry(ledger, firstId, "output.js");

    assert.throws(
      () => verifyComplete(ledger),
      (err: Error) => {
        assert.ok(
          err.message.includes("2"),
          "Should mention count of unassigned entries"
        );
        return true;
      }
    );
  });

  it("handles multiple input files without ID collisions", () => {
    const code1 = `function a() {}`;
    const code2 = `function a() {}`;

    const ast1 = parse(code1);
    const ast2 = parse(code2);

    const ledger1 = collectLedger(ast1, "file1.js");
    const ledger2 = collectLedger(ast2, "file2.js");

    // Merge entries from both ledgers
    for (const [id, entry] of ledger2.entries) {
      ledger1.entries.set(id, entry);
    }

    // Should have 2 entries (different files)
    assert.strictEqual(ledger1.entries.size, 2);

    const ids = Array.from(ledger1.entries.keys());
    assert.notStrictEqual(
      ids[0],
      ids[1],
      "IDs should be different for different files"
    );
  });

  it("summarize returns correct stats", () => {
    const code = `
      function a() {}
      const b = 1;
      class C {}
    `;
    const ast = parse(code);
    const ledger = collectLedger(ast, "test.js");

    const entries = Array.from(ledger.entries.keys());
    assignEntry(ledger, entries[0], "a.js");
    assignEntry(ledger, entries[1], "a.js");
    assignEntry(ledger, entries[2], "b.js");

    const stats = summarize(ledger);
    assert.strictEqual(stats.totalEntries, 3);
    assert.strictEqual(stats.assignedEntries, 3);
    assert.strictEqual(stats.unassignedEntries, 0);
    assert.strictEqual(stats.outputFiles, 2);
  });
});
