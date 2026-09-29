/**
 * The private-name half of the serializer (16-findings-queue #5): private
 * tokens serialize VERBATIM (`P=#f`) — the TS default both live callers
 * (vendor-churn, clone-census) rely on — and the abandoned
 * `privateNamesAsSlots` mode is GONE: its per-class slot map was the
 * implement of an output-validation scheme (order-keyed `P=$n`), the Rust
 * counterpart was removed 2026-09-25 (2f3362d0: every non-class child
 * clobbered the map before a private token could read it, so it never
 * changed a byte), and no harness caller has passed the option since
 * output-validation.ts died at the cutover. These pins prove the deletion
 * changes no serialization byte.
 */
import assert from "node:assert";
import { describe, it } from "node:test";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { parseFileAst, traverse } from "./babel.js";
import { serializePathTokens } from "./structural-tokens.js";

type Options = Parameters<typeof serializePathTokens>[1];

/** Two unrelated classes with the SAME private name, used and declared. */
const CODE =
  "class A { #f = 1; m() { return this.#f; } }\n" +
  "class B { #f = 2; g() { return this.#f; } }";

/** The byte-pinned streams (captured 2026-09-29, before the deletion). */
const A_STREAM =
  "ClassDeclaration{id:$0;superClass:null;body:ClassBody{body:[" +
  "ClassPrivateProperty{static:false;key:P=#f;value:N=0;}," +
  'ClassMethod{static:false;key:I=m;computed:false;kind:"method";id:null;' +
  "generator:false;async:false;params:[];body:BlockStatement{body:[" +
  "ReturnStatement{argument:MemberExpression{object:ThisExpression{};" +
  "computed:false;property:P=#f;};},];directives:[];};},];};}";
const B_STREAM =
  "ClassDeclaration{id:$0;superClass:null;body:ClassBody{body:[" +
  "ClassPrivateProperty{static:false;key:P=#f;value:N=0;}," +
  'ClassMethod{static:false;key:I=g;computed:false;kind:"method";id:null;' +
  "generator:false;async:false;params:[];body:BlockStatement{body:[" +
  "ReturnStatement{argument:MemberExpression{object:ThisExpression{};" +
  "computed:false;property:P=#f;};},];directives:[];};},];};}";

function programLevelClasses(): NodePath<t.ClassDeclaration>[] {
  const ast = parseFileAst(CODE);
  assert.ok(ast, "the corpus parses");
  const found: NodePath<t.ClassDeclaration>[] = [];
  traverse(ast, {
    ClassDeclaration(p: NodePath<t.ClassDeclaration>) {
      if (p.parentPath?.isProgram()) found.push(p);
    }
  });
  assert.strictEqual(found.length, 2, "two program-level classes");
  return found;
}

describe("structural-tokens: private names", () => {
  it("serializes private names VERBATIM, both classes, declaration and use (byte-pinned)", () => {
    const [a, b] = programLevelClasses();
    assert.strictEqual(serializePathTokens(a).join(""), A_STREAM);
    assert.strictEqual(serializePathTokens(b).join(""), B_STREAM);
  });

  it("the abandoned privateNamesAsSlots mode is GONE — the option cannot change the stream", () => {
    // The mode once flipped `P=#f` to an order-keyed `P=$1`; after its
    // removal the unknown key is inert and privates stay verbatim.
    const [a] = programLevelClasses();
    const withFlag = serializePathTokens(a, {
      privateNamesAsSlots: true
    } as Options).join("");
    assert.strictEqual(
      withFlag,
      A_STREAM,
      "privates stay verbatim — no P=$ slots may appear"
    );
  });
});
