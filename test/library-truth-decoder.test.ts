// exp107: the library-matching score's source-map decoder (experiments/096-library-matching-score/decoder.py).
// A source map's generated column counts UTF-16 code units (source map v3), and a one-field segment says "this
// generated code maps to nothing". Scorer 4's decoder read the column as a code-point index (every astral
// character, an emoji in a string, pushed every later mapping on its line one byte further right: 2.09M of one
// webpack bundle's 2.44M segments, crediting @babel/code-frame's bytes to the app's entry) and dropped one-field
// segments (a bundler's own glue was credited to whatever mapping preceded it).
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

// generated: a("😀");b();c()   (😀 = 2 UTF-16 units, 4 UTF-8 bytes)
// segments:  col 0 -> x.js, col 8 (`b`) -> y.js, col 11 (`;`) -> nothing, col 12 (`c`) -> x.js
const FIXTURE = {
  text: 'a("\u{1F600}");b();c()',
  map: {
    version: 3,
    sources: ["x.js", "y.js"],
    mappings: "AAAA,QCAA,G,CDAA"
  }
};

function decode(rules: string): unknown {
  const py = [
    "import json, sys",
    "sys.path.insert(0, sys.argv[3])",
    "import decoder",
    "a = json.loads(sys.argv[1])",
    "u16, gen = sys.argv[2].split(',')",
    "segs = decoder.segments(a['text'], a['map'], utf16=u16 == '1', unmapped=gen == '1')",
    "print(json.dumps([[s[0], s[1]] for s in segs]))"
  ].join("\n");
  return JSON.parse(
    execFileSync(
      "python3",
      ["-I", "-c", py, JSON.stringify(FIXTURE), rules, DIR],
      { encoding: "utf8" }
    )
  );
}

describe("library truth decoder", () => {
  it("reads generated columns as UTF-16 code units and keeps one-field segments as unmapped", () => {
    assert.deepStrictEqual(decode("1,1"), [
      [0, 0],
      [10, 1],
      [13, null],
      [14, 0]
    ]);
  });

  it("with both rules off reproduces scorer 4's decoder (code points, one-field segments dropped)", () => {
    assert.deepStrictEqual(decode("0,0"), [
      [0, 0],
      [11, 1],
      [15, 0]
    ]);
  });
});
