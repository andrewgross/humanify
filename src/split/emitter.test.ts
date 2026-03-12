import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  buildFileContents,
  collectReferencedNames,
  extractDeclaredNames,
  extractSourceRange,
  generateBarrelIndex,
  generateExports,
  generateImports
} from "./emitter.js";
import type { SplitLedgerEntry, SplitPlan } from "./types.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  return ast;
}

function firstStatement(code: string): t.Statement {
  return parse(code).program.body[0];
}

describe("extractDeclaredNames", () => {
  it("function declaration", () => {
    const names = extractDeclaredNames(firstStatement("function foo() {}"));
    assert.deepStrictEqual(names, ["foo"]);
  });

  it("const declaration", () => {
    const names = extractDeclaredNames(firstStatement("const x = 1;"));
    assert.deepStrictEqual(names, ["x"]);
  });

  it("let with multiple declarators", () => {
    const names = extractDeclaredNames(firstStatement("let a = 1, b = 2;"));
    assert.deepStrictEqual(names, ["a", "b"]);
  });

  it("var declaration", () => {
    const names = extractDeclaredNames(firstStatement("var z = true;"));
    assert.deepStrictEqual(names, ["z"]);
  });

  it("class declaration", () => {
    const names = extractDeclaredNames(firstStatement("class MyClass {}"));
    assert.deepStrictEqual(names, ["MyClass"]);
  });

  it("expression statement returns empty", () => {
    const names = extractDeclaredNames(firstStatement("foo();"));
    assert.deepStrictEqual(names, []);
  });

  it("destructuring declaration", () => {
    const names = extractDeclaredNames(
      firstStatement("const { a, b: c } = obj;")
    );
    assert.deepStrictEqual(names, ["a", "c"]);
  });

  it("export named declaration with function", () => {
    const names = extractDeclaredNames(
      firstStatement("export function bar() {}")
    );
    assert.deepStrictEqual(names, ["bar"]);
  });
});

describe("collectReferencedNames", () => {
  it("collects identifiers from function body", () => {
    const refs = collectReferencedNames(
      firstStatement("function foo() { return bar + baz; }")
    );
    assert.ok(refs.has("bar"));
    assert.ok(refs.has("baz"));
    assert.ok(!refs.has("foo")); // binding site
  });

  it("excludes JS builtins", () => {
    const refs = collectReferencedNames(
      firstStatement("const x = Object.keys(JSON.parse(str));")
    );
    assert.ok(!refs.has("Object"));
    assert.ok(!refs.has("JSON"));
    assert.ok(refs.has("str"));
  });

  it("excludes member expression property names", () => {
    const refs = collectReferencedNames(
      firstStatement("const x = obj.property;")
    );
    assert.ok(refs.has("obj"));
    assert.ok(!refs.has("property"));
  });

  it("includes computed member expressions", () => {
    const refs = collectReferencedNames(firstStatement("const x = obj[key];"));
    assert.ok(refs.has("obj"));
    assert.ok(refs.has("key"));
  });

  it("excludes declaration LHS", () => {
    const refs = collectReferencedNames(
      firstStatement("const myVar = otherVar;")
    );
    assert.ok(!refs.has("myVar"));
    assert.ok(refs.has("otherVar"));
  });
});

describe("extractSourceRange", () => {
  it("extracts single-line statement", () => {
    const source = "const x = 1;\nconst y = 2;\nconst z = 3;";
    const ast = parse(source);
    const stmt = ast.program.body[1]; // const y = 2;
    const extracted = extractSourceRange(source, stmt);
    assert.strictEqual(extracted, "const y = 2;");
  });

  it("extracts multi-line statement", () => {
    const source = "function foo() {\n  return 1;\n}\nconst x = 1;";
    const ast = parse(source);
    const stmt = ast.program.body[0]; // function foo
    const extracted = extractSourceRange(source, stmt);
    assert.strictEqual(extracted, "function foo() {\n  return 1;\n}");
  });
});

describe("generateImports", () => {
  it("generates import declarations", () => {
    const refs = new Map<string, string[]>([
      ["core.js", ["Component", "createElement"]]
    ]);
    const result = generateImports(refs);
    assert.ok(result.includes("import"));
    assert.ok(result.includes("Component"));
    assert.ok(result.includes("createElement"));
    assert.ok(result.includes("./core.js"));
  });

  it("sorts files and names", () => {
    const refs = new Map<string, string[]>([
      ["hooks.js", ["useState"]],
      ["core.js", ["render"]]
    ]);
    const result = generateImports(refs);
    const lines = result.split("\n");
    assert.strictEqual(lines.length, 2);
    assert.ok(lines[0].includes("core.js"));
    assert.ok(lines[1].includes("hooks.js"));
  });
});

describe("generateExports", () => {
  it("generates export declaration", () => {
    const result = generateExports(["foo", "bar"]);
    assert.ok(result.includes("export"));
    assert.ok(result.includes("bar")); // sorted
    assert.ok(result.includes("foo"));
  });

  it("returns empty string for no names", () => {
    assert.strictEqual(generateExports([]), "");
  });
});

describe("generateBarrelIndex", () => {
  it("generates re-exports grouped by file", () => {
    const exports = [
      { exported: "render", local: "render" },
      { exported: "useState", local: "useState" },
      { exported: "h", local: "createElement" }
    ];
    const nameToFile = new Map([
      ["render", "dom.js"],
      ["useState", "hooks.js"],
      ["createElement", "core.js"]
    ]);
    const result = generateBarrelIndex(exports, nameToFile);
    assert.ok(result.includes("core.js"));
    assert.ok(result.includes("dom.js"));
    assert.ok(result.includes("hooks.js"));
    // Check alias export
    assert.ok(result.includes("createElement"));
  });
});

describe("buildFileContents", () => {
  it("builds a 2-file split with correct imports/exports", () => {
    const source = [
      "const SHARED = 42;",
      "function helper() { return SHARED; }",
      "function main() { return helper(); }"
    ].join("\n");

    const ast = parse(source);
    const stmts = ast.program.body;

    // Build a minimal plan: helper → utils.js, main → app.js, SHARED → utils.js
    const entries = new Map<string, SplitLedgerEntry>();
    entries.set("test:1:VariableDeclaration", {
      id: "test:1:VariableDeclaration",
      node: stmts[0],
      type: "VariableDeclaration",
      source: "test.js",
      outputFile: "utils.js"
    });
    entries.set("test:2:FunctionDeclaration", {
      id: "test:2:FunctionDeclaration",
      node: stmts[1],
      type: "FunctionDeclaration",
      source: "test.js",
      outputFile: "utils.js"
    });
    entries.set("test:3:FunctionDeclaration", {
      id: "test:3:FunctionDeclaration",
      node: stmts[2],
      type: "FunctionDeclaration",
      source: "test.js",
      outputFile: "app.js"
    });

    const plan: SplitPlan = {
      clusters: [],
      shared: new Set(),
      orphans: new Set(),
      ledger: { entries, duplicated: new Map() },
      stats: {
        totalFunctions: 2,
        totalClusters: 2,
        avgClusterSize: 1,
        sharedFunctions: 0,
        sharedRatio: 0,
        orphanFunctions: 0,
        mqScore: 1
      }
    };

    const parsedFiles = [{ ast, filePath: "test.js", source }];
    const result = buildFileContents(plan, parsedFiles);

    assert.ok(result.has("utils.js"));
    assert.ok(result.has("app.js"));

    // app.js should import helper from utils.js
    const appContent = result.get("app.js")!;
    assert.ok(appContent.includes("import"), "app.js should have imports");
    assert.ok(appContent.includes("helper"), "app.js should import helper");
    assert.ok(
      appContent.includes("utils.js"),
      "app.js should import from utils.js"
    );
    assert.ok(
      appContent.includes("function main"),
      "app.js should contain main function"
    );

    // utils.js should not import from app.js
    const utilsContent = result.get("utils.js")!;
    assert.ok(
      !utilsContent.includes("app.js"),
      "utils.js should not import from app.js"
    );
    assert.ok(
      utilsContent.includes("SHARED"),
      "utils.js should contain SHARED"
    );
    assert.ok(
      utilsContent.includes("helper"),
      "utils.js should contain helper"
    );
  });

  it("generates barrel index from export block", () => {
    const source = [
      "function render() {}",
      "function useState() {}",
      "export { render, useState };"
    ].join("\n");

    const ast = parse(source);
    const stmts = ast.program.body;

    const entries = new Map<string, SplitLedgerEntry>();
    entries.set("test:1:FunctionDeclaration", {
      id: "test:1:FunctionDeclaration",
      node: stmts[0],
      type: "FunctionDeclaration",
      source: "test.js",
      outputFile: "dom.js"
    });
    entries.set("test:2:FunctionDeclaration", {
      id: "test:2:FunctionDeclaration",
      node: stmts[1],
      type: "FunctionDeclaration",
      source: "test.js",
      outputFile: "hooks.js"
    });
    entries.set("test:3:ExportNamedDeclaration", {
      id: "test:3:ExportNamedDeclaration",
      node: stmts[2],
      type: "ExportNamedDeclaration",
      source: "test.js",
      outputFile: "index.js"
    });

    const plan: SplitPlan = {
      clusters: [],
      shared: new Set(),
      orphans: new Set(),
      ledger: { entries, duplicated: new Map() },
      stats: {
        totalFunctions: 2,
        totalClusters: 2,
        avgClusterSize: 1,
        sharedFunctions: 0,
        sharedRatio: 0,
        orphanFunctions: 0,
        mqScore: 1
      }
    };

    const parsedFiles = [{ ast, filePath: "test.js", source }];
    const result = buildFileContents(plan, parsedFiles);

    assert.ok(result.has("index.js"), "Should generate index.js");
    const indexContent = result.get("index.js")!;
    assert.ok(
      indexContent.includes("render"),
      "index.js should re-export render"
    );
    assert.ok(
      indexContent.includes("useState"),
      "index.js should re-export useState"
    );
    assert.ok(
      indexContent.includes("dom.js"),
      "index.js should reference dom.js"
    );
    assert.ok(
      indexContent.includes("hooks.js"),
      "index.js should reference hooks.js"
    );
  });

  it("no code is dropped — all source content appears in output", () => {
    const source = [
      "const A = 1;",
      "function foo() { return A; }",
      "function bar() { return foo(); }"
    ].join("\n");

    const ast = parse(source);
    const stmts = ast.program.body;

    const entries = new Map<string, SplitLedgerEntry>();
    entries.set("test:1:VariableDeclaration", {
      id: "test:1:VariableDeclaration",
      node: stmts[0],
      type: "VariableDeclaration",
      source: "test.js",
      outputFile: "file1.js"
    });
    entries.set("test:2:FunctionDeclaration", {
      id: "test:2:FunctionDeclaration",
      node: stmts[1],
      type: "FunctionDeclaration",
      source: "test.js",
      outputFile: "file1.js"
    });
    entries.set("test:3:FunctionDeclaration", {
      id: "test:3:FunctionDeclaration",
      node: stmts[2],
      type: "FunctionDeclaration",
      source: "test.js",
      outputFile: "file2.js"
    });

    const plan: SplitPlan = {
      clusters: [],
      shared: new Set(),
      orphans: new Set(),
      ledger: { entries, duplicated: new Map() },
      stats: {
        totalFunctions: 2,
        totalClusters: 2,
        avgClusterSize: 1,
        sharedFunctions: 0,
        sharedRatio: 0,
        orphanFunctions: 0,
        mqScore: 1
      }
    };

    const parsedFiles = [{ ast, filePath: "test.js", source }];
    const result = buildFileContents(plan, parsedFiles);

    // Every source line should appear in exactly one output file
    const allOutput = Array.from(result.values()).join("\n");
    assert.ok(allOutput.includes("const A = 1;"), "A should be in output");
    assert.ok(allOutput.includes("function foo()"), "foo should be in output");
    assert.ok(allOutput.includes("function bar()"), "bar should be in output");
  });
});
