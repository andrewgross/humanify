import assert from "node:assert/strict";
import { test } from "node:test";
import * as t from "@babel/types";
import type { RefGraph } from "./graph.js";
import {
  type Ledger,
  buildLedger,
  inherit,
  referenceAffinity,
  reverseRefsOf,
  textualLocality
} from "./stability.js";

/** `function <name>() {}` — one top-level binding. */
function fn(name: string): t.FunctionDeclaration {
  return t.functionDeclaration(t.identifier(name), [], t.blockStatement([]));
}

test("full-ledger inherit reproduces the assignment exactly (self-stability)", () => {
  const body = [fn("a"), fn("b"), fn("c")];
  const order = ["f0.js", "f0.js", "f1.js"];
  const ledger = buildLedger(body, order);
  const { order: got, stats } = inherit(body, ledger, textualLocality("x.js"));
  assert.deepEqual(got, order);
  assert.equal(stats.inherited, 3);
  assert.equal(stats.placed, 0);
});

test("inherited statements never move, whatever placeNew returns", () => {
  const body = [fn("a"), fn("b"), fn("new")];
  const ledger: Ledger = {
    nameToFiles: new Map([
      ["a", ["A.js"]],
      ["b", ["B.js"]]
    ])
  };
  const { order } = inherit(body, ledger, () => "GARBAGE.js");
  assert.equal(order[0], "A.js"); // a inherits
  assert.equal(order[1], "B.js"); // b inherits
  assert.equal(order[2], "GARBAGE.js"); // only the new binding is placed
});

test("referenceAffinity places a new binding with its references, not its textual neighbor", () => {
  // a→A, b→B, c→B inherited; d is new and references a (index 0).
  const body = [fn("a"), fn("b"), fn("c"), fn("d")];
  const ledger: Ledger = {
    nameToFiles: new Map([
      ["a", ["A.js"]],
      ["b", ["B.js"]],
      ["c", ["B.js"]]
    ])
  };
  const g: RefGraph = {
    refs: [
      new Set<number>(),
      new Set<number>(),
      new Set<number>(),
      new Set([0])
    ],
    idf: [1, 1, 1, 1],
    lines: [1, 1, 1, 1],
    n: 4
  };
  const rev = reverseRefsOf(g);

  const affinity = inherit(body, ledger, referenceAffinity(g, rev, "root.js"));
  assert.equal(affinity.order[3], "A.js"); // grew a's cluster — correct

  const local = inherit(body, ledger, textualLocality("root.js"));
  assert.equal(local.order[3], "B.js"); // followed neighbor c — wrong cluster
});

test("referenceAffinity falls back to locality when nothing it touches is placed", () => {
  const body = [fn("a"), fn("new")];
  const ledger: Ledger = { nameToFiles: new Map([["a", ["A.js"]]]) };
  const g: RefGraph = {
    refs: [new Set<number>(), new Set<number>()],
    idf: [1, 1],
    lines: [1, 1],
    n: 2
  };
  const { order } = inherit(
    body,
    ledger,
    referenceAffinity(g, reverseRefsOf(g), "root.js")
  );
  assert.equal(order[1], "A.js"); // neighbor a
});
