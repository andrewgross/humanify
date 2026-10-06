import assert from "node:assert/strict";
import { describe, it, before, after } from "node:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { decomposeVendorChurn } from "./vendor-churn.js";

/**
 * The decomposition's whole value is that each bucket means what its name
 * says — three sizing predicates in this series produced confident wrong
 * numbers because they did not (docs/measurement-pitfalls.md rule 3). These
 * fixtures pin each bucket to a case whose answer is known by hand.
 */
let root: string;
let prior: string;
let fresh: string;

const write = (dir: string, rel: string, body: string) => {
  const p = path.join(dir, rel);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, body);
};

before(() => {
  root = fs.mkdtempSync(path.join(os.tmpdir(), "vendor-churn-"));
  prior = path.join(root, "prior");
  fresh = path.join(root, "fresh");

  // 1. minifier rerolled every local — the dominant real-world case
  write(
    prior,
    "a.js",
    "exports.f=function(aQ,bZ){var cX=aQ+1;return cX*bZ};\n"
  );
  write(
    fresh,
    "a.js",
    "exports.f=function(Jt,Kp){var Lm=Jt+1;return Lm*Kp};\n"
  );

  // 2. byte-identical
  write(prior, "b.js", "exports.f=function(){return 1};\n");
  write(fresh, "b.js", "exports.f=function(){return 1};\n");

  // 3. a STRING LITERAL changed at the same length — real change, and the
  //    exact case `structuralHash` cannot see (hash-probe.ts)
  write(
    prior,
    "c.js",
    "exports.f=function(){return fetch('https://a.example/v1')};\n"
  );
  write(
    fresh,
    "c.js",
    "exports.f=function(){return fetch('https://b.example/v2')};\n"
  );

  // 4. same content, humanify put it at a different path
  write(prior, "d.js", "exports.f=function(zz){return zz.slice(0,7)};\n");
  write(
    fresh,
    "nested/d-2.js",
    "exports.f=function(qq){return qq.slice(0,7)};\n"
  );

  // 5. only the intra-tree require path moved — humanify's own layout churn,
  //    not a library change
  write(
    prior,
    "e.js",
    "const x=require('./d.js');exports.f=function(){return x};\n"
  );
  write(
    fresh,
    "e.js",
    "const y=require('./nested/d-2.js');exports.f=function(){return y};\n"
  );

  // 6. a genuinely new library, and one that genuinely went away
  write(fresh, "added.js", "exports.f=function(){return 'brand new'};\n");
  write(prior, "gone.js", "exports.f=function(){return 'retired'};\n");
});

after(() => fs.rmSync(root, { recursive: true, force: true }));

describe("decomposeVendorChurn", () => {
  it("counts a whole-file local rename as name-only, not change", () => {
    const r = decomposeVendorChurn(prior, fresh);
    // TWO files qualify: a.js (pure local reroll) and e.js, whose only other
    // change is an intra-tree require path the predicate masks by design.
    assert.equal(r.bodies.nameOnly.files, 2);
    assert.ok(r.bodies.nameOnly.lines > 0);
    assert.ok(!r.realChangeFiles.includes("a.js"));
  });

  it("does not charge a byte-identical file", () => {
    const r = decomposeVendorChurn(prior, fresh);
    assert.equal(r.bodies.identical.files, 1);
  });

  it("charges a same-length string literal change as REAL change", () => {
    const r = decomposeVendorChurn(prior, fresh);
    assert.deepEqual(r.realChangeFiles, ["c.js"]);
  });

  it("classes a file that only moved path as moved, not added+removed", () => {
    const r = decomposeVendorChurn(prior, fresh);
    assert.equal(r.bodies.movedPath.files, 1);
  });

  it("masks intra-tree require paths so a dependent of a moved file is name-only", () => {
    const r = decomposeVendorChurn(prior, fresh);
    // e.js changed text only because d.js moved; it must not read as change.
    assert.ok(!r.realChangeFiles.includes("e.js"));
  });

  it("separates a genuinely new library from one that only moved", () => {
    const r = decomposeVendorChurn(prior, fresh);
    assert.equal(r.bodies.trulyAdded.files, 1);
    assert.equal(r.bodies.trulyRemoved.files, 1);
  });

  it("reports real dependency change as the added+removed+changed lines only", () => {
    const r = decomposeVendorChurn(prior, fresh);
    const expected =
      r.bodies.realChange.lines +
      r.bodies.trulyAdded.lines +
      r.bodies.trulyRemoved.lines;
    assert.equal(r.realDependencyChangeLines, expected);
    assert.ok(r.realDependencyChangeLines > 0);
  });
});

/**
 * Relocation pairing (2026-10-06). A vendor file whose content changed AND
 * whose humanify-chosen path changed used to be charged as a whole-file
 * removal plus a whole-file addition: the eslint-plugin-security case, a
 * 151-line text module with a real 6-line edit, read 6 lines when both
 * versions drew the same path and 303 when they did not. The fixtures below
 * are that shape (a `module.exports = <text>` factory), plus the two ways a
 * pairing could go wrong: pairing unrelated files, and picking one of two
 * equally good candidates.
 */
const TEXT_LINES = 148;

/** A bun-factory text module of `TEXT_LINES` prose lines (151 file lines). */
function textModule(topic: string, edits: Record<number, string> = {}) {
  const body: string[] = [];
  for (let i = 0; i < TEXT_LINES; i++) {
    body.push(
      edits[i] ??
        `Rule ${i} about ${topic}: keep the ${topic} policy number ${i * 7} strict and audited.`
    );
  }
  return [
    'const { __commonJS } = require("../.humanify/__bun-runtime.js");',
    "exports.f = __commonJS(function(a,b){b.exports=`",
    ...body,
    "`});",
    ""
  ].join("\n");
}

function pairFixture(files: {
  prior: Record<string, string>;
  fresh: Record<string, string>;
}): { prior: string; fresh: string } {
  const dir = fs.mkdtempSync(path.join(root, "pair-"));
  const p = path.join(dir, "prior");
  const f = path.join(dir, "fresh");
  for (const [rel, body] of Object.entries(files.prior)) write(p, rel, body);
  for (const [rel, body] of Object.entries(files.fresh)) write(f, rel, body);
  return { prior: p, fresh: f };
}

/** Three changed lines: a modified line counts twice, so 6 diff lines. */
const EDIT = {
  10: "Rule 10 was rewritten for the new release.",
  70: "Rule 70 was rewritten for the new release.",
  130: "Rule 130 was rewritten for the new release."
};

describe("decomposeVendorChurn: relocated files", () => {
  it("charges a moved file with a 6-line edit 6 real lines, not 303", () => {
    const { prior: p, fresh: f } = pairFixture({
      prior: { "eslint-plugin-security.js": textModule("security") },
      fresh: { "dynamic-anchor.js": textModule("security", EDIT) }
    });
    const r = decomposeVendorChurn(p, f);
    assert.equal(r.realDependencyChangeLines, 6);
    assert.equal(r.bodies.trulyAdded.files, 0);
    assert.equal(r.bodies.trulyRemoved.files, 0);
    assert.equal(r.bodies.relocated.files, 1);
    // The move stays visible: the rest of what the old charge was (the whole
    // prior file removed + the whole fresh file added, 152 + 152 by the
    // scorer's line count) less the 6 real lines is the path draw, booked as
    // noise — so the vendor TOTAL is unchanged and only its split moves.
    assert.equal(r.bodies.relocated.lines, 152 + 152 - 6);
    assert.equal(r.vendorTotalLines, 304);
    assert.equal(r.noiseLines, 298);
    assert.deepEqual(
      r.relocated.map((x) => [x.prior, x.fresh, x.realLines]),
      [["eslint-plugin-security.js", "dynamic-anchor.js", 6]]
    );
  });

  it("does not pair unrelated files", () => {
    const { prior: p, fresh: f } = pairFixture({
      prior: { "gone.js": textModule("security") },
      fresh: { "added.js": textModule("telemetry") }
    });
    const r = decomposeVendorChurn(p, f);
    assert.equal(r.bodies.relocated.files, 0);
    assert.equal(r.bodies.trulyAdded.files, 1);
    assert.equal(r.bodies.trulyRemoved.files, 1);
    assert.deepEqual(r.relocated, []);
  });

  it("refuses to pick between two equally good candidates", () => {
    // Both prior files are one edit away from the fresh one, at different
    // places: no margin, so neither is a credible predecessor.
    const { prior: p, fresh: f } = pairFixture({
      prior: {
        "a.js": textModule("security", { 20: "An older rule 20." }),
        "b.js": textModule("security", { 120: "An older rule 120." })
      },
      fresh: { "c.js": textModule("security") }
    });
    const r = decomposeVendorChurn(p, f);
    assert.equal(r.bodies.relocated.files, 0);
    assert.equal(r.bodies.trulyAdded.files, 1);
    assert.equal(r.bodies.trulyRemoved.files, 2);
  });

  it("does not pair across a path SWAP (the f616f33b Fortran/IRPF90 case)", () => {
    // Two related grammars. IRPF90 moved to a new path and Fortran took its
    // old one; the removed Fortran file looks enough like IRPF90 to pair if
    // the file that really is IRPF90's predecessor (still at its path, so
    // never a candidate) were not allowed to compete.
    const lines = (from: number, word: string) =>
      Object.fromEntries(
        Array.from({ length: 25 }, (_, i) => [
          from + i,
          `${word} keyword ${i} is reserved in ${word} sources only.`
        ])
      );
    const fortran = { ...lines(0, "Fortran") };
    const irpf90 = { ...lines(100, "IRPF90") };
    const { prior: p, fresh: f } = pairFixture({
      prior: {
        "cyp.js": textModule("grammar", fortran),
        "fortran.js": textModule("grammar", irpf90)
      },
      fresh: {
        "fortran.js": textModule("grammar", { ...fortran, 60: "New." }),
        "prism-fortran.js": textModule("grammar", { ...irpf90, 60: "New." })
      }
    });
    const r = decomposeVendorChurn(p, f);
    assert.equal(r.bodies.relocated.files, 0);
    assert.equal(r.bodies.trulyAdded.files, 1);
    assert.equal(r.bodies.trulyRemoved.files, 1);
  });

  it("pairs only files that lost their path, never one still at its own", () => {
    // `keep.js` exists on both sides (a same-path change); the fresh-only
    // file must not steal it as a predecessor.
    const { prior: p, fresh: f } = pairFixture({
      prior: { "keep.js": textModule("security") },
      fresh: {
        "keep.js": textModule("security", EDIT),
        "copy.js": textModule("security", { 5: "A copy." })
      }
    });
    const r = decomposeVendorChurn(p, f);
    assert.equal(r.bodies.relocated.files, 0);
    assert.equal(r.bodies.realChange.files, 1);
    assert.equal(r.bodies.trulyAdded.files, 1);
  });
});
