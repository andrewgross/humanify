import { describe, it } from "node:test";
import assert from "node:assert";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { extractExportNames, bundleSplitOutput, validateRoundtrip } from "./roundtrip.js";

describe("extractExportNames", () => {
  it("extracts named export specifiers", () => {
    const code = `function foo() {} function bar() {} function baz() {} export { foo, bar, baz };`;
    const names = extractExportNames(code);
    assert.deepStrictEqual(names, ["bar", "baz", "foo"]);
  });

  it("extracts export function declarations", () => {
    const code = `export function myFunc() {}`;
    const names = extractExportNames(code);
    assert.deepStrictEqual(names, ["myFunc"]);
  });

  it("extracts export const declarations", () => {
    const code = `export const x = 1;`;
    const names = extractExportNames(code);
    assert.deepStrictEqual(names, ["x"]);
  });

  it("extracts re-exports from source", () => {
    const code = `export { a, b } from './other.js';`;
    const names = extractExportNames(code);
    assert.deepStrictEqual(names, ["a", "b"]);
  });

  it("extracts default export", () => {
    const code = `export default function() {}`;
    const names = extractExportNames(code);
    assert.deepStrictEqual(names, ["default"]);
  });

  it("handles mixed export styles", () => {
    const code = `
      export { foo } from './a.js';
      export function bar() {}
      export const baz = 1;
    `;
    const names = extractExportNames(code);
    assert.deepStrictEqual(names, ["bar", "baz", "foo"]);
  });

  it("returns empty for no exports", () => {
    const code = `const x = 1; function foo() {}`;
    const names = extractExportNames(code);
    assert.deepStrictEqual(names, []);
  });

  it("handles unparseable code gracefully", () => {
    const code = `this is not javascript {{{`;
    const names = extractExportNames(code);
    assert.deepStrictEqual(names, []);
  });
});

describe("bundleSplitOutput", () => {
  it("bundles a simple split output", async () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "roundtrip-test-"));
    try {
      // Create a simple split output
      fs.writeFileSync(path.join(tmpDir, "core.js"), `
        function greet(name) { return "Hello, " + name; }
        export { greet };
      `);
      fs.writeFileSync(path.join(tmpDir, "index.js"), `
        export { greet } from './core.js';
      `);

      const bundled = await bundleSplitOutput(tmpDir);
      assert.ok(bundled.length > 0, "Bundle should produce output");
      assert.ok(bundled.includes("greet"), "Bundle should contain the function");
    } finally {
      fs.rmSync(tmpDir, { recursive: true });
    }
  });

  it("throws when no index.js exists", async () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "roundtrip-test-"));
    try {
      await assert.rejects(
        () => bundleSplitOutput(tmpDir),
        /No index\.js found/,
      );
    } finally {
      fs.rmSync(tmpDir, { recursive: true });
    }
  });
});

describe("validateRoundtrip", () => {
  it("validates a correct split", async () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "roundtrip-test-"));
    const outDir = path.join(tmpDir, "output");
    fs.mkdirSync(outDir);

    try {
      // Original file with exports
      const originalPath = path.join(tmpDir, "original.js");
      fs.writeFileSync(originalPath, `
        function greet(name) { return "Hello, " + name; }
        function add(a, b) { return a + b; }
        export { greet, add };
      `);

      // Split output
      fs.writeFileSync(path.join(outDir, "core.js"), `
        function greet(name) { return "Hello, " + name; }
        function add(a, b) { return a + b; }
        export { greet, add };
      `);
      fs.writeFileSync(path.join(outDir, "index.js"), `
        export { greet, add } from './core.js';
      `);

      const result = await validateRoundtrip(originalPath, outDir);

      assert.strictEqual(result.bundleSuccess, true);
      assert.strictEqual(result.exportsMatch, true);
      assert.deepStrictEqual(result.missingExports, []);
    } finally {
      fs.rmSync(tmpDir, { recursive: true });
    }
  });

  it("detects missing exports", async () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "roundtrip-test-"));
    const outDir = path.join(tmpDir, "output");
    fs.mkdirSync(outDir);

    try {
      const originalPath = path.join(tmpDir, "original.js");
      fs.writeFileSync(originalPath, `
        function greet(name) { return "Hello, " + name; }
        function add(a, b) { return a + b; }
        export { greet, add };
      `);

      // Split output missing 'add'
      fs.writeFileSync(path.join(outDir, "core.js"), `
        function greet(name) { return "Hello, " + name; }
        export { greet };
      `);
      fs.writeFileSync(path.join(outDir, "index.js"), `
        export { greet } from './core.js';
      `);

      const result = await validateRoundtrip(originalPath, outDir);

      assert.strictEqual(result.bundleSuccess, true);
      assert.strictEqual(result.exportsMatch, false);
      assert.deepStrictEqual(result.missingExports, ["add"]);
    } finally {
      fs.rmSync(tmpDir, { recursive: true });
    }
  });
});
