/**
 * The ground-truth harness's own tests (exp092).
 *
 * Two layers:
 * 1. `canonical()` — the ground-truth predicate's normalization: wrapper
 *    spelling (`function(a,b){` ≡ `(a,b)=>{`) and identifier renaming
 *    (a consistent bijection inside the slice is erased).
 * 2. `scoreMatchDump()` — the scorer's arithmetic on a hand-written dump
 *    where every number is derivable by hand, plus the committed
 *    two-version fixture whose ground truth is known BY CONSTRUCTION
 *    (fixtures/old.js → formatted → prior; fixtures/new.js → input).
 *
 * No network, no built binary: the fixture's dump is committed and
 * regenerated deliberately (see the README).
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { canonical, canonicalLoose as lo } from "./canonical.js";
import { scoreMatchDump } from "./score.js";

const here = dirname(fileURLToPath(import.meta.url));

// ---------------------------------------------------------------------------
// canonical()
// ---------------------------------------------------------------------------

test("wrapper spelling is normalized: function expression ≡ arrow", () => {
  assert.equal(
    canonical("function (a, b) {\n  return a + b;\n}"),
    canonical("(a, b) => {\n  return a + b;\n}")
  );
  assert.equal(
    canonical("async function (a) {\n  return a;\n}"),
    canonical("async (a) => {\n  return a;\n}")
  );
  // The statement slice carries its `var name =` prefix.
  assert.equal(
    canonical("var w = function (a) {\n  return a;\n};"),
    canonical("var w = (a) => {\n  return a;\n};")
  );
});

test("consistent identifier renaming is erased", () => {
  assert.equal(
    canonical("function (px, qy) {\n  return px * qy + 7;\n}"),
    canonical("function (xx, yy) {\n  return xx * yy + 7;\n}")
  );
  // The function's own name is just an identifier.
  assert.equal(
    canonical("function foo(a) {\n  return foo.length + a;\n}"),
    canonical("function bar(b) {\n  return bar.length + b;\n}")
  );
});

test("inconsistent renaming is NOT the same function", () => {
  // px→a, qy→a collapses two identifiers to one positional token; a
  // bijection cannot reproduce it.
  assert.notEqual(
    canonical("function (px, qy) {\n  return px + px;\n}"),
    canonical("function (a, b) {\n  return a + b;\n}")
  );
});

test("string contents are opaque, not identifiers", () => {
  assert.notEqual(
    canonical('function () {\n  return "alpha";\n}'),
    canonical('function () {\n  return "beta";\n}')
  );
  // An identifier-shaped string is still a string: renaming `aa` outside
  // must not reach inside the quotes.
  assert.equal(
    canonical('function (aa) {\n  return aa + "zz";\n}'),
    canonical('function (b) {\n  return b + "zz";\n}')
  );
});

test("structure and literals are kept", () => {
  assert.notEqual(
    canonical("function (n) {\n  return n * 2;\n}"),
    canonical("function (n) {\n  return n * 3;\n}")
  );
  assert.notEqual(
    canonical("function (a, b) {\n  return a + b;\n}"),
    canonical("function (a, b) {\n  return a - b;\n}")
  );
});

test("the loose form blurs literals but keeps operators", () => {
  // Literal-only difference → loose-equal (the should-match tier).
  assert.equal(
    lo("function (n) {\n  return n * 2 + 50;\n}"),
    lo("function (n) {\n  return n * 3 + 100;\n}")
  );
  assert.equal(
    lo('function () {\n  return "aa";\n}'),
    lo('function () {\n  return "bb";\n}')
  );
  // Operator/structure differences survive the blur.
  assert.notEqual(
    lo("function (a, b) {\n  return a + b;\n}"),
    lo("function (a, b) {\n  return a - b;\n}")
  );
});

// ---------------------------------------------------------------------------
// The two walk-scale blind spots the 2026-09-29 number-blur study measured
// on the claude-code 2.1.215 -> 2.1.216 runtime match (dump-runtime.json,
// 60,466 reported pairs): treating a template literal as one opaque string
// read every HUMAN name interpolated inside ${...} as a string-length
// change (4,386 PHANTOM literal diffs), and a quote character inside a
// regex literal desynced the string scanner (+55 phantoms).
// ---------------------------------------------------------------------------

test("template ${…} interpolations are CODE, not string content", () => {
  // A consistent identifier rename must be erased inside interpolations:
  // the human prior names the binding, the fresh side's is minified.
  assert.equal(
    canonical("function (count) {\n  return `${count} items`;\n}"),
    canonical("function (totalShown) {\n  return `${totalShown} items`;\n}")
  );
  // The bijection is shared across the template boundary: the same
  // identifier outside and inside an interpolation is one index.
  assert.equal(
    canonical("function (px) {\n  var msg = `got ${px}`;\n  return px;\n}"),
    canonical(
      "function (user) {\n  var msg = `got ${user}`;\n  return user;\n}"
    )
  );
  // Braces nested inside an interpolation do not close it early.
  assert.equal(
    canonical("function (a) {\n  return `${({ k: a }).k} end`;\n}"),
    canonical("function (b) {\n  return `${({ k: b }).k} end`;\n}")
  );
  // The quasi parts are still string content: a real difference there is
  // a difference.
  assert.notEqual(
    canonical("function () {\n  return `a${1}x`;\n}"),
    canonical("function () {\n  return `a${1}y`;\n}")
  );
});

test("the loose form blurs each template quasi by length, interpolations as code", () => {
  // The interpolated identifier's LENGTH no longer leaks into the string
  // blur: `${x} done` and `${somethingLong} done` are the same function.
  assert.equal(
    lo("function (x) {\n  return `${x} done`;\n}"),
    lo("function (userCount) {\n  return `${userCount} done`;\n}")
  );
  // A genuine quasi-length difference still fails the blur.
  assert.notEqual(
    lo("function (n) {\n  return `${n} aa`;\n}"),
    lo("function (n) {\n  return `${n} aaaa`;\n}")
  );
});

test("numeric literals are one token — a BigInt suffix is never an identifier", () => {
  // `0n`'s trailing `n` was read as a fresh identifier of the bijection:
  // when the fresh side happens to name a variable `n`, the suffix joined
  // ITS index on one side and minted a new one on the other — the last 7
  // phantom diffs the walk-scale check found after the template/regex
  // fixes (all BigInt arithmetic).
  assert.equal(
    canonical(
      "function (index) {\n  let counter = 21;\n  while (counter > 0n) {\n    counter--;\n  }\n}"
    ),
    canonical(
      "function (n) {\n  let a = 21;\n  while (a > 0n) {\n    a--;\n  }\n}"
    )
  );
  // The strict form keeps the literal's spelling; the loose form still
  // blurs it to one `#`.
  assert.equal(
    canonical("function (x) {\n  return x + 1024n;\n}"),
    "(_0) => {\n  return _0 + 1024n;\n}"
  );
  assert.equal(
    lo("function (x) {\n  return x + 1024n;\n}"),
    lo("function (x) {\n  return x + 999999n;\n}")
  );
});

test("a regex containing a quote character does not desync the string scanner", () => {
  // The quote inside the regex opened a "string" that swallowed real code,
  // so a pure identifier rename read as differing literal content.
  assert.equal(
    canonical(
      'function (px) {\n  var re = /"/;\n  return ("done " + px).replace(re, "x");\n}'
    ),
    canonical(
      'function (user) {\n  var re = /"/;\n  return ("done " + user).replace(re, "x");\n}'
    )
  );
  // Two DIFFERENT regexes must never read canonical-equal — the desync
  // could erase a real difference that sits before the quote.
  assert.notEqual(
    canonical('function () {\n  var re = /a"x/;\n}'),
    canonical('function () {\n  var re = /b"x/;\n}')
  );
  // `return /…/` is a regex (a keyword precedes it), not division.
  assert.equal(
    canonical('function (s) {\n  return /"\\d"/.test(s);\n}'),
    canonical('function (str) {\n  return /"\\d"/.test(str);\n}')
  );
  // Division is still division: no regex scan after a value.
  assert.notEqual(
    canonical("function (a, b) {\n  return a / b;\n}"),
    canonical("function (a, b) {\n  return a * b;\n}")
  );
  assert.equal(
    canonical("function (total) {\n  return total / 2;\n}"),
    canonical("function (sum) {\n  return sum / 2;\n}")
  );
});

// ---------------------------------------------------------------------------
// scoreMatchDump() on a hand-written dump — every number derivable by hand
// ---------------------------------------------------------------------------

const row = (name: string, slice: string, n: number) => ({
  id: `input.js:1:${n}`,
  start: n * 100,
  end: n * 100 + 10,
  name,
  slice
});

/** Six prior functions, six fresh ones, five reported pairs. */
function handDump(): unknown {
  const body = (mul: string, add: string) =>
    `function x(n) {\n  var t = 0;\n  for (var i = 0; i < 9; i++) {\n    t += i;\n  }\n  t = t * ${mul};\n  t = t + n;\n  t -= ${add};\n  return t;\n}`;
  const farBody =
    "function y(n) {\n  var s = 0;\n  while (n > 0) {\n    s += n % 10;\n    n = Math.floor(n / 10);\n  }\n  s = s * s;\n  s += 1000;\n  if (s > 5) {\n    s = 0;\n  }\n  return s;\n}";
  const prior = [
    row("alpha", "function alpha(x, y) {\n  return x + y;\n}", 1), // unique must-match
    row("renamed", "function renamed(p, q) {\n  return p * q;\n}", 2), // renamed twin of fresh "renamed2"
    row("near", body("2", "4"), 3), // one line differs from fresh "near2"
    row("far", farBody, 4), // paired with a DIFFERENT fresh fn entirely
    row("dupe", "function dupe() {\n  return 1;\n}", 5), // duplicate class (x2 prior, x2 fresh)
    row("dupe", "function dupe() {\n  return 1;\n}", 6)
  ];
  const fresh = [
    row("alpha", "function alpha(x, y) {\n  return x + y;\n}", 1), // matched [0]
    row("renamed2", "function renamed2(a, b) {\n  return a * b;\n}", 2), // matched [1]
    row("near2", body("3", "4"), 3), // matched [2] — one line differs from prior near
    row("other", "function other(z) {\n  return z - z;\n}", 4), // matched to prior far [3]
    row("dupe", "function dupe() {\n  return 1;\n}", 5),
    row("dupe", "function dupe() {\n  return 1;\n}", 6)
  ];
  const pair = (p: number, f: number, tier: string) => ({
    prior: p,
    fresh: f,
    priorId: prior[p].id,
    freshId: fresh[f].id,
    tier
  });
  const functions = {
    prior,
    fresh,
    pairs: [
      pair(0, 0, "identity"),
      pair(1, 1, "identity"),
      pair(2, 2, "ordinal"),
      pair(3, 4, "interchangeable"),
      pair(4, 5, "interchangeable")
    ],
    unmatched: [],
    ambiguous: [],
    rejections: [],
    stats: {}
  };
  const stmt = (slice: string, n: number) => ({
    start: n,
    end: n + 1,
    hash: `h${n}`,
    slice
  });
  const twins = {
    prior: [stmt("var a = 1;", 1), stmt("var b = 2;", 2)],
    fresh: [stmt("var a = 1;", 11), stmt("var b = 9;", 12)],
    gates: {
      rows: [
        {
          tier: "unique",
          prior: { start: 1, end: 2 },
          fresh: { start: 11, end: 12 },
          outcome: "bridged"
        },
        {
          tier: "unique",
          prior: { start: 2, end: 3 },
          fresh: { start: 12, end: 13 },
          outcome: "vetoed:callee"
        }
      ]
    }
  };
  return {
    schemaVersion: 1,
    tool: "humanify match",
    meta: {},
    files: [{ path: "index.js", freshText: "", functions, close: null, twins }]
  };
}

test("scorer arithmetic: recall, classes, duplicates, statements", () => {
  const card = scoreMatchDump(handDump());
  const f = card.functions;
  // Must-match set: alpha (identical), renamed (renamed), near? — near vs
  // near2 differ (n*2 vs n*3), so NOT canonical-identical: not in the must
  // set. dupe is the duplicate class (2x prior, 2x fresh): excluded.
  assert.equal(f.priorCount, 6);
  assert.equal(f.freshCount, 6);
  assert.equal(f.mustMatch, 2);
  assert.equal(f.matchedOfMust, 2);
  assert.equal(f.recall, 1);
  // Tier 2 (loose): near↔near2 differ only in a literal — the one
  // should-match pair, and it IS reported.
  assert.equal(f.shouldMatch, 1);
  assert.equal(f.matchedOfShould, 1);
  assert.equal(f.shouldRecall, 1);
  assert.equal(f.duplicateClass, 4); // 2 prior + 2 fresh rows
  assert.equal(f.reported, 5);
  // Reported pairs classified: alpha identical, renamed identical (renamed
  // twin), near↔near2 NEAR (one line differs), far↔other FAR,
  // dupe↔dupe identical.
  assert.equal(f.reportedClasses.identical, 3);
  assert.equal(f.reportedClasses.near, 1);
  assert.equal(f.reportedClasses.far, 1);
  assert.equal(f.farPairs.length, 1);
  assert.equal(f.farPairs[0].priorName, "far");
  // Tier attribution over the must-set.
  assert.deepEqual(f.tierCounts, { identity: 2 });
  // Statements: must = [var a = 1] (identical); [var b = 2] vs [var b = 9]
  // differ. One proposed (and bridged), and it is the must one.
  const s = card.statements;
  assert.equal(s.priorCount, 2);
  assert.equal(s.mustMatch, 1);
  assert.equal(s.proposedOfMust, 1);
  assert.equal(s.bridgedOfMust, 1);
  assert.equal(s.recall, 1);
});

test("a must-pair the matcher missed is reported, not averaged away", () => {
  const dump = handDump() as {
    files: Array<{ functions: { pairs: unknown[] } }>;
  };
  // Drop the alpha pair: alpha is in the must set and now unmatched.
  dump.files[0].functions.pairs = dump.files[0].functions.pairs.filter(
    (p) => (p as { prior: number }).prior !== 0
  );
  const card = scoreMatchDump(dump);
  assert.equal(card.functions.mustMatch, 2);
  assert.equal(card.functions.matchedOfMust, 1);
  assert.equal(card.functions.recall, 0.5);
  assert.equal(card.functions.missedMust.length, 1);
  assert.equal(card.functions.missedMust[0].priorName, "alpha");
});

test("the dump schema version is checked loudly", () => {
  const dump = handDump() as { schemaVersion: number };
  dump.schemaVersion = 99;
  assert.throws(() => scoreMatchDump(dump), /schemaVersion/);
});

// ---------------------------------------------------------------------------
// The committed fixture — ground truth known by construction
// ---------------------------------------------------------------------------

test("the committed fixture's dump scores its constructed ground truth", () => {
  const fixtureDump: unknown = JSON.parse(
    readFileSync(join(here, "fixtures", "dump.json"), "utf8")
  );
  const card = scoreMatchDump(fixtureDump);
  const f = card.functions;
  // By construction the fixture has three must-match functions:
  // keepExact (byte-identical), keepRenamed (identifier renaming) and
  // keepWrapper (arrow vs function-expression spelling). Matcher
  // behavior ON THIS DUMP, recorded exactly because the dump is
  // deterministic — the two findings below are the README's first two:
  assert.equal(f.mustMatch, 3);
  // FINDING 1: keepWrapper (the arrow↔function-expression spelling
  // change) is NOT matched — recall is 2/3 despite the byte-equal body.
  assert.equal(f.matchedOfMust, 2);
  assert.equal(f.missedMust.length, 1);
  assert.ok(
    f.missedMust[0].priorSlice.includes("keepExact(one, two) + 1"),
    `the missed must-pair is the wrapper-spelling one, got ${JSON.stringify(
      f.missedMust[0]
    )}`
  );
  assert.ok(
    f.missedMust.every((m) => !m.priorSlice.includes("alpha + beta")),
    "the byte-identical function is matched (a finding if this ever fails)"
  );
  // FINDING 2: changedSmall differs only in literals (n * 2 + 50 vs
  // n * 3 + 100) — a loose/should-match pair the matcher also missed.
  assert.equal(f.shouldMatch, 1);
  assert.equal(f.matchedOfShould, 0);
  assert.equal(f.missedShould[0].priorName, "changedSmall");
  // removedOld exists only in the old version — never silently absent.
  assert.ok(
    f.priorNames.includes("removedOld"),
    "the prior inventory carries the removed function"
  );
  // Statements: keepExact and keepRenamed were proposed as twins (both
  // abstained — nothing to bridge, by design); keepWrapper's statement
  // (spelling change) was not proposed at all.
  assert.equal(card.statements.mustMatch, 3);
  assert.equal(card.statements.proposedOfMust, 2);
  assert.equal(card.statements.recall, 2 / 3);
});
