import assert from "node:assert";
import { describe, it } from "node:test";
import { scoreArgs } from "../scripts/eval.js";

/**
 * `eval score`'s flag parsing: what reaches run.sh. `--pipeline-arg` is the
 * one flag whose VALUE may itself start with `--` (it is a pipeline flag,
 * e.g. `--sequential`), and it repeats in order — run-launch.test.ts proves
 * run.sh appends the args at every launch site.
 */
describe("eval score argument passthrough", () => {
  it("passes --pipeline-arg values through in order, even when they look like flags", () => {
    const r = scoreArgs([
      "lbl",
      "--pipeline-arg",
      "--sequential",
      "--pairs",
      "85->86",
      "--pipeline-arg",
      "--batch-size",
      "--pipeline-arg",
      "10"
    ]);
    assert.ok(typeof r !== "string", String(r));
    assert.strictEqual(r.label, "lbl");
    assert.deepStrictEqual(r.passthrough, [
      "--pipeline-arg",
      "--sequential",
      "--pairs",
      "85->86",
      "--pipeline-arg",
      "--batch-size",
      "--pipeline-arg",
      "10"
    ]);
  });

  it("refuses --pipeline-arg without a value", () => {
    assert.strictEqual(
      typeof scoreArgs(["lbl", "--pipeline-arg"]),
      "string",
      "a dangling --pipeline-arg must be refused"
    );
  });

  it("still refuses a value flag given a flag as its value", () => {
    assert.match(
      String(scoreArgs(["lbl", "--pairs", "--sequential"])),
      /--pairs needs a value/
    );
  });

  it("without --pipeline-arg the passthrough is exactly the parsed flags", () => {
    const r = scoreArgs(["lbl", "--force-mixed", "--bin", "b"]);
    assert.ok(typeof r !== "string");
    assert.deepStrictEqual(r.passthrough, ["--force-mixed", "--bin", "b"]);
  });
});
