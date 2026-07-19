import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { statementHash } from "./statement-hash.js";

function hashOf(code: string): string {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false
  }) as t.File | null;
  assert.ok(ast, `${code} must parse`);
  assert.strictEqual(ast.program.body.length, 1, "one statement per fixture");
  return statementHash(ast.program.body[0]);
}

describe("statementHash", () => {
  it("is rename-invariant across bindings, params, callees and member names", () => {
    assert.strictEqual(
      hashOf('function a(x) { return x.foo + b(1) + "lit"; }'),
      hashOf('function q(y) { return y.bar + z(1) + "lit"; }')
    );
  });

  it("masks object keys too (structure + literals only, by design)", () => {
    assert.strictEqual(
      hashOf("var a = { p: 1 };"),
      hashOf("var b = { q: 1 };")
    );
  });

  it("distinguishes string literals", () => {
    assert.notStrictEqual(hashOf('f("one");'), hashOf('f("two");'));
  });

  it("distinguishes numeric literals", () => {
    assert.notStrictEqual(hashOf("var a = 1;"), hashOf("var a = 2;"));
  });

  it("distinguishes template literal content", () => {
    assert.notStrictEqual(
      hashOf(`var a = \`x\${b}\`;`),
      hashOf(`var a = \`y\${b}\`;`)
    );
  });

  it("distinguishes operators", () => {
    assert.notStrictEqual(hashOf("var a = b + c;"), hashOf("var a = b - c;"));
    assert.notStrictEqual(hashOf("a++;"), hashOf("++a;"));
  });

  it("distinguishes declaration kinds", () => {
    assert.notStrictEqual(hashOf("var a = 1;"), hashOf("let a = 1;"));
  });

  it("distinguishes dot access from computed access", () => {
    assert.notStrictEqual(hashOf("f(a.b);"), hashOf('f(a["b"]);'));
  });

  it("distinguishes array holes from their hole-free spelling", () => {
    assert.notStrictEqual(
      hashOf("var a = [1, , 2];"),
      hashOf("var a = [1, 2];")
    );
  });

  it("survives a deep minified-style expression chain without overflowing", () => {
    const chain = Array.from({ length: 5000 }, (_, i) => `${i}`).join(" + ");
    const h = hashOf(`var total = ${chain};`);
    assert.match(h, /^[0-9a-f]{16}$/);
  });
});
