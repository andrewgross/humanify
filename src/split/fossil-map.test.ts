import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { extractFossilModules } from "./fossil-map.js";
import { statementHash } from "./statement-hash.js";

/** Parse a wrapper-body snippet into its statement list. */
function bodyOf(code: string): t.Statement[] {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File;
  return ast.program.body;
}

/**
 * A minimal lazy bundle in the fossil grammar (exp068 SPEC rule 2):
 * an `__esm`-shaped helper, two module segments whose inits carry the
 * import edge, and an eager tail statement.
 */
const LAZY_BUNDLE = [
  // eager zone: the helper itself (arrow of 2 args returning a thunk
  // whose sequence ends in an identifier = the "esm" helper shape)
  "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);",
  // module A segment: hoisted decl + hoisted var + init def (terminator)
  "function alphaCore(x) { return x + 1; }",
  "var alphaState;",
  "var init_alpha = __esm(() => { alphaState = alphaCore(1); });",
  // module B segment: imports A via leading init call
  "function betaRender(y) { return alphaState + y; }",
  "var betaCache;",
  "var init_beta = __esm(() => { init_alpha(); betaCache = betaRender(2); });",
  // eager tail: entry-point code, not in any segment
  "console.log(init_beta);"
].join("\n");

describe("extractFossilModules", () => {
  const body = bodyOf(LAZY_BUNDLE);
  const hashes = body.map(statementHash);

  it("finds one module per init def, segments ended by the init", () => {
    const extract = extractFossilModules(body, hashes);
    assert.strictEqual(extract.modules.length, 2);
    // Module A: statements 1..3 (the helper at 0 precedes the first
    // segment only because nothing separates it — SPEC rule 2 assigns
    // everything since the previous init to the segment; the leading
    // helper is part of segment A's span).
    assert.deepStrictEqual(extract.modules[0].statements, [0, 1, 2, 3]);
    assert.deepStrictEqual(extract.modules[1].statements, [4, 5, 6]);
  });

  it("reads the import graph from leading init calls", () => {
    const extract = extractFossilModules(body, hashes);
    assert.deepStrictEqual(extract.modules[0].imports, []);
    assert.deepStrictEqual(extract.modules[1].imports, [0]);
  });

  it("collects declared names per module and the eager tail", () => {
    const extract = extractFossilModules(body, hashes);
    assert.ok(extract.modules[1].declared.includes("betaRender"));
    assert.ok(extract.modules[1].declared.includes("betaCache"));
    assert.deepStrictEqual(extract.eagerZone, [7]);
  });

  it("throws on a hashes/body length mismatch", () => {
    assert.throws(() => extractFossilModules(body, hashes.slice(1)));
  });

  it("returns zero modules for a fossil-free (fully eager) body", () => {
    const eager = bodyOf("var a = 1;\nvar b = a + 1;\nconsole.log(b);");
    const extract = extractFossilModules(eager, eager.map(statementHash));
    assert.strictEqual(extract.modules.length, 0);
    assert.deepStrictEqual(extract.eagerZone, [0, 1, 2]);
  });
});

describe("fossil map — esbuild unminified form (exp075)", () => {
  // esbuild wraps a lazy module as an OBJECT with one keyed method, and its
  // __esm helper thunk is a FunctionExpression rather than bun's arrow:
  //   var __esm = (fn, res) => function __init() { return fn && (…), res; };
  //   var init_format = __esm({ "src/utils/format.js"() { … } });
  // The key IS the original source path — ground truth the minified form loses.
  const esbuildBundle = `
    var __esm = (fn, res) => function __init() {
      return fn && (res = (0, fn[Object.getOwnPropertyNames(fn)[0]])(fn = 0)), res;
    };
    var helper;
    var init_helper = __esm({
      "src/utils/helper.js"() {
        helper = () => 1;
      }
    });
    var main;
    var init_main = __esm({
      "src/main.js"() {
        init_helper();
        main = () => helper() + 1;
      }
    });
  `;

  it("finds modules in esbuild's object form", () => {
    const body = bodyOf(esbuildBundle);
    const ex = extractFossilModules(
      body,
      body.map((_, i) => `h${i}`)
    );
    assert.strictEqual(ex.modules.length, 2);
  });

  it("recovers the import edge from the leading init call", () => {
    const body = bodyOf(esbuildBundle);
    const ex = extractFossilModules(
      body,
      body.map((_, i) => `h${i}`)
    );
    const main = ex.modules[1];
    assert.deepStrictEqual([...main.imports], [0]);
  });

  it("captures the source path from the object key", () => {
    const body = bodyOf(esbuildBundle);
    const ex = extractFossilModules(
      body,
      body.map((_, i) => `h${i}`)
    );
    assert.deepStrictEqual(
      ex.modules.map((m) => m.sourcePath),
      ["src/utils/helper.js", "src/main.js"]
    );
  });
});
