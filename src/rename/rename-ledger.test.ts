import assert from "node:assert";
import { describe, it } from "node:test";
import type * as t from "@babel/types";
import { generate, parseFileAst, traverse } from "../babel-utils.js";
import {
  applyRenameLedger,
  buildRenameLedger,
  type RenameLedger
} from "./rename-ledger.js";

/** Parse `source`, rename each (oldName → newName) via Babel's scope.rename
 * in whichever scope owns the binding (exactly what the pipeline does), and
 * return the mutated AST. */
function renameAll(source: string, renames: Array<[string, string]>): t.File {
  const ast = parseFileAst(source);
  if (!ast) throw new Error("parse failed");
  const want = new Map(renames);
  const pending: Array<[import("@babel/traverse").Scope, string, string]> = [];
  traverse(ast, {
    enter(path) {
      if (path.scope.path !== path) return;
      for (const name of Object.keys(path.scope.bindings)) {
        const to = want.get(name);
        if (to) pending.push([path.scope, name, to]);
      }
    }
  });
  for (const [scope, from, to] of pending) scope.rename(from, to);
  return ast;
}

function outputOf(ast: t.File): string {
  return generate(ast).code;
}

/** The canonical beautified form — what the pipeline's babel plugin emits
 * and the rename passes then run on (a generate() fixed point). */
function canonical(source: string): string {
  const ast = parseFileAst(source);
  if (!ast) throw new Error("parse failed");
  return generate(ast).code;
}

describe("buildRenameLedger", () => {
  it("records one entry per renamed binding with original + final names", () => {
    const source = "function a(b) {\n  return b + 1;\n}\nvar c = a(2);\n";
    const ast = renameAll(source, [
      ["a", "addOne"],
      ["b", "value"],
      ["c", "result"]
    ]);
    const ledger = buildRenameLedger(source, ast);

    const byOriginal = new Map(ledger.entries.map((e) => [e.originalName, e]));
    assert.deepStrictEqual([...byOriginal.keys()].sort(), ["a", "b", "c"]);
    assert.strictEqual(byOriginal.get("a")?.finalName, "addOne");
    assert.strictEqual(byOriginal.get("b")?.finalName, "value");
    assert.strictEqual(byOriginal.get("c")?.finalName, "result");
  });

  it("captures every occurrence (declaration + reads + writes) of a binding", () => {
    const source = "var x = 1;\nx = x + 1;\nx++;\nconsole.log(x);\n";
    const ast = renameAll(source, [["x", "counter"]]);
    const ledger = buildRenameLedger(source, ast);
    const entry = ledger.entries.find((e) => e.originalName === "x");
    assert.ok(entry);
    // decl + (x = ) + (x + 1 read) + (x++) + (log read) = 5 occurrences.
    assert.strictEqual(entry.occurrences.length, 5, JSON.stringify(entry));
    // Each occurrence slices to the original identifier text.
    for (const [start, end] of entry.occurrences) {
      assert.strictEqual(source.slice(start, end), "x");
    }
  });

  it("does not record bindings whose name is unchanged", () => {
    const source = "function keep(a) {\n  return a;\n}\n";
    const ast = renameAll(source, [["a", "value"]]); // keep stays
    const ledger = buildRenameLedger(source, ast);
    assert.deepStrictEqual(
      ledger.entries.map((e) => e.originalName),
      ["a"]
    );
  });

  it("pins the source hash so a mismatched snapshot is detectable", () => {
    const source = "var q = 1;\n";
    const ast = renameAll(source, [["q", "quantity"]]);
    const ledger = buildRenameLedger(source, ast);
    assert.match(ledger.sourceSha256, /^[0-9a-f]{64}$/);
    assert.strictEqual(ledger.version, 1);
  });
});

describe("applyRenameLedger", () => {
  it("replays the ledger onto the source to reproduce the renamed output exactly", () => {
    const source = canonical(
      "function a(b, c) {\n  var d = b + c;\n  return d * 2;\n}\n" +
        "var e = a(1, 2);\ne = e + a(3, 4);\nconsole.log(e);\n"
    );
    const renames: Array<[string, string]> = [
      ["a", "sumDoubled"],
      ["b", "first"],
      ["c", "second"],
      ["d", "total"],
      ["e", "acc"]
    ];
    const ast = renameAll(source, renames);
    const expected = outputOf(ast); // what humanify would emit
    const ledger = buildRenameLedger(source, ast);

    const replayed = applyRenameLedger(source, ledger);
    assert.strictEqual(replayed, expected, `replay:\n${replayed}`);
  });

  it("throws when applied to a source that does not match the ledger hash", () => {
    const source = "var x = 1;\n";
    const ast = renameAll(source, [["x", "count"]]);
    const ledger = buildRenameLedger(source, ast);
    assert.throws(
      () => applyRenameLedger("var y = 1;\n", ledger),
      /source.*match|hash/i
    );
  });

  it("is a no-op for an empty ledger", () => {
    const source = "var unchanged = 1;\n";
    const ledger: RenameLedger = {
      version: 1,
      sourceSha256: buildRenameLedger(source, renameAll(source, []))
        .sourceSha256,
      entries: []
    };
    assert.strictEqual(applyRenameLedger(source, ledger), source);
  });
});

describe("applyRenameLedger — staged (post) renames", () => {
  // The pipeline's post-generate passes (reconcile, deferred sweep) rename the
  // GENERATED output, not the beautified input — a second coordinate space.
  // Each is captured as a `post` stage indexed into the prior stage's output.
  it("chains an output-space stage to reproduce the final output", () => {
    const source = canonical(
      "function a(b) {\n  return b + 1;\n}\nvar c = a(2);\n"
    );
    // Stage 1 (LLM): rename in the beautified-input space.
    const ast1 = renameAll(source, [
      ["a", "addOne"],
      ["b", "value"],
      ["c", "result"]
    ]);
    const stage1Output = outputOf(ast1);
    const base = buildRenameLedger(source, ast1);

    // Stage 2 (reconcile): rename in the STAGE-1 OUTPUT space.
    const ast2 = renameAll(stage1Output, [
      ["addOne", "increment"],
      ["result", "total"]
    ]);
    const finalOutput = outputOf(ast2);
    const stage2 = buildRenameLedger(stage1Output, ast2);

    const ledger: RenameLedger = {
      ...base,
      post: [{ sourceSha256: stage2.sourceSha256, entries: stage2.entries }]
    };
    // The single-stage ledger reproduces only stage 1; the chained ledger
    // must reproduce the final output.
    assert.notStrictEqual(finalOutput, stage1Output);
    assert.strictEqual(applyRenameLedger(source, ledger), finalOutput);
  });

  it("verifies each stage's hash — a tampered intermediate throws", () => {
    const source = canonical("var x = 1;\nx;\n");
    const ast1 = renameAll(source, [["x", "count"]]);
    const base = buildRenameLedger(source, ast1);
    const ledger: RenameLedger = {
      ...base,
      post: [{ sourceSha256: "deadbeef".repeat(8), entries: [] }]
    };
    assert.throws(() => applyRenameLedger(source, ledger), /source.*match/i);
  });
});
