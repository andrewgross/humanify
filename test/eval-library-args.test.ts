import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as path from "node:path";
import { describe, it } from "node:test";
import { libraryArgs } from "../scripts/eval.js";

const REPO = path.resolve(import.meta.dirname, "..");

/**
 * `eval library` (the exp096 library-matching score): every flag validated
 * upfront, the way `eval score`'s are — a typo must never silently no-op.
 */
describe("eval library argument parsing", () => {
  it("passes a full configuration through to run.sh in order", () => {
    const r = libraryArgs([
      "lbl",
      "--index",
      "/ix",
      "--rule",
      "structural",
      "--jobs",
      "2",
      "--force-mixed"
    ]);
    assert.ok(typeof r !== "string", String(r));
    assert.deepStrictEqual(r, [
      "lbl",
      "--index",
      "/ix",
      "--rule",
      "structural",
      "--jobs",
      "2",
      "--force-mixed"
    ]);
  });

  it("passes --corpus test through, and refuses an unknown corpus", () => {
    const r = libraryArgs([
      "test-lbl",
      "--index",
      "/ix",
      "--rule",
      "agreement",
      "--corpus",
      "test"
    ]);
    assert.ok(typeof r !== "string", String(r));
    assert.deepStrictEqual(r.slice(-2), ["--corpus", "test"]);
    assert.match(
      String(
        libraryArgs([
          "lbl",
          "--index",
          "/ix",
          "--rule",
          "agreement",
          "--corpus",
          "heldout"
        ])
      ),
      /--corpus dev\|test\|bundlers\|apps/
    );
    const b = libraryArgs([
      "dev-lbl",
      "--index",
      "/ix",
      "--rule",
      "structural",
      "--corpus",
      "bundlers"
    ]);
    assert.ok(typeof b !== "string", String(b));
    assert.deepStrictEqual(b.slice(-2), ["--corpus", "bundlers"]);
    const a = libraryArgs([
      "apps-lbl",
      "--index",
      "/ix",
      "--rule",
      "structural",
      "--corpus",
      "apps"
    ]);
    assert.ok(typeof a !== "string", String(a));
    assert.deepStrictEqual(a.slice(-2), ["--corpus", "apps"]);
  });

  it("passes --sets through (a subset of the corpus's sets), and refuses an empty one", () => {
    const r = libraryArgs([
      "test-lbl",
      "--index",
      "/ix",
      "--rule",
      "structural",
      "--corpus",
      "test",
      "--sets",
      "test-bun,test-esbuild"
    ]);
    assert.ok(typeof r !== "string", String(r));
    assert.deepStrictEqual(r.slice(-2), ["--sets", "test-bun,test-esbuild"]);
    assert.match(
      String(
        libraryArgs([
          "lbl",
          "--index",
          "/ix",
          "--rule",
          "structural",
          "--sets",
          ","
        ])
      ),
      /--sets/
    );
  });

  it("requires a label, --index and a known --rule", () => {
    assert.match(String(libraryArgs([])), /usage/);
    assert.match(
      String(libraryArgs(["lbl", "--rule", "agreement"])),
      /--index/
    );
    assert.match(
      String(libraryArgs(["lbl", "--index", "/ix", "--rule", "loose"])),
      /agreement\|structural/
    );
  });

  it("passes --pipeline-set [library] settings through, in order", () => {
    const r = libraryArgs([
      "lbl",
      "--index",
      "/ix",
      "--rule",
      "structural",
      "--pipeline-set",
      "library.discovery.max_rounds=7",
      "--pipeline-set",
      "library.lookup.cover=0.6"
    ]);
    assert.ok(typeof r !== "string", String(r));
    assert.deepStrictEqual(r.slice(5), [
      "--pipeline-set",
      "library.discovery.max_rounds=7",
      "--pipeline-set",
      "library.lookup.cover=0.6"
    ]);
  });

  it("refuses a --pipeline-set the verb cannot read, or the rule's own key", () => {
    const base = ["lbl", "--index", "/ix", "--rule", "agreement"];
    assert.match(
      String(libraryArgs([...base, "--pipeline-set", "split.enabled=true"])),
      /only \[library\.\*\] keys/
    );
    assert.match(
      String(
        libraryArgs([
          ...base,
          "--pipeline-set",
          "library.lookup.agreement=false"
        ])
      ),
      /library\.lookup\.agreement/
    );
    assert.match(
      String(libraryArgs([...base, "--pipeline-set", "nodots"])),
      /--pipeline-set/
    );
    assert.match(
      String(
        libraryArgs([
          ...base,
          "--pipeline-set",
          "library.lookup.cover=0.6",
          "--pipeline-set",
          "library.lookup.cover=0.7"
        ])
      ),
      /set twice/
    );
  });

  it("refuses an unknown flag before anything runs", () => {
    const r = spawnSync(
      "npx",
      [
        "tsx",
        path.join(REPO, "scripts/eval.ts"),
        "library",
        "x",
        "--index",
        "/ix",
        "--rule",
        "agreement",
        "--bogus"
      ],
      { encoding: "utf8", cwd: REPO }
    );
    assert.strictEqual(r.status, 2, `expected exit 2, got ${r.status}`);
    assert.match(r.stderr ?? "", /unknown flag --bogus/);
  });
});
