import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import {
  binCommitRefusal,
  pipelineCommandOf,
  sameCommit,
  sha256File,
  tsPipelineCommand,
  workspaceRootOf
} from "./pipeline-bin.js";

describe("pipeline-bin: which binary scored a label", () => {
  it("the TS command is exactly the one the harness always ran", () => {
    // run-pipeline.ts spawned this literal before --bin existed; a config
    // without `command` must still produce it, byte for byte.
    assert.deepStrictEqual(tsPipelineCommand("/r"), [
      "npx",
      "tsx",
      "/r/src/index.ts"
    ]);
  });

  it("a run config without `command` launches the TS program; with one, that", () => {
    assert.deepStrictEqual(pipelineCommandOf({ repo: "/r" }), [
      "npx",
      "tsx",
      "/r/src/index.ts"
    ]);
    assert.deepStrictEqual(
      pipelineCommandOf({ repo: "/r", command: ["/b/humanify"] }),
      ["/b/humanify"]
    );
  });

  it("finds the cargo workspace that owns target/release/<bin>", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-bin-"));
    try {
      fs.writeFileSync(path.join(root, "Cargo.toml"), "[workspace]\n");
      const bin = path.join(root, "target/release/humanify");
      fs.mkdirSync(path.dirname(bin), { recursive: true });
      fs.writeFileSync(bin, "");
      assert.strictEqual(workspaceRootOf(bin), root);
      // Not under target/: a binary copied elsewhere has no knowable build.
      const loose = path.join(root, "humanify");
      fs.writeFileSync(loose, "");
      assert.strictEqual(workspaceRootOf(loose), null);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it("short and long shas of one commit are the same commit", () => {
    assert.ok(sameCommit("36ce8c5", "36ce8c5aa1"));
    assert.ok(sameCommit("36ce8c5aa1", "36ce8c5"));
    assert.ok(!sameCommit("36ce8c5", "730eb99"));
    assert.ok(!sameCommit("", "730eb99"), "an unknown commit matches nothing");
  });

  it("refuses a binary built from another commit unless forced", () => {
    const base = { binDirty: false, labelCommit: "36ce8c5" };
    assert.strictEqual(
      binCommitRefusal({ ...base, binCommit: "36ce8c5aa1", force: false }),
      null
    );
    assert.match(
      binCommitRefusal({ ...base, binCommit: "730eb99", force: false }) ?? "",
      /730eb99.*36ce8c5[\s\S]*--force-mixed/
    );
    assert.strictEqual(
      binCommitRefusal({ ...base, binCommit: "730eb99", force: true }),
      null
    );
  });

  it("refuses an UNKNOWN build commit and a DIRTY build tree unless forced", () => {
    // Both mean the binary corresponds to no commit — the label would carry
    // a sha nobody can rebuild.
    assert.match(
      binCommitRefusal({
        binCommit: "",
        binDirty: false,
        labelCommit: "36ce8c5",
        force: false
      }) ?? "",
      /unknown/
    );
    assert.match(
      binCommitRefusal({
        binCommit: "36ce8c5",
        binDirty: true,
        labelCommit: "36ce8c5",
        force: false
      }) ?? "",
      /dirty/i
    );
  });

  it("hashes the file it is given", () => {
    const f = path.join(os.tmpdir(), `pipeline-bin-${process.pid}`);
    fs.writeFileSync(f, "abc");
    try {
      assert.strictEqual(
        sha256File(f),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
      );
    } finally {
      fs.rmSync(f);
    }
  });
});
