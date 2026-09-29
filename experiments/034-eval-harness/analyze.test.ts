/**
 * The determinism reader over `--stats-json` (the eval harness's only
 * consumer of the pipeline's stats): it must keep reading RECORDED
 * scorecards — the pre-2026-09-29 files have no `reask` block — and treat
 * the new block as additive (the 2026-09-29 stats-schema bump is additive
 * only, so every scorecard on record stays loadable).
 */
import assert from "node:assert";
import * as fs from "node:fs";
import { describe, it } from "node:test";
import { determinism } from "./analyze.js";

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
