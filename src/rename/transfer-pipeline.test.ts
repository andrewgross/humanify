import assert from "node:assert";
import fs from "node:fs";
import { describe, it } from "node:test";
import { TRANSFER_PIPELINE } from "./prior-transfer.js";

describe("transfer pipeline registry", () => {
  it("declares the phase-1 strategies in evidence-strength order", () => {
    assert.deepStrictEqual(
      TRANSFER_PIPELINE.map((s) => s.name),
      [
        "statement-twin",
        "exact-match",
        "close-match",
        "binding-cascade",
        "vote-propagation",
        "close-match-suggestions",
        "retry"
      ]
    );
  });

  it("every step carries a non-empty description and unique name", () => {
    const names = new Set<string>();
    for (const step of TRANSFER_PIPELINE) {
      assert.ok(
        step.description.length > 20,
        `${step.name} needs a real description`
      );
      assert.ok(!names.has(step.name), `duplicate step name ${step.name}`);
      names.add(step.name);
      assert.strictEqual(typeof step.run, "function");
    }
  });

  it("docs/naming-pipeline.md mentions every registered step", () => {
    const doc = fs.readFileSync(
      new URL("../../docs/naming-pipeline.md", import.meta.url),
      "utf8"
    );
    for (const step of TRANSFER_PIPELINE) {
      assert.ok(
        doc.includes(step.name),
        `docs/naming-pipeline.md is missing pass "${step.name}" — regenerate the phase-1 table`
      );
    }
  });
});

describe("retry cycle-break stranded temps", () => {
  it("never leaves __hf_retry_N when the landing fails positionally", async () => {
    // Swap cycle a<->b: the cycle break temps `a`, the mate lands on `a`,
    // and the temped entry's landing on `b` then fails FOREVER — inner()
    // binds `b` around a reference to the temped binding (shadows-child
    // exists only on the wanted name). The temp used to survive to the
    // LLM as churn, or ship if the LLM declined. Restoration must produce
    // a real name: the original is taken by the mate, so the wanted name
    // decorated through the owner ladder.
    const { parseSync } = await import("@babel/core");
    const { traverse } = await import("../babel-utils.js");
    const { retryRejectedTransfers } = await import("./prior-transfer.js");
    const ast = parseSync(
      `var a = 1; var b = 2; function inner() { var b = 9; return a + b; }`,
      { sourceType: "module" }
    );
    assert.ok(ast);
    let scope: import("@babel/traverse").Scope | undefined;
    traverse(ast, {
      Program(p) {
        scope = p.scope;
      }
    });
    assert.ok(scope);
    const entry = (oldName: string, newName: string) => ({
      scope: scope as import("@babel/traverse").Scope,
      oldName,
      newName,
      binding: (scope as import("@babel/traverse").Scope).bindings[oldName],
      lastReason: "target-in-scope" as const
    });
    const stats = retryRejectedTransfers([entry("a", "b"), entry("b", "a")]);
    const names = Object.keys(
      (scope as import("@babel/traverse").Scope).bindings
    );
    assert.ok(
      !names.some((n) => n.startsWith("__hf_retry_")),
      `a retry temp must never survive the pass, got bindings: ${names.join(", ")}`
    );
    assert.ok(
      names.includes("a"),
      `the mate lands on the freed original name, got: ${names.join(", ")}`
    );
    assert.ok(stats.applied >= 1, "the landable half of the swap applies");
  });
});
