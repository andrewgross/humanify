// WP5.1 content-anchor probe: contentAnchorVerdicts' exact output over
// the TS spec's shapes (src/split/content-anchor.test.ts) plus a seeded
// sweep over a small vocabulary of rare literals and words — including
// the regex edges the scanners must reproduce (escapes, newlines inside a
// quote, astral chars counted as two UTF-16 units, mixed quotes).
// Output: test/parity/wp51-anchor.json.
//
//   npx tsx test/parity/wp51-anchor-probe.ts > test/parity/wp51-anchor.json
import {
  changedLineFraction,
  contentAnchorVerdicts,
  type PriorStatement
} from "../../src/split/content-anchor.js";

const body = (marker: string) =>
  [
    "function handler(request, options) {",
    `  const parsed = parseRequest(request, "${marker} marker literal");`,
    "  const result = compute(parsed, options);",
    "  return result;",
    "}"
  ].join("\n");

const cases: Array<{ prior: PriorStatement[]; fresh: string[] }> = [
  {
    prior: [
      { text: body("Alpha"), file: "src/a/alpha.js" },
      { text: body("Beta"), file: "src/b/beta.js" }
    ],
    fresh: [body("Beta"), body("Alpha")]
  },
  {
    prior: [
      { text: body("Alpha"), file: "src/a/alpha.js" },
      { text: body("Alpha"), file: "src/b/beta.js" }
    ],
    fresh: [body("Alpha")]
  },
  {
    prior: [{ text: body("Alpha"), file: "src/a/alpha.js" }],
    fresh: [body("Alpha"), body("Alpha")]
  },
  {
    prior: [
      {
        text: 'function f(x) {\n  return g(x, "a distinctive quote \\" escaped here");\n}',
        file: "src/esc.js"
      },
      {
        text: "var s = 'a single quoted rare one' + \"😀😀😀😀😀😀\";",
        file: "src/astral.js"
      },
      { text: 'var t = "five😀😀😀😀 units";', file: "src/five.js" },
      { text: 'var u = "line one\nline two is long";', file: "src/nl.js" }
    ],
    fresh: [
      'function f(y) {\n  return g(y, "a distinctive quote \\" escaped here");\n}',
      "var s = 'a single quoted rare one' + \"😀😀😀😀😀😀\";",
      'var t = "five😀😀😀😀 units";',
      'var u = "line one\nline two is long";'
    ]
  }
];

// Seeded sweep.
let seed = 0xa11c;
const rnd = (n: number): number => {
  seed = (seed * 1103515245 + 12345) & 0x7fffffff;
  return seed % n;
};
const LITS = Array.from({ length: 10 }, (_, i) => `rare literal number ${i}`);
const WORDS = [
  "alpha",
  "beta",
  "gamma",
  "delta",
  "epsilon",
  "zeta",
  "eta",
  "theta"
];
function stmt(): string {
  const lines: string[] = [];
  const n = 1 + rnd(5);
  for (let k = 0; k < n; k++) {
    const w = WORDS[rnd(WORDS.length)];
    const lit = rnd(3) === 0 ? `, "${LITS[rnd(LITS.length)]}"` : "";
    lines.push(`  ${w}(${WORDS[rnd(WORDS.length)]}${lit});`);
  }
  return `function ${WORDS[rnd(WORDS.length)]}() {\n${lines.join("\n")}\n}`;
}
for (let c = 0; c < 300; c++) {
  const prior = Array.from({ length: 1 + rnd(6) }, (_, i) => ({
    text: stmt(),
    file: `src/f${i % 3}.js`
  }));
  const fresh = Array.from({ length: 1 + rnd(6) }, () =>
    rnd(2) ? prior[rnd(prior.length)].text.replace("alpha", "alphaX") : stmt()
  );
  cases.push({ prior, fresh });
}

const rows = cases.map(({ prior, fresh }) => ({
  prior,
  fresh,
  verdicts: [...contentAnchorVerdicts(prior, fresh)]
    .map(([i, v]) => [i, v.file, v.nearIdentical])
    .sort((a, b) => (a[0] as number) - (b[0] as number)),
  changed: fresh.map((f) => changedLineFraction(f, prior[0].text))
}));
process.stdout.write(`${JSON.stringify(rows)}\n`);
