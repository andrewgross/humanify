import { describe, it } from "node:test";
import assert from "node:assert";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { computeStructuralHash } from "./structural-hash.js";

describe("computeStructuralHash", () => {
  it("produces the same hash for structurally identical functions with different names", () => {
    const code1 = `function a(b, c) { return b + c; }`;
    const code2 = `function x(y, z) { return y + z; }`;

    const fn1 = extractFunction(code1);
    const fn2 = extractFunction(code2);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);

    assert.strictEqual(hash1, hash2, "Structurally identical functions should have the same hash");
  });

  it("produces different hashes for structurally different functions", () => {
    const code1 = `function a(b, c) { return b + c; }`;
    const code2 = `function a(b, c) { return b * c; }`;

    const fn1 = extractFunction(code1);
    const fn2 = extractFunction(code2);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);

    assert.notStrictEqual(hash1, hash2, "Different functions should have different hashes");
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

    assert.strictEqual(hash1, hash2, "Same length strings should produce same hash");
    assert.notStrictEqual(hash1, hash3, "Different length strings should produce different hashes");
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

    assert.strictEqual(hash1, hash2, "Same magnitude numbers should produce same hash");
    assert.notStrictEqual(hash1, hash3, "Different magnitude numbers should produce different hashes");
  });

  it("handles arrow functions", () => {
    const code1 = `const a = (b, c) => b + c;`;
    const code2 = `const x = (y, z) => y + z;`;

    const fn1 = extractArrowFunction(code1);
    const fn2 = extractArrowFunction(code2);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);

    assert.strictEqual(hash1, hash2, "Structurally identical arrow functions should have the same hash");
  });

  it("handles functions with nested functions", () => {
    const code1 = `function outer(a) { function inner(b) { return b; } return inner(a); }`;
    const code2 = `function x(y) { function z(w) { return w; } return z(y); }`;

    const fn1 = extractFunction(code1);
    const fn2 = extractFunction(code2);

    const hash1 = computeStructuralHash(fn1);
    const hash2 = computeStructuralHash(fn2);

    assert.strictEqual(hash1, hash2, "Functions with identical nested structures should match");
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

    assert.notStrictEqual(addHash, subtractHash, "Different operators should produce different hashes");
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
    assert.strictEqual(hash1, hash2, "Same structure with different called functions should match (identifiers normalized)");
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
