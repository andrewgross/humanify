import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as path from "node:path";
import { describe, it } from "node:test";

/**
 * The clone census (scripts/clone-census.ts) is a ratchet: a NEW cross-file
 * copy-paste twin must fail the gate unless it is allowlisted. A ratchet is
 * only worth trusting if BOTH directions are proven — that it passes on a
 * tree without twins, and that its failure path can actually fire (a
 * detector whose red was never observed is a zero nobody validated).
 *
 * CLONE_CENSUS_ROOT points the scan at tiny fixture trees under
 * test/fixtures/clone-census/ instead of src/.
 */

const REPO = path.resolve(import.meta.dirname, "..");
const SCRIPT = path.join(REPO, "scripts", "clone-census.ts");

function runCensus(root: string): { status: number | null; out: string } {
  const r = spawnSync("npx", ["tsx", SCRIPT], {
    cwd: REPO,
    encoding: "utf8",
    env: { ...process.env, CLONE_CENSUS_ROOT: root }
  });
  return { status: r.status, out: `${r.stdout}\n${r.stderr}` };
}

describe("clone census ratchet", () => {
  it("goes RED on a planted cross-file twin and names the group (ratchet fires)", () => {
    const { status, out } = runCensus("test/fixtures/clone-census/red");
    assert.strictEqual(status, 1, `expected RED, got:\n${out}`);
    assert.match(out, /CENSUS RED/);
    // The group is named: both files and the twinned function.
    assert.match(out, /clone-census\/red\/alpha\.ts:\d+\s+collectRetryDelays/);
    assert.match(out, /clone-census\/red\/beta\.ts:\d+\s+collectRetryDelays/);
  });

  it("is GREEN on a fixture tree without cross-file twins", () => {
    const { status, out } = runCensus("test/fixtures/clone-census/clean");
    assert.strictEqual(status, 0, `expected GREEN, got:\n${out}`);
    assert.match(out, /CENSUS GREEN/);
  });
});
