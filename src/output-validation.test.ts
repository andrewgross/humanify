import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import {
  captureSemanticBaseline,
  validateOutput,
  validateOutputParses
} from "./output-validation.js";

function baselineOf(code: string) {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("Failed to parse baseline fixture");
  return captureSemanticBaseline(ast);
}

describe("validateOutputParses", () => {
  it("returns null for valid code", () => {
    const code = [
      "var initializeArrayHelpers = lazyInitializer(() => {",
      "  registerHelpers();",
      "});",
      "function storeAgent(key, agent) {",
      "  AGENT_MAP.set(key, agent);",
      "}"
    ].join("\n");
    assert.strictEqual(validateOutputParses(code), null);
  });

  it("returns null for an empty file", () => {
    assert.strictEqual(validateOutputParses(""), null);
  });

  it("reports duplicate lexical declarations in the same scope", () => {
    // Run B failure 1: two bindings transferred to the same name `NH`
    const code = [
      "function render() {",
      "  let NH = () => {",
      '    setCurrentView("mcp-tools");',
      "  };",
      "  let NH = (pluginKey) => {",
      '    setCurrentView("plugin-list");',
      "  };",
      "}"
    ].join("\n");
    const failure = validateOutputParses(code);
    assert.ok(failure, "expected a parse failure");
    assert.match(failure.message, /NH/);
    assert.strictEqual(failure.line, 5);
  });

  it("reports a reserved word used as a parameter name", () => {
    // Run B failure 2: prior-version name `delete` applied to a parameter
    const code = [
      "function storeAgent(key, delete) {",
      "  AGENT_MAP.set(key, delete);",
      "}"
    ].join("\n");
    const failure = validateOutputParses(code);
    assert.ok(failure, "expected a parse failure");
    assert.strictEqual(failure.line, 1);
    assert.strictEqual(typeof failure.column, "number");
  });

  it("includes an excerpt marking the failing line", () => {
    const code = [
      "const a = 1;",
      "const b = 2;",
      "let c = 3;",
      "let c = 4;"
    ].join("\n");
    const failure = validateOutputParses(code);
    assert.ok(failure);
    assert.ok(failure.excerpt, "expected an excerpt");
    assert.match(failure.excerpt, /> +4 \| let c = 4;/);
    assert.match(failure.excerpt, / {3}3 \| let c = 3;/);
  });

  it("parses module syntax (import/export)", () => {
    const code = 'import { x } from "./x.js";\nexport const y = x + 1;';
    assert.strictEqual(validateOutputParses(code), null);
  });
});

describe("validateOutput semantic invariants", () => {
  it("passes when renames preserve free names and binding count", () => {
    const before = "var d = 1; console.log(myAppGlobal.title, d);";
    const after = "var dayCount = 1; console.log(myAppGlobal.title, dayCount);";
    const result = validateOutput(after, baselineOf(before));
    assert.strictEqual(result.parseFailure, undefined);
    assert.strictEqual(result.semanticFailure, undefined);
  });

  it("detects capture: a previously-free name became bound (C1 class)", () => {
    // Renaming d → myAppGlobal makes every myAppGlobal.* read resolve to
    // the local. The free-name set loses myAppGlobal.
    const before = "var d = 1; console.log(myAppGlobal.title, d);";
    const after =
      "var myAppGlobal = 1; console.log(myAppGlobal.title, myAppGlobal);";
    const result = validateOutput(after, baselineOf(before));
    assert.ok(result.semanticFailure, "expected a semantic failure");
    assert.deepStrictEqual(result.semanticFailure.removedFreeNames, [
      "myAppGlobal"
    ]);
  });

  it("detects a left-behind reference: a bound name became free", () => {
    // A missed reference keeps the old name after its binding was renamed.
    const before = "var a = 1; console.log(a);";
    const after = "var counter = 1; console.log(a);";
    const result = validateOutput(after, baselineOf(before));
    assert.ok(result.semanticFailure, "expected a semantic failure");
    assert.deepStrictEqual(result.semanticFailure.addedFreeNames, ["a"]);
  });

  it("detects a binding split from a missed duplicate declaration (C2 class)", () => {
    // `var a = 1; ... var a = 2` is ONE binding; renaming only the first
    // declarator splits it into two.
    const before = "var a = 1; console.log(a); var a = 2; console.log(a);";
    const after =
      "var counter = 1; console.log(counter); var a = 2; console.log(counter);";
    const result = validateOutput(after, baselineOf(before));
    assert.ok(result.semanticFailure, "expected a semantic failure");
    assert.strictEqual(result.semanticFailure.bindingCountBefore, 1);
    assert.strictEqual(result.semanticFailure.bindingCountAfter, 2);
  });

  it("counts bindings across nested scopes", () => {
    const before = "function f(a) { let b = a; return b; } f(1);";
    const after = "function fn(x) { let y = x; return y; } fn(1);";
    const result = validateOutput(after, baselineOf(before));
    assert.strictEqual(result.semanticFailure, undefined);
  });

  it("without a baseline, only checks parsing", () => {
    const result = validateOutput("var a = 1;");
    assert.strictEqual(result.parseFailure, undefined);
    assert.strictEqual(result.semanticFailure, undefined);
  });

  it("reports parse failures through the combined entry point", () => {
    const result = validateOutput("let c = 3;\nlet c = 4;");
    assert.ok(result.parseFailure, "expected a parse failure");
    assert.strictEqual(result.semanticFailure, undefined);
  });
});
