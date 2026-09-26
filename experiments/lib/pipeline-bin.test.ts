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
  workspaceRootOf
} from "./pipeline-bin.js";

describe("pipeline-bin: which binary scored a label", () => {
  it("a run config launches its `command`; without one it is REFUSED (the TS program is gone)", () => {
    assert.deepStrictEqual(
      pipelineCommandOf({ repo: "/r", command: ["/b/humanify"] }),
      ["/b/humanify"]
    );
    // Before the cutover a command-less config meant `npx tsx src/index.ts`.
    // That program is deleted; a config that names no binary is a harness
    // bug, and must fail loudly rather than launch anything.
    assert.throws(
      () => pipelineCommandOf({ repo: "/r" }),
      /no pipeline command/
    );
    assert.throws(
      () => pipelineCommandOf({ repo: "/r", command: [] }),
      /no pipeline command/
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
