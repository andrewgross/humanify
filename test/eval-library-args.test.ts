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
