import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import {
  assignInterchangeablePools,
  buildBindingFingerprintIndex,
  buildFingerprintIndex,
  certifyInterchangeablePools,
  getMatchStats,
  matchFunctions,
  resolveAmbiguousByOrdinal
} from "./fingerprint-index.js";
import {
  calleeShapesEqual,
  serializeCalleeShape
} from "./function-fingerprint.js";
import { buildFunctionGraph, buildUnifiedGraph } from "./function-graph.js";
import type {
  FunctionFingerprint,
  FunctionNode,
  ModuleBindingNode
} from "./types.js";
import { fingerprintFeatures } from "./types.js";

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

describe("stop-on-empty cascade", () => {
  it("a candidate rejected by memberKey cannot win at a weaker stage", () => {
    // Old F has memberKey "run". New bucket: X and Y share the key but call
    // a linear helper (calleeShapes contradiction); Z calls the same looping
    // helper F does (calleeHashes would pick it) but sits under key "walk".
    // The old cascade emptied at calleeShapes and fell back to the FULL
    // candidate set, letting the memberKey-rejected Z win via calleeHashes.
    const codeV1 = `
      var api = { run: function () { return work(); } };
      function work(x) {
        for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }
        return 1;
      }
    `;
    const codeV2 = `
      var a1 = { run: function () { return lin1(); } };
      var a2 = { run: function () { return lin2(); } };
      var a3 = { walk: function () { return loopHelper(); } };
      function lin1() { return 1; }
      function lin2() { return 2; }
      function loopHelper(x) {
        for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }
        return 1;
      }
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));
    const result = matchFunctions(v1Index, v2Index);

    assert.strictEqual(
      result.resolutionStats.calleeHashesResolved,
      0,
      "calleeHashes must not resolve using candidates memberKey rejected"
    );
    // F stays ambiguous; only the work→loopHelper pair may match.
    assert.strictEqual(result.matches.size, 1);
    assert.strictEqual(result.ambiguous.size, 1);
  });

  it("propagation does not match a candidate that contradicts a matched callee", () => {
    // P calls H, and H is matched to Hn. Neither new candidate calls Hn —
    // every candidate contradicts the callee constraint. The old code fell
    // back to the unfiltered pool and let the caller constraint pick Q1.
    const codeV1 = `
      function C(x) {
        for (let i = 0; i < 3; i++) { if (x) P(); }
        return 42;
      }
      function P() { return H(); }
      function H() { return true; }
    `;
    const codeV2 = `
      function Cn(x) {
        for (let i = 0; i < 3; i++) { if (x) Q1(); }
        return 42;
      }
      function C2n(y) {
        for (let k = 0; k < 9; k++) { if (y) Q2(); }
        return "s";
      }
      function Q1() { return L1(); }
      function Q2() { return L2(); }
      function L1() { return 1; }
      function L2() { return "z"; }
      function Hn() { return true; }
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));
    const result = matchFunctions(v1Index, v2Index, {
      enablePropagation: true
    });

    // C→Cn and H→Hn match; P must NOT be given Q1 by the caller constraint
    // when both candidates contradict the matched-callee constraint.
    assert.strictEqual(result.matches.size, 2);
    assert.ok(
      result.ambiguous.size >= 1,
      "P should stay ambiguous under contradiction"
    );
  });
});

describe("singleton-bucket corroboration gate", () => {
  it("rejects a singleton match whose memberKeys contradict", () => {
    // One old + one new function with the same hash, but assigned to
    // different (minifier-stable) object keys. Zero-corroboration accept
    // would transfer the whole name set of an unrelated function.
    const codeV1 = `
      var api = {
        run: function (x) {
          for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }
          return 1;
        }
      };
    `;
    const codeV2 = `
      var api = {
        walk: function (y) {
          for (let j = 0; j < 10; j++) { if (y > j) console.log(j); }
          return 2;
        }
      };
    `;

    const result = matchFunctions(
      buildFingerprintIndex(buildFunctionGraphAsMap(codeV1)),
      buildFingerprintIndex(buildFunctionGraphAsMap(codeV2))
    );

    assert.strictEqual(
      result.matches.size,
      0,
      "memberKey contradiction must reject the singleton match"
    );
    assert.strictEqual(result.unmatched.length, 1);
    assert.strictEqual(result.resolutionStats.singletonRejected, 1);
  });

  it("accepts a singleton when the signal is one-sided (no contradiction)", () => {
    // Old side has a memberKey, new side is a standalone expression —
    // a missing signal is not an opposing signal.
    const codeV1 = `
      var api = {
        run: function (x) {
          for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }
          return 1;
        }
      };
    `;
    const codeV2 = `
      var runner = function (y) {
        for (let j = 0; j < 10; j++) { if (y > j) console.log(j); }
        return 2;
      };
    `;

    const result = matchFunctions(
      buildFingerprintIndex(buildFunctionGraphAsMap(codeV1)),
      buildFingerprintIndex(buildFunctionGraphAsMap(codeV2))
    );

    assert.strictEqual(result.matches.size, 1);
    assert.strictEqual(result.resolutionStats.singletonRejected, 0);
  });

  // The guard reads `memberKey` and `features`. `buildBindingFullFingerprint`
  // sets NEITHER, so on the module-binding cascade `singletonContradicts` can
  // only ever return false — every zero-corroboration singleton binding is
  // auto-accepted with nothing checked. On 2.1.215->216 that was 11,094
  // accepts and 0 checks, and it reported as `singletonRejected: 0`, which
  // reads exactly like perfect precision (exp058, measurement-pitfalls rule 3).
  //
  // The count of accepts the guard could not examine has to be visible, or the
  // absence of a measurement keeps being read as the result of one.
  it("reports singleton accepts the guard could not examine, per cascade", () => {
    const fnResult = matchFunctions(
      buildFingerprintIndex(
        buildFunctionGraphAsMap("var runner = function (y) { return y + 1; };")
      ),
      buildFingerprintIndex(
        buildFunctionGraphAsMap("var walker = function (z) { return z + 1; };")
      )
    );
    assert.strictEqual(
      fnResult.resolutionStats.singletonUnguarded,
      0,
      "function fingerprints carry features, so every singleton accept IS examined"
    );

    const bindingCode = (a: string, b: string) =>
      `const ${a} = { path: "/x", timeout: 30 };\nfunction use() { return ${a}.path; }\nmodule.exports = { use, ${b}: ${a} };`;
    const bindingResult = matchFunctions(
      buildBindingIndexAsMap(bindingCode("alpha", "one")),
      buildBindingIndexAsMap(bindingCode("beta", "two"))
    );
    assert.ok(
      bindingResult.resolutionStats.structuralHashUnique > 0,
      "fixture must produce singleton binding accepts, or the assertion below is vacuous"
    );
    assert.strictEqual(
      bindingResult.resolutionStats.singletonUnguarded,
      bindingResult.resolutionStats.structuralHashUnique,
      "binding fingerprints carry no features and no memberKey, so EVERY singleton accept is unexamined"
    );
  });
});

describe("injectivity", () => {
  it("never matches two old functions to the same new function", () => {
    // Two identical old functions, one new function with the same structure:
    // a 2-old/1-new bucket. candidates.length === 1 short-circuits per old
    // id, so without enforcement BOTH old ids claim the single new id.
    const codeV1 = `
      function a() { return 1; }
      function b() { return 1; }
    `;
    const codeV2 = `
      function x() { return 1; }
    `;

    const result = matchFunctions(
      buildFingerprintIndex(buildFunctionGraphAsMap(codeV1)),
      buildFingerprintIndex(buildFunctionGraphAsMap(codeV2))
    );

    const claimed = new Set<string>();
    for (const newId of result.matches.values()) {
      assert.ok(!claimed.has(newId), `new id ${newId} claimed twice`);
      claimed.add(newId);
    }
    assert.strictEqual(
      result.matches.size,
      0,
      "Neither old function can safely claim the single new function"
    );
    assert.strictEqual(
      result.ambiguous.size,
      2,
      "Both old functions should be demoted to ambiguous"
    );
    assert.strictEqual(result.resolutionStats.injectivityDemoted, 2);
    assert.strictEqual(
      result.resolutionStats.structuralHashUnique,
      0,
      "Demoted matches must not be counted as resolved"
    );
    assert.strictEqual(result.resolutionStats.stillAmbiguous, 2);
  });

  it("propagation re-resolves demoted claims injectively", () => {
    // Old: two identical leaves; only leaf1 has a (distinctive) caller.
    // New: leaf2 was deleted, so the bucket is 2-old/1-new. The caller
    // constraint should give the new leaf to leaf1 and leave leaf2
    // ambiguous — never both.
    const codeV1 = `
      function bigCaller(x) {
        for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }
        return leaf1();
      }
      function leaf1() { return 1; }
      function leaf2() { return 1; }
    `;
    const codeV2 = `
      function bc(y) {
        for (let j = 0; j < 10; j++) { if (y > j) console.log(j); }
        return L();
      }
      function L() { return 1; }
    `;

    const v1Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV1));
    const v2Index = buildFingerprintIndex(buildFunctionGraphAsMap(codeV2));
    const result = matchFunctions(v1Index, v2Index, {
      enablePropagation: true
    });

    const claimed = new Set<string>();
    for (const newId of result.matches.values()) {
      assert.ok(!claimed.has(newId), `new id ${newId} claimed twice`);
      claimed.add(newId);
    }
    assert.strictEqual(
      result.matches.size,
      2,
      "bigCaller and exactly one leaf should match"
    );
    assert.strictEqual(
      result.ambiguous.size,
      1,
      "The unclaimed leaf stays ambiguous"
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

/** A binding fingerprint index, the counterpart to `buildFunctionGraphAsMap` —
 * module bindings run the same `matchFunctions` cascade via a different index
 * builder, and that asymmetry is what exp058 measured. */
function buildBindingIndexAsMap(code: string) {
  const graph = buildUnifiedGraph(parse(code), "test.js");
  const bindings: ModuleBindingNode[] = [];
  for (const node of graph.nodes.values()) {
    if (node.type === "module-binding") bindings.push(node.node);
  }
  return buildBindingFingerprintIndex(bindings);
}

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

/**
 * The cascade compares relational arrays two different ways, and only one of
 * them is order-insensitive:
 *
 *   - `calleeShapesEqual` (calleeShapes, callerShapes) re-sorts its inputs
 *   - `arraysEqual` (calleeHashes, twoHopShapes, and the feature lists in the
 *     singleton guard) compares position by position
 *
 * `arraysEqual` is correct ONLY because every one of those arrays is already
 * canonical when it is stored: both fingerprint builders `.sort()` them, and
 * `computeStructuralFeatures` dedupes-and-sorts `externalCalls` and
 * `propertyAccesses`. Nothing said so and nothing checked it.
 *
 * A fifth relational array added without a `.sort()`, or a builder that stops
 * sorting one, produces FALSE REJECTS in the middle of the cascade: two
 * genuinely-matching functions whose callees were visited in a different order
 * fail `arraysEqual`, the tier contradicts, and the match is lost. Nothing
 * would be thrown and no counter would move — the pair would simply not match.
 *
 * The re-sort inside `calleeShapesEqual` is therefore redundant rather than
 * load-bearing. It is left in place deliberately (it is defensive and the
 * arrays are small); this test is what records that the OTHER comparator has
 * no such protection and depends on the invariant below.
 */
describe("relational fingerprint arrays are canonical at construction", () => {
  const isSorted = (a: readonly string[]): boolean =>
    a.every((v, i) => i === 0 || a[i - 1] <= v);

  /**
   * Enough call structure that every relational array is non-empty on BOTH
   * builders. `comboList`/`namedPair` exist specifically to give a MODULE
   * BINDING two or more callees — without them the binding-side check has
   * nothing multi-element to look at and passes while the sort is missing.
   * (It did exactly that when first written; `assertChecked` is the guard.)
   */
  const CODE = `
    function zetaLeaf(v) { return v + 313; }
    function alphaLeaf(v) { return v * 727; }
    function midOne(a) { return zetaLeaf(a) + alphaLeaf(a); }
    function midTwo(b) { return alphaLeaf(b) - zetaLeaf(b); }
    function top(c) { return midOne(c) + midTwo(c); }
    const configHolder = { limit: 5, mode: "fast" };
    function readsHolder() { return configHolder.limit; }
    const comboList = [zetaLeaf, alphaLeaf, midOne];
    const namedPair = { a: alphaLeaf, z: zetaLeaf, m: midTwo };
    module.exports = { top, readsHolder, configHolder, comboList, namedPair };
  `;

  const RELATIONAL = ["calleeHashes", "twoHopShapes"] as const;

  /**
   * Asserts every multi-element array the positional comparator touches is
   * sorted, and returns how many it actually looked at. The count is the
   * point: 0 means the fixture examined nothing, so the check would pass
   * against a builder that never sorts.
   */
  function checkSorted(
    index: { fingerprints: Map<string, FunctionFingerprint> },
    label: string
  ): number {
    let checked = 0;
    for (const [id, fp] of index.fingerprints) {
      const arrays = [
        ...RELATIONAL.map((f) => [f, fp[f]] as const),
        // The singleton guard compares these two with arraysEqual as well.
        [
          "features.externalCalls",
          fingerprintFeatures(fp)?.externalCalls
        ] as const,
        [
          "features.propertyAccesses",
          fingerprintFeatures(fp)?.propertyAccesses
        ] as const
      ];
      for (const [field, arr] of arrays) {
        if (!arr || arr.length < 2) continue;
        checked++;
        assert.ok(
          isSorted(arr),
          `${label} ${id}.${field} is not sorted: ${JSON.stringify(arr)} — arraysEqual compares it positionally`
        );
      }
    }
    assert.ok(
      checked > 0,
      `${label}: fixture produced no multi-element relational array, so this check cannot detect an unsorted one`
    );
    return checked;
  }

  it("sorts every array arraysEqual compares — function fingerprints", () => {
    checkSorted(buildFingerprintIndex(buildFunctionGraphAsMap(CODE)), "fn");
  });

  it("sorts every array arraysEqual compares — binding fingerprints", () => {
    // The path where the analogous asymmetry already cost 11,094 unexamined
    // accepts. A second builder is a second chance to forget the sort.
    checkSorted(buildBindingIndexAsMap(CODE), "binding");
  });

  it("the sort is what makes arraysEqual agree with calleeShapesEqual", () => {
    // Same multiset, different order. calleeShapesEqual (re-sorts) says equal;
    // arraysEqual (positional) says not equal. Both are used on arrays derived
    // from the same callee set, so they diverge the moment the sort is lost.
    const shapes = [
      {
        arity: 2,
        complexity: 3,
        cfgType: "linear" as const,
        hasExternalCalls: false
      },
      {
        arity: 1,
        complexity: 1,
        cfgType: "looping" as const,
        hasExternalCalls: true
      }
    ];
    assert.ok(
      calleeShapesEqual(shapes, [...shapes].reverse()),
      "calleeShapesEqual is order-insensitive"
    );

    const serialized = shapes.map(serializeCalleeShape);
    const positional = (a: string[], b: string[]) =>
      a.length === b.length && a.every((v, i) => v === b[i]);
    assert.ok(
      !positional(serialized, [...serialized].reverse()),
      "the positional comparator is NOT — hence the construction-time sort"
    );
  });
});

describe("certifyInterchangeablePools (exp036 task B)", () => {
  // Four same-shaped wrappers: two wrap DISTINCT helpers (callee-hash
  // tier matches those), three wrap the SAME helper — indistinguishable
  // leftovers. The certificate must find the reciprocal 3:3 pool and
  // assign NOTHING.
  const V1 = `
    function helperAlpha(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }
    function helperBeta(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }
    function wrapAlphaCall(a) { return helperAlpha(a); }
    function wrapBetaCall(b) { return helperBeta(b); }
    function firstSameWrap(c) { return helperAlpha(c); }
    function secondSameWrap(d) { return helperAlpha(d); }
  `;
  const V2 = `
    function hA(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }
    function hB(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }
    function w1(a) { return hA(a); }
    function w2(b) { return hB(b); }
    function s1(c) { return hA(c); }
    function s2(d) { return hA(d); }
  `;

  function certified(v1Src: string, v2Src: string) {
    const v1 = buildFunctionGraphAsMap(v1Src);
    const v2 = buildFunctionGraphAsMap(v2Src);
    const v1Index = buildFingerprintIndex(v1);
    const v2Index = buildFingerprintIndex(v2);
    const result = matchFunctions(v1Index, v2Index);
    resolveAmbiguousByOrdinal(result, v1Index, v2Index);
    const pools = certifyInterchangeablePools(result, v1Index, v2Index);
    return { pools, result, v1, v2 };
  }

  it("certifies the reciprocal indistinguishable pool without assigning", () => {
    const { pools, result } = certified(V1, V2);
    assert.strictEqual(pools.length, 1, "one certified pool");
    assert.strictEqual(pools[0].priors.length, 3);
    assert.strictEqual(pools[0].candidates.length, 3);
    assert.ok(pools[0].evidenceKey.length > 0);
    // The certificate is read-only: nothing entered matches.
    for (const p of pools[0].priors) {
      assert.ok(!result.matches.has(p), "certificate must not assign");
      assert.ok(result.ambiguous.has(p), "members stay ambiguous");
    }
  });

  it("refuses unequal counts (membership churn)", () => {
    const v2Short = V2.replace("function s2(d) { return hA(d); }", "");
    const { pools } = certified(V1, v2Short);
    assert.strictEqual(pools.length, 0, "3:2 pool must not certify");
  });

  it("is stable across a re-parse of the same sources", () => {
    const a = certified(V1, V2);
    const b = certified(V1, V2);
    const nameOf = (graph: Map<string, FunctionNode>, id: string): string => {
      const fn = graph.get(id);
      const fnId = (fn?.path.node as { id?: { name?: string } }).id;
      return fnId?.name ?? id;
    };
    const shape = (r: typeof a) =>
      r.pools.map((p) => ({
        priors: p.priors.map((id) => nameOf(r.v1, id)),
        candidates: p.candidates.map((id) => nameOf(r.v2, id)),
        evidenceKey: p.evidenceKey
      }));
    assert.deepStrictEqual(
      shape(a),
      shape(b),
      "certificate must be reparse-stable"
    );
  });
});

describe("assignInterchangeablePools (exp036 task C)", () => {
  // Pool of two same-helper wrappers whose BUNDLE POSITIONS swapped
  // between versions, each travelling with a unique anchor neighbor.
  // Source-order pairing (the failed ordinal tier) would cross them up;
  // prior-anchored affinity must follow the anchors.
  const V1 = `
    function helperAlpha(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }
    function helperBeta(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }
    function wrapBeta(b) { return helperBeta(b); }
    function uniqueLeft(x) { let u = x + 13; for (let i = 0; i < 4; i++) { u ^= i; } return u; }
    function firstWrap(c) { return helperAlpha(c); }
    function uniqueRight(y) { let w = y * 31; do { w -= 5; } while (w > 50); return w; }
    function secondWrap(d) { return helperAlpha(d); }
  `;
  const V2_SWAPPED = `
    function hA(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }
    function hB(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }
    function wB(b) { return hB(b); }
    function uR(y) { let w = y * 31; do { w -= 5; } while (w > 50); return w; }
    function s2(d) { return hA(d); }
    function uL(x) { let u = x + 13; for (let i = 0; i < 4; i++) { u ^= i; } return u; }
    function s1(c) { return hA(c); }
  `;

  function assigned(v1Src: string, v2Src: string) {
    const v1 = buildFunctionGraphAsMap(v1Src);
    const v2 = buildFunctionGraphAsMap(v2Src);
    const v1Index = buildFingerprintIndex(v1);
    const v2Index = buildFingerprintIndex(v2);
    const result = matchFunctions(v1Index, v2Index);
    resolveAmbiguousByOrdinal(result, v1Index, v2Index);
    const resolved = assignInterchangeablePools(result, v1Index, v2Index);
    const nameOf = (graph: Map<string, FunctionNode>, id: string): string => {
      const fn = graph.get(id);
      const fnId = (fn?.path.node as { id?: { name?: string } }).id;
      return fnId?.name ?? id;
    };
    const byName = new Map<string, string>();
    for (const [oldId, newId] of result.matches) {
      byName.set(nameOf(v1, oldId), nameOf(v2, newId));
    }
    return { resolved, result, byName };
  }

  it("follows matched anchors when bundle positions swapped", () => {
    const { resolved, byName, result } = assigned(V1, V2_SWAPPED);
    assert.strictEqual(resolved, 2, "the 2:2 pool assigns");
    // Anchor-following: firstWrap travels with uniqueLeft, secondWrap
    // with uniqueRight — source order would say the opposite.
    assert.strictEqual(byName.get("firstWrap"), "s1");
    assert.strictEqual(byName.get("secondWrap"), "s2");
    assert.strictEqual(result.resolutionStats.interchangeableResolved, 2);
  });

  it("is the identity on a self-hop (same sources both sides)", () => {
    const { byName } = assigned(V1, V1);
    assert.strictEqual(byName.get("firstWrap"), "firstWrap");
    assert.strictEqual(byName.get("secondWrap"), "secondWrap");
  });

  it("is deterministic across repeated runs", () => {
    const a = assigned(V1, V2_SWAPPED);
    const b = assigned(V1, V2_SWAPPED);
    assert.deepStrictEqual([...a.byName].sort(), [...b.byName].sort());
  });

  it("never lets two pools claim the same fresh candidate (injectivity)", () => {
    // Two certified pools CAN overlap: evidence keys omit twoHop and
    // call-graph evidence, and propagation narrows ambiguous pools in
    // place — so pool {p1,p2}→{x,y} and pool {p3,p4}→{y,z} both certify
    // (every member shares the one evidence key) while sharing y. This
    // tier runs AFTER demoteNonInjectiveMatches, so a double claim here
    // ships both priors' names onto one fresh function. The pool that
    // finds a candidate already taken must abstain entirely — its
    // reciprocity certificate no longer holds.
    const V1_FOUR = `
      function helperAlpha(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }
      function wrapOne(a) { return helperAlpha(a); }
      function wrapTwo(b) { return helperAlpha(b); }
      function wrapThree(c) { return helperAlpha(c); }
      function wrapFour(d) { return helperAlpha(d); }
    `;
    const V2_THREE = `
      function hA(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }
      function x(a) { return hA(a); }
      function y(b) { return hA(b); }
      function z(c) { return hA(c); }
    `;
    const v1 = buildFunctionGraphAsMap(V1_FOUR);
    const v2 = buildFunctionGraphAsMap(V2_THREE);
    const v1Index = buildFingerprintIndex(v1);
    const v2Index = buildFingerprintIndex(v2);
    const result = matchFunctions(v1Index, v2Index);

    const idOf = (graph: Map<string, FunctionNode>, wanted: string): string => {
      for (const [id, fn] of graph) {
        const fnId = (fn.path.node as { id?: { name?: string } }).id;
        if (fnId?.name === wanted) return id;
      }
      throw new Error(`no function named ${wanted}`);
    };
    const [p1, p2, p3, p4] = [
      "wrapOne",
      "wrapTwo",
      "wrapThree",
      "wrapFour"
    ].map((n) => idOf(v1, n));
    const [cx, cy, cz] = ["x", "y", "z"].map((n) => idOf(v2, n));
    // The 4:3 wrapper bucket is ambiguous; narrow the pools in place the
    // way propagation does, to two overlapping candidate sets.
    assert.ok(result.ambiguous.has(p1), "wrappers must start ambiguous");
    result.ambiguous.set(p1, [cx, cy]);
    result.ambiguous.set(p2, [cx, cy]);
    result.ambiguous.set(p3, [cy, cz]);
    result.ambiguous.set(p4, [cy, cz]);

    assignInterchangeablePools(result, v1Index, v2Index);

    const claimed = new Map<string, string[]>();
    for (const [oldId, newId] of result.matches) {
      claimed.set(newId, [...(claimed.get(newId) ?? []), oldId]);
    }
    for (const [newId, claimants] of claimed) {
      assert.ok(
        claimants.length <= 1,
        `fresh ${newId} claimed by ${claimants.length} priors: ${claimants.join(", ")}`
      );
    }
  });
});
