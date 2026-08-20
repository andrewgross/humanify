import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { assignFossil } from "./fossil-assign.js";
import { placementTrail } from "./placement-trail.js";
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

  it("assigns each module's statements to one file, eager tail to bootstrap", async () => {
    const out = await assignFossil(body, hashes, undefined);
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

  it("matched modules inherit their prior file path verbatim", async () => {
    const first = await assignFossil(body, hashes, undefined);
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
    const second = await assignFossil(body, hashes, prior);
    assert.strictEqual(second.assignment[1], "src/legacy/kept-name.js");
    // and the new ledger re-records the inherited path
    assert.strictEqual(second.fossilModules[0].file, "src/legacy/kept-name.js");
  });

  it("throws loudly when the bundle records no fossils at all", async () => {
    const eager = bodyOf("var a = 1;\nconsole.log(a);");
    await assert.rejects(
      () => assignFossil(eager, eager.map(statementHash), undefined),
      /no module fossils/
    );
  });
});

describe("assignFossil — folder signals (exp070 addendum)", () => {
  it("a barrel module (init body = only init calls, fan-out >= 2) anchors a folder", async () => {
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
    const out = await assignFossil(
      barrel,
      barrel.map(statementHash),
      undefined
    );
    const aFile = out.assignment[1];
    const bFile = out.assignment[3];
    const barrelFile = out.assignment[5];
    const folderOf = (f: string) => f.slice(0, f.lastIndexOf("/"));
    // both leaves and the barrel index live in the barrel's folder
    assert.strictEqual(folderOf(aFile), folderOf(barrelFile));
    assert.strictEqual(folderOf(bFile), folderOf(barrelFile));
    assert.ok(out.stats.signals.barrel >= 1);
  });

  it("shared modules with identical importer sets group into one folder", async () => {
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
    const out = await assignFossil(
      shared,
      shared.map(statementHash),
      undefined
    );
    const folderOf = (f: string) => f.slice(0, f.lastIndexOf("/"));
    // the two shared modules co-locate, and not at bare src/
    assert.strictEqual(
      folderOf(out.assignment[1]),
      folderOf(out.assignment[3])
    );
    assert.notStrictEqual(folderOf(out.assignment[1]), "src");
    assert.ok(out.stats.signals.coImporter >= 2);
  });

  it("a flat file whose importers agree on a folder moves in with them", async () => {
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
    const out = await assignFossil(src, src.map(statementHash), undefined);
    const folderOf = (f: string) => f.slice(0, f.lastIndexOf("/"));
    const helperFolder = folderOf(out.assignment[1]);
    const leafFolder = folderOf(out.assignment[3]);
    assert.strictEqual(helperFolder, leafFolder);
    assert.notStrictEqual(helperFolder, "src");
  });
});

/**
 * exp076 addendum (Andrew, 2026-08-15): a folder holding one file should
 * hoist that file up a level rather than wrap it.
 *
 * `collapseSmallFolders` already dissolves folders under MIN_FOLDER_FILES —
 * but it runs inside `inferFossilPlacements`, which sees only INFERRED
 * placements. The emitted tree is mostly INHERITED paths, so it counts the
 * wrong population: a folder that looks populated at inference time can end
 * up holding a single file once matched modules have taken their carried
 * paths elsewhere. Measured on 2.1.86 against a 2.1.85 ledger: 4 such
 * folders, every one of the `src/<stem>/<stem>.js` shape.
 *
 * Only FRESH paths may be hoisted. An inherited path is the stability
 * property itself — tidying it would churn every file that carried it.
 */
describe("assignFossil — single-file folders hoist (exp076)", () => {
  const src = bodyOf(
    [
      "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);",
      "function leafA() { return 1; }",
      "var init_a = __esm(() => { leafA(); });",
      "function leafB() { return 2; }",
      "var init_b = __esm(() => { leafB(); });",
      "function leafC() { return 3; }",
      "var init_c = __esm(() => { leafC(); });",
      "function anchorMod() { return 4; }",
      "var init_anchor = __esm(() => { init_a(); init_b(); init_c(); anchorMod(); });"
    ].join("\n")
  );
  const hashes = src.map(statementHash);

  it("a fresh file alone in its folder is hoisted up one level", async () => {
    // The leaves match and carry paths OUT of the anchor's folder; the anchor
    // is GENUINELY NEW, so its inferred `src/anchor-mod/anchor-mod.js` would
    // be the folder's only file.
    //
    // "Genuinely new" means ABSENT FROM THE PRIOR LEDGER, not present with
    // unmatchable content. The original fixture used the latter and exp078's
    // tier D then matched it by graph position — correctly, since it kept all
    // three of its import edges. A fixture that fakes non-existence by
    // corrupting content only tests the tiers that read content.
    const first = await assignFossil(src, hashes, undefined);
    const prior: StableSplitLedger = {
      version: 1,
      files: [],
      nameToFiles: {},
      order: [],
      hashVersion: STATEMENT_HASH_VERSION,
      fossilModules: first.fossilModules
        .slice(0, 3)
        .map((m, i) => ({ ...m, file: `src/carried/mod-${i}.js` }))
    };
    const out = await assignFossil(src, hashes, prior);
    const anchorFile = out.assignment[7];
    assert.strictEqual(anchorFile, "src/anchor-mod.js");
    assert.strictEqual(out.stats.hoistedSingletons, 1);
  });

  it("never hoists an INHERITED path, however lonely its folder", async () => {
    const first = await assignFossil(src, hashes, undefined);
    const prior: StableSplitLedger = {
      version: 1,
      files: [],
      nameToFiles: {},
      order: [],
      hashVersion: STATEMENT_HASH_VERSION,
      // every module carries a path, each alone in its own folder
      fossilModules: first.fossilModules.map((m, i) => ({
        ...m,
        file: `src/lonely-${i}/mod-${i}.js`
      }))
    };
    const out = await assignFossil(src, hashes, prior);
    assert.strictEqual(out.assignment[1], "src/lonely-0/mod-0.js");
    assert.strictEqual(out.stats.hoistedSingletons, 0);
  });

  it("keeps the folder when hoisting would collide with a taken path", async () => {
    const first = await assignFossil(src, hashes, undefined);
    const prior: StableSplitLedger = {
      version: 1,
      files: [],
      nameToFiles: {},
      order: [],
      hashVersion: STATEMENT_HASH_VERSION,
      fossilModules: first.fossilModules.map((m, i) =>
        i === 3
          ? { ...m, hashes: ["no-match-1", "no-match-2"] }
          : // one carried file already occupies the hoist target
            {
              ...m,
              file: i === 0 ? "src/anchor-mod.js" : `src/carried/mod-${i}.js`
            }
      )
    };
    const out = await assignFossil(src, hashes, prior);
    assert.strictEqual(out.assignment[7], "src/anchor-mod/anchor-mod.js");
    assert.strictEqual(out.stats.hoistedSingletons, 0);
  });
});

describe("assignFossil — placement trail (exp082/083)", () => {
  // The trail was EMPTY on every fossil-split tree: the recorder is wired to
  // the stable-split statement tiers, which a fossil tree bypasses. That left
  // the busy hop's 1,480 moved lines undiagnosable by the instrument built
  // for exactly that question. Fossil assignment must record every statement:
  // which module file it landed in, by which evidence (match tier / fresh
  // mint / eager), and where its content lived in the PRIOR release so a
  // `priorFile !== file` reader sees upstream regrouping directly.
  const body = bodyOf(BUNDLE);
  const hashes = body.map(statementHash);

  it("records every statement, tiers on inherited paths, hash-keyed priorFile", async () => {
    const first = await assignFossil(body, hashes, undefined);
    placementTrail.reset(true);
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
    await assignFossil(body, hashes, prior);
    const report = placementTrail.report();
    // every wrapper statement has exactly one entry
    const byIndex = new Map(report.trails.map((e) => [e.index, e]));
    for (let i = 1; i < body.length; i++) {
      assert.ok(byIndex.has(i), `statement ${i} missing from the trail`);
    }
    // module A inherited its prior path via a match tier
    const a = byIndex.get(1);
    assert.strictEqual(a?.file, "src/legacy/kept-name.js");
    assert.match(a?.placedBy ?? "", /^fossil:/);
    // its content existed in the prior release under the prior file
    assert.strictEqual(a?.priorFile, "src/legacy/kept-name.js");
    assert.strictEqual(a?.priorFileFrom, "hash");
    // the eager tail is recorded too
    const eager = byIndex.get(body.length - 1);
    assert.strictEqual(eager?.file, "src/index.js");
    assert.strictEqual(eager?.placedBy, "fossil-eager");
    placementTrail.reset(false);
  });

  it("a statement whose hash lived in a DIFFERENT prior module reads as a move", async () => {
    const first = await assignFossil(body, hashes, undefined);
    placementTrail.reset(true);
    // Prior ledger: statement 4's hash (betaRender) recorded under module A's
    // file — the upstream bundler "regrouped" it into module B this release.
    const prior: StableSplitLedger = {
      version: 1,
      files: [],
      nameToFiles: {},
      order: [],
      hashVersion: STATEMENT_HASH_VERSION,
      fossilModules: [
        {
          ...first.fossilModules[0],
          hashes: [...first.fossilModules[0].hashes, hashes[4]],
          file: "src/alpha.js"
        },
        {
          ...first.fossilModules[1],
          hashes: first.fossilModules[1].hashes.filter((h) => h !== hashes[4]),
          file: "src/beta.js"
        }
      ]
    };
    await assignFossil(body, hashes, prior);
    const report = placementTrail.report();
    const moved = report.trails.find((e) => e.index === 4);
    assert.ok(moved);
    assert.strictEqual(moved.priorFile, "src/alpha.js");
    assert.notStrictEqual(moved.priorFile, moved.file);
    placementTrail.reset(false);
  });
});

describe("assignFossil — LLM-named mints (exp087)", () => {
  // Andrew, 2026-08-20: mint names should come from the module's CONTENTS,
  // not its first function — `strip-ansi-2.js` for a feature-flag module
  // whose first function happens to be stripAnsi. Scope: MINTS ONLY —
  // matched modules inherit their ledger path verbatim, so the LLM is asked
  // once per new module and stability still comes from the matcher.
  const body = bodyOf(BUNDLE);
  const hashes = body.map(statementHash);

  function priorLedgerKeepingOnly(
    first: Awaited<ReturnType<typeof assignFossil>>
  ) {
    return {
      version: 1,
      files: [],
      nameToFiles: {},
      order: [],
      hashVersion: STATEMENT_HASH_VERSION,
      fossilModules: [first.fossilModules[0]]
    } as StableSplitLedger;
  }

  it("names an unmatched module from the namer; matched modules are never asked", async () => {
    const first = await assignFossil(body, hashes, undefined);
    const prior = priorLedgerKeepingOnly(first);
    const asked: string[][] = [];
    const out = await assignFossil(body, hashes, prior, {
      mintNamer: async (requests) => {
        asked.push(...requests.map((r) => r.bindings));
        return requests.map(() => "tenguFeatureFlags");
      }
    });
    // module 0 matched (inherited path), module 1 minted with the LLM name
    assert.strictEqual(out.assignment[1], first.fossilModules[0].file);
    assert.match(out.assignment[4], /tengu-feature-flags\.js$/);
    assert.strictEqual(asked.length, 1, "exactly one request, for the mint");
    assert.ok(
      asked[0].includes("betaRender"),
      "request carries declared names"
    );
    assert.strictEqual(out.stats.llmNamedMints, 1);
  });

  it("a null or invalid proposal keeps the mechanical stem", async () => {
    const first = await assignFossil(body, hashes, undefined);
    const prior = priorLedgerKeepingOnly(first);
    const out = await assignFossil(body, hashes, prior, {
      mintNamer: async (requests) => requests.map(() => null)
    });
    assert.match(out.assignment[4], /beta-render\.js$/);
    assert.strictEqual(out.stats.llmNamedMints, 0);
  });

  it("a proposal colliding with a taken path falls back to the mechanical stem", async () => {
    const first = await assignFossil(body, hashes, undefined);
    const keptFile = first.fossilModules[0].file; // e.g. src/.../alpha-core.js
    const keptStem = keptFile
      .slice(keptFile.lastIndexOf("/") + 1)
      .replace(/\.js$/, "")
      .replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());
    const prior = priorLedgerKeepingOnly(first);
    const out = await assignFossil(body, hashes, prior, {
      // proposes exactly the inherited module's name
      mintNamer: async (requests) => requests.map(() => keptStem)
    });
    // never a -2: the mechanical stem is free, so it wins over suffixing
    assert.match(out.assignment[4], /beta-render\.js$/);
    assert.ok(!/-2\.js$/.test(out.assignment[4]));
  });
});

describe("assignFossil — mint namer request size (exp087 fix)", () => {
  it("siblings are the mint's FOLDER stems, bounded — not the whole tree", async () => {
    // The first walk shipped every claimed stem (4,800+) as siblings; the
    // namer packs siblings into the prompt and the batch died on model
    // context (744K > 32K), silently falling back to mechanical stems for
    // all 30 mints. Collision safety is claimPath's job — the prompt only
    // needs the names a reader would confuse: the target folder's.
    const body = bodyOf(BUNDLE);
    const hashes = body.map(statementHash);
    const first = await assignFossil(body, hashes, undefined);
    const prior: StableSplitLedger = {
      version: 1,
      files: [],
      nameToFiles: {},
      order: [],
      hashVersion: STATEMENT_HASH_VERSION,
      fossilModules: [
        { ...first.fossilModules[0], file: "src/legacy/kept-name.js" }
      ]
    };
    const seen: string[][] = [];
    await assignFossil(body, hashes, prior, {
      mintNamer: async (requests) => {
        seen.push(...requests.map((r) => r.siblings));
        return requests.map(() => null);
      }
    });
    assert.strictEqual(seen.length, 1);
    assert.ok(
      seen[0].length <= 24,
      `siblings must stay promptable, got ${seen[0].length}`
    );
    // The inherited module lives in src/legacy/ — a different folder, so its
    // stem must NOT be shipped to the model as a sibling.
    assert.ok(
      !seen[0].includes("kept-name"),
      `only the mint's own folder speaks: ${JSON.stringify(seen[0])}`
    );
  });
});
