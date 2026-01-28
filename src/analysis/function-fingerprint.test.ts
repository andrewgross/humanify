import { describe, it } from "node:test";
import assert from "node:assert";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildFunctionGraph } from "./function-graph.js";
import {
  computeCalleeShape,
  serializeCalleeShape,
  calleeShapesEqual,
  buildFullFingerprint,
  hashCalleeShapes,
  makeResolution1Key,
  computeEdgeNgrams,
  computePathNgrams,
} from "./function-fingerprint.js";
import type { StructuralFeatures, CalleeShape } from "./types.js";

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
      propertyAccesses: [],
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
      propertyAccesses: [],
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
      propertyAccesses: [],
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
      propertyAccesses: [],
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
      hasExternalCalls: true,
    };

    const serialized = serializeCalleeShape(shape);

    assert.strictEqual(serialized, "(2,5,complex,true)");
  });

  it("produces different strings for different shapes", () => {
    const shape1: CalleeShape = {
      arity: 1,
      complexity: 1,
      cfgType: "linear",
      hasExternalCalls: false,
    };
    const shape2: CalleeShape = {
      arity: 2,
      complexity: 1,
      cfgType: "linear",
      hasExternalCalls: false,
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
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true },
    ];
    const shapes2: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true },
    ];

    assert.strictEqual(calleeShapesEqual(shapes1, shapes2), true);
  });

  it("returns true for same shapes in different order", () => {
    const shapes1: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true },
    ];
    const shapes2: CalleeShape[] = [
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true },
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
    ];

    assert.strictEqual(calleeShapesEqual(shapes1, shapes2), true);
  });

  it("returns false for different lengths", () => {
    const shapes1: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
    ];
    const shapes2: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
      { arity: 2, complexity: 1, cfgType: "linear", hasExternalCalls: false },
    ];

    assert.strictEqual(calleeShapesEqual(shapes1, shapes2), false);
  });

  it("returns false for different shapes", () => {
    const shapes1: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
    ];
    const shapes2: CalleeShape[] = [
      { arity: 2, complexity: 1, cfgType: "linear", hasExternalCalls: false },
    ];

    assert.strictEqual(calleeShapesEqual(shapes1, shapes2), false);
  });

  it("returns true for empty arrays", () => {
    assert.strictEqual(calleeShapesEqual([], []), true);
  });
});

describe("hashCalleeShapes", () => {
  it("returns 'empty' for empty array", () => {
    assert.strictEqual(hashCalleeShapes([]), "empty");
  });

  it("produces consistent hash for same shapes", () => {
    const shapes: CalleeShape[] = [
      { arity: 1, complexity: 2, cfgType: "branching", hasExternalCalls: false },
    ];

    const hash1 = hashCalleeShapes(shapes);
    const hash2 = hashCalleeShapes(shapes);

    assert.strictEqual(hash1, hash2);
    assert.strictEqual(hash1.length, 8, "Hash should be 8 hex chars");
  });

  it("produces different hashes for different shapes", () => {
    const shapes1: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
    ];
    const shapes2: CalleeShape[] = [
      { arity: 2, complexity: 1, cfgType: "linear", hasExternalCalls: false },
    ];

    assert.notStrictEqual(hashCalleeShapes(shapes1), hashCalleeShapes(shapes2));
  });

  it("produces same hash regardless of order", () => {
    const shapes1: CalleeShape[] = [
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true },
    ];
    const shapes2: CalleeShape[] = [
      { arity: 2, complexity: 3, cfgType: "branching", hasExternalCalls: true },
      { arity: 1, complexity: 1, cfgType: "linear", hasExternalCalls: false },
    ];

    assert.strictEqual(hashCalleeShapes(shapes1), hashCalleeShapes(shapes2));
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
    assert.strictEqual(fingerprint.calleeShapes.length, 2, "Should have 2 callees");

    // One should be linear (simple), one should be complex
    const cfgTypes = fingerprint.calleeShapes.map((s) => s.cfgType).sort();
    assert.ok(cfgTypes.includes("linear"), "Should have linear callee");
    assert.ok(cfgTypes.includes("complex"), "Should have complex callee");
  });

  it("includes callee hashes for resolution 2", () => {
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
    assert.strictEqual(fingerprint.calleeHashes.length, 2, "Should have 2 callee hashes");
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
    assert.strictEqual(fingerprint.calleeShapes.length, 0, "Leaf should have no callees");
  });
});

describe("makeResolution1Key", () => {
  it("combines exactHash with callee shapes hash", () => {
    const code = `
      function a() { b(); }
      function b() { return 1; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const fnMap = new Map(functions.map((f) => [f.sessionId, f]));

    const fnA = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(fnA, "Should find function a");

    const fingerprint = buildFullFingerprint(fnA, fnMap);
    const key = makeResolution1Key(fingerprint);

    assert.ok(key.includes(fingerprint.exactHash), "Key should include exactHash");
    assert.ok(key.includes(":"), "Key should have separator");
  });

  it("produces different keys for different callee shapes", () => {
    const code1 = `
      function a() { simple(); }
      function simple() { return 1; }
    `;
    const code2 = `
      function a() { complex(); }
      function complex(x) { for(;;) { if(x) return x; } }
    `;

    const ast1 = parse(code1);
    const ast2 = parse(code2);
    const functions1 = buildFunctionGraph(ast1, "test1.js");
    const functions2 = buildFunctionGraph(ast2, "test2.js");

    const fnMap1 = new Map(functions1.map((f) => [f.sessionId, f]));
    const fnMap2 = new Map(functions2.map((f) => [f.sessionId, f]));

    const fnA1 = functions1.find((f) => f.sessionId.includes(":2:"));
    const fnA2 = functions2.find((f) => f.sessionId.includes(":2:"));

    assert.ok(fnA1 && fnA2, "Should find both functions");

    const fp1 = buildFullFingerprint(fnA1, fnMap1);
    const fp2 = buildFullFingerprint(fnA2, fnMap2);

    const key1 = makeResolution1Key(fp1);
    const key2 = makeResolution1Key(fp2);

    // The callee shapes are different, so keys should differ
    // (even if exactHash might be same for trivial wrapper)
    const shapesHash1 = key1.split(":")[1];
    const shapesHash2 = key2.split(":")[1];

    assert.notStrictEqual(shapesHash1, shapesHash2, "Shape hashes should differ");
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
      assert.ok(ngram.startsWith(fnA.fingerprint.exactHash), "Should start with caller hash");
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
    assert.ok(ngrams[0].includes("("), "Blurred n-gram should contain shape tuple");
  });

  it("returns empty array for leaf functions", () => {
    const code = `function leaf() { return 1; }`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const ngrams = computeEdgeNgrams(functions[0], "exact");

    assert.strictEqual(ngrams.length, 0);
  });
});

describe("computePathNgrams", () => {
  it("computes trigrams (depth 2)", () => {
    const code = `
      function a() { b(); }
      function b() { c(); }
      function c() { return 1; }
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const fnA = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(fnA, "Should find function a");

    const trigrams = computePathNgrams(fnA, 2);

    assert.strictEqual(trigrams.length, 1, "Should have 1 trigram path");

    const path = trigrams[0];
    const parts = path.split("→");
    assert.strictEqual(parts.length, 3, "Trigram should have 3 nodes");
  });

  it("handles branching call graphs", () => {
    const code = `
      function root() { a(); b(); }
      function a() { leaf(); }
      function b() { leaf(); }
      function leaf() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const root = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(root, "Should find root");

    const trigrams = computePathNgrams(root, 2);

    // root → a → leaf, root → b → leaf (but leaf has same hash, so paths may dedupe)
    // The important thing is we get trigram paths
    assert.ok(trigrams.length >= 1, "Should have at least 1 trigram path");
    trigrams.forEach((path) => {
      const parts = path.split("→");
      assert.ok(parts.length >= 2, "Path should have at least 2 hops");
    });
  });

  it("emits partial paths for shallow graphs", () => {
    const code = `
      function a() { b(); }
      function b() {}
    `;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const fnA = functions.find((f) => f.sessionId.includes(":2:"));
    assert.ok(fnA, "Should find function a");

    // Asking for depth 3 but graph only has depth 2
    const ngrams = computePathNgrams(fnA, 3);

    // Should emit partial path a → b
    assert.ok(ngrams.length >= 1, "Should emit at least partial path");
  });

  it("returns empty for leaf functions", () => {
    const code = `function leaf() {}`;

    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    const ngrams = computePathNgrams(functions[0], 2);

    // Leaf has no callees, so no paths beyond itself
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
    assert.ok(calleeShapesEqual(fp1.calleeShapes!, fp2.calleeShapes!),
      "Callee shapes should be stable when leaf has same structure");

    // Callee HASHES should also be identical (same structure = same hash)
    assert.deepStrictEqual(fp1.calleeHashes, fp2.calleeHashes,
      "Callee hashes should be identical when leaf structure is same");
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
    assert.ok(calleeShapesEqual(fp1.calleeShapes!, fp2.calleeShapes!),
      "Callee shapes should be stable even when leaf content differs");

    // But callee HASHES will differ (string length changed)
    assert.notDeepStrictEqual(fp1.calleeHashes, fp2.calleeHashes,
      "Callee hashes should differ when leaf structure changes");
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
    assert.strictEqual(calleeShapesEqual(fp1.calleeShapes!, fp2.calleeShapes!), false,
      "Callee shapes should change when leaf structure changes");
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
    assert.ok(calleeShapesEqual(fp1.calleeShapes!, fp2.calleeShapes!),
      "Grandparent callee shapes should be stable (1-hop isolation)");

    // Grandparent's exactHash should be identical (its own code didn't change)
    assert.strictEqual(fp1.exactHash, fp2.exactHash,
      "Grandparent exactHash should be identical");
  });
});

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}
