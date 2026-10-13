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
 * `--root <dir>` points the scan at tiny fixture trees under
 * test/fixtures/clone-census/ instead of the living TypeScript. (It was the
 * CLONE_CENSUS_ROOT environment variable until config sweep step 7,
 * 2026-10-13: harness configuration is flags, never ambient env reads.)
 */

const REPO = path.resolve(import.meta.dirname, "..");
const SCRIPT = path.join(REPO, "scripts", "clone-census.ts");

function census(
  args: string[],
  env: Record<string, string> = {}
): { status: number | null; out: string } {
  const r = spawnSync("npx", ["tsx", SCRIPT, ...args], {
    cwd: REPO,
    encoding: "utf8",
    env: { ...process.env, ...env }
  });
  return { status: r.status, out: `${r.stdout}\n${r.stderr}` };
}

function runCensus(root: string): { status: number | null; out: string } {
  return census(["--root", root]);
}

describe("clone census ratchet", () => {
  it("goes RED on a planted cross-file twin and names the group (ratchet fires)", () => {
    const { status, out } = runCensus("test/fixtures/clone-census/red");
    assert.strictEqual(status, 1, `expected RED, got:\n${out}`);
    assert.match(out, /CENSUS: 1 unreviewed potential-duplication/);
    // The group is named: both files and the twinned function.
    assert.match(out, /clone-census\/red\/alpha\.ts:\d+\s+collectRetryDelays/);
    assert.match(out, /clone-census\/red\/beta\.ts:\d+\s+collectRetryDelays/);
  });

  it("reads no CLONE_CENSUS_ROOT from the environment: --root is the one way", () => {
    // The env var points at the RED tree; the flag at the clean one. The
    // flag must win — an ambient read would scan the red tree.
    const { status, out } = census(
      ["--root", "test/fixtures/clone-census/clean"],
      { CLONE_CENSUS_ROOT: "test/fixtures/clone-census/red" }
    );
    assert.strictEqual(status, 0, out);
    assert.match(out, /CENSUS GREEN/);
  });

  it("refuses an unknown flag, and --root without a directory", () => {
    for (const args of [["--bogus"], ["--root"], ["--root", "--loose"]]) {
      const { status, out } = census(args);
      assert.strictEqual(status, 2, `${args}: ${out}`);
    }
  });

  it("is GREEN on a fixture tree without cross-file twins", () => {
    const { status, out } = runCensus("test/fixtures/clone-census/clean");
    assert.strictEqual(status, 0, `expected GREEN, got:\n${out}`);
    assert.match(out, /CENSUS GREEN/);
  });
});
