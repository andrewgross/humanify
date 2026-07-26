import assert from "node:assert";
import test from "node:test";
import {
  changedLineFraction,
  contentAnchorVerdicts,
  type PriorStatement
} from "./content-anchor.js";

/** The file verdicts alone — these cases are about WHICH prior statement is
 * identified, not about how much of it changed. */
function contentAnchorFiles(
  prior: readonly PriorStatement[],
  fresh: readonly string[]
): Map<number, string> {
  const files = new Map<number, string>();
  for (const [i, v] of contentAnchorVerdicts(prior, fresh))
    files.set(i, v.file);
  return files;
}

/** A statement body long enough to carry weight in the similarity gate. */
function body(marker: string, extra = 0): string {
  const lines = [
    `function handle${marker}(request, response, options) {`,
    `  const parsed = parseRequest(request, "${marker} marker literal");`,
    "  if (!parsed) return null;",
    "  const result = compute(parsed, options);",
    "  return finalize(result, response);",
    ...Array.from({ length: extra }, (_, i) => `  filler${i}(padding${i});`),
    "}"
  ];
  return lines.join("\n");
}

test("unique rare literal + similar text inherits the prior file", () => {
  const prior: PriorStatement[] = [
    { text: body("Alpha"), file: "src/a/alpha.js" },
    { text: body("Beta"), file: "src/b/beta.js" }
  ];
  const fresh = [body("Beta"), body("Alpha")];
  const got = contentAnchorFiles(prior, fresh);
  assert.strictEqual(got.get(0), "src/b/beta.js");
  assert.strictEqual(got.get(1), "src/a/alpha.js");
});

test("a literal carried by two PRIOR statements yields no verdict", () => {
  const prior: PriorStatement[] = [
    { text: body("Alpha"), file: "src/a/alpha.js" },
    { text: body("Alpha"), file: "src/b/beta.js" }
  ];
  const got = contentAnchorFiles(prior, [body("Alpha")]);
  assert.strictEqual(got.size, 0);
});

test("a literal carried by two FRESH statements yields no verdict", () => {
  const prior: PriorStatement[] = [
    { text: body("Alpha"), file: "src/a/alpha.js" }
  ];
  const got = contentAnchorFiles(prior, [body("Alpha"), body("Alpha")]);
  assert.strictEqual(got.size, 0);
});

test("a shared literal is NOT enough — a dissimilar statement abstains", () => {
  // The regression this gate exists for: one shared string once paired a
  // 5,073-line statement with a 7-line one and charged 5,080 lines for it.
  const huge = [
    'var big = lazyInitializer(() => { const marker = "Alpha marker literal";',
    ...Array.from({ length: 400 }, (_, i) => `  unrelated${i}(thing${i});`),
    "});"
  ].join("\n");
  const prior: PriorStatement[] = [{ text: huge, file: "src/a/alpha.js" }];
  const got = contentAnchorFiles(prior, [body("Alpha")]);
  assert.strictEqual(got.size, 0);
});

test("a statement with no rare literal yields no verdict", () => {
  const short = 'function f(a) { return g(a, "tiny"); }';
  const prior: PriorStatement[] = [{ text: short, file: "src/a/alpha.js" }];
  const got = contentAnchorFiles(prior, [short]);
  assert.strictEqual(got.size, 0);
});

test("two fresh statements claiming one prior statement both abstain", () => {
  // Distinct rare literals of the SAME prior statement: each fresh statement
  // sees a unique match, but they cannot both be it.
  const priorText = [
    "function render(node, options) {",
    '  const a = label("first rare marker string");',
    '  const b = label("second rare marker string");',
    "  return join(a, b, node, options);",
    "}"
  ].join("\n");
  const freshA = priorText.replace('label("second rare marker string")', "x()");
  const freshB = priorText.replace('label("first rare marker string")', "y()");
  const got = contentAnchorFiles(
    [{ text: priorText, file: "src/a.js" }],
    [freshA, freshB]
  );
  assert.strictEqual(got.size, 0);
});

test("verdicts do not depend on input order", () => {
  const prior: PriorStatement[] = [
    { text: body("Alpha"), file: "src/a/alpha.js" },
    { text: body("Beta"), file: "src/b/beta.js" },
    { text: body("Gamma"), file: "src/c/gamma.js" }
  ];
  const fresh = [body("Gamma"), body("Alpha"), body("Beta")];
  const forward = contentAnchorFiles(prior, fresh);
  const reversed = contentAnchorFiles([...prior].reverse(), fresh);
  assert.deepStrictEqual([...forward.entries()].sort(), [
    [0, "src/c/gamma.js"],
    [1, "src/a/alpha.js"],
    [2, "src/b/beta.js"]
  ]);
  assert.deepStrictEqual(
    [...forward.entries()].sort(),
    [...reversed.entries()].sort()
  );
});

test("empty sides produce no verdicts", () => {
  assert.strictEqual(contentAnchorFiles([], [body("Alpha")]).size, 0);
  assert.strictEqual(
    contentAnchorFiles([{ text: body("Alpha"), file: "a.js" }], []).size,
    0
  );
});

test("single-quoted literals are indexed too", () => {
  const prior: PriorStatement[] = [
    {
      text: "function f(x) {\n  return g(x, 'a distinctive single quoted');\n}",
      file: "src/a.js"
    }
  ];
  const fresh = [
    "function f(x, y) {\n  return g(x, 'a distinctive single quoted');\n}"
  ];
  assert.strictEqual(contentAnchorFiles(prior, fresh).get(0), "src/a.js");
});

test("changedLineFraction measures how much of the FRESH statement changed", () => {
  const prior = ["alpha();", "beta();", "gamma();", "delta();"].join("\n");
  const renamedOnly = ["alpha();", "beta();", "gamma();", "delta();"].join(
    "\n"
  );
  assert.strictEqual(changedLineFraction(renamedOnly, prior), 0);

  // One line of four differs -> 25%, whichever line it is.
  const oneEdited = ["alpha();", "beta();", "gammaRenamed();", "delta();"].join(
    "\n"
  );
  assert.strictEqual(changedLineFraction(oneEdited, prior), 0.25);

  // Denominator is the FRESH statement: a rewrite that doubles the statement
  // is mostly-new even though every prior line survived.
  const doubled = [
    "alpha();",
    "beta();",
    "gamma();",
    "delta();",
    "epsilon();",
    "zeta();",
    "eta();",
    "theta();"
  ].join("\n");
  assert.strictEqual(changedLineFraction(doubled, prior), 0.5);
});

test("changedLineFraction is insensitive to line ORDER, like git's own diff is not", () => {
  // Deliberate and documented: the measure asks whether the CONTENT is the same
  // code, not whether it is arranged the same way. Reordering is a separate
  // noise axis with its own lever (exp038's dependency-aware emit order).
  const prior = ["alpha();", "beta();", "gamma();"].join("\n");
  const shuffled = ["gamma();", "alpha();", "beta();"].join("\n");
  assert.strictEqual(changedLineFraction(shuffled, prior), 0);
});
