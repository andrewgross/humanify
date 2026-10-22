// exp137: scorer 14's rule `mapdefect` (experiments/137-scorer-mapdefect/scorer14/mapdefect14.py, applied by
// truth_audit.Audit.mapdefect in score.label_units). Bun 1.4's source maps credit code to a package whose
// installed files do not hold it; the rule relabels a unit only when the labelled package's installed closure
// (its dependencies included) lacks the unit's name evidence AND exactly one other installed package's file holds
// it. The rule never reads a claim: it moves a claim right -> wrong as readily as wrong -> right, and relabels an
// unclaimed unit (recall's truth) the same way.
import { describe, it } from "node:test";
import assert from "node:assert";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const ROOT = path.join(import.meta.dirname, "..");
const SCORE = path.join(ROOT, "experiments", "096-library-matching-score");
const RULE = path.join(ROOT, "experiments", "137-scorer-mapdefect", "scorer14");

// label_units and the rule read no numpy / scipy; the misses lane's modules they sit beside do at import
const PRELUDE = [
  "import json, sys, types",
  "for n in ('mmlib', 'numpy', 'scipy', 'scipy.sparse'):",
  "    sys.modules[n] = types.ModuleType(n)",
  "sys.path.insert(0, sys.argv[1])",
  "sys.path.insert(0, sys.argv[2])",
  "arg = json.loads(sys.argv[3])"
];

function py(body: string[], arg: unknown): unknown {
  return JSON.parse(
    execFileSync(
      "python3",
      [
        "-I",
        "-c",
        [...PRELUDE, ...body].join("\n"),
        SCORE,
        RULE,
        JSON.stringify(arg)
      ],
      { encoding: "utf8" }
    )
  );
}

const TOKS = Array.from({ length: 10 }, (_, i) => `evidenceToken${i}`);

function decide(c: {
  label: string;
  evidence: string[];
  closure: number;
  holders: [string, number, number, string][];
  app?: string;
}): unknown {
  return py(
    [
      "import mapdefect14 as M",
      "import families_lib",
      "f = families_lib.Families()",
      "hs = [(h[0], h[1], h[2], h[3], '/r/' + h[0]) for h in arg['holders']]",
      "r, best = M.decide(arg['label'], arg['evidence'], arg['closure'], hs, f.same, arg.get('app'))",
      "print(json.dumps([r, best[0] if best else None, best[3] if best else None]))"
    ],
    c
  );
}

describe("scorer 14 rule mapdefect: the verdict", () => {
  it("relabels when the label's closure lacks the evidence and one package's file holds it", () => {
    assert.deepStrictEqual(
      decide({
        label: "alphaqz",
        evidence: TOKS,
        closure: 0.1,
        holders: [["betaqz", 0.9, 4000, "/r/betaqz/index.js"]]
      }),
      ["relabel", "betaqz", "/r/betaqz/index.js"]
    );
  });

  it("keeps the label when its closure holds LACK (20%) or more of the evidence", () => {
    assert.deepStrictEqual(
      decide({
        label: "alphaqz",
        evidence: TOKS,
        closure: 0.2,
        holders: [["betaqz", 1, 4000, "/r/betaqz/index.js"]]
      }),
      ["label-holds", null, null]
    );
  });

  it("keeps the label with fewer than eight distinctive tokens", () => {
    assert.deepStrictEqual(
      decide({
        label: "alphaqz",
        evidence: TOKS.slice(0, 7),
        closure: 0,
        holders: [["betaqz", 1, 4000, "/r/betaqz/index.js"]]
      }),
      ["few-evidence", null, null]
    );
  });

  it("keeps the label when two families hold the code (a copy or a carrier: no single owner)", () => {
    assert.deepStrictEqual(
      decide({
        label: "alphaqz",
        evidence: TOKS,
        closure: 0,
        holders: [
          ["betaqz", 0.9, 4000, "/r/betaqz/index.js"],
          ["gammaqz", 0.85, 90000, "/r/gammaqz/dist/all.js"]
        ]
      }),
      ["ambiguous", null, null]
    );
  });

  it("keeps the label when another install of the label's own family holds it", () => {
    assert.deepStrictEqual(
      decide({
        label: "alphaqz",
        evidence: TOKS,
        closure: 0,
        holders: [["alphaqz", 0.9, 4000, "/r/alphaqz-2/index.js"]]
      }),
      ["label-family-elsewhere", null, null]
    );
  });

  it("keeps the label when the app's own package holds it", () => {
    assert.deepStrictEqual(
      decide({
        label: "alphaqz",
        evidence: TOKS,
        closure: 0,
        holders: [["myappqz", 0.9, 4000, "/r/myappqz/index.js"]],
        app: "myappqz"
      }),
      ["app-holds", null, null]
    );
  });

  it("takes the file holding the most when one package holds it in several files", () => {
    assert.deepStrictEqual(
      decide({
        label: "alphaqz",
        evidence: TOKS,
        closure: 0,
        holders: [
          ["betaqz", 0.8, 400, "/r/betaqz/a.js"],
          ["betaqz", 1, 9000, "/r/betaqz/b.js"]
        ]
      }),
      ["relabel", "betaqz", "/r/betaqz/b.js"]
    );
  });

  it("reads only distinctive tokens: at most five installed packages hold one, never one past the file cap", () => {
    assert.deepStrictEqual(
      py(
        [
          "import mapdefect14 as M",
          "print(json.dumps(M.distinctive(['aaaa', 'bbbb', 'cccc', 'dddd'], {'aaaa': 5, 'bbbb': 6, 'dddd': 1}, {'dddd'})))"
        ],
        null
      ),
      ["aaaa", "cccc"]
    );
  });
});

function write(p: string, body: unknown): void {
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, typeof body === "string" ? body : JSON.stringify(body));
}

describe("scorer 14 rule mapdefect: the label's installed closure", () => {
  it("resolves dependencies as node does: nested, hoisted, pnpm siblings, transitively; not unrelated packages", () => {
    const app = fs.mkdtempSync(path.join(os.tmpdir(), "md14-"));
    const nm = path.join(app, "node_modules");
    write(path.join(nm, "t", "package.json"), {
      name: "t",
      dependencies: { hoisted: "1", nested: "1" },
      peerDependencies: { linked: "1" }
    });
    write(path.join(nm, "t", "node_modules", "nested", "package.json"), {
      name: "nested"
    });
    write(path.join(nm, "hoisted", "package.json"), {
      name: "hoisted",
      optionalDependencies: { deep: "1" }
    });
    write(path.join(nm, "deep", "package.json"), { name: "deep" });
    write(path.join(nm, "unrelated", "package.json"), { name: "unrelated" });
    // pnpm: node_modules/linked -> .pnpm/linked@1/node_modules/linked, its dependency a sibling there
    const store = path.join(nm, ".pnpm", "linked@1", "node_modules");
    write(path.join(store, "linked", "package.json"), {
      name: "linked",
      dependencies: { sib: "1" }
    });
    write(path.join(store, "sib", "package.json"), { name: "sib" });
    fs.symlinkSync(path.join(store, "linked"), path.join(nm, "linked"));
    const got = py(
      [
        "import os, mapdefect14 as M",
        "app = os.path.realpath(arg)",
        "c = M.closure(os.path.join(app, 'node_modules', 't'), app)",
        "print(json.dumps(sorted(os.path.relpath(r, app) for r in c)))"
      ],
      app
    );
    fs.rmSync(app, { recursive: true });
    assert.deepStrictEqual(got, [
      "node_modules/.pnpm/linked@1/node_modules/linked",
      "node_modules/.pnpm/linked@1/node_modules/sib",
      "node_modules/deep",
      "node_modules/hoisted",
      "node_modules/t",
      "node_modules/t/node_modules/nested"
    ]);
  });
});

// two sources: 0 = alphaqz's index.js (the map's label for [0, 100)), 1 = gammaqz's a.js ([100, 150))
const LABEL_CASE = {
  truth: {
    method: "sourcemap",
    appPackage: "myappqz",
    segments: [
      [0, 0],
      [100, 1]
    ],
    sources: [
      ["alphaqz", "1.0.0", "index.js"],
      ["gammaqz", "2.0.0", "a.js"]
    ]
  },
  units: [
    { i: 0, s: 0, e: 50, b: 50, k: "Statement", rt: false, m: null },
    { i: 1, s: 50, e: 100, b: 50, k: "Statement", rt: false, m: null },
    { i: 2, s: 100, e: 150, b: 50, k: "Statement", rt: false, m: null }
  ],
  // [0, 50) is betaqz's code; the row for [100, 150) names an old label the unit no longer has (left alone)
  rows: [
    [0, 50, "alphaqz", "betaqz", "3.0.0", "lib/b.js", 12, 0, 1000, "index.js"],
    [100, 150, "otherqz", "betaqz", "3.0.0", "lib/b.js", 12, 0, 1000, "a.js"]
  ]
};

function labels(rule: boolean): unknown {
  return py(
    [
      "import score, truth_audit",
      "a = object.__new__(truth_audit.Audit)",
      "t = arg['case']['truth']",
      "a.offs = [s[0] for s in t['segments']]; a.segs = t['segments']; a.tok = {}; a.all = set(); a.text = b''",
      "a.copy_at = []; a.copies = []; a.mis13 = {}",
      "a.md14 = {(r[0], r[1]): r for r in arg['case']['rows']} if arg['rule'] else {}",
      "rows = score.label_units(t, arg['case']['units'], None, a)",
      "j = lambda c, r: score.judge_origin(c, r, t['appPackage'])[0]",
      "print(json.dumps([[rows[i]['label'], rows[i].get('mapDefectFrom'), j('betaqz', rows[i]), j('alphaqz', rows[i])]",
      "                  for i in range(3)]))"
    ],
    { case: LABEL_CASE, rule }
  );
}

describe("scorer 14 rule mapdefect: at score time (label_units)", () => {
  it("relabels the row's unit both ways: a claim naming the holder turns right, one naming the old label wrong", () => {
    // per unit: label, relabelled from, verdict of a claim naming betaqz, verdict of a claim naming alphaqz
    assert.deepStrictEqual(labels(true), [
      ["betaqz", "alphaqz", "right", "wrongFamily"],
      ["alphaqz", null, "wrongFamily", "right"],
      ["gammaqz", null, "wrongFamily", "wrongFamily"]
    ]);
  });

  it("with the rule off reproduces scorer 13's labels", () => {
    assert.deepStrictEqual(labels(false), [
      ["alphaqz", null, "wrongFamily", "right"],
      ["alphaqz", null, "wrongFamily", "right"],
      ["gammaqz", null, "wrongFamily", "wrongFamily"]
    ]);
  });
});
