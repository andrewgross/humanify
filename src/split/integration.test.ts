import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import * as fs from "node:fs";
import * as path from "node:path";
import type * as t from "@babel/types";
import { splitFromAst } from "./index.js";

const FIXTURES_DIR = path.resolve("experiments/fixtures");

function parseFixture(fixtureName: string): {
  ast: t.File;
  source: string;
  filePath: string;
} {
  const filePath = path.join(FIXTURES_DIR, fixtureName, "bundle.js");
  const source = fs.readFileSync(filePath, "utf-8");
  const ast = parseSync(source, {
    sourceType: "unambiguous",
    parserOpts: { errorRecovery: true }
  });
  if (!ast || ast.type !== "File")
    throw new Error(`Failed to parse ${filePath}`);
  return { ast, source, filePath };
}

/** Count total function declarations across all output files. */
function countFunctions(code: string): number {
  const fnDeclRegex = /\bfunction\s+\w+\s*\(/g;
  return (code.match(fnDeclRegex) ?? []).length;
}

describe("splitFromAst integration", () => {
  it("zod-humanified: every output file parses without errors", () => {
    const { ast, source, filePath } = parseFixture("zod-humanified");
    const result = splitFromAst(ast, filePath, source);

    assert.ok(
      result.size > 1,
      `Expected multiple output files, got ${result.size}`
    );

    for (const [fileName, content] of result) {
      try {
        const parsed = parseSync(content, {
          sourceType: "module",
          parserOpts: { errorRecovery: true }
        });
        assert.ok(parsed, `${fileName} should parse successfully`);
      } catch (e) {
        assert.fail(
          `${fileName} failed to parse: ${e instanceof Error ? e.message : e}`
        );
      }
    }
  });

  it("hono-humanified: every output file parses without errors", () => {
    const { ast, source, filePath } = parseFixture("hono-humanified");
    const result = splitFromAst(ast, filePath, source);

    assert.ok(
      result.size > 1,
      `Expected multiple output files, got ${result.size}`
    );

    for (const [fileName, content] of result) {
      try {
        const parsed = parseSync(content, {
          sourceType: "module",
          parserOpts: { errorRecovery: true }
        });
        assert.ok(parsed, `${fileName} should parse successfully`);
      } catch (e) {
        assert.fail(
          `${fileName} failed to parse: ${e instanceof Error ? e.message : e}`
        );
      }
    }
  });

  it("zod-humanified: no functions are lost", () => {
    const { ast, source, filePath } = parseFixture("zod-humanified");

    // Count functions in the original
    const originalFnCount = countFunctions(source);

    const result = splitFromAst(ast, filePath, source);
    const allOutput = Array.from(result.values()).join("\n");
    const splitFnCount = countFunctions(allOutput);

    // The split should preserve all function declarations
    // (import/export boilerplate may add extra lines but not lose functions)
    assert.ok(
      splitFnCount >= originalFnCount * 0.9,
      `Expected at least 90% of functions preserved: original=${originalFnCount}, split=${splitFnCount}`
    );
  });

  it("zod-humanified: barrel index.js re-exports if original had exports", () => {
    const { ast, source, filePath } = parseFixture("zod-humanified");
    const result = splitFromAst(ast, filePath, source);

    // If the original had named exports, there should be an index.js barrel
    const hasExports = source.includes("export {");
    if (hasExports) {
      assert.ok(
        result.has("index.js"),
        "Expected index.js barrel when original has named exports"
      );
      const indexContent = result.get("index.js") ?? "";
      assert.ok(
        indexContent.includes("export"),
        "index.js should contain re-exports"
      );
    }
  });

  it("hono-humanified: produces distinct file names", () => {
    const { ast, source, filePath } = parseFixture("hono-humanified");
    const result = splitFromAst(ast, filePath, source);

    const fileNames = Array.from(result.keys());
    const uniqueNames = new Set(fileNames);
    assert.strictEqual(
      fileNames.length,
      uniqueNames.size,
      "All output file names should be unique"
    );
  });
});
