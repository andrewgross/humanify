import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { after, describe, it } from "node:test";
import { labelProvenance } from "./summarize.js";

/**
 * The leaderboard across BASE MODES (2026-10-03). A scratch base (v-1 rebuilt
 * with no prior) and a seeded base (v-1 rebuilt inheriting the archive's
 * names) are different measurements of the same code; a leaderboard that
 * prints a delta between them reports the base change as the candidate's
 * effect. Named labels on different modes are REFUSED unless --force-mixed,
 * the eval dispatcher's precedent for mixed commits and pipelines.
 */
const HERE = import.meta.dirname;
const RESULTS = path.join(HERE, "results");
const LEADERBOARD = path.join(HERE, "leaderboard.ts");
const tag = `__leaderboard-test-${process.pid}`;
const made: string[] = [];
after(() => {
  for (const d of made) fs.rmSync(d, { recursive: true, force: true });
});

/** A label with a summary and one run manifest recording `baseMode`
 *  (or, with `inputs` given, whatever older shape a manifest had). */
function label(
  name: string,
  inputs: Record<string, string>,
  summaryExtra: Record<string, unknown> = {}
): string {
  const full = `${tag}-${name}`;
  const dir = path.join(RESULTS, full);
  fs.mkdirSync(dir, { recursive: true });
  made.push(dir);
  fs.writeFileSync(
    path.join(dir, "summary.json"),
    JSON.stringify({ model: full, totals: { noiseLines: 1 }, ...summaryExtra })
  );
  fs.writeFileSync(
    path.join(dir, "2.1.86-run.json"),
    JSON.stringify({
      pair: "2.1.85->2.1.86",
      inputs,
      config: { model: "m", endpoint: "e", reasoningEffort: "low" }
    })
  );
  return full;
}

function leaderboard(...args: string[]) {
  const r = spawnSync("npx", ["tsx", LEADERBOARD, ...args], {
    encoding: "utf8"
  });
  return { status: r.status, out: `${r.stdout}${r.stderr}` };
}

const REBASED = "/w/2.1.85-rebased/.humanify/humanified.js";

describe("leaderboard across base modes", () => {
  it("REFUSES named labels scored on different base modes", () => {
    const seeded = label("seeded", {
      prior: REBASED,
      priorKind: "rebased",
      baseMode: "seeded"
    });
    const scratch = label("scratch", {
      prior: REBASED,
      priorKind: "rebased",
      baseMode: "scratch"
    });
    const r = leaderboard(seeded, scratch);
    assert.strictEqual(r.status, 2, r.out);
    assert.match(r.out, /base mode/i);
    assert.match(r.out, /--force-mixed/);
    assert.doesNotMatch(r.out, /=== eval leaderboard/);
  });

  it("--force-mixed prints the table with a loud MIXED BASE MODES note", () => {
    const seeded = label("seeded2", {
      prior: REBASED,
      priorKind: "rebased",
      baseMode: "seeded"
    });
    const scratch = label("scratch2", {
      prior: REBASED,
      priorKind: "rebased",
      baseMode: "scratch"
    });
    const r = leaderboard(seeded, scratch, "--force-mixed");
    assert.strictEqual(r.status, 0, r.out);
    assert.match(r.out, /=== eval leaderboard/);
    assert.match(r.out, /MIXED BASE MODES/);
  });

  it("a pre-field reference (rebased prior, no baseMode) reads as SEEDED", () => {
    // Every recorded reference (main-2026-09-18 and the 843826be/8af0574f
    // labels) was scored before the field, on archive-seeded rebuilds.
    const old = label("old", { prior: REBASED, priorKind: "rebased" });
    const scratch = label("scratch3", {
      prior: REBASED,
      priorKind: "rebased",
      baseMode: "scratch"
    });
    const r = leaderboard(old, scratch);
    assert.strictEqual(r.status, 2, r.out);
    assert.match(r.out, new RegExp(`${old}=seeded`));
  });

  it("same-mode labels print the table and each label's base", () => {
    const a = label("a", { prior: REBASED, baseMode: "scratch" });
    const b = label("b", { prior: REBASED, baseMode: "scratch" });
    const r = leaderboard(a, b);
    assert.strictEqual(r.status, 0, r.out);
    assert.match(r.out, new RegExp(`base: ${a}=scratch · ${b}=scratch`));
  });

  it("the summary's recorded base modes win over re-deriving them", () => {
    const a = label(
      "rec",
      { prior: REBASED, priorKind: "rebased" },
      {
        provenance: {
          models: ["m"],
          endpoints: ["e"],
          reasoningEfforts: ["low"],
          baseModes: ["scratch"]
        }
      }
    );
    const b = label("rec2", { prior: REBASED, baseMode: "scratch" });
    const r = leaderboard(a, b);
    assert.strictEqual(r.status, 0, r.out);
  });
});

describe("summary records the label's base modes", () => {
  it("labelProvenance carries baseModes from the manifests", () => {
    const l = label("prov", { prior: REBASED, baseMode: "scratch" });
    assert.deepStrictEqual(labelProvenance(path.join(RESULTS, l)).baseModes, [
      "scratch"
    ]);
  });
});
