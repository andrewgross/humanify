import assert from "node:assert";
import { describe, it } from "node:test";
import { matchFossilModules } from "./fossil-match.js";

/** Shorthand: a module signature for the matcher (hashes pre-sorted). */
function mod(hashes: string[], imports: number[] = []) {
  return { hashes: [...hashes].sort(), imports };
}

describe("matchFossilModules", () => {
  it("tier A: unique signatures match exactly, duplicates do not", () => {
    const prior = [mod(["h1", "h2"]), mod(["twin"]), mod(["twin"])];
    const fresh = [mod(["twin"]), mod(["h1", "h2"]), mod(["twin"])];
    const { matches, tiers } = matchFossilModules(prior, fresh);
    // Unique signature h1|h2 matches; the twin pair stays unmatched —
    // identical signatures with silent edges must NEVER pair by position
    // (the i36/Pd8 lesson).
    assert.strictEqual(matches.get(1), 0);
    assert.strictEqual(matches.size, 1);
    assert.strictEqual(tiers["unique-signature"], 1);
  });

  it("tier B: edge agreement disambiguates twins", () => {
    // Two identical twins on each side; each twin imports a DIFFERENT
    // uniquely-matched module, so edges license the pairing.
    const prior = [
      mod(["anchorA"]),
      mod(["anchorB"]),
      mod(["twin"], [0]),
      mod(["twin"], [1])
    ];
    const fresh = [
      mod(["anchorB"]),
      mod(["anchorA"]),
      mod(["twin"], [1]),
      mod(["twin"], [0])
    ];
    const { matches } = matchFossilModules(prior, fresh);
    // anchors by tier A (crossed order), then twins by their edges:
    // fresh 2 imports fresh[1]=anchorA, so it pairs with prior 2 (which
    // imports prior[0]=anchorA); fresh 3 pairs with prior 3 via anchorB.
    assert.strictEqual(matches.get(1), 0); // anchorA
    assert.strictEqual(matches.get(0), 1); // anchorB
    assert.strictEqual(matches.get(2), 2); // twin importing anchorA
    assert.strictEqual(matches.get(3), 3); // twin importing anchorB
  });

  it("a changed module matches by overlap only when unique and high", () => {
    const prior = [mod(["a", "b", "c", "d", "e"])];
    const fresh = [mod(["a", "b", "c", "d", "x"])]; // 4/6 = 0.67 Jaccard
    const { matches } = matchFossilModules(prior, fresh);
    // 0.67 < 0.8 and no edge evidence: abstain.
    assert.strictEqual(matches.size, 0);

    const fresh2 = [mod(["a", "b", "c", "d", "e", "x"])]; // 5/6 = 0.83
    const second = matchFossilModules(prior, fresh2);
    assert.strictEqual(second.matches.get(0), 0);
  });
});

describe("matchFossilModules — tier C: stem corroboration (exp074)", () => {
  /** A module signature carrying its declared-name stem. */
  function named(stem: string, hashes: string[], imports: number[] = []) {
    return { hashes: [...hashes].sort(), imports, stem };
  }

  it("recovers a slightly-changed module the overlap tiers abstain on", () => {
    // The exp074 case, measured on 85→86: `access-property` went 17→18
    // statements (overlap 0.75) with a single import edge, so tier B's
    // licensing (edge agreement, or a lone candidate at ≥0.8) could not
    // fire — and the fresh mint moved it to another folder, churning
    // 3,204 require lines. Its STEM was identical across both runs.
    // 3 shared of 4 union = 0.75 overlap, matching the real case.
    const prior = [named("access-property", ["a", "b", "c", "d"])];
    const fresh = [named("access-property", ["a", "b", "c"])];
    const bare = matchFossilModules(
      prior.map(({ hashes, imports }) => ({ hashes, imports })),
      fresh.map(({ hashes, imports }) => ({ hashes, imports }))
    );
    assert.strictEqual(bare.matches.size, 0, "overlap tiers must abstain");

    const { matches, tiers } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.get(0), 0);
    assert.strictEqual(tiers["stem-corroborated"], 1);
  });

  it("disambiguates content-twins whose stems are unique", () => {
    // Identical content, distinct declared names: the signature tier
    // cannot pair them (twins) but the stems can. A mispairing here is
    // harmless by construction — the contents are identical — which is
    // part of why this tier is licensed for twins at all.
    const prior = [named("noop-one", ["t"]), named("noop-two", ["t"])];
    const fresh = [named("noop-two", ["t"]), named("noop-one", ["t"])];
    const { matches } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.get(0), 1, "noop-two → noop-two");
    assert.strictEqual(matches.get(1), 0, "noop-one → noop-one");
  });

  it("abstains when a stem is not unique on both sides", () => {
    // Contents deliberately distinct so tiers A and B cannot fire and the
    // stem tier is the only one left to (wrongly) pair them.
    const prior = [
      named("dup", ["a", "b", "c"]),
      named("dup", ["a", "b", "d"])
    ];
    const fresh = [named("dup", ["a", "b", "c", "e"])];
    const { matches } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.size, 0);
  });

  it("abstains when the stem matches but the content does not", () => {
    // Same name, unrelated content: a module renamed away and a new one
    // that happened to take the name. Zero overlap must veto.
    const prior = [named("helper", ["a", "b", "c"])];
    const fresh = [named("helper", ["x", "y", "z"])];
    const { matches } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.size, 0);
  });
});
