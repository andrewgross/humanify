import assert from "node:assert";
import { describe, it } from "node:test";
import { validateOutputParses } from "./output-validation.js";

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
