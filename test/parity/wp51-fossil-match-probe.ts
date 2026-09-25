// WP5.2 fossil-match probe: matchFossilModules' exact output — matches in
// RECORD order (the Map's iteration order, which assignFossil's `used`
// set inherits), per-tier counts in first-recorded order, and each pair's
// tier — over the TS spec's cases (src/split/fossil-match.test.ts) plus a
// seeded random sweep that mixes every field (hashes, edges, stems,
// tokens, declared). Output: test/parity/wp51-fossil-match.json.
//
//   npx tsx test/parity/wp51-fossil-match-probe.ts > test/parity/wp51-fossil-match.json
import {
  type FossilSignature,
  matchFossilModules
} from "../../src/split/fossil-match.js";

const mod = (hashes: string[], imports: number[] = []) => ({
  hashes: [...hashes].sort(),
  imports
});
const named = (stem: string, hashes: string[], imports: number[] = []) => ({
  ...mod(hashes, imports),
  stem
});
const graded = (hashes: string[], tokens: string[]) => ({
  ...mod(hashes),
  tokens
});
const shared = Array.from({ length: 20 }, (_, i) => `tok${i}`);

const cases: Array<[FossilSignature[], FossilSignature[]]> = [
  [
    [mod(["h1", "h2"]), mod(["twin"]), mod(["twin"])],
    [mod(["twin"]), mod(["h1", "h2"]), mod(["twin"])]
  ],
  [
    [
      mod(["anchorA"]),
      mod(["anchorB"]),
      mod(["twin"], [0]),
      mod(["twin"], [1])
    ],
    [mod(["anchorB"]), mod(["anchorA"]), mod(["twin"], [1]), mod(["twin"], [0])]
  ],
  [[mod(["a", "b", "c", "d", "e"])], [mod(["a", "b", "c", "d", "x"])]],
  [[mod(["a", "b", "c", "d", "e"])], [mod(["a", "b", "c", "d", "e", "x"])]],
  [
    [named("access-property", ["a", "b", "c", "d"])],
    [named("access-property", ["a", "b", "c"])]
  ],
  [
    [named("noop-one", ["t"]), named("noop-two", ["t"])],
    [named("noop-two", ["t"]), named("noop-one", ["t"])]
  ],
  [
    [named("dup", ["a", "b", "c"]), named("dup", ["a", "b", "d"])],
    [named("dup", ["a", "b", "c", "e"])]
  ],
  [[named("helper", ["a", "b", "c"])], [named("helper", ["x", "y", "z"])]],
  [
    [graded(["keep", "a", "b"], [...shared, "old1", "old2"])],
    [graded(["keep", "c", "d"], [...shared, "new1", "new2"])]
  ],
  [
    [graded(["boiler", "p1"], ["a1", "a2", "a3", "a4", "a5", "a6"])],
    [graded(["boiler", "f1"], ["b1", "b2", "b3", "b4", "b5", "b6"])]
  ],
  [
    [graded(["p1"], [...shared, "x"]), graded(["p2"], [...shared, "y"])],
    [graded(["f1"], [...shared, "z"])]
  ],
  [
    [
      mod(["anchorA"], [2]),
      mod(["anchorB"], [2]),
      mod(["old1", "old2", "old3"])
    ],
    [
      mod(["anchorA"], [2]),
      mod(["anchorB"], [2]),
      mod(["new1", "new2", "new3"])
    ]
  ],
  [
    [mod(["anchor"], [1, 2]), mod(["oldX"]), mod(["oldY"])],
    [mod(["anchor"], [1, 2]), mod(["newX"]), mod(["newY"])]
  ],
  [
    [mod(["anchor"], [1]), mod(["old"])],
    [mod(["anchor"], [1, 2]), mod(["newA"]), mod(["newB"])]
  ],
  [
    [
      {
        hashes: ["a", "b", "c"],
        imports: [],
        declared: ["loadTemplateModule", "skillRegistryRef"]
      }
    ],
    [
      {
        hashes: ["x", "y", "z"],
        imports: [],
        declared: ["loadWorkshopTemplateModule", "workshopTemplates"]
      },
      {
        hashes: ["a", "b", "c", "d", "e", "f", "g", "h"],
        imports: [],
        declared: ["loadTemplateModule", "skillRegistryRef"]
      }
    ]
  ],
  [
    [
      { hashes: ["anchor"], imports: [] },
      {
        hashes: ["old2", "shared"],
        imports: [0],
        declared: ["handlePostToolUseHook", "utilityModuleRef"]
      }
    ],
    [
      { hashes: ["anchor"], imports: [] },
      {
        hashes: ["shared"],
        imports: [0],
        declared: ["initializePluginsAndUtilities"]
      },
      {
        hashes: ["new1", "new2"],
        imports: [0],
        declared: ["handlePostToolUseHook", "utilityModuleRef"]
      }
    ]
  ],
  [
    [
      {
        hashes: ["a", "b"],
        imports: [],
        declared: [
          "commandRegistry",
          "createVQsComponent",
          "reactFactory",
          "initPluginRegistry"
        ]
      }
    ],
    [
      {
        hashes: ["x", "y", "z"],
        imports: [],
        declared: [
          "commandRegistry",
          "createVQsComponent",
          "initPluginRegistry",
          "getArtifactsDialog",
          "factoryComponent",
          "utilityHelper"
        ]
      }
    ]
  ]
];

// Seeded sweep: small alphabets so every tier collides and ties.
let seed = 0x5eed;
const rnd = (n: number): number => {
  seed = (seed * 1103515245 + 12345) & 0x7fffffff;
  return seed % n;
};
const pick = <T>(xs: T[], k: number): T[] =>
  Array.from({ length: k }, () => xs[rnd(xs.length)]);
const H = ["a", "b", "c", "d", "e", "f"];
const S = ["alpha", "beta", "gamma", "delta"];
const T = Array.from({ length: 12 }, (_, i) => `t${i}`);
const D = ["x", "y", "z", "w", "v"];
function randomSide(n: number): FossilSignature[] {
  return Array.from({ length: n }, () => {
    const sig: FossilSignature = {
      hashes: pick(H, 1 + rnd(4)).sort(),
      imports: pick(
        Array.from({ length: n }, (_, i) => i),
        rnd(3)
      )
    };
    if (rnd(2)) sig.stem = S[rnd(S.length)];
    if (rnd(2)) sig.tokens = [...new Set(pick(T, 2 + rnd(8)))];
    if (rnd(2)) sig.declared = pick(D, rnd(4));
    return sig;
  });
}
for (let k = 0; k < 400; k++) {
  cases.push([randomSide(1 + rnd(7)), randomSide(1 + rnd(7))]);
}
// A second sweep with a WIDE hash alphabet (the exact tiers rarely fire)
// and dense declared/token/stem fields, so the later tiers are exercised.
const WIDE = Array.from({ length: 30 }, (_, i) => `h${i}`);
function sparseSide(n: number): FossilSignature[] {
  return Array.from({ length: n }, () => ({
    hashes: pick(WIDE, 1 + rnd(4)).sort(),
    imports: pick(
      Array.from({ length: n }, (_, i) => i),
      rnd(3)
    ),
    stem: S[rnd(S.length)],
    tokens: [...new Set(pick(T, 4 + rnd(8)))],
    declared: [...new Set(pick(D, 1 + rnd(3)))]
  }));
}
for (let k = 0; k < 400; k++) {
  cases.push([sparseSide(1 + rnd(8)), sparseSide(1 + rnd(8))]);
}

const rows = cases.map(([prior, fresh]) => {
  const r = matchFossilModules(prior, fresh);
  return {
    prior,
    fresh,
    matches: [...r.matches.entries()],
    tiers: Object.entries(r.tiers),
    pairTiers: [...r.pairTiers.entries()]
  };
});
process.stdout.write(`${JSON.stringify(rows)}\n`);
