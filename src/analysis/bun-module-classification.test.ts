import assert from "node:assert";
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as babelTraverse from "@babel/traverse";
import type * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import {
  classifyBunModules,
  nameCjsFactories
} from "./bun-module-classification.js";
import {
  findWrapperFunction,
  type WrapperFunctionResult
} from "./wrapper-detection.js";

/**
 * Force-build a WrapperFunctionResult for the outermost FunctionExpression
 * in `ast`, regardless of binding count. Used for tests where we want to
 * exercise wrapper-body scanning without 50+ synthetic bindings.
 */
function forceWrapper(ast: t.File): WrapperFunctionResult | null {
  let result: WrapperFunctionResult | null = null;
  traverse(ast, {
    Function(path: babelTraverse.NodePath<t.Function>) {
      result = { scope: path.scope, functionPath: path };
      path.stop();
    }
  });
  return result;
}

const FIXTURES_DIR = path.resolve("experiments/fixtures");

function parse(code: string): t.File {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    parserOpts: { errorRecovery: true }
  });
  if (!ast || ast.type !== "File") throw new Error("parse failed");
  return ast;
}

describe("classifyBunModules", () => {
  it("returns null when source has no Bun CJS factory helper", () => {
    const source = `
      function a() { return 1; }
      const b = () => 2;
    `;
    const ast = parse(source);
    const result = classifyBunModules(ast, source, null);
    assert.strictEqual(result, null);
  });

  it("detects synthetic CJS factories at program scope", () => {
    const source = [
      "var A = (q, _) => () => (_ || q((_ = {exports: {}}).exports, _), _.exports);",
      "var g = A((_q, l) => { l.exports = function() { return 1; }; });",
      "var p = A(($q, s) => { s.exports = function() { return 2; }; });"
    ].join("\n");

    const ast = parse(source);
    const result = classifyBunModules(ast, source, null);
    assert.ok(result, "expected classification result");
    assert.strictEqual(result.cjsFactoryHelperVar, "A");
    assert.strictEqual(result.factories.length, 2);
    assert.deepStrictEqual(
      result.factories.map((f) => f.factoryVar),
      ["g", "p"]
    );

    // Each factory has a populated contentHash and byteRange.
    for (const factory of result.factories) {
      assert.match(factory.contentHash, /^[0-9a-f]{16}$/);
      assert.ok(factory.byteRange[0] < factory.byteRange[1]);
      assert.ok(factory.bodyScope, "expected bodyScope to be set");
    }
  });

  it("parses banner package when banner sits inside the factory body", () => {
    // Bun often places the banner inside the body block, not before the
    // declaration. Babel attaches it as innerComments / leadingComments on
    // the first body statement.
    const source = [
      "var A = (q, _) => () => (_ || q((_ = {exports: {}}).exports, _), _.exports);",
      "var m = A((q, _) => {",
      "  /*! @azure/msal-common v15.13.1 2025-10-29 */",
      "  _.exports = {};",
      "});"
    ].join("\n");

    const ast = parse(source);
    const result = classifyBunModules(ast, source, null);
    assert.ok(result);
    assert.strictEqual(result.factories.length, 1);
    assert.strictEqual(result.factories[0].bannerPackage, "@azure/msal-common");
    assert.strictEqual(result.factories[0].bannerVersion, "15.13.1");
  });

  it("parses banner package and version from a leading bang block", () => {
    const source = [
      "var A = (q, _) => () => (_ || q((_ = {exports: {}}).exports, _), _.exports);",
      "/*! @azure/msal-common v15.13.1 (c) Microsoft */",
      "var m = A((q, _) => { _.exports = {}; });"
    ].join("\n");

    const ast = parse(source);
    const result = classifyBunModules(ast, source, null);
    assert.ok(result);
    assert.strictEqual(result.factories.length, 1);

    const factory = result.factories[0];
    assert.strictEqual(factory.bannerPackage, "@azure/msal-common");
    assert.strictEqual(factory.bannerVersion, "15.13.1");
    assert.ok(factory.bannerText?.includes("msal-common"));
  });

  it("scans wrapper IIFE body when a wrapper is present", () => {
    const source = [
      "(function(exports, require, module) {",
      "  var A = (q, _) => () => (_ || q((_ = {exports: {}}).exports, _), _.exports);",
      "  var x = A((q, _) => { _.exports = 1; });",
      "  var y = A((q, _) => { _.exports = 2; });",
      "  function notAFactory() { return 3; }",
      "});"
    ].join("\n");

    const ast = parse(source);
    // findWrapperFunction enforces a 50-binding threshold that this synthetic
    // bundle doesn't reach, so build a wrapper handle directly.
    const wrapper = forceWrapper(ast);
    assert.ok(wrapper, "expected to find a wrapper function expression");
    const result = classifyBunModules(ast, source, wrapper);
    assert.ok(result);
    assert.strictEqual(result.factories.length, 2);
    assert.deepStrictEqual(result.factories.map((f) => f.factoryVar).sort(), [
      "x",
      "y"
    ]);
  });

  it("does not match calls to a helper that is not the CJS factory", () => {
    const source = [
      "var A = (q, _) => () => (_ || q((_ = {exports: {}}).exports, _), _.exports);",
      "var Z = (q, _) => () => (q && (_ = q(q = 0)), _);",
      "var lazy = Z(() => { /* lazy init */ });",
      "var real = A((q, _) => { _.exports = 1; });"
    ].join("\n");

    const ast = parse(source);
    const result = classifyBunModules(ast, source, null);
    assert.ok(result);
    assert.strictEqual(result.factories.length, 1);
    assert.strictEqual(result.factories[0].factoryVar, "real");
  });

  it("naming cascade: banner → url → carry-over → fallback", () => {
    const source = [
      "var A = (q, _) => () => (_ || q((_ = {exports: {}}).exports, _), _.exports);",
      "/*! @azure/msal-common v15.13.1 */",
      "var withBanner = A((q, _) => { _.exports = {}; });",
      "var withUrl = A((q, _) => {",
      "  // see https://github.com/sindresorhus/got for docs",
      "  _.exports = function got() {};",
      "});",
      "var withCarryOver = A((q, _) => { _.exports = 'a'; });",
      "var withFallback = A((q, _) => { _.exports = 'b'; });"
    ].join("\n");

    const ast = parse(source);
    const result = classifyBunModules(ast, source, null);
    assert.ok(result);
    assert.strictEqual(result.factories.length, 4);

    // Snapshot the contentHash for `withCarryOver` so the prior-name map hits.
    const carryOverHash = result.factories[2].contentHash;
    const priorNames = new Map([[carryOverHash, "carried-over-pkg"]]);

    const counts = nameCjsFactories(result, source, priorNames);

    assert.deepStrictEqual(counts, {
      banner: 1,
      url: 1,
      carryOver: 1,
      llm: 0,
      fallback: 1
    });

    assert.strictEqual(result.factories[0].name, "@azure/msal-common@15.13.1");
    assert.strictEqual(result.factories[0].nameSource, "banner");
    assert.strictEqual(result.factories[1].name, "got");
    assert.strictEqual(result.factories[1].nameSource, "url");
    assert.strictEqual(result.factories[2].name, "carried-over-pkg");
    assert.strictEqual(result.factories[2].nameSource, "carry-over");
    const fallbackName = result.factories[3].name ?? "";
    assert.match(fallbackName, /^lib_[0-9a-f]{8}$/);
    assert.strictEqual(result.factories[3].nameSource, "fallback");
  });

  it("classifies all CJS factories in the app-cjs-bun fixture", () => {
    const filePath = path.join(FIXTURES_DIR, "app-cjs-bun", "bundle.js");
    if (!fs.existsSync(filePath)) {
      // Fixture is optional during early development; skip if missing.
      return;
    }
    const source = fs.readFileSync(filePath, "utf-8");
    const ast = parse(source);
    const wrapper = findWrapperFunction(ast);
    const result = classifyBunModules(ast, source, wrapper);
    assert.ok(result, "expected app-cjs-bun to be detected as Bun CJS");
    // app-cjs-bun bundles minimist, ms, and four debug submodules.
    assert.strictEqual(
      result.factories.length,
      6,
      `expected 6 factories, got ${result.factories.length}`
    );
    // npm-published versions of these libs don't ship bang banners.
    const bannered = result.factories.filter((f) => f.bannerPackage);
    assert.strictEqual(
      bannered.length,
      0,
      `expected zero banners, got ${bannered.length}`
    );
  });
});
