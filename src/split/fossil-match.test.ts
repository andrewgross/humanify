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
