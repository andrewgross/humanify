import { describe, it } from "node:test";
import assert from "node:assert";
import vm from "node:vm";
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, "fixtures");

/**
 * Evaluate an ESM-style module in a sandbox, returning its exports.
 * Converts `export` statements to property assignments on `module.exports`.
 */
function evalModule(code: string): Record<string, unknown> {
  // Transform ESM exports to CommonJS-style for vm evaluation
  let transformed = code
    // export let/const/var name = ... → let/const/var name = ...; exports.name = name;
    .replace(/export\s+(let|const|var)\s+(\w+)\s*=/g, (_m, kw, name) => {
      return `${kw} ${name} = exports.${name} =`;
    })
    // export function name(...) → function name(...) ... + exports.name = name;
    .replace(/export\s+function\s+(\w+)/g, (_m, name) => {
      return `function ${name}`;
    });

  // Collect exported function names and add exports at the end
  const exportedFns: string[] = [];
  const fnExportRe = /export\s+function\s+(\w+)/g;
  let fnMatch: RegExpExecArray | null = fnExportRe.exec(code);
  while (fnMatch !== null) {
    exportedFns.push(fnMatch[1]);
    fnMatch = fnExportRe.exec(code);
  }
  if (exportedFns.length > 0) {
    transformed += `\n${exportedFns.map((n) => `exports.${n} = ${n};`).join("\n")}`;
  }

  // Remove any remaining import/export statements that might cause issues
  transformed = transformed.replace(/^import\s+.*$/gm, "// [import removed]");
  transformed = transformed.replace(
    /^export\s*\{[^}]*\}\s*(?:from\s*['"][^'"]*['"])?\s*;?\s*$/gm,
    "// [re-export removed]"
  );

  const exports: Record<string, unknown> = {};
  const sandbox = { exports, console, Math, Buffer, setTimeout, clearTimeout };
  vm.runInNewContext(transformed, sandbox, { timeout: 5000 });
  return exports;
}

describe("Functional verification: nanoid non-secure", () => {
  const nonSecurePath = join(
    fixturesDir,
    "nanoid",
    ".tmp-clone",
    "non-secure",
    "index.js"
  );

  let nanoidModule: Record<string, unknown>;

  it("loads nanoid non-secure module", () => {
    const code = readFileSync(nonSecurePath, "utf-8");
    nanoidModule = evalModule(code);
    assert.ok(
      typeof nanoidModule.nanoid === "function",
      "Should export nanoid function"
    );
    assert.ok(
      typeof nanoidModule.customAlphabet === "function",
      "Should export customAlphabet function"
    );
  });

  it("nanoid() returns a 21-character string", () => {
    const code = readFileSync(nonSecurePath, "utf-8");
    const mod = evalModule(code);
    const nanoid = mod.nanoid as (size?: number) => string;

    const id = nanoid();
    assert.strictEqual(typeof id, "string", "nanoid() should return a string");
    assert.strictEqual(id.length, 21, "Default size should be 21");
  });

  it("nanoid() returns URL-safe characters", () => {
    const code = readFileSync(nonSecurePath, "utf-8");
    const mod = evalModule(code);
    const nanoid = mod.nanoid as (size?: number) => string;

    const urlSafe = /^[A-Za-z0-9_-]+$/;
    for (let i = 0; i < 100; i++) {
      const id = nanoid();
      assert.ok(
        urlSafe.test(id),
        `nanoid() should produce URL-safe chars, got: ${id}`
      );
    }
  });

  it("nanoid(size) respects custom size", () => {
    const code = readFileSync(nonSecurePath, "utf-8");
    const mod = evalModule(code);
    const nanoid = mod.nanoid as (size?: number) => string;

    assert.strictEqual(nanoid(5).length, 5);
    assert.strictEqual(nanoid(10).length, 10);
    assert.strictEqual(nanoid(50).length, 50);
  });

  it("customAlphabet produces IDs from specified alphabet", () => {
    const code = readFileSync(nonSecurePath, "utf-8");
    const mod = evalModule(code);
    const customAlphabet = mod.customAlphabet as (
      alphabet: string,
      size?: number
    ) => (size?: number) => string;

    const generate = customAlphabet("abc", 5);
    for (let i = 0; i < 100; i++) {
      const id = generate();
      assert.strictEqual(
        id.length,
        5,
        `Custom alphabet ID should be length 5, got ${id.length}`
      );
      assert.ok(
        /^[abc]+$/.test(id),
        `ID should only contain 'abc', got: ${id}`
      );
    }
  });

  it("customAlphabet respects runtime size override", () => {
    const code = readFileSync(nonSecurePath, "utf-8");
    const mod = evalModule(code);
    const customAlphabet = mod.customAlphabet as (
      alphabet: string,
      size?: number
    ) => (size?: number) => string;

    const generate = customAlphabet("xyz", 10);
    const id = generate(3);
    assert.strictEqual(id.length, 3, "Runtime size override should work");
    assert.ok(/^[xyz]+$/.test(id), `ID should only contain 'xyz', got: ${id}`);
  });

  it("nanoid produces unique IDs", () => {
    const code = readFileSync(nonSecurePath, "utf-8");
    const mod = evalModule(code);
    const nanoid = mod.nanoid as (size?: number) => string;

    const ids = new Set<string>();
    for (let i = 0; i < 1000; i++) {
      ids.add(nanoid());
    }
    // With 21-char IDs from 64-char alphabet, collisions in 1000 IDs are virtually impossible
    assert.strictEqual(ids.size, 1000, "All 1000 IDs should be unique");
  });
});
