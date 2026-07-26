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
