import assert from "node:assert";
import { describe, it } from "node:test";
import { type CommandOptions, checkFlagInvariants } from "./unified.js";

/** Build a CommandOptions with only the fields a rule reads. */
function opts(overrides: Partial<CommandOptions>): CommandOptions {
  return { split: false, ...overrides } as CommandOptions;
}

describe("checkFlagInvariants", () => {
  it("returns no violations for a plain run", () => {
    assert.deepStrictEqual(checkFlagInvariants(opts({})), []);
  });

  const splitDependents: Array<[keyof CommandOptions, string]> = [
    ["splitRunnable", "--split-runnable"],
    ["splitLlmNames", "--split-llm-names"],
    ["splitLedger", "--split-ledger"]
  ];

  for (const [field, flag] of splitDependents) {
    it(`crashes when ${flag} is passed without --split`, () => {
      const value = field === "splitLedger" ? "ledger.json" : true;
      assert.deepStrictEqual(checkFlagInvariants(opts({ [field]: value })), [
        `${flag} requires --split`
      ]);
    });

    it(`allows ${flag} together with --split`, () => {
      const value = field === "splitLedger" ? "ledger.json" : true;
      assert.deepStrictEqual(
        checkFlagInvariants(opts({ [field]: value, split: true })),
        []
      );
    });
  }

  it("requires --naming-floor for --naming-floor-sweep", () => {
    assert.deepStrictEqual(
      checkFlagInvariants(opts({ namingFloorSweep: true })),
      ["--naming-floor-sweep requires --naming-floor"]
    );
    assert.deepStrictEqual(
      checkFlagInvariants(opts({ namingFloorSweep: true, namingFloor: true })),
      []
    );
  });

  it("requires --prior-version for --reconcile-prior-diff", () => {
    assert.deepStrictEqual(
      checkFlagInvariants(opts({ reconcilePriorDiff: true })),
      ["--reconcile-prior-diff requires --prior-version"]
    );
    assert.deepStrictEqual(
      checkFlagInvariants(
        opts({ reconcilePriorDiff: true, priorVersion: "prior.js" })
      ),
      []
    );
  });

  it("reports every violation at once, in flag order", () => {
    assert.deepStrictEqual(
      checkFlagInvariants(
        opts({
          splitRunnable: true,
          splitLlmNames: true,
          namingFloorSweep: true,
          reconcilePriorDiff: true
        })
      ),
      [
        "--split-runnable requires --split",
        "--split-llm-names requires --split",
        "--naming-floor-sweep requires --naming-floor",
        "--reconcile-prior-diff requires --prior-version"
      ]
    );
  });
});
