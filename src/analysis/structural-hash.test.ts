import assert from "node:assert";
import { describe, it } from "node:test";
import type { NodePath } from "@babel/core";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import {
  buildCfgShapeString,
  buildPlaceholderMapping,
  computeBindingFingerprint,
  computeFingerprint,
  computeStructuralHash,
  computeStructuralSignature,
  extractStructuralFeatures,
  serializePathTokens
} from "./structural-hash.js";

describe("computeStructuralHash", () => {
  it("produces the same hash for structurally identical functions with different names", () => {
    const code1 = `function a(b, c) { return b + c; }`;
    const code2 = `function x(y, z) { return y + z; }`;

    assert.strictEqual(
      computeStructuralHash(fnPath(code1)),
      computeStructuralHash(fnPath(code2)),
      "Structurally identical functions should have the same hash"
    );
  });

  it("produces different hashes for structurally different functions", () => {
    const code1 = `function a(b, c) { return b + c; }`;
    const code2 = `function a(b, c) { return b * c; }`;

    assert.notStrictEqual(
      computeStructuralHash(fnPath(code1)),
      computeStructuralHash(fnPath(code2)),
      "Different functions should have different hashes"
    );
  });

  it("normalizes string literals to length markers", () => {
    const code1 = `function f() { return "hello"; }`;
    const code2 = `function f() { return "world"; }`;
    const code3 = `function f() { return "hi"; }`;

    const hash1 = computeStructuralHash(fnPath(code1));
    const hash2 = computeStructuralHash(fnPath(code2));
    const hash3 = computeStructuralHash(fnPath(code3));

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

    const hash1 = computeStructuralHash(fnPath(code1));
    const hash2 = computeStructuralHash(fnPath(code2));
    const hash3 = computeStructuralHash(fnPath(code3));

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

    assert.strictEqual(
      computeStructuralHash(fnPath(code1)),
      computeStructuralHash(fnPath(code2)),
      "Structurally identical arrow functions should have the same hash"
    );
  });

  it("handles functions with nested functions", () => {
    const code1 = `function outer(a) { function inner(b) { return b; } return inner(a); }`;
    const code2 = `function x(y) { function z(w) { return w; } return z(y); }`;

    assert.strictEqual(
      computeStructuralHash(fnPath(code1)),
      computeStructuralHash(fnPath(code2)),
      "Functions with identical nested structures should match"
    );
  });

  it("does not mutate the original AST when computing hash", () => {
    const code = "function f(x) { throw TypeError(`Invalid: ${x}`); }";
    const fn = fnPath(code);

    // Verify the original template literal content before hashing
    const body = (fn.node.body as t.BlockStatement).body;
    const throwStmt = body[0] as t.ThrowStatement;
    const callExpr = throwStmt.argument as t.CallExpression;
    const tpl = callExpr.arguments[0] as t.TemplateLiteral;

    assert.strictEqual(tpl.quasis[0].value.raw, "Invalid: ");
    assert.strictEqual(tpl.quasis[1].value.raw, "");

    computeStructuralHash(fn);

    // Verify template literal content is unchanged
    assert.strictEqual(
      tpl.quasis[0].value.raw,
      "Invalid: ",
      "Template literal quasi should not be mutated by hashing"
    );
    assert.strictEqual(
      tpl.quasis[1].value.raw,
      "",
      "Template literal quasi should not be mutated by hashing"
    );
  });

  it("produces a 16-character hex hash", () => {
    const code = `function f(x) { return x * 2; }`;
    const hash = computeStructuralHash(fnPath(code));

    assert.strictEqual(hash.length, 16, "Hash should be 16 characters");
    assert.match(hash, /^[0-9a-f]+$/, "Hash should be hexadecimal");
  });

  it("produces different hashes for different operators", () => {
    const addCode = `function add(a, b) { return a + b; }`;
    const subtractCode = `function subtract(a, b) { return a - b; }`;

    assert.notStrictEqual(
      computeStructuralHash(fnPath(addCode)),
      computeStructuralHash(fnPath(subtractCode)),
      "Different operators should produce different hashes"
    );
  });

  it("produces different hashes for calls to different free functions", () => {
    const code1 = `function f() { return fetch(); }`;
    const code2 = `function f() { return save(); }`;

    // fetch/save are free (undeclared) — version-stable content that
    // should discriminate. Bound callees still normalize per binding.
    assert.notStrictEqual(
      computeStructuralHash(fnPath(code1)),
      computeStructuralHash(fnPath(code2)),
      "Different free callees should produce different hashes"
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

describe("buildPlaceholderMapping", () => {
  it("assigns placeholders in AST traversal order", () => {
    const code = `function foo(a, b) { return a + b; }`;
    const mapping = buildPlaceholderMapping(fnPath(code));

    // foo is $0 (function name encountered first), a is $1, b is $2
    assert.strictEqual(mapping.get("$0"), "foo");
    assert.strictEqual(mapping.get("$1"), "a");
    assert.strictEqual(mapping.get("$2"), "b");
  });

  it("two structurally identical functions produce same placeholder order", () => {
    const code1 = `function foo(a, b) { return a + b; }`;
    const code2 = `function bar(x, y) { return x + y; }`;

    const mapping1 = buildPlaceholderMapping(fnPath(code1));
    const mapping2 = buildPlaceholderMapping(fnPath(code2));

    // Both should have exactly 3 placeholders in same positions
    assert.strictEqual(mapping1.size, mapping2.size);
    assert.strictEqual(mapping1.size, 3);
    // $0 maps to the function name, $1 to first param, $2 to second param
    assert.strictEqual(mapping1.get("$0"), "foo");
    assert.strictEqual(mapping2.get("$0"), "bar");
    assert.strictEqual(mapping1.get("$1"), "a");
    assert.strictEqual(mapping2.get("$1"), "x");
  });

  it("deduplicates repeated references to one binding", () => {
    const code = `function f(x) { return x + x; }`;
    const mapping = buildPlaceholderMapping(fnPath(code));

    // f=$0, x=$1 — x appears 3 times but only gets one placeholder
    assert.strictEqual(mapping.size, 2);
    assert.strictEqual(mapping.get("$0"), "f");
    assert.strictEqual(mapping.get("$1"), "x");
  });

  it("handles arrow functions with expression body", () => {
    const code = `const add = (a, b) => a + b;`;
    const mapping = buildPlaceholderMapping(fnPath(code));

    // Arrow functions have no id, so params come first: a=$0, b=$1
    assert.strictEqual(mapping.get("$0"), "a");
    assert.strictEqual(mapping.get("$1"), "b");
  });
});

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

  it("ignores computed member calls — the property is a binding name, not stable content", () => {
    // x[cb]() references the BINDING cb; recording "*.cb" would make
    // externalCalls rename-variant (minified j vs humanified callback).
    const code = `
      function f(x, cb) {
        x[cb]();
        return x.map(cb);
      }
    `;

    const features = extractStructuralFeatures(extractFunction(code));

    assert.ok(
      !features.externalCalls.some((c) => c.includes("cb")),
      `computed member call must not record the binding name, got ${JSON.stringify(features.externalCalls)}`
    );
    assert.ok(
      features.externalCalls.includes("*.map"),
      "non-computed method call still recorded"
    );
  });

  it("serializes rename-equivalent functions to identical token streams", () => {
    // serializePathTokens is the hash's raw material and the divergence-
    // inspection tool's substrate: same hash means same stream, and
    // diffing streams pinpoints WHY two hashes differ.
    const a = serializePathTokens(fnPath(`function f(u) { return u + 1; }`));
    const b = serializePathTokens(
      fnPath(`function g(count) { return count + 1; }`)
    );
    assert.deepStrictEqual(a, b);
    assert.ok(
      a.some((token) => token.startsWith("$")),
      "binding occurrences serialize as slots"
    );

    const c = serializePathTokens(fnPath(`function f(u) { return u - 1; }`));
    assert.notDeepStrictEqual(a, c, "operators are content");
  });

  it("hashes shorthand and longhand object properties identically", () => {
    // Renaming a shorthand binding forces Babel to expand {u} → {u: userId}
    // (the key must keep its external name). If the shorthand flag is hash
    // content, every function containing a renamed shorthand property
    // changes hash after humanify+regenerate — the 0.4% instability that
    // starves cross-version matching.
    const minified = `function f(u) { return { u, kind: 1 }; }`;
    const humanified = `function f(userId) { return { u: userId, kind: 1 }; }`;

    assert.strictEqual(
      computeStructuralHash(fnPath(minified)),
      computeStructuralHash(fnPath(humanified)),
      "shorthand expansion is a rename artifact, not a structural change"
    );
  });

  it("hashes shorthand and longhand destructuring patterns identically", () => {
    const minified = `function g({ u }) { return u; }`;
    const humanified = `function g({ u: userId }) { return userId; }`;

    assert.strictEqual(
      computeStructuralHash(fnPath(minified)),
      computeStructuralHash(fnPath(humanified))
    );
  });

  it("still distinguishes crossed shorthand values", () => {
    // {u, w} pairs key u with binding u; {u: w, w: u} crosses them.
    // Dropping the shorthand flag must not conflate these — the key
    // (verbatim) and value (slot) are serialized independently.
    const straight = `function h(u, w) { return { u, w }; }`;
    const crossed = `function h(u, w) { return { u: w, w: u }; }`;

    assert.notStrictEqual(
      computeStructuralHash(fnPath(straight)),
      computeStructuralHash(fnPath(crossed))
    );
  });

  it("does not classify $ as a known global — it is a minifier-alphabet name", () => {
    // esbuild assigns $ as a minified binding name; treating it as jQuery
    // records "$" / "$.call" in one version and nothing / "*.call" in the
    // next, making externalCalls rename-variant.
    const code = `
      function f(x) {
        $(x);
        return $.call(null, x);
      }
    `;

    const features = extractStructuralFeatures(extractFunction(code));

    assert.ok(
      !features.externalCalls.includes("$"),
      "bare $ call must not be recorded as a global"
    );
    assert.ok(
      features.externalCalls.includes("*.call"),
      `$.call records the stable method name only, got ${JSON.stringify(features.externalCalls)}`
    );
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

/** Fingerprint features for the first function in `code`, asserted present. */
function fingerprintFeatures(code: string) {
  const features = computeFingerprint(fnPath(code)).features;
  assert.ok(features, "computeFingerprint must carry features");
  return features;
}

describe("computeFingerprint externalCalls boundness", () => {
  it("records *.method when the callee object is a resolved binding named like a global", () => {
    // The prior humanified leg names a module binding `React`; the new
    // minified leg calls the same binding `Op`. A KNOWN_GLOBALS name must
    // not leak into externalCalls when the object is BOUND — otherwise
    // the two sides' features disagree and the singleton corroboration
    // gate rejects a true twin pair (5 confirmed cases in exp014).
    const prior = `
      const React = makeVendor();
      function target(ctx) { return React.useContext(ctx); }
    `;
    const next = `
      const Op = makeVendor();
      function target(ctx) { return Op.useContext(ctx); }
    `;

    const priorFeatures = fingerprintFeatures(prior);
    const nextFeatures = fingerprintFeatures(next);

    assert.deepStrictEqual(
      priorFeatures.externalCalls,
      nextFeatures.externalCalls,
      `externalCalls must be rename-invariant across binding renames, got prior=${JSON.stringify(priorFeatures.externalCalls)} next=${JSON.stringify(nextFeatures.externalCalls)}`
    );
    assert.ok(
      priorFeatures.externalCalls.includes("*.useContext"),
      `bound object records the stable method form, got ${JSON.stringify(priorFeatures.externalCalls)}`
    );
  });

  it("keeps the global form for a truly free object reference", () => {
    const code = `function g(ctx) { return React.useContext(ctx); }`;
    const features = fingerprintFeatures(code);
    assert.ok(
      features.externalCalls.includes("React.useContext"),
      `free known-global object keeps its name, got ${JSON.stringify(features.externalCalls)}`
    );
  });

  it("does not record a call through a bound identifier as a global call", () => {
    const code = `
      const fetch = makeFetcher();
      function t(x) { return fetch(x); }
    `;
    const features = fingerprintFeatures(code);
    assert.ok(
      !features.externalCalls.includes("fetch"),
      `a local binding shadowing a known global is not an external call, got ${JSON.stringify(features.externalCalls)}`
    );
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

// ---------------------------------------------------------------------------
// Helpers for binding fingerprint tests
// ---------------------------------------------------------------------------

function exprPath(code: string): NodePath<t.Expression> {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse code");
  let found: NodePath<t.Expression> | null = null;
  traverse(ast, {
    ExpressionStatement(p: NodePath<t.ExpressionStatement>) {
      if (!found) found = p.get("expression") as NodePath<t.Expression>;
      p.stop();
    }
  });
  if (!found) throw new Error("No expression found");
  return found;
}

describe("computeBindingFingerprint", () => {
  it("var a = 1 and var b = 1 produce same hash (different var names)", () => {
    const fp1 = computeBindingFingerprint(declInitPath("var a = 1;"));
    const fp2 = computeBindingFingerprint(declInitPath("var b = 1;"));
    assert.ok(fp1);
    assert.ok(fp2);
    assert.strictEqual(fp1.structuralHash, fp2.structuralHash);
  });

  it("structurally different inits produce different hashes", () => {
    const fp1 = computeBindingFingerprint(declInitPath("var a = [1, 2, 3];"));
    const fp2 = computeBindingFingerprint(declInitPath("var a = { x: 1 };"));
    assert.ok(fp1);
    assert.ok(fp2);
    assert.notStrictEqual(fp1.structuralHash, fp2.structuralHash);
  });

  it("different numeric values produce different hashes (no magnitude bucketing)", () => {
    const fp1 = computeBindingFingerprint(declInitPath("var a = 4;"));
    const fp2 = computeBindingFingerprint(declInitPath("var b = 2;"));
    assert.ok(fp1);
    assert.ok(fp2);
    assert.notStrictEqual(
      fp1.structuralHash,
      fp2.structuralHash,
      "4 and 2 should hash differently for bindings"
    );
  });

  it("different string values produce different hashes (no length bucketing)", () => {
    const fp1 = computeBindingFingerprint(declInitPath('var a = "hello";'));
    const fp2 = computeBindingFingerprint(declInitPath('var b = "world";'));
    assert.ok(fp1);
    assert.ok(fp2);
    assert.notStrictEqual(
      fp1.structuralHash,
      fp2.structuralHash,
      "different strings should hash differently for bindings"
    );
  });

  it("same string values produce same hash", () => {
    const fp1 = computeBindingFingerprint(declInitPath('var a = "hello";'));
    const fp2 = computeBindingFingerprint(declInitPath('var b = "hello";'));
    assert.ok(fp1);
    assert.ok(fp2);
    assert.strictEqual(fp1.structuralHash, fp2.structuralHash);
  });

  it("calls to renamed BOUND functions produce same hash", () => {
    const fp1 = computeBindingFingerprint(
      declInitPath("var fn = () => 1; var a = fn();", 1)
    );
    const fp2 = computeBindingFingerprint(
      declInitPath("var gn = () => 1; var b = gn();", 1)
    );
    assert.ok(fp1);
    assert.ok(fp2);
    assert.strictEqual(
      fp1.structuralHash,
      fp2.structuralHash,
      "bound callees normalize per binding"
    );
  });

  it("hashes first assignment RHS when init is null", () => {
    const fp = computeBindingFingerprint(null, exprPath("new WeakMap()"));
    assert.ok(fp);
    assert.strictEqual(fp.hashSource, "assignment");
    assert.strictEqual(fp.structuralHash.length, 16);
  });

  it("returns null when no init and no assignment", () => {
    const fp = computeBindingFingerprint(null);
    assert.strictEqual(fp, null);
  });

  it("arrow function init hashes correctly", () => {
    const fp1 = computeBindingFingerprint(
      declInitPath("var a = (x) => x != null;")
    );
    const fp2 = computeBindingFingerprint(
      declInitPath("var b = (y) => y != null;")
    );
    assert.ok(fp1);
    assert.ok(fp2);
    assert.strictEqual(fp1.structuralHash, fp2.structuralHash);
    assert.strictEqual(fp1.hashSource, "init");
  });
});

describe("computeBindingFingerprint property keys", () => {
  it("same property key, different bound value identifiers → same hash", () => {
    const fp1 = computeBindingFingerprint(
      declInitPath("var x = 1; var a = { prop: x };", 1)
    );
    const fp2 = computeBindingFingerprint(
      declInitPath("var y = 1; var b = { prop: y };", 1)
    );
    assert.ok(fp1);
    assert.ok(fp2);
    assert.strictEqual(
      fp1.structuralHash,
      fp2.structuralHash,
      "Same property key with different bound value identifiers should produce same hash"
    );
  });

  it("different property keys → different hashes", () => {
    const fp1 = computeBindingFingerprint(
      declInitPath("var x = 1; var a = { propA: x };", 1)
    );
    const fp2 = computeBindingFingerprint(
      declInitPath("var x = 1; var b = { propB: x };", 1)
    );
    assert.ok(fp1);
    assert.ok(fp2);
    assert.notStrictEqual(
      fp1.structuralHash,
      fp2.structuralHash,
      "Different property keys should produce different hashes"
    );
  });

  it("computed property keys with bound identifiers are still normalized", () => {
    const fp1 = computeBindingFingerprint(
      declInitPath("var x = 1; var a = { [x]: 1 };", 1)
    );
    const fp2 = computeBindingFingerprint(
      declInitPath("var y = 1; var b = { [y]: 1 };", 1)
    );
    assert.ok(fp1);
    assert.ok(fp2);
    assert.strictEqual(
      fp1.structuralHash,
      fp2.structuralHash,
      "Computed property keys should still be normalized (bound identifiers replaced)"
    );
  });
});

// ---------------------------------------------------------------------------
// Rename-invariant hashing (binding-keyed placeholders) — the C10 fix.
// A valid rename of BINDINGS must never change a structural hash, and
// non-binding identifiers (property names, object keys, free globals) are
// version-stable content that should discriminate, not normalize away.
// Evidence: experiments/013-bun-cjs-classification/CLOSE-MATCH-ANOMALY.md
// ---------------------------------------------------------------------------

function fnPath(code: string, nth = 0): NodePath<t.Function> {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse code");
  const paths: NodePath<t.Function>[] = [];
  traverse(ast, {
    Function(p: NodePath<t.Function>) {
      paths.push(p);
    }
  });
  const found = paths[nth];
  if (!found) throw new Error(`No function at index ${nth}`);
  return found;
}

function declInitPath(code: string, nth = 0): NodePath<t.Expression> {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse code");
  const paths: NodePath<t.Expression>[] = [];
  traverse(ast, {
    VariableDeclarator(p: NodePath<t.VariableDeclarator>) {
      const init = p.get("init");
      if (init.node) paths.push(init as NodePath<t.Expression>);
    }
  });
  const found = paths[nth];
  if (!found) throw new Error(`No declarator init at index ${nth}`);
  return found;
}

describe("computeStructuralHash rename invariance (binding-keyed)", () => {
  it("split: diversifying a name reused across sibling scopes keeps the hash", () => {
    // Minifier reused `e` for two distinct bindings; humanify diversifies.
    const minified = `function f() { const g = (e) => e + 1; const h = (e) => e * 2; return g(1) + h(2); }`;
    const humanified = `function f() { const g = (item) => item + 1; const h = (event) => event * 2; return g(1) + h(2); }`;
    assert.strictEqual(
      computeStructuralHash(fnPath(minified)),
      computeStructuralHash(fnPath(humanified)),
      "diversifying reused names must not change the hash"
    );
  });

  it("merge: renaming distinct bindings to the same name keeps the hash", () => {
    const minified = `function f() { const g = (a) => a + 1; const h = (b) => b * 2; return g(1) + h(2); }`;
    const humanified = `function f() { const g = (n) => n + 1; const h = (n) => n * 2; return g(1) + h(2); }`;
    assert.strictEqual(
      computeStructuralHash(fnPath(minified)),
      computeStructuralHash(fnPath(humanified)),
      "unifying distinct names across scopes must not change the hash"
    );
  });

  it("property collision: renaming a binding to a property's name keeps the hash", () => {
    // The dominant real-world merge: LLM names a binding `cache` and the
    // function also accesses a `.cache` property.
    const minified = `function f(c) { return c.cache.get(1); }`;
    const humanified = `function f(cache) { return cache.cache.get(1); }`;
    assert.strictEqual(
      computeStructuralHash(fnPath(minified)),
      computeStructuralHash(fnPath(humanified)),
      "binding↔property name collisions must not change the hash"
    );
  });

  it("renaming a named function expression's self-reference keeps the hash", () => {
    const minified = `function f() { const r = function s() { return s; }; return r; }`;
    const humanified = `function f() { const retry = function walker() { return walker; }; return retry; }`;
    assert.strictEqual(
      computeStructuralHash(fnPath(minified)),
      computeStructuralHash(fnPath(humanified)),
      "fn-expression self-binding renames must not change the hash"
    );
  });

  it("label names never affect the hash", () => {
    const code1 = `function f() { x: for (;;) { break x; } }`;
    const code2 = `function f() { y: for (;;) { break y; } }`;
    assert.strictEqual(
      computeStructuralHash(fnPath(code1)),
      computeStructuralHash(fnPath(code2)),
      "labels are renamed by minifiers and must normalize"
    );
  });

  it("property names distinguish structure (no more property erasure)", () => {
    const code1 = `function f(m) { return m.get(1); }`;
    const code2 = `function f(m) { return m.delete(1); }`;
    assert.notStrictEqual(
      computeStructuralHash(fnPath(code1)),
      computeStructuralHash(fnPath(code2)),
      "minifier-stable property names should discriminate"
    );
  });

  it("free identifiers (globals) distinguish structure", () => {
    const code1 = `function f() { return console.log(1); }`;
    const code2 = `function f() { return window.log(1); }`;
    assert.notStrictEqual(
      computeStructuralHash(fnPath(code1)),
      computeStructuralHash(fnPath(code2)),
      "free names are version-stable and should discriminate"
    );
  });

  it("plain bound-identifier renames still normalize (regression)", () => {
    const code1 = `function a(b, c) { return b + c; }`;
    const code2 = `function x(y, z) { return y + z; }`;
    assert.strictEqual(
      computeStructuralHash(fnPath(code1)),
      computeStructuralHash(fnPath(code2))
    );
  });
});

describe("buildPlaceholderMapping (binding slots only)", () => {
  it("contains only binding names — no property names, keys, or globals", () => {
    const code = `function f(a) { return a.length + JSON.parse(a); }`;
    const mapping = buildPlaceholderMapping(fnPath(code));
    const names = new Set(mapping.values());
    assert.strictEqual(
      mapping.size,
      2,
      `only f and a are bindings, got: ${[...mapping.entries()].map(([k, v]) => `${k}→${v}`).join(", ")}`
    );
    assert.ok(names.has("f"));
    assert.ok(names.has("a"));
  });

  it("shadowing bindings with the same name get distinct slots", () => {
    const code = `function f(a) { const g = (a) => a * 2; return g(a); }`;
    const mapping = buildPlaceholderMapping(fnPath(code));
    // f, outer a, g, inner a — four bindings, four slots
    assert.strictEqual(mapping.size, 4);
    const aSlots = [...mapping.entries()].filter(([, name]) => name === "a");
    assert.strictEqual(aSlots.length, 2, "both `a` bindings get own slots");
  });

  it("ordinals align across a renamed exact pair", () => {
    const minified = `function f(a, b) { const c = a.map((x) => x + b); return c; }`;
    const humanified = `function getData(list, offset) { const result = list.map((item) => item + offset); return result; }`;
    const p1 = fnPath(minified);
    const p2 = fnPath(humanified);
    assert.strictEqual(
      computeStructuralHash(p1),
      computeStructuralHash(p2),
      "pair must exact-match"
    );
    const m1 = buildPlaceholderMapping(p1);
    const m2 = buildPlaceholderMapping(p2);
    assert.strictEqual(m1.size, 5, "f, a, b, c, x");
    assert.strictEqual(m1.size, m2.size);
    for (const [placeholder, minName] of m1) {
      const humanName = m2.get(placeholder);
      assert.ok(
        humanName,
        `slot ${placeholder} (${minName}) must exist on both sides`
      );
    }
    // Spot-check the translation this feeds (translatePriorNames semantics)
    assert.strictEqual(m1.get("$0"), "f");
    assert.strictEqual(m2.get("$0"), "getData");
    assert.strictEqual(m1.get("$1"), "a");
    assert.strictEqual(m2.get("$1"), "list");
  });
});

describe("computeBindingFingerprint rename invariance (binding-keyed)", () => {
  it("renaming a bound identifier that collides with a property keeps the hash", () => {
    const s1 = `var helper = (v) => v; var a = { run: (x) => helper(x.helper) };`;
    const s2 = `var doIt = (v) => v; var b = { run: (x) => doIt(x.helper) };`;
    const fp1 = computeBindingFingerprint(declInitPath(s1, 1));
    const fp2 = computeBindingFingerprint(declInitPath(s2, 1));
    assert.ok(fp1 && fp2);
    assert.strictEqual(
      fp1.structuralHash,
      fp2.structuralHash,
      "bound-name↔property-name collisions must not change binding hashes"
    );
  });

  it("free identifiers in inits are content and discriminate", () => {
    const fp1 = computeBindingFingerprint(declInitPath("var a = fn();"));
    const fp2 = computeBindingFingerprint(declInitPath("var b = gn();"));
    assert.ok(fp1 && fp2);
    assert.notStrictEqual(
      fp1.structuralHash,
      fp2.structuralHash,
      "undeclared (free) callees are version-stable content"
    );
  });

  it("member property names in inits discriminate", () => {
    const s1 = `var q = { x: 1 }; var a = () => q.parse;`;
    const s2 = `var q = { x: 1 }; var b = () => q.stringify;`;
    const fp1 = computeBindingFingerprint(declInitPath(s1, 1));
    const fp2 = computeBindingFingerprint(declInitPath(s2, 1));
    assert.ok(fp1 && fp2);
    assert.notStrictEqual(fp1.structuralHash, fp2.structuralHash);
  });
});

describe("computeStructuralSignature", () => {
  function sig(code: string): string {
    const ast = parseSync(code, { sourceType: "unambiguous" });
    if (!ast) throw new Error("parse failed");
    let programPath: NodePath | undefined;
    traverse(ast, {
      Program(p) {
        programPath = p;
        p.stop();
      }
    });
    if (!programPath) throw new Error("no program path");
    return computeStructuralSignature(programPath);
  }

  it("is identical for programs differing only in binding names", () => {
    assert.strictEqual(
      sig(`function f(a) { let b = a + 1; return g(b); }`),
      sig(`function h(x) { let y = x + 1; return g(y); }`)
    );
  });

  it("is stable when a shorthand binding is renamed (expands to longhand)", () => {
    // Renaming `a` turns `{ a }` into `{ a: renamed }`; that expansion is a
    // rename artifact, not a structural change, and must NOT trip the signature.
    assert.strictEqual(
      sig(`function f() { let a = 1; return { a, k: 2 }; }`),
      sig(`function f() { let renamed = 1; return { a: renamed, k: 2 }; }`)
    );
  });

  it("changes when a numeric literal changes", () => {
    assert.notStrictEqual(
      sig(`function f(a) { return a + 1; }`),
      sig(`function f(a) { return a + 2; }`)
    );
  });

  it("changes when a string literal changes", () => {
    assert.notStrictEqual(
      sig(`const v = "2.1.119";`),
      sig(`const v = "2.1.120";`)
    );
  });

  it("changes when an operator changes", () => {
    assert.notStrictEqual(
      sig(`function f(a) { return a + 1; }`),
      sig(`function f(a) { return a - 1; }`)
    );
  });

  it("changes when a property key changes (keys are verbatim)", () => {
    assert.notStrictEqual(
      sig(`function f(x) { return x.foo; }`),
      sig(`function f(x) { return x.bar; }`)
    );
  });

  it("changes when a free/global name changes (free names are verbatim)", () => {
    assert.notStrictEqual(
      sig(`function f(a) { return console.log(a); }`),
      sig(`function f(a) { return window.log(a); }`)
    );
  });

  it("changes when a statement is dropped", () => {
    assert.notStrictEqual(
      sig(`function f(a) { g(); return a; }`),
      sig(`function f(a) { return a; }`)
    );
  });

  it("changes when call arguments are reordered", () => {
    assert.notStrictEqual(
      sig(`function f(a, b) { return g(a, b); }`),
      sig(`function f(a, b) { return g(b, a); }`)
    );
  });
});
