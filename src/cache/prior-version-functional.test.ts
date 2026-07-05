/**
 * Functional validation: humanified preact code must behave identically to the original.
 *
 * Tests that renaming (fresh humanification and prior-version transfer)
 * does not change runtime behavior. We dynamically import the original and
 * humanified bundles and compare the outputs of pure preact functions.
 *
 * Both legs run through the REAL pipeline (createRenamePlugin) with a
 * deterministic batch provider — no hand-rolled rename application with
 * its own collision policy.
 *
 * No DOM environment needed — we test createElement, createRef, Fragment,
 * cloneElement, isValidElement, toChildArray, createContext, and Component.
 */
import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import type { RenamePluginResult } from "../rename/plugin.js";
import { createRenamePlugin } from "../rename/plugin.js";

const FIXTURES_DIR = path.resolve("test/e2e/fixtures/preact/minified");

function readFixture(version: string, minifier: string): string {
  const filePath = path.join(FIXTURES_DIR, version, `${minifier}.js`);
  return fs.readFileSync(filePath, "utf-8");
}

/** Deterministic batch provider simulating LLM humanification. */
function simulationProvider(): LLMProvider {
  return {
    async suggestAllNames(request: BatchRenameRequest) {
      const renames: Record<string, string> = {};
      for (const id of request.identifiers) {
        renames[id] = `humanified_${id}`;
      }
      return { renames };
    }
  };
}

function assertValidOutput(result: RenamePluginResult, label: string): void {
  assert.strictEqual(
    result.parseFailure,
    undefined,
    `${label}: output must parse`
  );
  assert.strictEqual(
    result.semanticFailure,
    undefined,
    `${label}: rename invariants must hold`
  );
}

/** Humanify through the production pipeline without a prior version. */
async function humanifyFresh(code: string): Promise<string> {
  const plugin = createRenamePlugin({
    provider: simulationProvider(),
    concurrency: 4
  });
  const result = await plugin(code);
  assertValidOutput(result, "fresh");
  return result.code;
}

/** Humanify through the production pipeline with a prior version. */
async function humanifyWithPriorVersion(
  priorHumanifiedCode: string,
  newMinifiedCode: string
): Promise<string> {
  const plugin = createRenamePlugin({
    provider: simulationProvider(),
    priorVersionCode: priorHumanifiedCode,
    concurrency: 4
  });
  const result = await plugin(newMinifiedCode);
  assertValidOutput(result, "with-prior");
  return result.code;
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
    const humanified = await humanifyFresh(code);

    const original = (await importCode(code)) as unknown as PreactExports;
    const renamed = (await importCode(humanified)) as unknown as PreactExports;

    validatePreactBehavior(original, "original");
    validatePreactBehavior(renamed, "humanified");
  });

  it("v10.26.0 terser: humanified code passes functional checks", async () => {
    const code = readFixture("v10.26.0", "terser-default");
    const humanified = await humanifyFresh(code);

    const original = (await importCode(code)) as unknown as PreactExports;
    const renamed = (await importCode(humanified)) as unknown as PreactExports;

    validatePreactBehavior(original, "original");
    validatePreactBehavior(renamed, "humanified");
  });

  it("v25→v26 with prior-version: humanified code passes functional checks", async () => {
    const v25Code = readFixture("v10.25.0", "terser-default");
    const v26Code = readFixture("v10.26.0", "terser-default");

    // First humanify v25 (simulates a previous run)
    const humanifiedV25 = await humanifyFresh(v25Code);

    // Now humanify v26 using v25 as prior version (exact + close matches)
    const humanifiedV26 = await humanifyWithPriorVersion(
      humanifiedV25,
      v26Code
    );

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

      const humanifiedV25 = await humanifyFresh(v25Code);
      const humanifiedV26 = await humanifyWithPriorVersion(
        humanifiedV25,
        v26Code
      );

      const original = (await importCode(v26Code)) as unknown as PreactExports;
      const renamed = (await importCode(
        humanifiedV26
      )) as unknown as PreactExports;

      validatePreactBehavior(original, `original-${minifier}`);
      validatePreactBehavior(renamed, `humanified-${minifier}-with-prior`);
    });
  }
});
