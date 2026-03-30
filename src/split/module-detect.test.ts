import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import { detectModules, assignFunctionsToModules } from "./module-detect.js";

describe("detectModules", () => {
  it("detects esbuild ESM file-path comments", () => {
    const source = [
      "var x = 1;",
      "// src/foo.ts",
      "function foo() {}",
      "function bar() {}",
      "// src/bar.ts",
      "function baz() {}"
    ].join("\n");

    const result = detectModules(source);
    assert.equal(result.bundler, "esbuild-esm");
    assert.equal(result.modules.length, 2);
    assert.equal(result.modules[0].id, "src/foo.ts");
    assert.equal(result.modules[0].startLine, 2); // Includes comment line
    assert.equal(result.modules[0].endLine, 4);
    assert.equal(result.modules[1].id, "src/bar.ts");
    assert.equal(result.modules[1].startLine, 5); // Includes comment line
    assert.equal(result.modules[1].endLine, 6);
  });

  it("detects esbuild CJS moduleFactory wrappers", () => {
    const source = [
      "var moduleFactory = (f, m) => () => (m || f((m = { exports: {} }).exports, m), m.exports);",
      "var require_a = moduleFactory((exports, module) => {",
      "  function helper() { return 1; }",
      "  exports.default = helper;",
      "});",
      "var require_b = moduleFactory((exports) => {",
      "  exports.value = 42;",
      "});",
      "var main = require_a();"
    ].join("\n");

    const result = detectModules(source);
    assert.equal(result.bundler, "esbuild-cjs");
    assert.equal(result.modules.length, 2);
    assert.equal(result.modules[0].id, "require_a");
    assert.equal(result.modules[0].startLine, 2);
    assert.equal(result.modules[1].id, "require_b");
    assert.equal(result.modules[1].startLine, 6);
  });

  it("detects Bun CJS factory wrappers", () => {
    const source = [
      `import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);`,
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `var mod_a=x((exports,module)=>{`,
      `  function helper(){return 1}`,
      `  module.exports=helper;`,
      `});`,
      `var mod_b=x((exports)=>{`,
      `  exports.value=42;`,
      `});`,
      `var main=mod_a();`
    ].join("\n");

    const result = detectModules(source);
    assert.equal(result.bundler, "bun-cjs");
    assert.equal(result.modules.length, 2);
    assert.equal(result.modules[0].id, "mod_a");
    assert.equal(result.modules[0].startLine, 3);
    assert.equal(result.modules[1].id, "mod_b");
    assert.equal(result.modules[1].startLine, 7);
  });

  it("handles Bun factories with different helper names", () => {
    const source = [
      `var C=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `var foo=C((exports)=>{`,
      `  exports.x=1;`,
      `});`,
      `var bar=C((exports)=>{`,
      `  exports.y=2;`,
      `});`
    ].join("\n");

    const result = detectModules(source);
    assert.equal(result.bundler, "bun-cjs");
    assert.equal(result.modules.length, 2);
    assert.equal(result.modules[0].id, "foo");
    assert.equal(result.modules[1].id, "bar");
  });

  it("handles Bun factory on single mega-line", () => {
    // In real Bun bundles, each factory is on its own line (potentially 300K+ chars)
    const source = [
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `var mod_a=x((exports)=>{exports.x=1;});`,
      `var mod_b=x((exports)=>{exports.y=2;});`
    ].join("\n");

    const result = detectModules(source);
    assert.equal(result.bundler, "bun-cjs");
    // startLine === endLine for single-line factories
    assert.equal(result.modules[0].startLine, 2);
    assert.equal(result.modules[0].endLine, 2);
  });

  it("returns unknown when no patterns found", () => {
    const source = "var x = 1;\nvar y = 2;\n";
    const result = detectModules(source);
    assert.equal(result.bundler, "unknown");
    assert.equal(result.modules.length, 0);
  });

  it("computes uncovered ranges", () => {
    const source = [
      "var runtime = 1;", // line 1 - uncovered
      "// src/a.ts", // line 2
      "function a() {}", // line 3
      "// src/b.ts", // line 4
      "function b() {}" // line 5
    ].join("\n");

    const result = detectModules(source);
    assert.equal(result.uncoveredRanges.length, 1);
    assert.equal(result.uncoveredRanges[0].startLine, 1);
    assert.equal(result.uncoveredRanges[0].endLine, 1);
  });
});

describe("assignFunctionsToModules", () => {
  it("assigns functions to their enclosing module", () => {
    const modules = [
      { id: "src/a.ts", startLine: 2, endLine: 10 },
      { id: "src/b.ts", startLine: 12, endLine: 20 }
    ];
    const functions = [
      { sessionId: "f1", startLine: 3 },
      { sessionId: "f2", startLine: 8 },
      { sessionId: "f3", startLine: 15 }
    ];

    const result = assignFunctionsToModules(functions, modules);
    assert.equal(result.get("f1"), "src/a.ts");
    assert.equal(result.get("f2"), "src/a.ts");
    assert.equal(result.get("f3"), "src/b.ts");
  });

  it("skips functions not in any module", () => {
    const modules = [{ id: "src/a.ts", startLine: 5, endLine: 10 }];
    const functions = [
      { sessionId: "before", startLine: 2 },
      { sessionId: "inside", startLine: 7 },
      { sessionId: "after", startLine: 15 }
    ];

    const result = assignFunctionsToModules(functions, modules);
    assert.equal(result.has("before"), false);
    assert.equal(result.get("inside"), "src/a.ts");
    assert.equal(result.has("after"), false);
  });
});
