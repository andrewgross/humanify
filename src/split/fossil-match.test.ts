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

/**
 * GRADED content similarity (exp078 Task 2d).
 *
 * The exact-hash comparison asks, per statement, "same fingerprint or not" —
 * one bit. A statement 95% identical and one 0% identical both score zero, so
 * two enclosures sharing only a trivial `var X = {};` score exactly as high
 * as two sharing most of their body.
 *
 * Measured on a real release: the 74 pairs the 0.5 content floor REJECTS have
 * median exact overlap 0.30 and median GRADED similarity 0.858, with 70 of 74
 * above 0.5 — the floor was rejecting enclosures that are ~86% the same.
 * Confidently-matched pairs score a median 1.000 and true leftovers 0.18–0.38:
 * three populations, three separated bands.
 *
 * Deliberately built from SHAPE and LITERALS only. Measured with and without
 * identifier-derived tokens (property keys, member names) and the result was
 * identical — 0.857 vs 0.858 median, 69 vs 70 above the floor — so names buy
 * nothing here and are left out, keeping identity free of anything the naming
 * stage produces.
 */
describe("matchFossilModules — graded similarity (exp078)", () => {
  /** A signature carrying graded shape tokens as well as exact hashes. */
  function graded(hashes: string[], tokens: string[], imports: number[] = []) {
    return { hashes: [...hashes].sort(), imports, tokens };
  }

  it("matches a heavily-rewritten enclosure that shares most of its shape", () => {
    // Exact overlap 1/5 = 0.20 — below every content floor — but the token
    // sets agree almost entirely, which is the real state of the 74.
    const shared = Array.from({ length: 20 }, (_, i) => `tok${i}`);
    const prior = [graded(["keep", "a", "b"], [...shared, "old1", "old2"])];
    const fresh = [graded(["keep", "c", "d"], [...shared, "new1", "new2"])];
    const bare = matchFossilModules(
      prior.map(({ hashes, imports }) => ({ hashes, imports })),
      fresh.map(({ hashes, imports }) => ({ hashes, imports }))
    );
    assert.strictEqual(bare.matches.size, 0, "exact tiers must abstain");

    const { matches, tiers } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.get(0), 0);
    assert.strictEqual(tiers["graded-content"], 1);
  });

  it("abstains when the shapes genuinely differ", () => {
    // The create-env-proxy case: same family, same skeleton, different body.
    // Sharing one trivial statement must not be enough.
    // Hashes must DIFFER or tier A pairs them on the unique signature before
    // anything graded is consulted — they share only the boilerplate one,
    // which is exactly the 0.14 the real create-env-proxy pair scores.
    const prior = [
      graded(["boiler", "p1"], ["a1", "a2", "a3", "a4", "a5", "a6"])
    ];
    const fresh = [
      graded(["boiler", "f1"], ["b1", "b2", "b3", "b4", "b5", "b6"])
    ];
    const { matches } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.size, 0);
  });

  it("abstains when two candidates are equally similar", () => {
    const shared = Array.from({ length: 20 }, (_, i) => `tok${i}`);
    const prior = [
      graded(["p1"], [...shared, "x"]),
      graded(["p2"], [...shared, "y"])
    ];
    const fresh = [graded(["f1"], [...shared, "z"])];
    const { matches } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.size, 0, "a tie must mint fresh, not guess");
  });

  it("callers without tokens keep exactly the old behaviour", () => {
    const prior = [{ hashes: ["a", "b"], imports: [] }];
    const fresh = [{ hashes: ["a", "c"], imports: [] }];
    const { matches } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.size, 0);
  });
});

/**
 * Tier D — GRAPH POSITION carries identity when content cannot (exp078).
 *
 * Andrew's framing: an enclosure is "the thing these 12 files import and
 * which imports these 3". Its body is what CHANGES between releases; its
 * position is what persists. Every tier above demands content overlap ≥ 0.5
 * BEFORE looking at edges, so an enclosure that held its graph position and
 * rewrote half its body was never even a candidate — it minted a fresh
 * identity and its file appeared in git as a delete plus an add.
 *
 * exp078 Task 0 measured this on a real walk (2.1.215→2.1.216): of 115
 * unmatched enclosures, 98 EXISTED in the prior release, and ALL 74
 * unambiguous ones sat below 0.5 overlap — median 0.30. No content threshold
 * could reach them. 81.9% of the added-file mass was recoverable in
 * principle, worth ~17,781 add+delete lines on one release.
 */
describe("matchFossilModules — tier D: graph position (exp078)", () => {
  it("pairs a rewritten enclosure that kept its neighbours", () => {
    // `target` rewrote its whole body (zero overlap) but is still imported
    // by the same two uniquely-identified modules. Nothing else can claim it.
    const prior = [
      mod(["anchorA"]),
      mod(["anchorB"]),
      mod(["old1", "old2", "old3"])
    ];
    const fresh = [
      mod(["anchorA"]),
      mod(["anchorB"]),
      mod(["new1", "new2", "new3"])
    ];
    // both anchors import the target on both sides
    prior[0].imports = [2];
    prior[1].imports = [2];
    fresh[0].imports = [2];
    fresh[1].imports = [2];
    const { matches, tiers } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.get(2), 2, "the rewritten enclosure holds");
    assert.strictEqual(tiers["graph-position"], 1);
  });

  it("abstains when two candidates sit in the same position", () => {
    // Task 0 found 24 such cases at overlap 0.00. Pairing one arbitrarily
    // carries a name onto unrelated code, which is worse than minting a
    // fresh one — the same reason tier A refuses silent-edged twins.
    const prior = [mod(["anchor"]), mod(["oldX"], []), mod(["oldY"], [])];
    const fresh = [mod(["anchor"]), mod(["newX"], []), mod(["newY"], [])];
    prior[0].imports = [1, 2];
    fresh[0].imports = [1, 2];
    const { matches } = matchFossilModules(prior, fresh);
    // The anchor matches by signature; both leftovers have identical edge
    // evidence, so neither may be paired.
    assert.strictEqual(matches.get(0), 0);
    assert.strictEqual(matches.size, 1);
  });

  it("abstains with no edge evidence at all", () => {
    // A rewritten enclosure nobody imports and which imports nothing is
    // indistinguishable from a genuinely new file, and a genuinely new file
    // SHOULD get a new name.
    const prior = [mod(["old1", "old2"])];
    const fresh = [mod(["new1", "new2"])];
    const { matches } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.size, 0);
  });

  it("requires mutual best, so one prior cannot claim two fresh", () => {
    // Two fresh enclosures both sit under the same anchor; the prior has one.
    // Whichever is 'best' for the prior, the prior must also be uniquely best
    // for it — otherwise the loser silently keeps a fresh mint while the
    // winner is arbitrary.
    const prior = [mod(["anchor"]), mod(["old"], [])];
    const fresh = [mod(["anchor"]), mod(["newA"], []), mod(["newB"], [])];
    prior[0].imports = [1];
    fresh[0].imports = [1, 2];
    const { matches } = matchFossilModules(prior, fresh);
    assert.strictEqual(matches.get(0), 0, "anchor still matches");
    assert.strictEqual(matches.size, 1, "the tied leftovers stay unmatched");
  });
});

/**
 * exp080 — a module that GREW substantially has weak statement overlap however
 * certain its identity is. On 2.1.215->216 `pr-review-artifact-template.js`
 * went 653 -> 1,006 lines, went unmatched, and so lost its filename to a
 * newcomer and moved to `-2`: 916 git lines, the largest single cross-file move
 * in the tree. Its export set overlapped the prior module 100% and the new
 * occupant 0%.
 */
describe("export-set tier (exp080)", () => {
  it("matches a module that grew, and does not hand its identity to a newcomer", () => {
    const prior = [
      {
        hashes: ["a", "b", "c"],
        imports: [],
        declared: ["loadTemplateModule", "skillRegistryRef"]
      }
    ];
    const fresh = [
      // The NEWCOMER: sits first, shares nothing but is otherwise plausible.
      {
        hashes: ["x", "y", "z"],
        imports: [],
        declared: ["loadWorkshopTemplateModule", "workshopTemplates"]
      },
      // The SAME module, grown — statement overlap now 3 of 8.
      {
        hashes: ["a", "b", "c", "d", "e", "f", "g", "h"],
        imports: [],
        declared: ["loadTemplateModule", "skillRegistryRef"]
      }
    ];

    const { matches, tiers } = matchFossilModules(prior, fresh);
    assert.strictEqual(
      matches.get(1),
      0,
      "the grown module must keep its prior identity"
    );
    assert.ok(
      !matches.has(0),
      "the newcomer must not inherit the prior module's identity"
    );
    assert.ok((tiers["export-set"] ?? 0) > 0, "export-set tier should fire");
  });

  it("stays silent when export names churned", () => {
    // Names not inherited => low overlap => the tier must not guess.
    const prior = [{ hashes: ["a"], imports: [], declared: ["alpha", "beta"] }];
    const fresh = [
      { hashes: ["q"], imports: [], declared: ["gamma", "delta"] }
    ];
    const { tiers } = matchFossilModules(prior, fresh);
    assert.strictEqual(tiers["export-set"] ?? 0, 0);
  });
});

describe("export-heir veto (exp082)", () => {
  // The 2.1.215→216 chain: prior `handle-post-tool-use-hook` had 2
  // statements; a 1-statement NEIGHBOR shared one boilerplate hash
  // (overlap exactly 0.5) plus an agreeing import edge, so the edge tier
  // gave it the prior module's identity — and its FILENAME. The real
  // heir (content fully changed, but export names inherited from the
  // function matcher) then minted `handle-post-tool-use-hook-2.js`, and
  // the neighbor's own prior cascaded the same theft one module further:
  // three files misfiled per chain, 298 git lines of moves on the hop.
  const anchorP = { hashes: ["anchor"], imports: [] };
  const anchorF = { hashes: ["anchor"], imports: [] };

  it("edge tier must not take a contradicted module whose export heir is present", () => {
    const prior = [
      anchorP,
      {
        hashes: ["old2", "shared"],
        imports: [0],
        declared: ["handlePostToolUseHook", "utilityModuleRef"]
      }
    ];
    const fresh = [
      anchorF,
      // the neighbor: one statement, one shared hash (overlap 0.5),
      // agreeing edge, but export names flatly contradict
      {
        hashes: ["shared"],
        imports: [0],
        declared: ["initializePluginsAndUtilities"]
      },
      // the heir: content fully changed, export names inherited
      {
        hashes: ["new1", "new2"],
        imports: [0],
        declared: ["handlePostToolUseHook", "utilityModuleRef"]
      }
    ];
    const { matches, tiers } = matchFossilModules(prior, fresh);
    assert.strictEqual(
      matches.get(2),
      1,
      "the heir must keep the prior module's identity"
    );
    assert.ok(
      !matches.has(1),
      "the neighbor must not steal a module whose heir is on the table"
    );
    assert.ok((tiers["export-set"] ?? 0) > 0);
  });

  it("without an heir, the same edge match still stands", () => {
    const prior = [
      anchorP,
      {
        hashes: ["old2", "shared"],
        imports: [0],
        declared: ["handlePostToolUseHook", "utilityModuleRef"]
      }
    ];
    const fresh = [
      anchorF,
      {
        hashes: ["shared"],
        imports: [0],
        declared: ["initializePluginsAndUtilities"]
      }
    ];
    const { matches } = matchFossilModules(prior, fresh);
    assert.strictEqual(
      matches.get(1),
      1,
      "no heir in sight: the edge evidence must still be honored"
    );
  });
});
