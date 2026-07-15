import assert from "node:assert/strict";
import { test } from "node:test";
import * as t from "@babel/types";
import { detectCjsHelper, factoryCallee, partitionLibraries } from "./split.js";

/** `var <bind> = <callee>((...params) => {})` */
function factoryStmt(
  bind: string,
  callee: string,
  params: string[]
): t.Statement {
  return t.variableDeclaration("var", [
    t.variableDeclarator(
      t.identifier(bind),
      t.callExpression(t.identifier(callee), [
        t.arrowFunctionExpression(
          params.map((p) => t.identifier(p)),
          t.blockStatement([])
        )
      ])
    )
  ]);
}

test("factoryCallee: CJS factory (>=1 param) detected, ESM init (0 param) is not", () => {
  assert.deepEqual(
    factoryCallee(factoryStmt("wcq", "d", ["exports", "module"])),
    {
      binding: "wcq",
      callee: "d"
    }
  );
  assert.equal(factoryCallee(factoryStmt("UY8", "R", [])), null); // ESM init
  assert.equal(
    factoryCallee(
      t.functionDeclaration(t.identifier("f"), [], t.blockStatement([]))
    ),
    null
  );
});

test("detectCjsHelper picks the identifier that wraps the most modules", () => {
  const body = [
    factoryStmt("a", "d", ["e", "m"]),
    factoryStmt("b", "d", ["e", "m"]),
    factoryStmt("c", "R", []), // ESM, ignored
    factoryStmt("x", "z", ["e", "m"]) // only one -> not dominant
  ];
  assert.equal(detectCjsHelper(body), "d");
});

test("partitionLibraries routes CJS factories to libraries/, keeps app code", () => {
  const body = [
    factoryStmt("wcq", "d", ["e", "m"]), // library
    t.functionDeclaration(t.identifier("appFn"), [], t.blockStatement([])), // app
    factoryStmt("RC_", "d", ["e", "m"]), // library
    factoryStmt("mod", "R", []) // ESM init -> app
  ];
  const { helper, isLibrary, libraryFile } = partitionLibraries(body);
  assert.equal(helper, "d");
  assert.deepEqual(isLibrary, [true, false, true, false]);
  assert.equal(libraryFile[0], "libraries/wcq.js");
  assert.equal(libraryFile[2], "libraries/RC_.js");
  assert.equal(libraryFile[1], null);
});

test("partitionLibraries: no libraries when nothing wraps >=2 modules", () => {
  const body = [
    t.functionDeclaration(t.identifier("f"), [], t.blockStatement([]))
  ];
  assert.equal(partitionLibraries(body).helper, null);
});
