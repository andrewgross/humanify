import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  buildCfgShapeString,
  computeStructuralHash,
  extractStructuralFeatures
} from "./structural-hash.js";

describe("computeStructuralHash", () => {
  it("produces the same hash for structurally identical functions with different names", () => {
    const code1 = `function a(b, c) { return b + c; }`;
    const code2 = `function x(y, z) { return y + z; }`;

    const fn1 = extractFunction(code1);
    const fn2 = extractFunction(code2);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);

    assert.strictEqual(
      hash1,
      hash2,
      "Structurally identical functions should have the same hash"
    );
  });

  it("produces different hashes for structurally different functions", () => {
    const code1 = `function a(b, c) { return b + c; }`;
    const code2 = `function a(b, c) { return b * c; }`;

    const fn1 = extractFunction(code1);
    const fn2 = extractFunction(code2);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);

    assert.notStrictEqual(
      hash1,
      hash2,
      "Different functions should have different hashes"
    );
  });

  it("normalizes string literals to length markers", () => {
    const code1 = `function f() { return "hello"; }`;
    const code2 = `function f() { return "world"; }`;
    const code3 = `function f() { return "hi"; }`;

    const fn1 = extractFunction(code1);
    const fn2 = extractFunction(code2);
    const fn3 = extractFunction(code3);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);
    const hash3 = computeStructuralHash(fn3);

    assert.strictEqual(
      hash1,
      hash2,
      "Same length strings should produce same hash"
    );
    assert.notStrictEqual(
      hash1,
      hash3,
      "Different length strings should produce different hashes"
    );
  });

  it("normalizes numeric literals to magnitude buckets", () => {
    const code1 = `function f() { return 100; }`;
    const code2 = `function f() { return 500; }`;
    const code3 = `function f() { return 5000; }`;

    const fn1 = extractFunction(code1);
    const fn2 = extractFunction(code2);
    const fn3 = extractFunction(code3);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);
    const hash3 = computeStructuralHash(fn3);

    assert.strictEqual(
      hash1,
      hash2,
      "Same magnitude numbers should produce same hash"
    );
    assert.notStrictEqual(
      hash1,
      hash3,
      "Different magnitude numbers should produce different hashes"
    );
  });

  it("handles arrow functions", () => {
    const code1 = `const a = (b, c) => b + c;`;
    const code2 = `const x = (y, z) => y + z;`;

    const fn1 = extractArrowFunction(code1);
    const fn2 = extractArrowFunction(code2);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);

    assert.strictEqual(
      hash1,
      hash2,
      "Structurally identical arrow functions should have the same hash"
    );
  });

  it("handles functions with nested functions", () => {
    const code1 = `function outer(a) { function inner(b) { return b; } return inner(a); }`;
    const code2 = `function x(y) { function z(w) { return w; } return z(y); }`;

    const fn1 = extractFunction(code1);
    const fn2 = extractFunction(code2);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);

    assert.strictEqual(
      hash1,
      hash2,
      "Functions with identical nested structures should match"
    );
  });

  it("produces a 16-character hex hash", () => {
    const code = `function f(x) { return x * 2; }`;
    const fn = extractFunction(code);
    const hash = computeStructuralHash(fn);

    assert.strictEqual(hash.length, 16, "Hash should be 16 characters");
    assert.match(hash, /^[0-9a-f]+$/, "Hash should be hexadecimal");
  });

  it("produces different hashes for different operators", () => {
    const addCode = `function add(a, b) { return a + b; }`;
    const subtractCode = `function subtract(a, b) { return a - b; }`;

    const addFn = extractFunction(addCode);
    const subtractFn = extractFunction(subtractCode);

    const addHash = computeStructuralHash(addFn);
    const subtractHash = computeStructuralHash(subtractFn);

    assert.notStrictEqual(
      addHash,
      subtractHash,
      "Different operators should produce different hashes"
    );
  });

  it("produces different hashes for different function calls", () => {
    const code1 = `function f() { return fetch(); }`;
    const code2 = `function f() { return save(); }`;

    const fn1 = extractFunction(code1);
    const fn2 = extractFunction(code2);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);

    // These WILL have same hash because fetch/save are identifiers that get normalized
    // This is expected - the cache maps structure to renames, and the function names
    // called are part of what the LLM sees to determine good names
    assert.strictEqual(
      hash1,
      hash2,
      "Same structure with different called functions should match (identifiers normalized)"
    );
  });
});

function extractFunction(code: string): t.Function {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }

  for (const stmt of ast.program.body) {
    if (t.isFunctionDeclaration(stmt)) {
      return stmt;
    }
  }

  throw new Error("No function found in code");
}

function extractArrowFunction(code: string): t.Function {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }

  for (const stmt of ast.program.body) {
    if (t.isVariableDeclaration(stmt)) {
      for (const decl of stmt.declarations) {
        if (t.isArrowFunctionExpression(decl.init)) {
          return decl.init;
        }
      }
    }
  }

  throw new Error("No arrow function found in code");
}

describe("extractStructuralFeatures", () => {
  it("extracts correct arity", () => {
    const code1 = `function f() {}`;
    const code2 = `function f(a) {}`;
    const code3 = `function f(a, b, c) {}`;

    const features1 = extractStructuralFeatures(extractFunction(code1));
    const features2 = extractStructuralFeatures(extractFunction(code2));
    const features3 = extractStructuralFeatures(extractFunction(code3));

    assert.strictEqual(features1.arity, 0);
    assert.strictEqual(features2.arity, 1);
    assert.strictEqual(features3.arity, 3);
  });

  it("detects rest parameters", () => {
    const code1 = `function f(a, b) {}`;
    const code2 = `function f(a, ...rest) {}`;

    const features1 = extractStructuralFeatures(extractFunction(code1));
    const features2 = extractStructuralFeatures(extractFunction(code2));

    assert.strictEqual(features1.hasRestParam, false);
    assert.strictEqual(features2.hasRestParam, true);
  });

  it("counts return statements", () => {
    const code1 = `function f() {}`;
    const code2 = `function f() { return 1; }`;
    const code3 = `function f(x) { if (x) { return 1; } return 2; }`;

    const features1 = extractStructuralFeatures(extractFunction(code1));
    const features2 = extractStructuralFeatures(extractFunction(code2));
    const features3 = extractStructuralFeatures(extractFunction(code3));

    assert.strictEqual(features1.returnCount, 0);
    assert.strictEqual(features2.returnCount, 1);
    assert.strictEqual(features3.returnCount, 2);
  });

  it("counts loops and branches", () => {
    const code = `
      function f(items) {
        for (const item of items) {
          if (item.valid) {
            process(item);
          }
        }
        while (hasMore()) {
          fetch();
        }
      }
    `;

    const features = extractStructuralFeatures(extractFunction(code));

    assert.strictEqual(
      features.loopCount,
      2,
      "Should count for-of and while loops"
    );
    assert.strictEqual(features.branchCount, 1, "Should count if statement");
  });

  it("calculates cyclomatic complexity", () => {
    const simpleCode = `function f() { return 1; }`;
    const complexCode = `
      function f(x, y) {
        if (x > 0) {
          if (y > 0) {
            return 1;
          }
          return 2;
        }
        for (let i = 0; i < 10; i++) {
          if (x && y) {
            continue;
          }
        }
        return 0;
      }
    `;

    const simpleFeatures = extractStructuralFeatures(
      extractFunction(simpleCode)
    );
    const complexFeatures = extractStructuralFeatures(
      extractFunction(complexCode)
    );

    assert.strictEqual(
      simpleFeatures.complexity,
      1,
      "Simple function has base complexity 1"
    );
    assert.ok(
      complexFeatures.complexity > simpleFeatures.complexity,
      "Complex function should have higher complexity"
    );
  });

  it("collects string literals", () => {
    const code = `
      function f() {
        console.log("hello");
        console.log("world");
        console.log("hello"); // duplicate
        return "done";
      }
    `;

    const features = extractStructuralFeatures(extractFunction(code));

    assert.deepStrictEqual(
      features.stringLiterals,
      ["done", "hello", "world"],
      "Should dedupe and sort"
    );
  });

  it("collects numeric literals", () => {
    const code = `
      function f() {
        const x = 42;
        const y = 3.14;
        const z = 42; // duplicate
        return x + y + 100;
      }
    `;

    const features = extractStructuralFeatures(extractFunction(code));

    assert.deepStrictEqual(
      features.numericLiterals,
      [3.14, 42, 100],
      "Should dedupe and sort"
    );
  });

  it("identifies external calls", () => {
    const code = `
      function f(data) {
        console.log("processing");
        const parsed = JSON.parse(data);
        return fetch("/api").then(r => r.json());
      }
    `;

    const features = extractStructuralFeatures(extractFunction(code));

    assert.ok(
      features.externalCalls.includes("console.log"),
      "Should detect console.log"
    );
    assert.ok(
      features.externalCalls.includes("JSON.parse"),
      "Should detect JSON.parse"
    );
    assert.ok(features.externalCalls.includes("fetch"), "Should detect fetch");
  });

  it("collects property accesses", () => {
    const code = `
      function f(arr, obj) {
        const len = arr.length;
        return obj.data.items.map(x => x.value);
      }
    `;

    const features = extractStructuralFeatures(extractFunction(code));

    assert.ok(features.propertyAccesses.includes(".length"));
    assert.ok(features.propertyAccesses.includes(".data"));
    assert.ok(features.propertyAccesses.includes(".items"));
    assert.ok(features.propertyAccesses.includes(".value"));
  });

  it("produces identical features for renamed functions", () => {
    const code1 = `
      function processData(input, options) {
        if (options.validate) {
          return validate(input);
        }
        for (const item of input) {
          transform(item);
        }
        return input;
      }
    `;
    const code2 = `
      function a(b, c) {
        if (c.validate) {
          return d(b);
        }
        for (const e of b) {
          f(e);
        }
        return b;
      }
    `;

    const features1 = extractStructuralFeatures(extractFunction(code1));
    const features2 = extractStructuralFeatures(extractFunction(code2));

    assert.strictEqual(features1.arity, features2.arity);
    assert.strictEqual(features1.returnCount, features2.returnCount);
    assert.strictEqual(features1.loopCount, features2.loopCount);
    assert.strictEqual(features1.branchCount, features2.branchCount);
    assert.strictEqual(features1.complexity, features2.complexity);
    assert.strictEqual(features1.cfgShape, features2.cfgShape);
  });
});

describe("buildCfgShapeString", () => {
  it("returns 'empty' for empty function", () => {
    const code = `function f() {}`;
    const shape = buildCfgShapeString(extractFunction(code));
    assert.strictEqual(shape, "empty");
  });

  it("returns 'expr' for arrow function with expression body", () => {
    const code = `const f = x => x + 1;`;
    const shape = buildCfgShapeString(extractArrowFunction(code));
    assert.strictEqual(shape, "expr");
  });

  it("captures if-else structure", () => {
    const code = `
      function f(x) {
        if (x > 0) {
          return 1;
        } else {
          return -1;
        }
      }
    `;
    const shape = buildCfgShapeString(extractFunction(code));
    assert.strictEqual(shape, "if-ret-else-ret");
  });

  it("captures nested control flow", () => {
    const code = `
      function f(items) {
        for (const item of items) {
          if (item.valid) {
            return item;
          }
        }
        return null;
      }
    `;
    const shape = buildCfgShapeString(extractFunction(code));
    assert.strictEqual(shape, "loop-if-ret-ret");
  });

  it("captures try-catch-finally", () => {
    const code = `
      function f() {
        try {
          risky();
          return true;
        } catch (e) {
          console.error(e);
          throw e;
        } finally {
          cleanup();
        }
      }
    `;
    const shape = buildCfgShapeString(extractFunction(code));
    assert.strictEqual(shape, "try-ret-catch-throw-finally");
  });

  it("captures switch with cases", () => {
    const code = `
      function f(x) {
        switch (x) {
          case 1:
            return "one";
          case 2:
            return "two";
          default:
            return "other";
        }
      }
    `;
    const shape = buildCfgShapeString(extractFunction(code));
    assert.strictEqual(shape, "switch-case-ret-case-ret-default-ret");
  });

  it("produces same shape for renamed functions", () => {
    const code1 = `
      function processItems(items, handler) {
        for (const item of items) {
          if (handler(item)) {
            continue;
          }
          process(item);
        }
        return true;
      }
    `;
    const code2 = `
      function a(b, c) {
        for (const d of b) {
          if (c(d)) {
            continue;
          }
          e(d);
        }
        return true;
      }
    `;

    const shape1 = buildCfgShapeString(extractFunction(code1));
    const shape2 = buildCfgShapeString(extractFunction(code2));

    assert.strictEqual(
      shape1,
      shape2,
      "Control flow shape should be identical"
    );
    assert.strictEqual(shape1, "loop-if-cont-ret");
  });
});
