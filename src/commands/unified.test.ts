import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import {
  type CommandOptions,
  checkFlagInvariants,
  effectiveLeverConfig,
  releaseSplitSourceState,
  removeConsumedSourceFile
} from "./unified.js";

/** Build a CommandOptions with only the fields a rule reads. */
function opts(overrides: Partial<CommandOptions>): CommandOptions {
  return { split: false, ...overrides } as CommandOptions;
}

describe("checkFlagInvariants", () => {
  it("returns no violations for a plain run", () => {
    assert.deepStrictEqual(checkFlagInvariants(opts({})), []);
  });

  const splitDependents: Array<[keyof CommandOptions, string]> = [
    ["splitPure", "--split-pure"],
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

  it("allows --naming-floor-sweep alone (the floor is on by default)", () => {
    assert.deepStrictEqual(
      checkFlagInvariants(opts({ namingFloorSweep: true })),
      []
    );
  });

  it("rejects --naming-floor-sweep with --no-naming-floor", () => {
    assert.deepStrictEqual(
      checkFlagInvariants(opts({ namingFloorSweep: true, namingFloor: false })),
      ["--naming-floor-sweep requires --naming-floor"]
    );
  });

  it("does not treat reconcile-without-prior as a violation (default-on, effective-gated)", () => {
    // reconcilePriorDiff defaults ON and is gated by prior presence at
    // assembly — a prior-less run silently skips the pass instead of
    // erroring, since the default would otherwise break every plain run.
    assert.deepStrictEqual(
      checkFlagInvariants(opts({ reconcilePriorDiff: true })),
      []
    );
  });

  it("reports every violation at once, in flag order", () => {
    assert.deepStrictEqual(
      checkFlagInvariants(
        opts({
          splitPure: true,
          namingFloorSweep: true,
          namingFloor: false
        })
      ),
      [
        "--split-pure requires --split",
        "--naming-floor-sweep requires --naming-floor"
      ]
    );
  });

  describe("effectiveLeverConfig", () => {
    // The three shipped noise levers were flag-gated and dormant in every
    // production walk run. Deterministic levers now default ON: the
    // naming floor always, the prior-diff reconcile whenever a prior is
    // present (the pass self-discards if it cannot hold the pure-rename
    // invariant). The LLM sweep stays opt-in.
    it("defaults the naming floor on and reconcile on-with-prior", () => {
      assert.deepStrictEqual(effectiveLeverConfig(opts({}), true), {
        namingFloor: true,
        namingFloorSweep: false,
        reconcilePriorDiff: true
      });
      assert.deepStrictEqual(effectiveLeverConfig(opts({}), false), {
        namingFloor: true,
        namingFloorSweep: false,
        reconcilePriorDiff: false
      });
    });

    it("honors explicit opt-outs", () => {
      assert.deepStrictEqual(
        effectiveLeverConfig(
          opts({ namingFloor: false, reconcilePriorDiff: false }),
          true
        ),
        {
          namingFloor: false,
          namingFloorSweep: false,
          reconcilePriorDiff: false
        }
      );
    });

    it("keeps the sweep opt-in and dependent on the floor", () => {
      assert.strictEqual(
        effectiveLeverConfig(opts({ namingFloorSweep: true }), false)
          .namingFloorSweep,
        true
      );
      assert.strictEqual(
        effectiveLeverConfig(
          opts({ namingFloorSweep: true, namingFloor: false }),
          false
        ).namingFloorSweep,
        false
      );
    });
  });

  describe("flag values", () => {
    it("accepts every documented --bundler value", () => {
      for (const value of [
        "webpack",
        "browserify",
        "rollup",
        "esbuild",
        "parcel",
        "bun"
      ]) {
        assert.deepStrictEqual(
          checkFlagInvariants(opts({ bundler: value })),
          []
        );
      }
    });

    it("accepts every documented --minifier value, including none", () => {
      for (const value of ["terser", "esbuild", "swc", "bun", "none"]) {
        assert.deepStrictEqual(
          checkFlagInvariants(opts({ minifier: value })),
          []
        );
      }
    });

    it("rejects an unknown --bundler value", () => {
      assert.deepStrictEqual(checkFlagInvariants(opts({ bundler: "foobar" })), [
        '--bundler must be one of: webpack, browserify, rollup, esbuild, parcel, bun (got "foobar")'
      ]);
    });

    it("rejects an unknown --minifier value", () => {
      assert.deepStrictEqual(checkFlagInvariants(opts({ minifier: "gzip" })), [
        '--minifier must be one of: terser, esbuild, swc, bun, none (got "gzip")'
      ]);
    });

    it('rejects the sentinel "unknown" — it can never take effect', () => {
      assert.deepStrictEqual(
        checkFlagInvariants(opts({ bundler: "unknown" })),
        [
          '--bundler must be one of: webpack, browserify, rollup, esbuild, parcel, bun (got "unknown")'
        ]
      );
    });

    it("reports precondition and value violations together", () => {
      assert.deepStrictEqual(
        checkFlagInvariants(
          opts({ splitPure: true, bundler: "foobar", minifier: "gzip" })
        ),
        [
          "--split-pure requires --split",
          '--bundler must be one of: webpack, browserify, rollup, esbuild, parcel, bun (got "foobar")',
          '--minifier must be one of: terser, esbuild, swc, bun, none (got "gzip")'
        ]
      );
    });
  });
});

describe("removeConsumedSourceFile", () => {
  let dir: string;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-consumed-"));
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("removes the unpack copy inside the output dir", () => {
    const copy = path.join(dir, "index.js");
    fs.writeFileSync(copy, "code");
    removeConsumedSourceFile(dir, copy, "/elsewhere/input.js");
    assert.strictEqual(fs.existsSync(copy), false);
  });

  it("never deletes a path outside the output dir", () => {
    const outside = path.join(
      os.tmpdir(),
      `humanify-outside-${process.pid}.js`
    );
    fs.writeFileSync(outside, "code");
    try {
      removeConsumedSourceFile(dir, outside, "/elsewhere/input.js");
      assert.strictEqual(fs.existsSync(outside), true);
    } finally {
      fs.rmSync(outside, { force: true });
    }
  });

  it("never deletes the user's own input, even when it sits inside the output dir", () => {
    // `humanify /proj/index.js --split -o /proj`: the Bun passthrough writes
    // outputDir/index.js === the input, so the consumed copy IS the input.
    const input = path.join(dir, "index.js");
    fs.writeFileSync(input, "the user's source");
    removeConsumedSourceFile(dir, input, input);
    assert.strictEqual(
      fs.existsSync(input),
      true,
      "must not delete the input the run was given"
    );
  });

  it("compares the input by resolved path, not string identity", () => {
    const input = path.join(dir, "index.js");
    fs.writeFileSync(input, "the user's source");
    // Same file, non-normalized spelling — must still be recognized as input.
    removeConsumedSourceFile(dir, input, path.join(dir, ".", "index.js"));
    assert.strictEqual(fs.existsSync(input), true);
  });
});

describe("releaseSplitSourceState", () => {
  it("clears the post-rename AST and wrapper parse", () => {
    // These two hold the whole bundle's scope-resolved NodePath/Scope graph
    // (~GBs). Once the stable tree is on disk the Bun re-link (which reads the
    // tree from disk) needs neither; leaving them reachable makes every GC the
    // re-link triggers trace the graph, turning seconds into tens of minutes.
    const renameResult = { ast: { type: "File" }, code: "x=1;" };
    const stable = { wrapper: { path: {} }, ledger: {} };

    releaseSplitSourceState(renameResult, stable);

    assert.strictEqual(renameResult.ast, undefined);
    assert.strictEqual(stable.wrapper, undefined);
    // Only the heavy references are dropped; the rest of each object is intact.
    assert.strictEqual(renameResult.code, "x=1;");
    assert.deepStrictEqual(stable.ledger, {});
  });
});
