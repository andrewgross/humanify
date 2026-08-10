import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as path from "node:path";
import { describe, it } from "node:test";

/**
 * The switch census (scripts/switch-census.ts) is a ratchet: every registry
 * kill switch must carry a reviewed kind, and every option field must be
 * reachable and read. A ratchet is only worth trusting if BOTH directions
 * are proven — that it passes on the current tree, and that its failure
 * path can actually fire (a detector whose red was never observed is a
 * zero nobody validated).
 */

const REPO = path.resolve(import.meta.dirname, "..");
const TSX = path.join(REPO, "node_modules", ".bin", "tsx");
const SCRIPT = path.join(REPO, "scripts", "switch-census.ts");

function runCensus(env: Record<string, string> = {}): {
  status: number | null;
  out: string;
} {
  const r = spawnSync(TSX, [SCRIPT], {
    cwd: REPO,
    encoding: "utf8",
    env: { ...process.env, ...env }
  });
  return { status: r.status, out: `${r.stdout}\n${r.stderr}` };
}

describe("switch census", () => {
  it("is green on the current tree and covers every registry switch", () => {
    const { status, out } = runCensus();
    assert.strictEqual(status, 0, `census not green:\n${out}`);
    assert.match(out, /CENSUS GREEN/);
    // Spot-check that the registry parse found real switches — an empty
    // parse would be vacuously green.
    assert.match(out, /HUMANIFY_NO_FAMILY_PERMUTE/);
    assert.match(out, /HUMANIFY_NO_POST_SPLIT_RECONCILE/);
    assert.match(out, /--split-strategy/);
  });

  it("goes RED when a registry switch has no reviewed kind (ratchet fires)", () => {
    const { status, out } = runCensus({
      SWITCH_CENSUS_INJECT_FAKE_SWITCH: "1"
    });
    assert.strictEqual(status, 1, `expected RED, got:\n${out}`);
    assert.match(out, /UNCLASSIFIED/);
    assert.match(out, /HUMANIFY_FAKE_SWITCH_FOR_SELF_TEST/);
    // The fake switch is unread too, but UNCLASSIFIED must win: kind review
    // comes before reachability.
    assert.match(out, /CENSUS RED/);
  });
});
