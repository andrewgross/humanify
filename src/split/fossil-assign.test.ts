import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { assignFossil } from "./fossil-assign.js";
import type { StableSplitLedger } from "./stable-split.js";
import { STATEMENT_HASH_VERSION, statementHash } from "./statement-hash.js";

function bodyOf(code: string): t.Statement[] {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File;
  return ast.program.body;
}

const BUNDLE = [
  "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);",
  "function alphaCore(x) { return x + 1; }",
  "var alphaState;",
  "var init_alpha = __esm(() => { alphaState = alphaCore(1); });",
  "function betaRender(y) { return alphaState + y; }",
  "var betaCache;",
  "var init_beta = __esm(() => { init_alpha(); betaCache = betaRender(2); });",
  "console.log(init_beta);"
].join("\n");

describe("assignFossil", () => {
  const body = bodyOf(BUNDLE);
  const hashes = body.map(statementHash);

  it("assigns each module's statements to one file, eager tail to bootstrap", () => {
    const out = assignFossil(body, hashes, undefined);
    assert.strictEqual(out.assignment.length, body.length);
    // one file per module
    const fileA = out.assignment[1];
    assert.strictEqual(out.assignment[2], fileA);
    assert.strictEqual(out.assignment[3], fileA);
    const fileB = out.assignment[4];
    assert.notStrictEqual(fileA, fileB);
    assert.strictEqual(out.assignment[6], fileB);
    // eager tail
    // The eager tail IS the entry file (nothing imports it, so the
    // bundler had nothing to defer), so it gets the entry name — exp074.
    assert.strictEqual(out.assignment[7], "src/index.js");
    // names derive from module content, kebab-cased, under src/
    assert.match(fileA, /^src\/.*alpha-core\.js$/);
    // the ledger record carries every fresh module for the next hop
    assert.strictEqual(out.fossilModules.length, 2);
    assert.strictEqual(out.fossilModules[0].file, fileA);
  });

  it("matched modules inherit their prior file path verbatim", () => {
    const first = assignFossil(body, hashes, undefined);
    const prior: StableSplitLedger = {
      version: 1,
      files: [],
      nameToFiles: {},
      order: [],
      hashVersion: STATEMENT_HASH_VERSION,
      fossilModules: [
        { ...first.fossilModules[0], file: "src/legacy/kept-name.js" },
        first.fossilModules[1]
      ]
    };
    const second = assignFossil(body, hashes, prior);
    assert.strictEqual(second.assignment[1], "src/legacy/kept-name.js");
    // and the new ledger re-records the inherited path
    assert.strictEqual(second.fossilModules[0].file, "src/legacy/kept-name.js");
  });

  it("throws loudly when the bundle records no fossils at all", () => {
    const eager = bodyOf("var a = 1;\nconsole.log(a);");
    assert.throws(
      () => assignFossil(eager, eager.map(statementHash), undefined),
      /no module fossils/
    );
  });
});

describe("assignFossil — folder signals (exp070 addendum)", () => {
  it("a barrel module (init body = only init calls, fan-out >= 2) anchors a folder", () => {
    const barrel = bodyOf(
      [
        "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);",
        "function leafAlphaThing(x) { return x; }",
        "var init_a = __esm(() => { leafAlphaThing(1); });",
        "function leafBetaThing(x) { return x; }",
        "var init_b = __esm(() => { leafBetaThing(2); });",
        // the barrel: nothing of its own, only re-export plumbing
        "var init_bundle = __esm(() => { init_a(); init_b(); });",
        // an importer of the barrel keeps it live
        "function useAll() { return 1; }",
        "var init_main = __esm(() => { init_bundle(); useAll(); });"
      ].join("\n")
    );
    const out = assignFossil(barrel, barrel.map(statementHash), undefined);
    const aFile = out.assignment[1];
    const bFile = out.assignment[3];
    const barrelFile = out.assignment[5];
    const folderOf = (f: string) => f.slice(0, f.lastIndexOf("/"));
    // both leaves and the barrel index live in the barrel's folder
    assert.strictEqual(folderOf(aFile), folderOf(barrelFile));
    assert.strictEqual(folderOf(bFile), folderOf(barrelFile));
    assert.ok(out.stats.signals.barrel >= 1);
  });

  it("shared modules with identical importer sets group into one folder", () => {
    // THREE shared leaves, not two: exp074 dissolves folders under
    // MIN_FOLDER_FILES into their parent, because a two-file folder is
    // fragmentation rather than structure (measured on 2.1.86: blanket
    // collapse takes 809 folders to ~316 at a median of 4 files each).
    const shared = bodyOf(
      [
        "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);",
        // shared leaves, all imported by BOTH consumers below
        "function sharedOne(x) { return x; }",
        "var init_s1 = __esm(() => { sharedOne(1); });",
        "function sharedTwo(x) { return x; }",
        "var init_s2 = __esm(() => { sharedTwo(2); });",
        "function sharedThree(x) { return x; }",
        "var init_s3 = __esm(() => { sharedThree(3); });",
        "function consumerA() { return 1; }",
        "var init_ca = __esm(() => { init_s1(); init_s2(); init_s3(); consumerA(); });",
        "function consumerB() { return 2; }",
        "var init_cb = __esm(() => { init_s1(); init_s2(); init_s3(); consumerB(); });"
      ].join("\n")
    );
    const out = assignFossil(shared, shared.map(statementHash), undefined);
    const folderOf = (f: string) => f.slice(0, f.lastIndexOf("/"));
    // the two shared modules co-locate, and not at bare src/
    assert.strictEqual(
      folderOf(out.assignment[1]),
      folderOf(out.assignment[3])
    );
    assert.notStrictEqual(folderOf(out.assignment[1]), "src");
    assert.ok(out.stats.signals.coImporter >= 2);
  });

  it("a flat file whose importers agree on a folder moves in with them", () => {
    // exp074 signal 4. `helper` is imported by three modules that all sit
    // in one anchor's folder, so consensus (≥50%) moves it there instead
    // of leaving it at the flat root. Files whose importers do NOT agree
    // stay flat and are counted — inventing a home for a genuinely
    // shared utility would be a guess.
    const src = bodyOf(
      [
        "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);",
        "function helper(x) { return x; }",
        "var init_h = __esm(() => { helper(0); });",
        "function leafA() { return 1; }",
        "var init_a = __esm(() => { init_h(); leafA(); });",
        "function leafB() { return 2; }",
        "var init_b = __esm(() => { init_h(); leafB(); });",
        "function leafC() { return 3; }",
        "var init_c = __esm(() => { init_h(); leafC(); });",
        "function anchor() { return 4; }",
        "var init_anchor = __esm(() => { init_a(); init_b(); init_c(); anchor(); });"
      ].join("\n")
    );
    const out = assignFossil(src, src.map(statementHash), undefined);
    const folderOf = (f: string) => f.slice(0, f.lastIndexOf("/"));
    const helperFolder = folderOf(out.assignment[1]);
    const leafFolder = folderOf(out.assignment[3]);
    assert.strictEqual(helperFolder, leafFolder);
    assert.notStrictEqual(helperFolder, "src");
  });
});
