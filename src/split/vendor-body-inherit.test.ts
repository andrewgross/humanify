import assert from "node:assert/strict";
import {
  configureKillSwitches,
  resetKillSwitchesForTests
} from "../kill-switches.js";
import { describe, it } from "node:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createVendorBodyInheritor } from "./vendor-body-inherit.js";

/**
 * humanify deliberately skips naming the ~1,650 vendored library files, so
 * Bun's minifier reroll of every local passes straight through to the output:
 * on every gate hop essentially EVERY common vendor file changes and no
 * library changed with it — 13,900 diff lines across the four hops
 * (experiments/046-vendor-noise). Reusing the prior release's bytes when the
 * two are the same program removes that churn.
 *
 * The whole safety of this rests on the key. `structuralHash`, the manifest's
 * cross-version join key, is NOT usable here: it serializes with
 * `preserveLiterals: false`, keeping only a string's LENGTH and a number's
 * order-of-magnitude bucket, so two modules differing in a URL or a timeout
 * share it. These tests pin the cases that separates them.
 */
function makeTree(files: Record<string, string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "vendor-inherit-"));
  for (const [rel, body] of Object.entries(files)) {
    const p = path.join(root, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, body);
  }
  return root;
}

const PRIOR_RENAMED = `const { __commonJS } = require("../.humanify/__bun-runtime.js");
exports.f = __commonJS(function(iet, aQ) { var cX = iet.x + 1; return cX * aQ; });
`;
const FRESH_RENAMED = `const { __commonJS } = require("../.humanify/__bun-runtime.js");
exports.f = __commonJS(function(Jet, Kp) { var Lm = Jet.x + 1; return Lm * Kp; });
`;

describe("createVendorBodyInheritor", () => {
  it("reuses the prior body when only local names were rerolled", () => {
    const prior = makeTree({ "vendor/a.js": PRIOR_RENAMED });
    const inherit = createVendorBodyInheritor(prior);
    assert.ok(inherit);
    assert.equal(inherit.bytesFor("vendor/a.js", FRESH_RENAMED), PRIOR_RENAMED);
    assert.equal(inherit.stats().inherited, 1);
  });

  it("does NOT reuse when a string literal changed at the same length", () => {
    // The exact case structuralHash cannot see. Inheriting here would ship
    // the previous release's endpoint.
    const prior = makeTree({
      "vendor/a.js": `exports.f = function(q) { return fetch("https://a.example/v1", q); };\n`
    });
    const fresh = `exports.f = function(z) { return fetch("https://b.example/v2", z); };\n`;
    const inherit = createVendorBodyInheritor(prior);
    assert.ok(inherit);
    assert.equal(inherit.bytesFor("vendor/a.js", fresh), fresh);
    assert.equal(inherit.stats().inherited, 0);
  });

  it("does NOT reuse when a number changed within one magnitude bucket", () => {
    // structuralHash keeps Math.floor(log10(|v|+1)), so 1000 and 2000 collide.
    const prior = makeTree({
      "vendor/a.js": `exports.f = function(q) { return setTimeout(q, 1000); };\n`
    });
    const fresh = `exports.f = function(z) { return setTimeout(z, 2000); };\n`;
    const inherit = createVendorBodyInheritor(prior);
    assert.ok(inherit);
    assert.equal(inherit.bytesFor("vendor/a.js", fresh), fresh);
  });

  it("does NOT reuse when an intra-tree require path changed", () => {
    // The prior bytes would require a path this tree may not have. Keeping
    // require paths IN the key is what makes a match drop-in safe.
    const prior = makeTree({
      "vendor/a.js": `const d = require("./lodash/lib_aaaa.js");\nexports.f = function(q) { return d.f(q); };\n`
    });
    const fresh = `const y = require("./lodash/lib_aaaa-2.js");\nexports.f = function(z) { return y.f(z); };\n`;
    const inherit = createVendorBodyInheritor(prior);
    assert.ok(inherit);
    assert.equal(inherit.bytesFor("vendor/a.js", fresh), fresh);
  });

  it("does NOT reuse when a property name changed", () => {
    const prior = makeTree({
      "vendor/a.js": `exports.f = function(q) { return q.readFileSync; };\n`
    });
    const fresh = `exports.f = function(z) { return z.writeFileSync; };\n`;
    const inherit = createVendorBodyInheritor(prior);
    assert.ok(inherit);
    assert.equal(inherit.bytesFor("vendor/a.js", fresh), fresh);
  });

  it("leaves a file with no prior counterpart alone", () => {
    const inherit = createVendorBodyInheritor(makeTree({}));
    assert.ok(inherit);
    assert.equal(
      inherit.bytesFor("vendor/new.js", FRESH_RENAMED),
      FRESH_RENAMED
    );
    assert.equal(inherit.stats().inherited, 0);
  });

  it("is a no-op when the prior file is already byte-identical", () => {
    const prior = makeTree({ "vendor/a.js": PRIOR_RENAMED });
    const inherit = createVendorBodyInheritor(prior);
    assert.ok(inherit);
    // Self-hop: prior IS this run's own output, so nothing may change.
    assert.equal(inherit.bytesFor("vendor/a.js", PRIOR_RENAMED), PRIOR_RENAMED);
  });

  it("returns undefined without a prior tree", () => {
    assert.equal(createVendorBodyInheritor(undefined), undefined);
  });

  it("returns undefined when the kill switch is set", () => {
    const prior = makeTree({ "vendor/a.js": PRIOR_RENAMED });
    configureKillSwitches({ disable: ["vendor-inherit"] });
    try {
      assert.equal(createVendorBodyInheritor(prior), undefined);
    } finally {
      resetKillSwitchesForTests();
    }
  });

  it("does not reuse a body that fails to parse", () => {
    const prior = makeTree({ "vendor/a.js": "this is ( not javascript\n" });
    const inherit = createVendorBodyInheritor(prior);
    assert.ok(inherit);
    assert.equal(inherit.bytesFor("vendor/a.js", FRESH_RENAMED), FRESH_RENAMED);
  });
});
