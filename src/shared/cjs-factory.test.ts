import assert from "node:assert/strict";
import { test } from "node:test";
import * as t from "@babel/types";
import { parseFileAst } from "../babel-utils.js";
import {
  factoryCallOf,
  isVendorWorthyBinding,
  stripJsExtension,
  vendorStemFor
} from "./cjs-factory.js";

function declaratorsOf(code: string): t.VariableDeclarator[] {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  const stmt = ast.program.body[0];
  if (!t.isVariableDeclaration(stmt)) throw new Error("not a var decl");
  return stmt.declarations;
}

test("factoryCallOf matches the X = CALLEE(fn) shape and reports params", () => {
  const [cjs] = declaratorsOf("var wcq = d((exports, module) => {});");
  assert.deepEqual(factoryCallOf(cjs), {
    binding: "wcq",
    callee: "d",
    paramCount: 2
  });
  const [esm] = declaratorsOf("var m = R(() => {});");
  assert.deepEqual(factoryCallOf(esm), {
    binding: "m",
    callee: "R",
    paramCount: 0
  });
});

test("factoryCallOf rejects non-factory shapes", () => {
  for (const code of [
    "var a = 5;",
    "var b = d();",
    "var c = d(42);",
    "var e = obj.method(() => {});",
    "var { f } = d((exports) => {});"
  ]) {
    const [decl] = declaratorsOf(code);
    assert.equal(factoryCallOf(decl), null, `must reject: ${code}`);
  }
});

test("isVendorWorthyBinding floors minified residue", () => {
  for (const bad of ["H", "qA", "_", "$2"]) {
    assert.equal(isVendorWorthyBinding(bad), false, `${bad} must fail`);
  }
  for (const good of ["yaml", "DepType", "lib_00e21d2a", "axios"]) {
    assert.equal(isVendorWorthyBinding(good), true, `${good} must pass`);
  }
});

test("vendorStemFor keeps worthy names and hashes the rest", () => {
  assert.equal(vendorStemFor("yaml", "whatever"), "yaml");
  const floored = vendorStemFor("H", "function body text");
  assert.match(floored, /^lib_[0-9a-f]{8}$/);
  // Deterministic: same body, same stem.
  assert.equal(floored, vendorStemFor("H", "function body text"));
});

test("vendorStemFor and stripJsExtension drop a trailing .js (highlight.js.js)", () => {
  assert.equal(stripJsExtension("highlight.js"), "highlight");
  assert.equal(stripJsExtension("highlight.JS"), "highlight");
  assert.equal(stripJsExtension("axios@1.2.3"), "axios@1.2.3");
  assert.equal(vendorStemFor("highlight.js", "x"), "highlight");
});
