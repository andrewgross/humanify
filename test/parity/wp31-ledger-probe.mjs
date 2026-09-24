// WP3.1 probe: the rename ledger (src/rename/rename-ledger.ts) on the
// rename-ledger.test.ts cases. For each case: the source snapshot, the
// renames, the TS ledger, and the GENERATED output (Babel's generator —
// what `applyRenameLedger` must reproduce). A staged case chains a second
// rename stage over the first stage's generated output.
//
// The Rust port applies the same renames through the validated overlay,
// derives its own ledger, and must (a) produce the TS ledger's entries and
// (b) replay to the TS generated output.
//
// Run: npx tsx test/parity/wp31-ledger-probe.mjs > test/parity/wp31-ledger.json
const { generate, parseFileAst, traverse } = await import(
  "../../src/babel-utils.js"
);
const { buildRenameLedger } = await import("../../src/rename/rename-ledger.js");

function renameAll(source, renames) {
  const ast = parseFileAst(source);
  const want = new Map(renames);
  const pending = [];
  traverse(ast, {
    enter(path) {
      if (path.scope.path !== path) return;
      for (const name of Object.keys(path.scope.bindings)) {
        const to = want.get(name);
        if (to) pending.push([path.scope, name, to]);
      }
    }
  });
  for (const [scope, from, to] of pending) scope.rename(from, to);
  return ast;
}

const canonical = (source) => generate(parseFileAst(source)).code;

function stage(source, renames) {
  const ast = renameAll(source, renames);
  return {
    source,
    // The replay ⇔ generate invariant holds on a generate FIXED POINT (the
    // beautified text the pipeline renames); the non-canonical fixtures
    // only pin the entries.
    fixedPoint: canonical(source) === source,
    renames,
    ledger: buildRenameLedger(source, ast),
    output: generate(ast).code
  };
}

const CASES = [
  {
    label: "records one entry per renamed binding with original + final names",
    stages: [
      [
        "function a(b) {\n  return b + 1;\n}\nvar c = a(2);\n",
        [
          ["a", "addOne"],
          ["b", "value"],
          ["c", "result"]
        ]
      ]
    ]
  },
  {
    label:
      "captures every occurrence (declaration + reads + writes) of a binding",
    stages: [
      ["var x = 1;\nx = x + 1;\nx++;\nconsole.log(x);\n", [["x", "counter"]]]
    ]
  },
  {
    label: "does not record bindings whose name is unchanged",
    stages: [["function keep(a) {\n  return a;\n}\n", [["a", "value"]]]]
  },
  {
    label: "pins the source hash so a mismatched snapshot is detectable",
    stages: [["var q = 1;\n", [["q", "quantity"]]]]
  },
  {
    label:
      "replays the ledger onto the source to reproduce the renamed output exactly",
    stages: [
      [
        canonical(
          "function a(b, c) {\n  var d = b + c;\n  return d * 2;\n}\n" +
            "var e = a(1, 2);\ne = e + a(3, 4);\nconsole.log(e);\n"
        ),
        [
          ["a", "sumDoubled"],
          ["b", "first"],
          ["c", "second"],
          ["d", "total"],
          ["e", "acc"]
        ]
      ]
    ]
  },
  {
    label: "chains an output-space stage to reproduce the final output",
    stages: [
      [
        canonical("function a(b) {\n  return b + 1;\n}\nvar c = a(2);\n"),
        [
          ["a", "addOne"],
          ["b", "value"],
          ["c", "result"]
        ]
      ],
      [
        null,
        [
          ["addOne", "increment"],
          ["result", "total"]
        ]
      ]
    ]
  },
  {
    label: "x: destructuring writes, a redeclaration and a for-of head",
    stages: [
      [
        canonical(
          "var a = 1, o = {};\n[a] = [2];\n({ k: a } = o);\nvar a = 3;\nfor (var a of [4]) {}\nfor (a in o) {}\nconsole.log(a);\n"
        ),
        [["a", "value"]]
      ]
    ]
  }
];

const out = CASES.map(({ label, stages }) => {
  const done = [];
  let prev = null;
  for (const [source, renames] of stages) {
    const s = stage(source ?? prev.output, renames);
    done.push(s);
    prev = s;
  }
  return { label, stages: done };
});
console.log(JSON.stringify({ cases: out }, null, 1));
