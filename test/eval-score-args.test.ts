import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import { guardBaseMode, scoreArgs } from "../scripts/eval.js";

/**
 * `eval score`'s flag parsing: what reaches run.sh. `--pipeline-set` is a
 * pipeline setting (`section.key=value`, a --config key since config sweep
 * step 5), validated here and repeated in order — run-launch.test.ts
 * proves run.sh writes them into every launch's config file.
 */
describe("eval score argument passthrough", () => {
  it("passes --pipeline-set values through in order", () => {
    const r = scoreArgs([
      "lbl",
      "--pipeline-set",
      "naming.schedule=sequential",
      "--pairs",
      "85->86",
      "--pipeline-set",
      "naming.batch_size=10"
    ]);
    assert.ok(typeof r !== "string", String(r));
    assert.strictEqual(r.label, "lbl");
    assert.deepStrictEqual(r.passthrough, [
      "--pipeline-set",
      "naming.schedule=sequential",
      "--pairs",
      "85->86",
      "--pipeline-set",
      "naming.batch_size=10"
    ]);
  });

  it("refuses --pipeline-set without a value, or with a malformed one", () => {
    for (const bad of [
      ["lbl", "--pipeline-set"],
      ["lbl", "--pipeline-set", "--sequential"],
      ["lbl", "--pipeline-set", "nodot=1"],
      ["lbl", "--pipeline-set", "naming.batch_size"]
    ]) {
      assert.strictEqual(typeof scoreArgs(bad), "string", bad.join(" "));
    }
  });

  it("refuses the retired --pipeline-arg, naming its replacement", () => {
    assert.match(
      String(scoreArgs(["lbl", "--pipeline-arg", "--sequential"])),
      /--pipeline-set/
    );
  });

  it("still refuses a value flag given a flag as its value", () => {
    assert.match(
      String(scoreArgs(["lbl", "--pairs", "--sequential"])),
      /--pairs needs a value/
    );
  });

  it("without --pipeline-set the passthrough is exactly the parsed flags", () => {
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

  // Config sweep 2026-10-13, no backwards compatibility: a renamed flag's old
  // spelling is refused naming the new one; a deleted one says why.
  it("refuses --no-vendor, naming its new spelling --no-vendor-score", () => {
    assert.match(
      String(scoreArgs(["lbl", "--no-vendor"])),
      /--no-vendor-score/
    );
    const r = scoreArgs(["lbl", "--no-vendor-score"]);
    assert.ok(typeof r !== "string", String(r));
    assert.deepStrictEqual(r.passthrough, ["--no-vendor-score"]);
  });

  it("refuses --heap-mb: it was inert for the binary and is deleted", () => {
    assert.match(String(scoreArgs(["lbl", "--heap-mb", "1"])), /removed/);
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
