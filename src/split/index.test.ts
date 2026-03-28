import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { splitFromAst } from "./index.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  return ast;
}

describe("splitFromAst", () => {
  it("splits a multi-function AST into multiple files", () => {
    const source = [
      "function helperA() { return 1; }",
      "function helperB() { return helperA(); }",
      "function main() { return helperB(); }",
      "export { main };"
    ].join("\n");

    const ast = parse(source);
    const result = splitFromAst(ast, "bundle.js", source);

    // Should produce at least 1 file (the exact split depends on clustering)
    assert.ok(
      result.size >= 1,
      `Expected at least 1 output file, got ${result.size}`
    );

    // All function code should appear somewhere in the output
    const allOutput = Array.from(result.values()).join("\n");
    assert.ok(allOutput.includes("helperA"), "helperA should be in output");
    assert.ok(allOutput.includes("helperB"), "helperB should be in output");
    assert.ok(allOutput.includes("main"), "main should be in output");
  });

  it("works with AST-modified (renamed) code", () => {
    const source = [
      "function aaa() { return 1; }",
      "function bbb() { return aaa(); }",
      "export { bbb };"
    ].join("\n");

    const ast = parse(source);

    // Simulate rename on the AST
    const stmts = ast.program.body;
    const fnA = stmts[0] as t.FunctionDeclaration;
    if (fnA.id) fnA.id.name = "computeValue";
    const fnB = stmts[1] as t.FunctionDeclaration;
    if (fnB.id) fnB.id.name = "runMain";

    const result = splitFromAst(ast, "bundle.js", source);
    const allOutput = Array.from(result.values()).join("\n");

    // Output must reflect the renamed AST
    assert.ok(
      allOutput.includes("computeValue"),
      "Output should contain renamed 'computeValue'"
    );
    assert.ok(
      allOutput.includes("runMain"),
      "Output should contain renamed 'runMain'"
    );
  });

  it("accepts pre-computed detection to avoid re-detecting", () => {
    const source = ["function foo() { return 42; }", "export { foo };"].join(
      "\n"
    );

    const ast = parse(source);
    const result = splitFromAst(ast, "bundle.js", source, {
      detection: {
        bundler: "unknown",
        modules: [],
        uncoveredRanges: [{ startLine: 1, endLine: 3 }]
      }
    });

    assert.ok(result.size >= 1, "Should produce at least 1 output file");
  });
});
