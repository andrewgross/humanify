import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import {
  collectMintedBindings,
  isBunToken,
  isBelowFloorName,
  isHalfMintHead,
  isWordlessMintShape,
  summarizeCensus
} from "./minted-census.js";
import { createIsEligible } from "./rename-eligibility.js";

const IS_ELIGIBLE = createIsEligible("bun", "bun");

function parse(code: string): t.File {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  assert.ok(ast);
  return ast as t.File;
}

function census(code: string) {
  return collectMintedBindings(parse(code), IS_ELIGIBLE).entries;
}

describe("isBunToken (loose census shape — over-counts by design)", () => {
  it("flags Bun mint shapes", () => {
    for (const name of ["uq", "M2_", "FH3", "j2_", "$2", "H", "Tj_", "wP_"]) {
      assert.strictEqual(isBunToken(name), true, `${name} should be a token`);
    }
  });

  it("does not flag ordinary descriptive names", () => {
    for (const name of ["BaseError", "completionState", "sessionStartTime"]) {
      assert.strictEqual(isBunToken(name), false, `${name} is descriptive`);
    }
  });

  it("does not flag common short words", () => {
    for (const name of ["fs", "os", "id", "url", "map"]) {
      assert.strictEqual(isBunToken(name), false, `${name} is a real word`);
    }
  });

  it("does not flag CONSTANT_CASE names regardless of digits", () => {
    for (const name of [
      "AI_PROMPT",
      "MS_PER_SECOND",
      "CF_BUNDLE_IDENTIFIER",
      "GH_STRING",
      "EC2_METADATA_TOKEN_PATH",
      "EC2_METADATA_V1_DISABLED_ENV"
    ]) {
      assert.strictEqual(isBunToken(name), false, `${name} is a constant`);
    }
  });

  it("does not flag names built on known domain stems", () => {
    for (const name of [
      "e164PhonePattern",
      "ec2MetadataService",
      "EC2MetadataServiceConfigSelector",
      "s3Config",
      "sha256Hash",
      "utf8Decoder",
      "base64Payload"
    ]) {
      assert.strictEqual(isBunToken(name), false, `${name} is domain-termed`);
    }
  });

  it("census counts collision-decorated descriptive names separately", () => {
    // The validator appends `_` when the LLM's good name collides — a
    // decoration on a real name. isBunToken still FLAGS it (the
    // decoration-retry pass needs the candidate), but the census must
    // not call it a mint.
    const { entries } = collectMintedBindings(
      parse("var fsPromises_ = 1; var M2_ = 2; var descriptiveName = 3;"),
      IS_ELIGIBLE
    );
    const census = summarizeCensus(entries);
    assert.strictEqual(census.total, 1, "only M2_ is a mint");
    assert.strictEqual(census.decorated, 1, "fsPromises_ is decorated");
  });

  it("does not flag w3c-stemmed names", () => {
    assert.strictEqual(isBunToken("w3cTraceContextPropagator"), false);
  });

  it("still flags half-named mint stems", () => {
    for (const name of ["do7Function", "T7Class", "hl1Setting", "yl1Setting"]) {
      assert.strictEqual(isBunToken(name), true, `${name} is a mint stem`);
    }
  });

  it("does not flag suffix-required stems worn with a word tail", () => {
    // h1Regex/h2Handler are LLM-authored names about <h1>/<h2> tags;
    // it2ExecutablePath is iTerm2; v1PluginData/x0Coord are version and
    // coordinate names. The bare stem alone keeps the mint shape — only
    // the suffixed form is evidence of a deliberate name.
    for (const name of [
      "h1Regex",
      "h2Handler",
      "h6_style",
      "it2ExecutablePath",
      "v1PluginData",
      "x0Coord"
    ]) {
      assert.strictEqual(isBunToken(name), false, `${name} is a real term`);
    }
    for (const name of ["h1", "h4", "x0", "v1"]) {
      assert.strictEqual(isBunToken(name), true, `bare ${name} stays flagged`);
    }
  });

  it("does not flag k8s/b64/u2f/x509 domain names", () => {
    for (const name of [
      "k8sNamespaces",
      "b64Flag",
      "u2fModule",
      "X509CertificateClass"
    ]) {
      assert.strictEqual(isBunToken(name), false, `${name} is domain-termed`);
    }
  });
});

describe("isHalfMintHead (mint stem + derived word tail)", () => {
  it("matches evidenced fossil stem shapes", () => {
    for (const name of [
      "do7Function", // 2 letters + digit
      "T7Class", // 1 letter + digit
      "sm6Factory",
      "h06Result", // 1 letter + 2 digits
      "j3lResult", // letter + digit + lowercase letter
      "p1tComponent",
      "p2cValue"
    ]) {
      assert.strictEqual(isHalfMintHead(name), true, name);
    }
  });

  it("rejects decorated, domain, heading, acronym, and plain names", () => {
    for (const name of [
      "fsPromises_", // collision decoration, not a mint stem
      "h1Regex", // HTML heading carve-out
      "it2ExecutablePath", // iTerm2 carve-out
      "v1PluginData", // version carve-out
      "x0Coord", // coordinate carve-out
      "is2017Api", // word + year: digits too long for a mint stem
      "v8Engine", // domain stem
      "LZ77Compressor", // 2 letters + 2 digits: domain shape
      "X509CertificateClass", // x509 carve-out + acronym tail
      "P2PConnection", // acronym run, no word tail
      "b64Flag", // base64 carve-out
      "hashMapBuilder",
      "iIn" // raw no-digit mint: not a HALF-mint (no derived tail)
    ]) {
      assert.strictEqual(isHalfMintHead(name), false, name);
    }
  });
});

describe("collectMintedBindings — totals", () => {
  it("counts the full binding population alongside minted entries", () => {
    const { entries, totalBindings } = collectMintedBindings(
      parse(
        "var Qx1 = 1; var descriptiveName = 2; function f(a) { return a; }"
      ),
      IS_ELIGIBLE
    );
    // Qx1, descriptiveName, f, a — all bindings counted; only the
    // minted-looking subset becomes entries.
    assert.strictEqual(totalBindings, 4);
    assert.ok(entries.length < totalBindings);
  });
});

describe("collectMintedBindings — family classification", () => {
  it("classifies a class-expression inner id and its derivation source", () => {
    const bindings = census(`var BaseError = class uq extends Error {};`);
    const entry = bindings.find((b) => b.name === "uq");
    assert.ok(entry, "uq must be collected");
    assert.strictEqual(entry.family, "classExprId");
    assert.strictEqual(entry.derivedFrom, "BaseError");
  });

  it("derives from an assignment target and a property key", () => {
    const assigned = census(`Registry.Foo = class q2 {};`);
    assert.strictEqual(
      assigned.find((b) => b.name === "q2")?.derivedFrom,
      "Foo"
    );
    const prop = census(`var obj = { Widget: class w3 {} };`);
    assert.strictEqual(
      prop.find((b) => b.name === "w3")?.derivedFrom,
      "Widget"
    );
  });

  it("returns null derivation when the source is itself minted", () => {
    const bindings = census(`var Z9 = class q2 {};`);
    assert.strictEqual(
      bindings.find((b) => b.name === "q2")?.derivedFrom,
      null
    );
  });

  it("classifies params, whole declarations, and var/other", () => {
    const bindings = census(`
      function updateState(H) { return H; }
      function j2_() {}
      var Kq_ = load();
    `);
    assert.strictEqual(bindings.find((b) => b.name === "H")?.family, "param");
    assert.strictEqual(
      bindings.find((b) => b.name === "j2_")?.family,
      "fnDecl"
    );
    assert.strictEqual(
      bindings.find((b) => b.name === "Kq_")?.family,
      "varOther"
    );
  });

  it("skips eligible descriptive names and skip-listed names", () => {
    const bindings = census(`
      var completionState = load();
      function __helper() {}
    `);
    assert.deepStrictEqual(
      bindings.map((b) => b.name),
      []
    );
  });

  it("records reference counts", () => {
    const bindings = census(`
      function f() {
        var Tj_ = load();
        return Tj_ + Tj_;
      }
    `);
    assert.strictEqual(bindings.find((b) => b.name === "Tj_")?.refCount, 2);
  });
});

describe("summarizeCensus", () => {
  it("totals per family and reports derivable/zero-ref expression ids", () => {
    const bindings = census(`
      var BaseError = class uq extends Error {};
      register(class w7 {});
      function useIt(H) { return H; }
      function j2_() {}
      var Kq_ = load();
      var Wm$ = other();
    `);
    const summary = summarizeCensus(bindings);
    assert.strictEqual(summary.total, 6);
    assert.strictEqual(summary.byFamily.classExprId, 2);
    assert.strictEqual(summary.byFamily.param, 1);
    assert.strictEqual(summary.byFamily.fnDecl, 1);
    assert.strictEqual(summary.byFamily.varOther, 2);
    // uq derives from BaseError; w7 is in argument position → no source.
    // Both class ids have zero references here.
    assert.strictEqual(summary.derivableExprIds, 1);
    assert.strictEqual(summary.zeroRefExprIds, 2);
  });

  it("is all-zero for an output with no minted leftovers", () => {
    const summary = summarizeCensus(census(`var completionState = load();`));
    assert.strictEqual(summary.total, 0);
    assert.strictEqual(summary.byFamily.classExprId, 0);
  });
});

describe("summarizeCensus — names for the terminal-state ledger", () => {
  it("carries the minted and decorated name lists", () => {
    const { entries, totalBindings } = collectMintedBindings(
      parse("var fsPromises_ = 1; var M2_ = 2; var descriptiveName = 3;"),
      IS_ELIGIBLE
    );
    const census = summarizeCensus(entries, totalBindings);
    assert.deepStrictEqual(census.names, ["M2_"]);
    assert.deepStrictEqual(census.decoratedNames, ["fsPromises_"]);
  });
});

describe("isBelowFloorName (the guard's transfer predicate)", () => {
  it("refuses true mints and mint-stem decorations, exempts decorated descriptive", () => {
    for (const name of ["M2_", "qi_15", "uq6", "do7Function"]) {
      assert.strictEqual(isBelowFloorName(name), true, `${name} below floor`);
    }
    for (const name of [
      "fsPromises_",
      "initializeApp_",
      "React_",
      "completionState"
    ]) {
      assert.strictEqual(isBelowFloorName(name), false, `${name} is a name`);
    }
  });
});

describe("isWordlessMintShape (the reconcile's coarse metric shape)", () => {
  it("keeps the legacy attribute-noise semantics exactly", () => {
    // TRUE: no 3-letter lowercase run and not SCREAMING_CASE.
    for (const name of ["iIn", "Tj_", "q7x", "__m", "ACc", "Val", "x1Bb"]) {
      assert.strictEqual(isWordlessMintShape(name), true, `${name} wordless`);
    }
    // FALSE: a real word run or a deliberate constant — including
    // half-mints (do7Function), which is exactly why the reconcile
    // ALSO needs isHalfMintHead (exp035 task C).
    for (const name of [
      "do7Function",
      "completionState",
      "OS_MODULE",
      "response$",
      "fsPromises_"
    ]) {
      assert.strictEqual(isWordlessMintShape(name), false, `${name} wordful`);
    }
  });
});
