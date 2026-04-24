/**
 * Functional validation: humanified preact code must behave identically to the original.
 *
 * Tests that renaming (both simulated humanification and prior-version matching)
 * does not change runtime behavior. We dynamically import the original and
 * humanified bundles and compare the outputs of pure preact functions.
 *
 * No DOM environment needed — we test createElement, createRef, Fragment,
 * cloneElement, isValidElement, toChildArray, createContext, and Component.
 */
import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as babelTraverse from "@babel/traverse";
import { generate, traverse } from "../babel-utils.js";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import type { FunctionNode } from "../analysis/types.js";
import { matchPriorVersion } from "./prior-version.js";

const FIXTURES_DIR = path.resolve("test/e2e/fixtures/preact/minified");

function readFixture(version: string, minifier: string): string {
  const filePath = path.join(FIXTURES_DIR, version, `${minifier}.js`);
  return fs.readFileSync(filePath, "utf-8");
}

/** Simulate humanification by renaming all minified-looking bindings. */
function simulateHumanify(code: string): string {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("Failed to parse");

  traverse(ast, {
    Scope(scopePath: babelTraverse.NodePath) {
      for (const [name, binding] of Object.entries(scopePath.scope.bindings)) {
        if (binding.scope !== scopePath.scope) continue;
        if (name.length <= 4 && /^[a-z_]/.test(name)) {
          const line = binding.identifier.loc?.start.line ?? 0;
          const col = binding.identifier.loc?.start.column ?? 0;
          const newName = `humanified_${name}_${line}_${col}`;
          scopePath.scope.rename(name, newName);
        }
      }
    }
  });

  return generate(ast).code;
}

/**
 * Apply prior-version close-match context to functions (simulating what the
 * plugin does), then simulate humanification on the unmatched functions.
 * Returns the final code with all renames applied.
 */
/** Apply a name mapping to a scope, skipping collisions. */
function applyRenames(
  scope: {
    bindings: Record<string, unknown>;
    rename: (o: string, n: string) => void;
  },
  names: Record<string, string>
): void {
  for (const [oldName, newName] of Object.entries(names)) {
    if (
      oldName !== newName &&
      scope.bindings[oldName] &&
      !scope.bindings[newName]
    ) {
      scope.rename(oldName, newName);
    }
  }
}

function humanifyWithPriorVersion(
  priorHumanifiedCode: string,
  newMinifiedCode: string
): string {
  const ast = parseSync(newMinifiedCode, { sourceType: "unambiguous" });
  if (!ast) throw new Error("Failed to parse");

  const functions = buildFunctionGraph(ast, "test.js");
  const fnMap = new Map<string, FunctionNode>(
    functions.map((f) => [f.sessionId, f])
  );
  const result = matchPriorVersion(priorHumanifiedCode, fnMap);

  // Apply exact-match renames to AST (same as plugin.ts does)
  for (const fn of fnMap.values()) {
    if (fn.renameMapping) applyRenames(fn.path.scope, fn.renameMapping.names);
  }

  // Apply close-match name transfers (function name + params)
  for (const [newId, info] of result.closeMatchContext) {
    const fn = fnMap.get(newId);
    if (!fn || fn.renameMapping) continue;
    applyRenames(fn.path.scope, info.nameTransfers);
  }

  // Simulate LLM humanification on remaining functions (including close-matched ones)
  traverse(ast, {
    Scope(scopePath: babelTraverse.NodePath) {
      for (const [name, binding] of Object.entries(scopePath.scope.bindings)) {
        if (binding.scope !== scopePath.scope) continue;
        if (name.length <= 4 && /^[a-z_]/.test(name)) {
          const line = binding.identifier.loc?.start.line ?? 0;
          const col = binding.identifier.loc?.start.column ?? 0;
          const newName = `humanified_${name}_${line}_${col}`;
          scopePath.scope.rename(name, newName);
        }
      }
    }
  });

  console.log(
    `    exact=${result.functionsMatched}, already=${result.functionsAlreadyNamed}, close=${result.closeMatchCount}`
  );

  return generate(ast).code;
}

/** Write code to a temp file and dynamically import it. */
async function importCode(code: string): Promise<Record<string, unknown>> {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-test-"));
  const tmpFile = path.join(tmpDir, "bundle.mjs");
  fs.writeFileSync(tmpFile, code);
  try {
    return await import(tmpFile);
  } finally {
    fs.unlinkSync(tmpFile);
    fs.rmdirSync(tmpDir);
  }
}

interface PreactExports {
  createElement: (
    type: string,
    props: unknown,
    ...children: unknown[]
  ) => unknown;
  h: (type: string, props: unknown, ...children: unknown[]) => unknown;
  createRef: () => { current: unknown };
  Fragment: (props: { children: unknown }) => unknown;
  isValidElement: (value: unknown) => boolean;
  toChildArray: (children: unknown) => unknown[];
  cloneElement: (
    vnode: unknown,
    props?: unknown,
    ...children: unknown[]
  ) => unknown;
  createContext: (defaultValue: unknown) => unknown;
  Component: new (props: unknown, context: unknown) => unknown;
}

/** Run a battery of pure-function checks against a preact module. */
function validatePreactBehavior(mod: PreactExports, label: string): void {
  // createElement / h
  const vnode = mod.createElement("div", { id: "test" }, "hello");
  assert.ok(vnode, `${label}: createElement should return a vnode`);
  assert.strictEqual(
    (vnode as { type: string }).type,
    "div",
    `${label}: vnode.type`
  );
  assert.strictEqual(
    (vnode as { props: { id: string } }).props.id,
    "test",
    `${label}: vnode.props.id`
  );

  // h is aliased to createElement
  const vnode2 = mod.h("span", null, "world");
  assert.strictEqual(
    (vnode2 as { type: string }).type,
    "span",
    `${label}: h alias`
  );

  // createRef
  const ref = mod.createRef();
  assert.strictEqual(ref.current, null, `${label}: createRef.current`);

  // isValidElement
  assert.strictEqual(
    mod.isValidElement(vnode),
    true,
    `${label}: isValidElement(vnode)`
  );
  assert.strictEqual(
    mod.isValidElement("string"),
    false,
    `${label}: isValidElement(string)`
  );
  assert.strictEqual(
    mod.isValidElement(null),
    false,
    `${label}: isValidElement(null)`
  );
  assert.strictEqual(
    mod.isValidElement(42),
    false,
    `${label}: isValidElement(number)`
  );

  // toChildArray
  const children = mod.toChildArray(["a", "b", ["c", "d"]]);
  assert.ok(Array.isArray(children), `${label}: toChildArray returns array`);
  assert.strictEqual(children.length, 4, `${label}: toChildArray flattens`);

  const emptyChildren = mod.toChildArray(null);
  assert.strictEqual(emptyChildren.length, 0, `${label}: toChildArray(null)`);

  // cloneElement
  const original = mod.createElement("div", { className: "a" }, "child");
  const cloned = mod.cloneElement(original, { className: "b" }) as {
    type: string;
    props: { className: string; children: unknown };
  };
  assert.strictEqual(cloned.type, "div", `${label}: cloneElement type`);
  assert.strictEqual(
    cloned.props.className,
    "b",
    `${label}: cloneElement overrides props`
  );
  assert.strictEqual(
    cloned.props.children,
    "child",
    `${label}: cloneElement preserves children`
  );

  // createContext
  const ctx = mod.createContext("defaultVal") as {
    Provider: unknown;
    Consumer: unknown;
    _defaultValue: string;
  };
  assert.ok(ctx.Provider, `${label}: context.Provider exists`);
  assert.ok(ctx.Consumer, `${label}: context.Consumer exists`);
  assert.strictEqual(
    ctx._defaultValue,
    "defaultVal",
    `${label}: context default value`
  );

  // Component constructor
  const comp = new (mod.Component as new (
    p: unknown,
    c: unknown
  ) => { props: unknown; context: unknown })({ a: 1 }, { b: 2 });
  assert.deepStrictEqual(comp.props, { a: 1 }, `${label}: Component.props`);
  assert.deepStrictEqual(comp.context, { b: 2 }, `${label}: Component.context`);

  // Fragment
  assert.strictEqual(typeof mod.Fragment, "function", `${label}: Fragment`);

  // createElement with children array
  const withChildren = mod.createElement("ul", null, "a", "b", "c");
  assert.strictEqual(
    (withChildren as { props: { children: unknown[] } }).props.children.length,
    3,
    `${label}: createElement multiple children`
  );
}

describe("functional validation: humanified preact behaves identically", () => {
  it("v10.25.0 terser: humanified code passes functional checks", async () => {
    const code = readFixture("v10.25.0", "terser-default");
    const humanified = simulateHumanify(code);

    const original = (await importCode(code)) as unknown as PreactExports;
    const renamed = (await importCode(humanified)) as unknown as PreactExports;

    validatePreactBehavior(original, "original");
    validatePreactBehavior(renamed, "humanified");
  });

  it("v10.26.0 terser: humanified code passes functional checks", async () => {
    const code = readFixture("v10.26.0", "terser-default");
    const humanified = simulateHumanify(code);

    const original = (await importCode(code)) as unknown as PreactExports;
    const renamed = (await importCode(humanified)) as unknown as PreactExports;

    validatePreactBehavior(original, "original");
    validatePreactBehavior(renamed, "humanified");
  });

  it("v25→v26 with prior-version: humanified code passes functional checks", async () => {
    const v25Code = readFixture("v10.25.0", "terser-default");
    const v26Code = readFixture("v10.26.0", "terser-default");

    // First humanify v25 (simulates a previous run)
    const humanifiedV25 = simulateHumanify(v25Code);

    // Now humanify v26 using v25 as prior version (exact + close matches)
    const humanifiedV26 = humanifyWithPriorVersion(humanifiedV25, v26Code);

    const original = (await importCode(v26Code)) as unknown as PreactExports;
    const renamed = (await importCode(
      humanifiedV26
    )) as unknown as PreactExports;

    validatePreactBehavior(original, "original-v26");
    validatePreactBehavior(renamed, "humanified-v26-with-prior");
  });

  for (const minifier of ["esbuild-default", "swc-default"]) {
    it(`v25→v26 with prior-version (${minifier}): humanified code passes functional checks`, async () => {
      const v25Code = readFixture("v10.25.0", minifier);
      const v26Code = readFixture("v10.26.0", minifier);

      const humanifiedV25 = simulateHumanify(v25Code);
      const humanifiedV26 = humanifyWithPriorVersion(humanifiedV25, v26Code);

      const original = (await importCode(v26Code)) as unknown as PreactExports;
      const renamed = (await importCode(
        humanifiedV26
      )) as unknown as PreactExports;

      validatePreactBehavior(original, `original-${minifier}`);
      validatePreactBehavior(renamed, `humanified-${minifier}-with-prior`);
    });
  }
});
