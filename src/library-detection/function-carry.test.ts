import assert from "node:assert";
import { describe, it } from "node:test";
import type * as t from "@babel/types";
import { parseSourceAst } from "../babel-utils.js";
import type { FileContext } from "../pipeline/types.js";
import { createBabelPlugin } from "../plugins/babel/babel.js";
import { findCommentRegions } from "./comment-regions.js";
import {
  carryFunctionLibraries,
  functionsInTreeOrder,
  resolveFunctionLibraries
} from "./function-carry.js";

function parse(code: string) {
  const ast = parseSourceAst(code);
  assert.ok(ast);
  return ast;
}

/** Beautify with the carry armed; return the carry and the re-parsed tree. */
async function beautifyWithCarry(raw: string) {
  const context: FileContext = { commentRegions: findCommentRegions(raw) };
  const beautified = await createBabelPlugin()(raw, context);
  assert.ok(context.functionLibraries, "the carry must be recorded");
  return { carry: context.functionLibraries, reparsed: parse(beautified) };
}

/** Library name per function, keyed by the function's first param name. */
function libraryByParam(
  reparsed: ReturnType<typeof parse>,
  byNode: Map<unknown, string>
): Record<string, string | null> {
  const out: Record<string, string | null> = {};
  for (const fn of functionsInTreeOrder(reparsed)) {
    const p = fn.params[0];
    if (p?.type === "Identifier") out[p.name] = byNode.get(fn) ?? null;
  }
  return out;
}

describe("functionsInTreeOrder", () => {
  it("walks functions in pre-order, parents before children, siblings in source order", () => {
    const ast = parse(
      "function a(){ function b(){} } var c = () => function d(){}; class E { m(){} } var o = { n(){} };"
    );
    const label = (fn: t.Function): string => {
      if (fn.type === "ClassMethod" || fn.type === "ObjectMethod") {
        return (fn.key as t.Identifier).name;
      }
      if (
        fn.type === "FunctionDeclaration" ||
        fn.type === "FunctionExpression"
      ) {
        return fn.id?.name ?? "?";
      }
      return fn.type === "ArrowFunctionExpression" ? "arrow" : "?";
    };
    const names = functionsInTreeOrder(ast).map(label);
    assert.deepStrictEqual(names, ["a", "b", "arrow", "d", "m", "n"]);
  });
});

describe("library carry across beautify (#32)", () => {
  it("classifies by RAW start even when the banner sits mid-sequence", async () => {
    // Beautify splits the sequence into statements; the banner lives between
    // the two function expressions of one raw statement.
    const raw =
      "var a=function(app){return app},b=(0,/*! tinylib v1.2.3 */function(lib){return lib});";
    const { carry, reparsed } = await beautifyWithCarry(raw);
    const byNode = resolveFunctionLibraries(reparsed, carry);
    assert.deepStrictEqual(libraryByParam(reparsed, byNode), {
      app: null,
      lib: "tinylib"
    });
  });

  it("survives a transform that REORDERS functions (literal flipped to the right)", async () => {
    // flipComparisonsTheRightWayAround moves the template literal (holding
    // `early`) after the call (holding `late`): tree order changes, raw
    // starts do not, so each function keeps its raw classification.
    const raw =
      // biome-ignore lint/suspicious/noTemplateCurlyInString: the INPUT is JS source holding a template literal
      "`${function(early){return early}}`===f(/*! tinylib v1.2.3 */function(late){return late});";
    const { carry, reparsed } = await beautifyWithCarry(raw);
    const order = functionsInTreeOrder(reparsed).map(
      (fn) => (fn.params[0] as { name: string }).name
    );
    assert.deepStrictEqual(order, ["late", "early"], "the flip reorders");
    const byNode = resolveFunctionLibraries(reparsed, carry);
    assert.deepStrictEqual(libraryByParam(reparsed, byNode), {
      early: null,
      late: "tinylib"
    });
  });

  it("carries through conditional/logical rewrites and nested functions", async () => {
    const raw =
      "x?function(p1){}:function(p2){};y&&function(p3){return function(p4){}};/*! tinylib v1.2.3 */z||function(p5){};";
    const { carry, reparsed } = await beautifyWithCarry(raw);
    const byNode = resolveFunctionLibraries(reparsed, carry);
    assert.deepStrictEqual(libraryByParam(reparsed, byNode), {
      p1: null,
      p2: null,
      p3: null,
      p4: null,
      p5: "tinylib"
    });
  });

  it("fails loud when the re-parsed tree does not line up with the carry", () => {
    const carried = parse("var a = function(){}; var b = () => 1;");
    const carry = carryFunctionLibraries(carried, [
      { libraryName: "tinylib", startOffset: 0, endOffset: null }
    ]);
    assert.throws(
      () => resolveFunctionLibraries(parse("var a = function(){};"), carry),
      /2 functions carried across beautify, 1 found/
    );
    assert.throws(
      () =>
        resolveFunctionLibraries(
          parse("var a = () => 1; var b = function(){};"),
          carry
        ),
      /function #0 is a FunctionExpression before re-parse and a ArrowFunctionExpression after/
    );
  });

  it("records nothing when the file has no regions", async () => {
    const context: FileContext = { commentRegions: [] };
    await createBabelPlugin()("var a=function(){};", context);
    assert.strictEqual(context.functionLibraries, undefined);
  });
});
