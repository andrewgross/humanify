import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import {
  buildFingerprintIndex,
  getMatchStats,
  matchFunctions
} from "./fingerprint-index.js";
import { buildFunctionGraph } from "./function-graph.js";
import type { FunctionNode } from "./types.js";

describe("buildFingerprintIndex", () => {
  it("indexes all functions by structuralHash", () => {
    // Use structurally different functions to get unique hashes
    const code = `
      function a() { return "hello"; }
      function b(x) { return x + 1; }
      function c(x, y) { if (x) return y; return null; }
    `;

    const functions = buildFunctionGraphAsMap(code);
    const index = buildFingerprintIndex(functions);

    assert.strictEqual(
      index.fingerprints.size,
      3,
      "Should have 3 fingerprints"
    );
    assert.strictEqual(
      index.byStructuralHash.size,
      3,
      "Should have 3 unique hashes"
    );
  });

  it("groups duplicate structures under same structuralHash", () => {
    const code = `
      function a() { return 1; }
      function b() { return 1; }
    `;

    const functions = buildFunctionGraphAsMap(code);
    const index = buildFingerprintIndex(functions);

    // Both functions have identical structure, so same structuralHash
    assert.strictEqual(
      index.byStructuralHash.size,
      1,
      "Should have 1 unique hash"
    );

    const hashEntries = [...index.byStructuralHash.values()][0];
    assert.strictEqual(
      hashEntries.length,
      2,
      "Hash should map to 2 sessionIds"
    );
  });
});

describe("matchFunctions", () => {
  it("matches identical functions across versions", () => {
    const codeV1 = `
      function add(a, b) { return a + b; }
      function sub(a, b) { return a - b; }
    `;
    const codeV2 = `
      function add(a, b) { return a + b; }
      function sub(a, b) { return a - b; }
    `;

    const v1Functions = buildFunctionGraphAsMap(codeV1);
    const v2Functions = buildFunctionGraphAsMap(codeV2);

    const v1Index = buildFingerprintIndex(v1Functions);
    const v2Index = buildFingerprintIndex(v2Functions);

    const result = matchFunctions(v1Index, v2Index);

    assert.strictEqual(result.matches.size, 2, "Should match both functions");
    assert.strictEqual(result.unmatched.length, 0, "Should have no unmatched");
    assert.strictEqual(result.ambiguous.size, 0, "Should have no ambiguous");
  });

  it("handles renamed identifiers (minification)", () => {
    // Simple case: function structure is identical, just names changed
    const codeV1 = `
      function add(a, b) { return a + b; }
      function multiply(x, y) { return x * y; }
    `;
    const codeV2 = `
      function n(o, p) { return o + p; }
      function q(r, s) { return r * s; }
    `;

    const v1Functions = buildFunctionGraphAsMap(codeV1);
    const v2Functions = buildFunctionGraphAsMap(codeV2);

    const v1Index = buildFingerprintIndex(v1Functions);
    const v2Index = buildFingerprintIndex(v2Functions);

    const result = matchFunctions(v1Index, v2Index);

    assert.strictEqual(
      result.matches.size,
      2,
      "Should match both functions despite rename"
    );
    assert.strictEqual(result.unmatched.length, 0, "Should have no unmatched");
  });

  it("marks changed functions as unmatched", () => {
    const codeV1 = `
      function calc(x) { return x + 1; }
    `;
    const codeV2 = `
      function calc(x) { return x * 2; }
    `;

    const v1Functions = buildFunctionGraphAsMap(codeV1);
    const v2Functions = buildFunctionGraphAsMap(codeV2);

    const v1Index = buildFingerprintIndex(v1Functions);
    const v2Index = buildFingerprintIndex(v2Functions);

    const result = matchFunctions(v1Index, v2Index);

    assert.strictEqual(
      result.unmatched.length,
      1,
      "Changed function should be unmatched"
    );
    assert.strictEqual(result.matches.size, 0, "Should have no matches");
  });

  it("uses callee shapes to disambiguate duplicates", () => {
    // Two wrapper functions with same structure but different callees
    const codeV1 = `
      function wrapper1() { return simple(); }
      function wrapper2() { return complex(); }
      function simple() { return 1; }
      function complex(x) { for(let i=0;i<10;i++) { if(x) return i; } return 0; }
    `;
    const codeV2 = `
      function a() { return b(); }
      function c() { return d(); }
      function b() { return 1; }
      function d(x) { for(let i=0;i<10;i++) { if(x) return i; } return 0; }
    `;

    const v1Functions = buildFunctionGraphAsMap(codeV1);
    const v2Functions = buildFunctionGraphAsMap(codeV2);

    const v1Index = buildFingerprintIndex(v1Functions);
    const v2Index = buildFingerprintIndex(v2Functions);

    const result = matchFunctions(v1Index, v2Index);

    // All 4 functions should match
    assert.strictEqual(result.matches.size, 4, "Should match all 4 functions");
    assert.strictEqual(
      result.ambiguous.size,
      0,
      "Should have no ambiguous matches"
    );
  });

  it("reports ambiguous when cannot disambiguate", () => {
    // Two identical wrapper functions calling two identical simple functions
    const codeV1 = `
      function wrapper1() { return helper1(); }
      function wrapper2() { return helper2(); }
      function helper1() { return 1; }
      function helper2() { return 1; }
    `;
    const codeV2 = `
      function a() { return c(); }
      function b() { return d(); }
      function c() { return 1; }
      function d() { return 1; }
    `;

    const v1Functions = buildFunctionGraphAsMap(codeV1);
    const v2Functions = buildFunctionGraphAsMap(codeV2);

    const v1Index = buildFingerprintIndex(v1Functions);
    const v2Index = buildFingerprintIndex(v2Functions);

    const result = matchFunctions(v1Index, v2Index);

    // The helpers should be ambiguous (identical structure and callee shapes)
    // The wrappers may or may not be ambiguous depending on resolution
    assert.ok(
      result.ambiguous.size > 0 || result.matches.size === 4,
      "Should either have ambiguous or all matched"
    );
  });
});

describe("resolutionStats tracking", () => {
  it("counts structuralHashUnique when each function has a unique hash", () => {
    // structuralHash alone is enough — no disambiguation needed
    const code = `
      function a() { return "hello"; }
      function b(x) { return x + 1; }
    `;

    const v1 = buildFunctionGraphAsMap(code);
    const v2 = buildFunctionGraphAsMap(code);
    const result = matchFunctions(
      buildFingerprintIndex(v1),
      buildFingerprintIndex(v2)
    );

    assert.strictEqual(result.resolutionStats.structuralHashUnique, 2);
    assert.strictEqual(result.resolutionStats.unmatched, 0);
    assert.strictEqual(result.resolutionStats.stillAmbiguous, 0);
  });

  it("counts unmatched when hash not found", () => {
    const codeV1 = `function a(x) { return x + 1; }`;
    const codeV2 = `function b(x) { return x * 2; }`;

    const result = matchFunctions(
      buildFingerprintIndex(buildFunctionGraphAsMap(codeV1)),
      buildFingerprintIndex(buildFunctionGraphAsMap(codeV2))
    );

    assert.strictEqual(result.resolutionStats.unmatched, 1);
    assert.strictEqual(result.resolutionStats.structuralHashUnique, 0);
  });

  it("respects maxCascadeDepth option", () => {
    // Two wrapper functions with same structure but different callees
    const codeV1 = `
      function wrapper1() { return simple(); }
      function wrapper2() { return complex(); }
      function simple() { return 1; }
      function complex(x) { for(let i=0;i<10;i++) { if(x) return i; } return 0; }
    `;
    const codeV2 = `
      function a() { return b(); }
      function c() { return d(); }
      function b() { return 1; }
      function d(x) { for(let i=0;i<10;i++) { if(x) return i; } return 0; }
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));

    // hashOnly — the wrappers are ambiguous (same structuralHash)
    const hashOnlyResult = matchFunctions(v1Index, v2Index, {
      maxCascadeDepth: 0
    });
    assert.ok(
      hashOnlyResult.resolutionStats.stillAmbiguous > 0,
      "Should have ambiguous at hash-only matching"
    );

    // Full cascade — should resolve everything
    const fullResult = matchFunctions(v1Index, v2Index, { maxCascadeDepth: 2 });
    assert.strictEqual(fullResult.resolutionStats.stillAmbiguous, 0);
  });
});

describe("cross-version matching integration", () => {
  it("handles realistic minification scenario", () => {
    // Version 1: Original readable code - simpler version without arrow functions
    const codeV1 = `
      function fetchUserData(userId) {
        if (!userId) {
          throw new Error("userId required");
        }
        return fetch("/api/users/" + userId);
      }

      function processResponse(data) {
        if (!data) return [];
        for (var i = 0; i < data.length; i++) {
          console.log(data[i]);
        }
        return data;
      }

      function main() {
        var result = fetchUserData(123);
        return processResponse(result);
      }
    `;

    // Version 2: Same code but "minified" (renamed)
    const codeV2 = `
      function a(b) {
        if (!b) {
          throw new Error("userId required");
        }
        return fetch("/api/users/" + b);
      }

      function c(d) {
        if (!d) return [];
        for (var e = 0; e < d.length; e++) {
          console.log(d[e]);
        }
        return d;
      }

      function f() {
        var g = a(123);
        return c(g);
      }
    `;

    const v1Functions = buildFunctionGraphAsMap(codeV1);
    const v2Functions = buildFunctionGraphAsMap(codeV2);

    const v1Index = buildFingerprintIndex(v1Functions);
    const v2Index = buildFingerprintIndex(v2Functions);

    const result = matchFunctions(v1Index, v2Index);
    const stats = getMatchStats(result);

    // All 3 functions should match
    assert.strictEqual(
      stats.matched,
      3,
      `Should match all 3 functions, got ${stats.matched}`
    );
    assert.strictEqual(stats.unmatched, 0, "Should have no unmatched");
  });

  it("detects when function internals change", () => {
    const codeV1 = `
      function calculate(x) {
        return x + 1;
      }
    `;
    const codeV2 = `
      function calculate(x) {
        // Bug fix: multiply instead of add
        return x * 2;
      }
    `;

    const v1Functions = buildFunctionGraphAsMap(codeV1);
    const v2Functions = buildFunctionGraphAsMap(codeV2);

    const v1Index = buildFingerprintIndex(v1Functions);
    const v2Index = buildFingerprintIndex(v2Functions);

    const result = matchFunctions(v1Index, v2Index);

    assert.strictEqual(
      result.matches.size,
      0,
      "Changed function should not match"
    );
    assert.strictEqual(result.unmatched.length, 1, "Should be unmatched");
  });

  it("matches functions when unrelated code is added", () => {
    const codeV1 = `
      function existing() { return 42; }
    `;
    const codeV2 = `
      function newFeature() {
        for (let i = 0; i < 10; i++) {
          if (i > 5) console.log(i);
        }
      }
      function existing() { return 42; }
      function anotherNew() { return "hello"; }
    `;

    const v1Functions = buildFunctionGraphAsMap(codeV1);
    const v2Functions = buildFunctionGraphAsMap(codeV2);

    const v1Index = buildFingerprintIndex(v1Functions);
    const v2Index = buildFingerprintIndex(v2Functions);

    const result = matchFunctions(v1Index, v2Index);
    const matchedNewIds = new Set(result.matches.values());
    const newFunctions = [...v2Index.fingerprints.keys()].filter(
      (id) => !matchedNewIds.has(id)
    );

    assert.strictEqual(
      result.matches.size,
      1,
      "Should match existing function"
    );
    assert.strictEqual(
      newFunctions.length,
      2,
      "Should identify 2 new functions"
    );
  });
});

describe("callerShapes disambiguation", () => {
  it("resolves when callerShapes differ", () => {
    // Two identical leaf functions, but called by structurally different callers
    const codeV1 = `
      function complexCaller() {
        for (let i = 0; i < 10; i++) {
          if (i > 5) leaf1();
        }
      }
      function simpleCaller() { leaf2(); }
      function leaf1() { return 1; }
      function leaf2() { return 1; }
    `;
    const codeV2 = `
      function a() {
        for (let i = 0; i < 10; i++) {
          if (i > 5) c();
        }
      }
      function b() { d(); }
      function c() { return 1; }
      function d() { return 1; }
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));

    const result = matchFunctions(v1Index, v2Index);

    // The leaf functions should be disambiguated by their callers' shapes
    assert.strictEqual(
      result.resolutionStats.callerShapesResolved > 0,
      true,
      "Should have callerShapes resolutions"
    );
    assert.strictEqual(
      result.resolutionStats.stillAmbiguous,
      0,
      "Should have no ambiguous matches"
    );
  });

  it("falls through when callerShapes also identical", () => {
    // Two identical leaf functions called by identical callers
    const codeV1 = `
      function caller1() { return leaf1(); }
      function caller2() { return leaf2(); }
      function leaf1() { return 1; }
      function leaf2() { return 1; }
    `;
    const codeV2 = `
      function a() { return c(); }
      function b() { return d(); }
      function c() { return 1; }
      function d() { return 1; }
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));

    const result = matchFunctions(v1Index, v2Index);

    // callerShapes can't help — caller shapes are identical too
    assert.strictEqual(
      result.resolutionStats.callerShapesResolved,
      0,
      "Should not resolve via callerShapes when caller shapes are identical"
    );
  });

  it("falls through when callerShapes empty", () => {
    // Entry-point functions with no callers — callerShapes passes all through
    const codeV1 = `
      function entry1() { return 1; }
      function entry2() { return 1; }
    `;
    const codeV2 = `
      function a() { return 1; }
      function b() { return 1; }
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));

    const result = matchFunctions(v1Index, v2Index);

    assert.strictEqual(
      result.resolutionStats.callerShapesResolved,
      0,
      "Should not resolve via callerShapes when no callers"
    );
    assert.ok(
      result.resolutionStats.stillAmbiguous > 0,
      "Should remain ambiguous"
    );
  });
});

describe("memberKey disambiguation", () => {
  it("resolves two identical-hash functions by different object keys", () => {
    // SWC-style: functions inlined directly into ObjectExpression
    const codeV1 = `
      var store = {
        getCount: function() { return 1; },
        getLabel: function() { return 1; }
      };
    `;
    const codeV2 = `
      var s = {
        getCount: function() { return 1; },
        getLabel: function() { return 1; }
      };
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));

    const result = matchFunctions(v1Index, v2Index);

    assert.strictEqual(
      result.resolutionStats.memberKeyResolved,
      2,
      "Both functions should resolve via memberKey"
    );
    assert.strictEqual(result.resolutionStats.stillAmbiguous, 0);
    assert.strictEqual(result.matches.size, 2);
  });

  it("memberKey runs before callerShapes in the cascade", () => {
    // Functions have different keys AND different callers, but memberKey
    // should resolve first (before callerShapes gets a chance)
    const codeV1 = `
      function complexCaller() {
        for (let i = 0; i < 10; i++) { if (i > 5) obj.getCount(); }
      }
      function simpleCaller() { obj.getLabel(); }
      var obj = {
        getCount: function() { return 1; },
        getLabel: function() { return 1; }
      };
    `;
    const codeV2 = `
      function a() {
        for (let i = 0; i < 10; i++) { if (i > 5) o.getCount(); }
      }
      function b() { o.getLabel(); }
      var o = {
        getCount: function() { return 1; },
        getLabel: function() { return 1; }
      };
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));

    const result = matchFunctions(v1Index, v2Index);

    // memberKey should fire, not callerShapes
    assert.strictEqual(
      result.resolutionStats.memberKeyResolved,
      2,
      "Should resolve via memberKey, not callerShapes"
    );
    assert.strictEqual(
      result.resolutionStats.callerShapesResolved,
      0,
      "callerShapes should not fire when memberKey already resolved"
    );
  });

  it("falls through when functions have no memberKey", () => {
    // Standalone function declarations — no ObjectProperty parent
    const codeV1 = `
      function leaf1() { return 1; }
      function leaf2() { return 1; }
    `;
    const codeV2 = `
      function a() { return 1; }
      function b() { return 1; }
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));

    const result = matchFunctions(v1Index, v2Index);

    assert.strictEqual(
      result.resolutionStats.memberKeyResolved,
      0,
      "Should not resolve via memberKey for standalone functions"
    );
  });

  it("falls through when memberKey filter yields 0 matches", () => {
    // Old side has memberKeys but new side doesn't (different structure)
    const codeV1 = `
      var obj = {
        alpha: function() { return 1; },
        beta: function() { return 1; }
      };
    `;
    const codeV2 = `
      function a() { return 1; }
      function b() { return 1; }
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));

    const result = matchFunctions(v1Index, v2Index);

    // memberKey filter yields 0 for both (new side has no keys),
    // so it falls through to later stages
    assert.strictEqual(
      result.resolutionStats.memberKeyResolved,
      0,
      "Should not resolve via memberKey when new side has no matching keys"
    );
  });
});

describe("enablePropagation integration", () => {
  it("resolves ambiguous functions that cascade alone cannot", () => {
    // Two identical wrappers calling different unique callees.
    // At resolution 0: cascade can't help (same calleeShapes disabled).
    // With propagation: resolved via matched-callee constraint.
    const codeV1 = `
      function wrapper1() { return uniqueA(); }
      function wrapper2() { return uniqueB(); }
      function uniqueA() { return "hello"; }
      function uniqueB(x) { return x + 1; }
    `;
    const codeV2 = `
      function w1() { return uA(); }
      function w2() { return uB(); }
      function uA() { return "hello"; }
      function uB(x) { return x + 1; }
    `;

    const v1 = buildFunctionGraphAsMap(codeV1);
    const v2 = buildFunctionGraphAsMap(codeV2);
    const v1Index = buildFingerprintIndex(v1);
    const v2Index = buildFingerprintIndex(v2);

    // Without propagation at maxCascadeDepth: 0 (hash-only matching)
    const without = matchFunctions(v1Index, v2Index, { maxCascadeDepth: 0 });
    assert.ok(
      without.ambiguous.size > 0,
      "Should have ambiguous without propagation"
    );

    // With propagation at maxCascadeDepth: 0
    const result = matchFunctions(v1Index, v2Index, {
      maxCascadeDepth: 0,
      enablePropagation: true
    });
    assert.strictEqual(
      result.ambiguous.size,
      0,
      "Propagation should resolve all"
    );
    assert.ok(
      result.resolutionStats.propagationResolved > 0,
      "Should track propagation resolved count"
    );
  });

  it("propagationResolved is 0 when propagation not enabled", () => {
    const code = `function a() { return 1; }`;
    const v1 = buildFunctionGraphAsMap(code);
    const v2 = buildFunctionGraphAsMap(code);

    const result = matchFunctions(
      buildFingerprintIndex(v1),
      buildFingerprintIndex(v2)
    );

    assert.strictEqual(result.resolutionStats.propagationResolved, 0);
  });
});

// Helper to build function graph as Map (what buildFingerprintIndex expects)
function buildFunctionGraphAsMap(code: string): Map<string, FunctionNode> {
  const ast = parse(code);
  const functions = buildFunctionGraph(ast, "test.js");
  return new Map(functions.map((f) => [f.sessionId, f]));
}

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}
