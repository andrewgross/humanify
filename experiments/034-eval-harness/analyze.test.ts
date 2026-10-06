/**
 * The determinism reader over `--stats-json` (the eval harness's only
 * consumer of the pipeline's stats): it must keep reading RECORDED
 * scorecards — the pre-2026-09-29 files have no `reask` block — and treat
 * the new block as additive (the 2026-09-29 stats-schema bump is additive
 * only, so every scorecard on record stays loadable).
 */
import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import { determinism, vendorChurn } from "./analyze.js";

/** A recorded scorecard from the exp050 cold runs (old format: no reask). */
const RECORDED = "results/exp050-cold/2.1.216.stats.json";

function readStats(): Record<string, unknown> {
  return JSON.parse(
    fs.readFileSync(new URL(RECORDED, import.meta.url), "utf8")
  ) as Record<string, unknown>;
}

describe("analyze determinism over --stats-json", () => {
  it("still reads a RECORDED (pre-reask) scorecard", () => {
    const card = determinism(readStats());
    assert.ok(card.functions.total > 0, "the coverage block is read");
    assert.ok(card.mintedLeftovers >= 0, "the minted census is read");
  });

  it("treats the reask block as additive — it cannot move a KPI", () => {
    const stats = readStats();
    const before = determinism(stats);
    stats.reask = {
      unrecoverableRejections: 1,
      lateRejections: 2,
      invalidSuggestionFinishes: 1,
      allFailedWindows: 4,
      sweepReasked: 6,
      sweepReaskApplied: 4,
      sweepReaskDropped: 2
    };
    assert.deepStrictEqual(determinism(stats), before);
  });
});

/**
 * The vendor card carries the relocation pairs (2026-10-06): a file that
 * changed a little AND drew a new path is charged its own diff as real, and
 * the move is listed rather than absorbed.
 */
describe("analyze vendor card", () => {
  it("lists a relocated file and charges only its own diff as real", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "analyze-vendor-"));
    try {
      const text = (edit: string) =>
        [
          'const { __commonJS } = require("../.humanify/__bun-runtime.js");',
          "exports.f = __commonJS(function(a,b){b.exports=`",
          ...Array.from({ length: 60 }, (_, i) =>
            i === 30 ? edit : `Line ${i} of a bundled prompt, kept verbatim.`
          ),
          "`});",
          ""
        ].join("\n");
      fs.mkdirSync(path.join(root, "prior"));
      fs.mkdirSync(path.join(root, "fresh"));
      fs.writeFileSync(path.join(root, "prior/old-name.js"), text("Old."));
      fs.writeFileSync(path.join(root, "fresh/new-name.js"), text("New."));
      const card = vendorChurn(
        path.join(root, "prior"),
        path.join(root, "fresh")
      );
      assert.strictEqual(card.real, 2);
      assert.strictEqual(card.relocated.files, 1);
      assert.deepStrictEqual(
        card.relocated.pairs.map((p) => [p.prior, p.fresh, p.realLines]),
        [["old-name.js", "new-name.js", 2]]
      );
      assert.strictEqual(card.churnLines, card.noise + card.real);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
