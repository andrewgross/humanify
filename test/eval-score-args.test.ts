import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import { guardBaseMode, scoreArgs } from "../scripts/eval.js";

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

  it("defaults to a SCRATCH base, with no flag passed to run.sh", () => {
    const r = scoreArgs(["lbl"]);
    assert.ok(typeof r !== "string");
    assert.strictEqual(r.baseMode, "scratch");
    assert.deepStrictEqual(r.passthrough, []);
  });

  it("--seeded-base selects the archive-seeded rebuild and reaches run.sh", () => {
    const r = scoreArgs(["lbl", "--seeded-base"]);
    assert.ok(typeof r !== "string", String(r));
    assert.strictEqual(r.baseMode, "seeded");
    assert.deepStrictEqual(r.passthrough, ["--seeded-base"]);
  });

  it("--archive-prior is the archive mode", () => {
    const r = scoreArgs(["lbl", "--archive-prior"]);
    assert.ok(typeof r !== "string");
    assert.strictEqual(r.baseMode, "archive");
  });

  it("refuses --seeded-base together with --archive-prior: one base per run", () => {
    assert.match(
      String(scoreArgs(["lbl", "--seeded-base", "--archive-prior"])),
      /--seeded-base.*--archive-prior|--archive-prior.*--seeded-base/
    );
  });
});

/**
 * summarize totals EVERY card in a label's directory, so scoring a scratch
 * run into a label that already holds seeded cards would produce a summary
 * that reads as one run on one base — refused, like a mixed commit.
 */
describe("eval score refuses to mix base modes inside one label", () => {
  const mkLabel = (files: Record<string, string>): string => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "basemode-"));
    for (const [f, body] of Object.entries(files)) {
      fs.writeFileSync(path.join(dir, f), body);
    }
    return dir;
  };

  it("refuses a scratch run into a label whose pipeline.json says seeded", () => {
    const dir = mkLabel({
      "commit.txt": "abc\n",
      "pipeline.json": JSON.stringify({
        pipeline: { kind: "rust-bin" },
        baseMode: "seeded"
      })
    });
    const err = guardBaseMode(dir, "lbl", "scratch", false);
    assert.match(String(err), /seeded/);
    assert.match(String(err), /scratch/);
    assert.strictEqual(guardBaseMode(dir, "lbl", "seeded", false), null);
    assert.strictEqual(guardBaseMode(dir, "lbl", "scratch", true), null);
  });

  it("a label from before the field is read from its manifests (rebased = seeded)", () => {
    const dir = mkLabel({
      "commit.txt": "abc\n",
      "2.1.86-run.json": JSON.stringify({
        pair: "2.1.85->2.1.86",
        inputs: {
          input: "/i",
          prior: "/w/2.1.85-rebased/.humanify/humanified.js",
          priorKind: "rebased"
        }
      })
    });
    assert.match(String(guardBaseMode(dir, "lbl", "scratch", false)), /seeded/);
  });

  it("a fresh label is never refused", () => {
    assert.strictEqual(
      guardBaseMode(
        path.join(os.tmpdir(), "no-such-label-xyz"),
        "lbl",
        "scratch",
        false
      ),
      null
    );
  });
});
