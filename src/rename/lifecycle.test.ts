import assert from "node:assert";
import { describe, it } from "node:test";
import {
  isPending,
  isSettled,
  markFailed,
  markLlmDone,
  markSkipped,
  markTransferred,
  transition,
  type Stateful
} from "./lifecycle.js";

function node(): Stateful {
  return { sessionId: "test:1:0", state: { kind: "pending" } };
}

describe("lifecycle state machine", () => {
  it("starts pending and is not settled", () => {
    const n = node();
    assert.strictEqual(isPending(n), true);
    assert.strictEqual(isSettled(n), false);
  });

  it("transitions pending → transferred carrying names", () => {
    const n = node();
    markTransferred(n, { a: "alpha" });
    assert.strictEqual(n.state.kind, "transferred");
    assert.strictEqual(isSettled(n), true);
    assert.strictEqual(isPending(n), false);
    if (n.state.kind === "transferred") {
      assert.deepStrictEqual(n.state.names, { a: "alpha" });
    }
  });

  it("transitions pending → llm-done carrying names", () => {
    const n = node();
    markLlmDone(n, { b: "beta" });
    assert.strictEqual(n.state.kind, "llm-done");
    if (n.state.kind === "llm-done") {
      assert.deepStrictEqual(n.state.names, { b: "beta" });
    }
  });

  it("defaults llm-done names to an empty object", () => {
    const n = node();
    markLlmDone(n);
    assert.strictEqual(n.state.kind, "llm-done");
    if (n.state.kind === "llm-done") {
      assert.deepStrictEqual(n.state.names, {});
    }
  });

  it("transitions pending → skipped carrying a reason", () => {
    const n = node();
    markSkipped(n, "library");
    assert.strictEqual(n.state.kind, "skipped");
    if (n.state.kind === "skipped") {
      assert.strictEqual(n.state.reason, "library");
    }
  });

  it("transitions pending → failed carrying an error", () => {
    const n = node();
    markFailed(n, "boom");
    assert.strictEqual(n.state.kind, "failed");
    if (n.state.kind === "failed") {
      assert.strictEqual(n.state.error, "boom");
    }
  });

  it("throws on any transition out of a settled state", () => {
    const n = node();
    markSkipped(n, "library");
    assert.throws(
      () => markLlmDone(n, {}),
      /illegal lifecycle transition/,
      "settled → settled must throw"
    );
    assert.throws(
      () => transition(n, { kind: "pending" }),
      /illegal lifecycle transition/,
      "settled → pending must throw"
    );
  });

  it("names the node in the error message", () => {
    const n = node();
    markLlmDone(n, {});
    assert.throws(() => markFailed(n, "x"), /test:1:0/);
  });
});
