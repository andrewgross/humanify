import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import {
  buildOwnedBindingMap,
  collectOwnedBindingInfos,
  collectShadowedBlockBindings
} from "./function-bindings.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}

/** Find a function path by its declared/expression name, or the first function. */
function getFnPath(ast: t.File, name?: string): NodePath<t.Function> {
  let fnPath: NodePath<t.Function> | undefined;
  traverse(ast, {
    Function(path) {
      if (fnPath) return;
      if (!name) {
        fnPath = path;
        return;
      }
      const node = path.node;
      if ("id" in node && node.id?.name === name) {
        fnPath = path;
      }
    }
  });
  if (!fnPath) {
    throw new Error(`No function found${name ? ` named ${name}` : ""}`);
  }
  return fnPath;
}

describe("collectOwnedBindingInfos", () => {
  it("collects params, vars, lets, and the function's own name", () => {
    const ast = parse(
      `function foo(a, b) { var c = 1; let d = 2; return a + b + c + d; }`
    );
    const infos = collectOwnedBindingInfos(getFnPath(ast, "foo"));
    const names = infos.map((i) => i.name).sort();
    assert.deepStrictEqual(names, ["a", "b", "c", "d", "foo"]);
  });

  it("excludes nested function declaration names (they self-name)", () => {
    const ast = parse(
      `function outer(x) { function inner(y) { return y; } return inner(x); }`
    );
    const names = collectOwnedBindingInfos(getFnPath(ast, "outer")).map(
      (i) => i.name
    );
    assert.ok(!names.includes("inner"), `"inner" leaked into ${names}`);
    assert.deepStrictEqual(names.sort(), ["outer", "x"]);
  });

  it("includes a nested function's own declaration name in ITS pass", () => {
    const ast = parse(
      `function outer(x) { function inner(y) { return y; } return inner(x); }`
    );
    const names = collectOwnedBindingInfos(getFnPath(ast, "inner")).map(
      (i) => i.name
    );
    assert.deepStrictEqual(names.sort(), ["inner", "y"]);
  });

  it("includes a named function expression's self-name", () => {
    const ast = parse(
      `const f = function orig(n) { return n > 0 ? orig(n - 1) : 0; };`
    );
    const names = collectOwnedBindingInfos(getFnPath(ast, "orig")).map(
      (i) => i.name
    );
    assert.deepStrictEqual(names.sort(), ["n", "orig"]);
  });

  it("collects arrow function bindings without a self-name", () => {
    const ast = parse(`const f = (a) => { let b = a; return b; };`);
    const names = collectOwnedBindingInfos(getFnPath(ast)).map((i) => i.name);
    assert.deepStrictEqual(names.sort(), ["a", "b"]);
  });

  it("collects block-scoped bindings from nested blocks with their owning scope", () => {
    const ast = parse(`function foo(x) {
      if (x) { let e = 1; use(e); }
      for (const i of x) { use(i); }
      try { go(); } catch (err) { log(err); }
      switch (x) { case 1: { let s = 2; use(s); } }
    }`);
    const infos = collectOwnedBindingInfos(getFnPath(ast, "foo"));
    const names = infos.map((i) => i.name);
    for (const expected of ["e", "i", "err", "s"]) {
      assert.ok(names.includes(expected), `missing "${expected}" in ${names}`);
    }
    for (const info of infos) {
      assert.ok(
        info.scope.bindings[info.name],
        `scope for "${info.name}" does not own the binding`
      );
    }
  });

  it("collects body-scope bindings when params have defaults", () => {
    const ast = parse(`function foo(a = 1) { var b = 2; return a + b; }`);
    const names = collectOwnedBindingInfos(getFnPath(ast, "foo")).map(
      (i) => i.name
    );
    assert.deepStrictEqual(names.sort(), ["a", "b", "foo"]);
  });

  it("keeps only the first of same-named sibling-block bindings", () => {
    const ast = parse(
      `function f() { { let e = 1; use(e); } { let e = 2; use2(e); } }`
    );
    const infos = collectOwnedBindingInfos(getFnPath(ast, "f"));
    const es = infos.filter((i) => i.name === "e");
    assert.strictEqual(es.length, 1);
  });

  it("excludes a nested fn decl name even when params force a body scope", () => {
    const ast = parse(
      `function foo(a = 1) { function inner(y) { return y; } return inner(a); }`
    );
    const names = collectOwnedBindingInfos(getFnPath(ast, "foo")).map(
      (i) => i.name
    );
    assert.ok(!names.includes("inner"), `"inner" leaked into ${names}`);
    assert.deepStrictEqual(names.sort(), ["a", "foo"]);
  });

  it("excludes a block-scoped nested fn decl name", () => {
    const ast = parse(
      `function foo(x) { if (x) { function g() { return 1; } return g(); } return 0; }`
    );
    const names = collectOwnedBindingInfos(getFnPath(ast, "foo")).map(
      (i) => i.name
    );
    assert.ok(!names.includes("g"), `"g" leaked into ${names}`);
  });

  it("does not collect bindings owned by nested functions", () => {
    const ast = parse(
      `function outer(x) { const g = (z) => { let w = z; return w; }; return g(x); }`
    );
    const names = collectOwnedBindingInfos(getFnPath(ast, "outer")).map(
      (i) => i.name
    );
    assert.deepStrictEqual(names.sort(), ["g", "outer", "x"]);
  });
});

describe("buildOwnedBindingMap", () => {
  it("includes nested function declaration names for the transfer path", () => {
    const ast = parse(
      `function outer(x) { function inner(y) { return y; } return inner(x); }`
    );
    const map = buildOwnedBindingMap(getFnPath(ast, "outer"));
    assert.ok(map.has("inner"), "transfer map must carry nested fn decl name");
    assert.ok(map.get("inner")?.bindings.inner, "scope must own the binding");
  });

  it("includes the named function expression's self-name", () => {
    const ast = parse(
      `const f = function orig(n) { return n > 0 ? orig(n - 1) : 0; };`
    );
    const map = buildOwnedBindingMap(getFnPath(ast, "orig"));
    assert.ok(map.has("orig"), "transfer map must carry the NFE self-name");
    assert.ok(map.get("orig")?.bindings.orig, "scope must own the binding");
  });

  it("maps the function declaration's own name to its owning scope", () => {
    const ast = parse(`function foo(a) { return a; }`);
    const map = buildOwnedBindingMap(getFnPath(ast, "foo"));
    assert.ok(map.has("foo"));
    assert.ok(map.get("foo")?.bindings.foo, "scope must own the binding");
  });

  it("maps every name to the scope that owns its binding", () => {
    const ast = parse(`function foo(a, b = 2) {
      var c = 1;
      let d = 2;
      if (a) { var hoisted = 3; let blockLocal = 4; use(blockLocal); }
      for (const i of b) { use(i); }
      try { go(); } catch (err) { log(err); }
      function inner(y) { return y; }
      return inner(a + c + d + hoisted);
    }`);
    const map = buildOwnedBindingMap(getFnPath(ast, "foo"));
    for (const expected of [
      "a",
      "b",
      "c",
      "d",
      "hoisted",
      "blockLocal",
      "i",
      "err",
      "inner",
      "foo"
    ]) {
      assert.ok(map.has(expected), `missing "${expected}"`);
      assert.ok(
        map.get(expected)?.bindings[expected],
        `scope for "${expected}" does not own the binding`
      );
    }
  });

  it("includes a body-scope nested fn decl name for the transfer path", () => {
    const ast = parse(
      `function foo(a = 1) { function inner(y) { return y; } return inner(a); }`
    );
    const map = buildOwnedBindingMap(getFnPath(ast, "foo"));
    assert.ok(map.has("inner"));
    assert.ok(map.get("inner")?.bindings.inner, "scope must own the binding");
  });

  it("keeps the first scope for same-named sibling-block bindings", () => {
    const ast = parse(
      `function f() { { let e = 1; use(e); } { let e = 2; use2(e); } }`
    );
    const map = buildOwnedBindingMap(getFnPath(ast, "f"));
    assert.ok(map.has("e"));
    assert.strictEqual(map.size, 2); // e + f
  });
});

describe("collectShadowedBlockBindings", () => {
  const isEligible = (name: string) => name.length <= 2;

  it("finds catch clause binding that shadows a parameter", () => {
    const code = `function f(t) { try { } catch(t) { console.log(t); } }`;
    const ast = parse(code);
    const fnPath = getFnPath(ast);
    fnPath.scope.rename("t", "component");

    const bindings = collectShadowedBlockBindings(fnPath, isEligible);
    assert.strictEqual(bindings.length, 1);
    assert.strictEqual(bindings[0].name, "t");
  });

  it("finds block-scoped const in for-loop that shadows a parameter", () => {
    const code = `function f(o, r) {
      for (let i = 0; i < 10; i++) {
        const o = compute(i);
        const r = transform(o);
        emit(r);
      }
      return o + r;
    }`;
    const ast = parse(code);
    const fnPath = getFnPath(ast);
    // Simulate phase 1 renaming params
    fnPath.scope.rename("o", "currentIndexOffset");
    fnPath.scope.rename("r", "transformedValue");

    const bindings = collectShadowedBlockBindings(fnPath, isEligible);
    const names = bindings.map((b) => b.name).sort();
    assert.ok(names.includes("o"), `expected "o" in ${JSON.stringify(names)}`);
    assert.ok(names.includes("r"), `expected "r" in ${JSON.stringify(names)}`);
  });

  it("finds if-block let that shadows a parameter", () => {
    const code = `function f(x) {
      let y = x * 2;
      if (y > 10) {
        let x = transform(y);
        use(x);
      }
      return y;
    }`;
    const ast = parse(code);
    const fnPath = getFnPath(ast);
    fnPath.scope.rename("x", "input");

    const bindings = collectShadowedBlockBindings(fnPath, isEligible);
    const names = bindings.map((b) => b.name);
    assert.ok(names.includes("x"), `expected "x" in ${JSON.stringify(names)}`);
  });

  it("finds sibling block bindings that reuse a name", () => {
    const code = `function f(a) {
      if (a.length > 0) { let r = a[0]; process(r); }
      if (a.length > 1) { let r = a[1]; process(r); }
    }`;
    const ast = parse(code);
    const fnPath = getFnPath(ast);

    const bindings = collectShadowedBlockBindings(fnPath, isEligible);
    const rBindings = bindings.filter((b) => b.name === "r");
    assert.strictEqual(
      rBindings.length,
      2,
      `expected 2 "r" bindings, got ${rBindings.length}`
    );
  });

  it("finds switch case block bindings", () => {
    const code = `function f(n) {
      switch(n) {
        case 1: { let r = computeA(); return r; }
        case 2: { let r = computeB(); return r; }
      }
    }`;
    const ast = parse(code);
    const fnPath = getFnPath(ast);

    const bindings = collectShadowedBlockBindings(fnPath, isEligible);
    const rBindings = bindings.filter((b) => b.name === "r");
    assert.strictEqual(
      rBindings.length,
      2,
      `expected 2 "r" bindings, got ${rBindings.length}`
    );
  });

  it("finds multiple catch clause bindings", () => {
    const code = `function f(e) {
      try { a(); } catch(e) { log(e); }
      try { b(); } catch(e) { log(e); }
    }`;
    const ast = parse(code);
    const fnPath = getFnPath(ast);
    fnPath.scope.rename("e", "error");

    const bindings = collectShadowedBlockBindings(fnPath, isEligible);
    const eBindings = bindings.filter((b) => b.name === "e");
    assert.strictEqual(
      eBindings.length,
      2,
      `expected 2 "e" bindings, got ${eBindings.length}`
    );
  });

  it("skips bindings with descriptive names", () => {
    const code = `function f(t) { try { } catch(error) { console.log(error); } }`;
    const ast = parse(code);
    const fnPath = getFnPath(ast);
    fnPath.scope.rename("t", "component");

    const bindings = collectShadowedBlockBindings(fnPath, isEligible);
    assert.strictEqual(bindings.length, 0);
  });

  it("does not descend into nested functions", () => {
    const code = `function f(t) { try { } catch(t) {} function g(x) { try { } catch(x) {} } }`;
    const ast = parse(code);
    const fnPath = getFnPath(ast);
    fnPath.scope.rename("t", "component");

    const bindings = collectShadowedBlockBindings(fnPath, isEligible);
    assert.strictEqual(bindings.length, 1);
    assert.strictEqual(bindings[0].name, "t");
  });
});
