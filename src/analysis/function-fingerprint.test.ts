import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import {
  buildFullFingerprint,
  calleeShapesEqual,
  computeCalleeShape,
  computeEdgeNgrams,
  computeShingleSet,
  extractMemberKey,
  jaccardSimilarity,
  serializeCalleeShape
} from "./function-fingerprint.js";
import { buildFunctionGraph } from "./function-graph.js";
import type { CalleeShape, StructuralFeatures } from "./types.js";

describe("computeCalleeShape", () => {
  it("classifies linear functions correctly", () => {
    const features: StructuralFeatures = {
      arity: 2,
      hasRestParam: false,
      returnCount: 1,
      complexity: 1,
      cfgShape: "ret",
      loopCount: 0,
      branchCount: 0,
      tryCount: 0,
      stringLiterals: [],
      numericLiterals: [],
      externalCalls: [],
      propertyAccesses: []
    };

    const shape = computeCalleeShape(features);

    assert.strictEqual(shape.arity, 2);
    assert.strictEqual(shape.complexity, 1);
    assert.strictEqual(shape.cfgType, "linear");
    assert.strictEqual(shape.hasExternalCalls, false);
  });

  it("classifies branching functions correctly", () => {
    const features: StructuralFeatures = {
      arity: 1,
      hasRestParam: false,
      returnCount: 2,
      complexity: 3,
      cfgShape: "if-ret-else-ret",
      loopCount: 0,
      branchCount: 2,
      tryCount: 0,
      stringLiterals: [],
      numericLiterals: [],
      externalCalls: [],
      propertyAccesses: []
    };

    const shape = computeCalleeShape(features);

    assert.strictEqual(shape.cfgType, "branching");
  });

  it("classifies looping functions correctly", () => {
    const features: StructuralFeatures = {
      arity: 1,
      hasRestParam: false,
      returnCount: 1,
      complexity: 2,
      cfgShape: "loop-ret",
      loopCount: 1,
      branchCount: 0,
      tryCount: 0,
      stringLiterals: [],
      numericLiterals: [],
      externalCalls: [],
      propertyAccesses: []
    };

    const shape = computeCalleeShape(features);

    assert.strictEqual(shape.cfgType, "looping");
  });

  it("classifies complex functions correctly", () => {
    const features: StructuralFeatures = {
      arity: 2,
      hasRestParam: false,
      returnCount: 3,
      complexity: 5,
      cfgShape: "loop-if-ret-ret",
      loopCount: 1,
      branchCount: 1,
      tryCount: 0,
      stringLiterals: [],
      numericLiterals: [],
      externalCalls: ["fetch"],
      propertyAccesses: []
    };

    const shape = computeCalleeShape(features);

    assert.strictEqual(shape.cfgType, "complex");
    assert.strictEqual(shape.hasExternalCalls, true);
  });
});

describe("serializeCalleeShape", () => {
  it("produces deterministic string representation", () => {
    const shape: CalleeShape = {
      arity: 2,
      complexity: 5,
      cfgType: "complex",
      hasExternalCalls: true
    };

    const serialized = serializeCalleeShape(shape);

    assert.strictEqual(serialized, "(2,5,complex,true)");
  });

  it("produces different strings for different shapes", () => {
    const shape1: CalleeShape = {
      arity: 1,
      complexity: 1,
      cfgType: "linear",
      hasExternalCalls: false
    };
    const shape2: CalleeShape = {
      arity: 2,
      complexity: 1,
      cfgType: "linear",
      hasExternalCalls: false
    };

    assert.notStrictEqual(
      serializeCalleeShape(shape1),
      serializeCalleeShape(shape2)
    );
  });
});

describe("calleeShapesEqual", () => {
  it("returns true for identical shape arrays", () => {
    const shapes1: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true }
    ];
    const shapes2: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true }
    ];

    assert.strictEqual(calleeShapesEqual(shapes1, shapes2), true);
  });

  it("returns true for same shapes in different order", () => {
    const shapes1: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true }
    ];
    const shapes2: CalleeShape[] = [
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true },
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false }
    ];

    assert.strictEqual(calleeShapesEqual(shapes1, shapes2), true);
  });

  it("returns false for different lengths", () => {
    const shapes1: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false }
    ];
    const shapes2: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
      { arity: 2, complexity: 1, cfgType: "linear", hasExternalCalls: false }
    ];

    assert.strictEqual(calleeShapesEqual(shapes1, shapes2), false);
  });

  it("returns false for different shapes", () => {
    const shapes1: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false }
    ];
    const shapes2: CalleeShape[] = [
      { arity: 2, complexity: 1, cfgType: "linear", hasExternalCalls: false }
    ];

    assert.strictEqual(calleeShapesEqual(shapes1, shapes2), false);
  });

  it("returns true for empty arrays", () => {
    assert.strictEqual(calleeShapesEqual([], []), true);
  });
});

describe("buildFullFingerprint", () => {
  it("includes callee shapes for functions with callees", () => {
    const code = `
      function caller() {
        simple();
        complex();
      }
      function simple() { return 1; }
      function complex(x) {
        for (let i = 0; i < x; i++) {
          if (i > 5) return i;
        }
        return 0;
      }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const fnMap = new Map(functions.map((f) => [f.sessionId, f]));

    const caller = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(caller, "Should find caller");

    const fingerprint = buildFullFingerprint(caller, fnMap);

    assert.ok(fingerprint.calleeShapes, "Should have callee shapes");
    assert.strictEqual(
      fingerprint.calleeShapes.length,
      2,
      "Should have 2 callees"
    );

    // One should be linear (simple), one should be complex
    const cfgTypes = fingerprint.calleeShapes.map((s) => s.cfgType).sort();
    assert.ok(cfgTypes.includes("linear"), "Should have linear callee");
    assert.ok(cfgTypes.includes("complex"), "Should have complex callee");
  });

  it("includes callee hashes for calleeHashes stage", () => {
    const code = `
      function a() { b(); c(); }
      function b() {}
      function c() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const fnMap = new Map(functions.map((f) => [f.sessionId, f]));

    const fnA = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(fnA, "Should find function a");

    const fingerprint = buildFullFingerprint(fnA, fnMap);

    assert.ok(fingerprint.calleeHashes, "Should have callee hashes");
    assert.strictEqual(
      fingerprint.calleeHashes.length,
      2,
      "Should have 2 callee hashes"
    );
    fingerprint.calleeHashes.forEach((hash) => {
      assert.strictEqual(hash.length, 16, "Each hash should be 16 hex chars");
    });
  });

  it("includes two-hop shapes", () => {
    const code = `
      function root() { middle(); }
      function middle() { leaf(); }
      function leaf() { return 42; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const fnMap = new Map(functions.map((f) => [f.sessionId, f]));

    const root = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(root, "Should find root");

    const fingerprint = buildFullFingerprint(root, fnMap);

    assert.ok(fingerprint.twoHopShapes, "Should have two-hop shapes");
    assert.strictEqual(
      fingerprint.twoHopShapes.length,
      1,
      "Should have 1 two-hop shape (leaf)"
    );
  });

  it("has empty calleeShapes for leaf functions", () => {
    const code = `
      function leaf() { return console.log("hi"); }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const fnMap = new Map(functions.map((f) => [f.sessionId, f]));

    const leaf = functions[0];
    const fingerprint = buildFullFingerprint(leaf, fnMap);

    assert.ok(fingerprint.calleeShapes, "Should have calleeShapes array");
    assert.strictEqual(
      fingerprint.calleeShapes.length,
      0,
      "Leaf should have no callees"
    );
  });
});

describe("computeEdgeNgrams", () => {
  it("computes exact edge n-grams", () => {
    const code = `
      function a() { b(); c(); }
      function b() {}
      function c() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const fnA = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(fnA, "Should find function a");

    const ngrams = computeEdgeNgrams(fnA, "exact");

    assert.strictEqual(ngrams.length, 2, "Should have 2 edge n-grams");
    ngrams.forEach((ngram) => {
      assert.ok(ngram.includes("→"), "N-gram should contain arrow");
      assert.ok(
        ngram.startsWith(fnA.fingerprint.structuralHash),
        "Should start with caller hash"
      );
    });
  });

  it("computes blurred edge n-grams", () => {
    const code = `
      function a() { b(); }
      function b() { return 1; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const fnA = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(fnA, "Should find function a");

    const ngrams = computeEdgeNgrams(fnA, "blurred");

    assert.strictEqual(ngrams.length, 1, "Should have 1 edge n-gram");
    assert.ok(
      ngrams[0].includes("("),
      "Blurred n-gram should contain shape tuple"
    );
  });

  it("returns empty array for leaf functions", () => {
    const code = `function leaf() { return 1; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const ngrams = computeEdgeNgrams(functions[0], "exact");

    assert.strictEqual(ngrams.length, 0);
  });
});

describe("cascade behavior", () => {
  it("callee shape is stable when leaf implementation changes (same structure)", () => {
    // Version 1: leaf returns "hello"
    const code1 = `
      function caller() { return leaf(); }
      function leaf() { return "hello"; }
    `;

    // Version 2: leaf returns "world" (different string, but same length = same structure)
    const code2 = `
      function caller() { return leaf(); }
      function leaf() { return "world"; }
    `;

    const ast1 = parse(code1);
    const ast2 = parse(code2);

    const functions1 = buildFunctionGraph(ast1, "test.js");
    const functions2 = buildFunctionGraph(ast2, "test.js");

    const fnMap1 = new Map(functions1.map((f) => [f.sessionId, f]));
    const fnMap2 = new Map(functions2.map((f) => [f.sessionId, f]));

    const caller1 = functions1.find((f) => f.sessionId.includes(":2:"));
    const caller2 = functions2.find((f) => f.sessionId.includes(":2:"));

    assert.ok(caller1 && caller2, "Should find callers");

    const fp1 = buildFullFingerprint(caller1, fnMap1);
    const fp2 = buildFullFingerprint(caller2, fnMap2);

    // Callee SHAPES should be identical (arity=0, complexity=1, linear, no external)
    assert.ok(fp1.calleeShapes != null, "fp1 should have calleeShapes");
    assert.ok(fp2.calleeShapes != null, "fp2 should have calleeShapes");
    assert.ok(
      calleeShapesEqual(fp1.calleeShapes, fp2.calleeShapes),
      "Callee shapes should be stable when leaf has same structure"
    );

    // Callee HASHES should also be identical (same structure = same hash)
    assert.deepStrictEqual(
      fp1.calleeHashes,
      fp2.calleeHashes,
      "Callee hashes should be identical when leaf structure is same"
    );
  });

  it("callee hash changes when leaf content changes structurally", () => {
    // Version 1: leaf returns short string
    const code1 = `
      function caller() { return leaf(); }
      function leaf() { return "hi"; }
    `;

    // Version 2: leaf returns longer string (different string length = different structure)
    const code2 = `
      function caller() { return leaf(); }
      function leaf() { return "hello world"; }
    `;

    const ast1 = parse(code1);
    const ast2 = parse(code2);

    const functions1 = buildFunctionGraph(ast1, "test.js");
    const functions2 = buildFunctionGraph(ast2, "test.js");

    const fnMap1 = new Map(functions1.map((f) => [f.sessionId, f]));
    const fnMap2 = new Map(functions2.map((f) => [f.sessionId, f]));

    const caller1 = functions1.find((f) => f.sessionId.includes(":2:"));
    const caller2 = functions2.find((f) => f.sessionId.includes(":2:"));

    assert.ok(caller1 && caller2, "Should find callers");

    const fp1 = buildFullFingerprint(caller1, fnMap1);
    const fp2 = buildFullFingerprint(caller2, fnMap2);

    // Callee SHAPES should still be identical (both are arity=0, complexity=1, linear)
    assert.ok(fp1.calleeShapes != null, "fp1 should have calleeShapes");
    assert.ok(fp2.calleeShapes != null, "fp2 should have calleeShapes");
    assert.ok(
      calleeShapesEqual(fp1.calleeShapes, fp2.calleeShapes),
      "Callee shapes should be stable even when leaf content differs"
    );

    // But callee HASHES will differ (string length changed)
    assert.notDeepStrictEqual(
      fp1.calleeHashes,
      fp2.calleeHashes,
      "Callee hashes should differ when leaf structure changes"
    );
  });

  it("callee shape changes when leaf structure changes", () => {
    // Version 1: simple leaf
    const code1 = `
      function caller() { return leaf(); }
      function leaf() { return 1; }
    `;

    // Version 2: complex leaf with loop
    const code2 = `
      function caller() { return leaf(); }
      function leaf() { for(let i=0; i<10; i++) {} return 1; }
    `;

    const ast1 = parse(code1);
    const ast2 = parse(code2);

    const functions1 = buildFunctionGraph(ast1, "test.js");
    const functions2 = buildFunctionGraph(ast2, "test.js");

    const fnMap1 = new Map(functions1.map((f) => [f.sessionId, f]));
    const fnMap2 = new Map(functions2.map((f) => [f.sessionId, f]));

    const caller1 = functions1.find((f) => f.sessionId.includes(":2:"));
    const caller2 = functions2.find((f) => f.sessionId.includes(":2:"));

    assert.ok(caller1 && caller2, "Should find callers");

    const fp1 = buildFullFingerprint(caller1, fnMap1);
    const fp2 = buildFullFingerprint(caller2, fnMap2);

    // Callee shapes should differ (linear vs looping)
    assert.ok(fp1.calleeShapes != null, "fp1 should have calleeShapes");
    assert.ok(fp2.calleeShapes != null, "fp2 should have calleeShapes");
    assert.strictEqual(
      calleeShapesEqual(fp1.calleeShapes, fp2.calleeShapes),
      false,
      "Callee shapes should change when leaf structure changes"
    );
  });

  it("grandparent is stable when leaf changes (2-hop isolation)", () => {
    // Test that changes don't cascade beyond 1 hop for blurred shapes
    const code1 = `
      function grandparent() { return parent(); }
      function parent() { return leaf(); }
      function leaf() { return 1; }
    `;

    const code2 = `
      function grandparent() { return parent(); }
      function parent() { return leaf(); }
      function leaf() { return 2; }
    `;

    const ast1 = parse(code1);
    const ast2 = parse(code2);

    const functions1 = buildFunctionGraph(ast1, "test.js");
    const functions2 = buildFunctionGraph(ast2, "test.js");

    const fnMap1 = new Map(functions1.map((f) => [f.sessionId, f]));
    const fnMap2 = new Map(functions2.map((f) => [f.sessionId, f]));

    const gp1 = functions1.find((f) => f.sessionId.includes(":2:"));
    const gp2 = functions2.find((f) => f.sessionId.includes(":2:"));

    assert.ok(gp1 && gp2, "Should find grandparents");

    const fp1 = buildFullFingerprint(gp1, fnMap1);
    const fp2 = buildFullFingerprint(gp2, fnMap2);

    // Grandparent's direct callee shapes should be identical
    // (parent's shape didn't change, only leaf's content)
    assert.ok(fp1.calleeShapes != null, "fp1 should have calleeShapes");
    assert.ok(fp2.calleeShapes != null, "fp2 should have calleeShapes");
    assert.ok(
      calleeShapesEqual(fp1.calleeShapes, fp2.calleeShapes),
      "Grandparent callee shapes should be stable (1-hop isolation)"
    );

    // Grandparent's structuralHash should be identical (its own code didn't change)
    assert.strictEqual(
      fp1.structuralHash,
      fp2.structuralHash,
      "Grandparent structuralHash should be identical"
    );
  });
});

/**
 * CHARACTERIZATION of a bias in `computeShingleSet`, not a spec for it.
 *
 * Every edge n-gram is prefixed with the function's OWN structuralHash. Inside a
 * hash bucket that prefix is constant, so it costs nothing. But
 * `shinglesCorroborate` compares CLOSE-match pairs, which by construction did
 * not pair by hash — so their prefixes differ, no edge n-gram can ever
 * intersect, and every distinct callee SHAPE (they dedupe in the Set) adds two
 * tokens to the union and none to the intersection.
 *
 * With S distinct callee shapes and F fully-agreeing feature tokens the ceiling
 * is F / (F + 2S). These tests pin the effect; whether it changes any real
 * verdict is measured separately in experiments/053-shingle-audit.
 */
describe("computeShingleSet: self-hash prefix on edge n-grams", () => {
  const HELPERS = `
function helper1(a) { return a.map(x => x); }
function helper2(a) { return a.filter(x => x); }
`;
  const targetCode = (extra: string) => `${HELPERS}
function target(items) {
  console.log("processing items now");
  ${extra}
  const r = helper1(items);
  const s = helper2(items);
  return r.concat(s).length;
}`;

  function shinglesOfTarget(extra: string): {
    set: Set<string>;
    hash: string;
  } {
    const fns = buildFunctionGraph(parse(targetCode(extra)), "test.js");
    const map = new Map(fns.map((f) => [f.sessionId, f]));
    for (const f of fns) buildFullFingerprint(f, map);
    const fn = fns.find(
      (f) =>
        f.path.node.type === "FunctionDeclaration" &&
        (f.path.node as t.FunctionDeclaration).id?.name === "target"
    );
    assert.ok(fn, "target function should be in the graph");
    return { set: computeShingleSet(fn), hash: fn.fingerprint.structuralHash };
  }

  /** Edge n-grams with the self-hash replaced by a constant. */
  const unprefixed = (s: Set<string>) =>
    new Set(
      [...s].map((tok) =>
        tok.includes("\u2192") ? `edge:${tok.split("\u2192")[1]}` : tok
      )
    );

  it("costs similarity for a pair whose features agree completely", () => {
    // One added statement flips the hash and nothing else: identical literals,
    // identical external calls, identical property accesses, identical callees.
    const prior = shinglesOfTarget("");
    const fresh = shinglesOfTarget("const t = items.length;");
    assert.notStrictEqual(
      prior.hash,
      fresh.hash,
      "the added statement must flip the structural hash"
    );

    const asIs = jaccardSimilarity(prior.set, fresh.set);
    const constPrefix = jaccardSimilarity(
      unprefixed(prior.set),
      unprefixed(fresh.set)
    );
    assert.strictEqual(constPrefix, 1, "every feature token agrees");
    assert.ok(
      asIs < constPrefix,
      `self-hash prefix should cost similarity: ${asIs} vs ${constPrefix}`
    );
  });

  it("cannot share a single edge n-gram once the hashes differ", () => {
    const prior = shinglesOfTarget("");
    const fresh = shinglesOfTarget("const t = items.length;");
    const edges = (s: Set<string>) =>
      [...s].filter((tok) => tok.includes("\u2192"));
    const priorEdges = new Set(edges(prior.set));
    assert.ok(priorEdges.size > 0, "the target has internal callees");
    for (const e of edges(fresh.set)) {
      assert.ok(!priorEdges.has(e), `edge n-gram ${e} must not intersect`);
    }
  });
});

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}

/**
 * exp079 Task 1 — the property name that is already there.
 *
 * zustand's `getState` and `getInitialState` minify to `A=()=>d` and `()=>D`.
 * Once identifiers are masked — which they must be — both are "a function of
 * no arguments returning one variable": no calls, no literals, no branches,
 * nothing to tell them apart. The fixture harness has scored this a known
 * shortfall for as long as it has existed (71% and 83% against ground truth).
 *
 * But the distinguishing evidence IS in the source:
 *
 *     B = { setState: z, getState: A, getInitialState: () => D }
 *
 * `getInitialState` is written directly as a property value and we read its
 * key. `getState` is assigned to a variable first and only then used as a
 * property — one hop of indirection, and we drop it.
 *
 * This is NOT an ambiguity fix. Nothing was ambiguous; we were not reading
 * what was there.
 */
describe("extractMemberKey — through a variable (exp079)", () => {
  const keyOf = (code: string, sessionIdPart: string) => {
    const functions = buildFunctionGraph(parse(code), "test.js");
    const fn = functions.find((f) => f.sessionId.includes(sessionIdPart));
    assert.ok(fn, `no function at ${sessionIdPart}`);
    return extractMemberKey(fn);
  };

  it("reads the key when the function reaches the property via a variable", () => {
    // The zustand shape exactly.
    const code = `
      const store = () => {
        let state, initial;
        const getState = () => state;
        const api = { getState: getState, getInitialState: () => initial };
        return api;
      };
    `;
    assert.strictEqual(keyOf(code, ":4:"), "getState");
  });

  it("still reads a directly-assigned property value", () => {
    const code = `const api = { getInitialState: () => 1 };`;
    assert.strictEqual(keyOf(code, ":1:"), "getInitialState");
  });

  it("abstains when the variable is used under two different keys", () => {
    // Two keys is not evidence, it is a contradiction — and a wrong member
    // key is worse than none, because the cascade trusts it over shapes.
    const code = `
      const f = () => 1;
      const a = { alpha: f };
      const b = { beta: f };
    `;
    assert.strictEqual(keyOf(code, ":2:"), undefined);
  });

  it("abstains when the variable is never used as a property", () => {
    const code = `
      const f = () => 1;
      f();
    `;
    assert.strictEqual(keyOf(code, ":2:"), undefined);
  });
});

describe("extractMemberKey — assignment form (exp079 census)", () => {
  /** Select by what the test MEANS — the function assigned to a variable —
   * rather than by a line-number-derived session id, which silently selects
   * nothing when the fixture is re-indented. */
  const keyOfAssigned = (code: string) => {
    const functions = buildFunctionGraph(parse(code), "test.js");
    const fn = functions.find(
      (f) => f.path.parent.type === "AssignmentExpression"
    );
    assert.ok(fn, "no function assigned to a variable in the fixture");
    return extractMemberKey(fn);
  };

  it("reads the key when the function is ASSIGNED to a declared variable", () => {
    // The census found 95 sites of `someVar = function(){}` in a 600-file
    // sample, 20 of which reach a property with a unique key — and it is the
    // shape bun's own lazy-init emits (`var x; … x = …` inside the init).
    // Examples measured: confirmHandler -> onConfirm, toolSelectHandler ->
    // onSelect. The property name is source-derived and stable; the variable
    // name is ours and is not.
    const code = `
      function build() {
        let confirmHandler;
        confirmHandler = () => 1;
        return { onConfirm: confirmHandler };
      }
    `;
    assert.strictEqual(keyOfAssigned(code), "onConfirm");
  });

  it("abstains when the assigned variable reaches two different keys", () => {
    const code = `
      function build() {
        let h;
        h = () => 1;
        return [{ onA: h }, { onB: h }];
      }
    `;
    assert.strictEqual(keyOfAssigned(code), undefined);
  });
});
