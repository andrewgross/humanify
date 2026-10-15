// exp112: scorer 7's two origin-layer rules (experiments/096-library-matching-score/origins.py, RULES7).
// The test split's wrong-family bytes were mostly RIGHT claims the truth credited to the shipper: (1) rolldown
// emits a lazily initialised module as its `//#region` (hoisted declarations) followed, after `//#endregion`, by
// the module's body `__esmMin(() => {...})()`, and the origin layer ended the copy at `//#endregion`; (2) a file
// a package built from another package is named after it (`dist/jiti-Bb1dT6Cw.mjs`) and the package declares that
// one as a build input only.
import { describe, it } from "node:test";
import assert from "node:assert";
import { execFileSync } from "node:child_process";
import path from "node:path";

const DIR = path.join(
  import.meta.dirname,
  "..",
  "experiments",
  "096-library-matching-score"
);

const LAZY_FILE = [
  'import { n as __esmMin } from "./rolldown-runtime.js";', // 0
  "//#region ../../node_modules/.pnpm/prettier@3.9.6/node_modules/prettier/plugins/flow.mjs", // 1
  "function By(A) { return A; }", // 2
  "var DE, Mg;", // 3
  "//#endregion", // 4
  "__esmMin((() => {", // 5
  "\tDE = Object.create;", // 6
  "}))();", // 7
  "export { By as default };" // 8
].join("\n");

const NO_CHUNK = [
  "//#region ../../node_modules/x/dist/index.mjs", // 0
  "var a = 1;", // 1
  "//#endregion", // 2
  "var own = 2;" // 3
].join("\n");

// sources: 0 = the shipper's lazy-wrapped prettier copy; 1 = a region naming another package's chunk
// (owner p@1.0.0, dist/jiti-Bb1dT6Cw.mjs); 2 = the shipper's own built chunk named after a build input;
// 3 = a chunk named after a RUNTIME dependency (a wrapper, not a copy); 4 = a region, then the shipper's own code
const CASE = {
  paths: [
    "node_modules/fmt/dist/flow-CQ2VkW6e.js",
    "node_modules/fmt/dist/jiti-X.js",
    "node_modules/env-runner/dist/_chunks/libs/cjs-module-lexer.mjs",
    "node_modules/tool/dist/debug-Ab12Cd34.mjs",
    "node_modules/fmt/dist/other.js"
  ],
  contents: [
    LAZY_FILE,
    "//#region ../../node_modules/.pnpm/p@1.0.0_q@2/node_modules/p/dist/jiti-Bb1dT6Cw.mjs\nvar require_jiti = 1;\n//#endregion\nexport { require_jiti as t };",
    "var lexer = 1;",
    "var d = 1;",
    NO_CHUNK
  ],
  shippers: ["fmt", "fmt", "env-runner", "tool", "fmt"],
  versions: ["0.63.0", "0.63.0", "0.1.0", "1.0.0", "0.63.0"],
  inputs: {
    "p@1.0.0": [[], ["jiti"]],
    "env-runner@0.1.0": [["srvx"], ["cjs-module-lexer"]],
    "tool@1.0.0": [["debug"], ["debug"]]
  },
  // [source, original line] probes
  at: [
    [0, 2],
    [0, 6],
    [0, 8],
    [1, 1],
    [2, 0],
    [3, 0],
    [4, 1],
    [4, 3]
  ]
};

function probe(rules: Record<string, boolean> | null): unknown {
  const py = [
    "import json, sys",
    "sys.path.insert(0, sys.argv[1])",
    "import origins as O",
    "c = json.loads(sys.argv[2])",
    "rules = json.loads(sys.argv[3])",
    "def inputs(p, v):",
    "    r = c['inputs'].get(f'{p}@{v}')",
    "    return (set(r[0]), set(r[1])) if r else (set(), set())",
    "o = O.Origins(c['paths'], c['contents'], c['shippers'], rules=rules, versions=c['versions'],",
    "              inputs=inputs, same_family=lambda a, b: a == b)",
    "print(json.dumps([o.at(si, ln) for si, ln in c['at']]))"
  ].join("\n");
  return JSON.parse(
    execFileSync(
      "python3",
      ["-I", "-c", py, DIR, JSON.stringify(CASE), JSON.stringify(rules)],
      { encoding: "utf8" }
    )
  );
}

describe("library truth origin layer, scorer 7", () => {
  it("credits a rolldown lazy module body after `//#endregion` to the region's package (lazybody)", () => {
    const got = probe({ lazybody: true, chunk: false }) as unknown[];
    assert.deepStrictEqual(got.slice(0, 3), [
      ["marker", "prettier"],
      ["lazybody", "prettier"],
      null
    ]);
  });

  it("names a chunk after a build input of its owner, never after a runtime dependency (chunk)", () => {
    const got = probe({ lazybody: true, chunk: true }) as unknown[];
    assert.deepStrictEqual(got.slice(3, 6), [
      ["chunk", "jiti"],
      ["chunk", "cjs-module-lexer"],
      null
    ]);
  });

  it("keeps a region's end: code after `//#endregion` with no lazy body is the shipper's own", () => {
    const got = probe({ lazybody: true, chunk: true }) as unknown[];
    assert.deepStrictEqual(got.slice(6), [["marker", "x"], null]);
  });

  it("with every rule off reproduces scorer 4-6's layer (the copy ends at `//#endregion`, no chunk names)", () => {
    assert.deepStrictEqual(probe(null), [
      ["marker", "prettier"],
      null,
      null,
      ["marker", "p"],
      null,
      null,
      ["marker", "x"],
      null
    ]);
  });
});
