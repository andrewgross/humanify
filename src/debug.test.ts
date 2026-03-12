import assert from "node:assert";
import { afterEach, beforeEach, describe, it } from "node:test";
import { debug } from "./debug.js";
import { verbose } from "./verbose.js";

describe("debug output redirection", () => {
  let originalLevel: number;

  beforeEach(() => {
    originalLevel = verbose.level;
    verbose.level = 2; // Enable debug logging
  });

  afterEach(() => {
    verbose.level = originalLevel;
    debug.resetOutput();
  });

  it("setOutput redirects all debug output", () => {
    const captured: string[] = [];
    debug.setOutput((text) => captured.push(text));

    debug.log("test", "hello world");

    assert.ok(captured.length > 0, "Should have captured output");
    assert.ok(
      captured.some((l) => l.includes("hello world")),
      "Should contain the message"
    );
  });

  it("resetOutput restores default behavior", () => {
    const captured: string[] = [];
    debug.setOutput((text) => captured.push(text));
    debug.resetOutput();

    // After reset, output should go to console.log (not our captured array)
    // We can't easily test console.log without mocking, so just verify the
    // captured array doesn't grow
    const countBefore = captured.length;
    debug.log("test", "after reset");
    assert.strictEqual(
      captured.length,
      countBefore,
      "Should not capture after reset"
    );
  });

  it("redirects llmRoundtrip output", () => {
    const captured: string[] = [];
    debug.setOutput((text) => captured.push(text));

    debug.llmRoundtrip("test-method", {
      model: "test-model",
      durationMs: 100
    });

    assert.ok(captured.length > 0, "Should have captured roundtrip output");
    assert.ok(
      captured.some((l) => l.includes("test-method")),
      "Should contain method name"
    );
  });

  it("redirects rename output", () => {
    const captured: string[] = [];
    debug.setOutput((text) => captured.push(text));

    debug.rename({
      functionId: "test-fn",
      oldName: "a",
      newName: "counter"
    });

    assert.ok(
      captured.some((l) => l.includes("a") && l.includes("counter")),
      "Should contain rename info"
    );
  });

  it("redirects validation output", () => {
    const captured: string[] = [];
    debug.setOutput((text) => captured.push(text));

    debug.validation({
      valid: { a: "counter" },
      duplicates: ["b"],
      invalid: [],
      missing: ["c"]
    });

    assert.ok(
      captured.some((l) => l.includes("VALIDATION")),
      "Should contain validation header"
    );
  });

  it("does not output when disabled", () => {
    verbose.level = 0;
    const captured: string[] = [];
    debug.setOutput((text) => captured.push(text));

    debug.log("test", "should not appear");

    assert.strictEqual(captured.length, 0, "Should not output when disabled");
  });

  it("renameFallback outputs with RENAME-FALLBACK prefix", () => {
    const captured: string[] = [];
    debug.setOutput((text) => captured.push(text));

    debug.renameFallback({
      functionId: "fn:10:0",
      identifier: "a",
      suggestedName: "counter",
      rejectionReason: "duplicate",
      fallbackResult: "counterVal",
      round: 2
    });

    assert.ok(
      captured.some((l) => l.includes("[RENAME-FALLBACK]")),
      "Should contain RENAME-FALLBACK prefix"
    );
    assert.ok(
      captured.some((l) => l.includes("fn:10:0")),
      "Should contain function ID"
    );
    assert.ok(
      captured.some((l) => l.includes("counter")),
      "Should contain suggested name"
    );
    assert.ok(
      captured.some((l) => l.includes("duplicate")),
      "Should contain rejection reason"
    );
    assert.ok(
      captured.some((l) => l.includes("counterVal")),
      "Should contain fallback result"
    );
  });

  it("renameFallback does not output when disabled", () => {
    verbose.level = 0;
    const captured: string[] = [];
    debug.setOutput((text) => captured.push(text));

    debug.renameFallback({
      functionId: "fn:1:0",
      identifier: "x"
    });

    assert.strictEqual(captured.length, 0, "Should not output when disabled");
  });
});
