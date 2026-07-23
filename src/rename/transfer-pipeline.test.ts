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
