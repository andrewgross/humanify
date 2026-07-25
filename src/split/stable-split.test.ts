import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import type { LoadOrderFacts } from "./load-order.js";
import {
  acceptProposedName,
  alignEmissionOrder,
  reconstructBody,
  type StableSplitLedger,
  stableSplitFromCode
} from "./stable-split.js";
import { STATEMENT_HASH_VERSION, statementHash } from "./statement-hash.js";

/** Tiny clustering knobs so a handful of statements split into a nested tree. */
const SMALL = {
  targetFiles: 12,
  maxLines: 3,
  maxSeg: 2,
  maxTop: 3,
  maxSub: 2,
  window: 4,
  minGap: 1
};

/** Filler declarations so the fixture clears the 50-binding wrapper
 * detection threshold (WRAPPER_IIFE_BINDING_THRESHOLD). */
const PAD_COUNT = 60;
const PADDING = Array.from(
  { length: PAD_COUNT },
  (_, i) => `var padFiller${i} = ${i};`
);

function wrap(bodyLines: string[]): string {
  return [
    "(function (exports, require, module, __filename, __dirname) {",
    ...bodyLines.map((l) => `  ${l}`),
    ...PADDING.map((l) => `  ${l}`),
    "});"
  ].join("\n");
}

const FIXTURE = wrap([
  "function alphaCore(x) {",
  "  return betaHelper(x) + 1;",
  "}",
  "function betaHelper(x) {",
  "  return x * 2;",
  "}",
  "var alphaConfig = alphaCore(1);",
  "function gammaRender(y) {",
  "  return deltaFormat(y);",
  "}",
  "function deltaFormat(y) {",
  "  return String(y);",
  "}",
  "var gammaState = gammaRender(2);",
  "console.log(alphaConfig, gammaState);"
]);

/** Every clustered app path is `src/` plus zero, one, or two folder levels
 * and a file: subfolders collapse into parents when redundant (repeated
 * name, only child, small top group) and a singleton dir hoists its file
 * up — so root files like `src/version.js` are legal output. */
const CLUSTERED_PATH = /^src\/([A-Za-z_$][\w$-]*\/){0,2}[A-Za-z_$][\w$-]*\.js$/;

describe("alignEmissionOrder", () => {
  const V = STATEMENT_HASH_VERSION;
  const led = (order: string[], hashes: string[]): StableSplitLedger => ({
    version: 1,
    files: [...new Set(order)],
    nameToFiles: {},
    order,
    hashes,
    hashVersion: V
  });
  /** A hoisted function declaration: nothing constrains where it sits. */
  const fn = (): LoadOrderFacts => ({
    hoisted: true,
    writes: [],
    reads: [],
    effects: false
  });
  /** An effect-free declaration — movable subject only to its own data edges. */
  const decl = (
    writes: string[] = [],
    reads: string[] = []
  ): LoadOrderFacts => ({
    hoisted: false,
    writes,
    reads,
    effects: false
  });
  /** A statement that can observably do something while the module loads. */
  const barrier = (reads: string[] = []): LoadOrderFacts => ({
    hoisted: false,
    writes: [],
    reads,
    effects: true
  });
  const allFns = (n: number) => Array.from({ length: n }, fn);

  it("orders movable statements to match the prior file order (kills reorder churn)", () => {
    // Fresh bundle order h1,h2,h3; prior emitted them h3,h1,h2. All are movable
    // (function declarations), so emission follows prior -> body indices [2,0,1].
    const perm = alignEmissionOrder(
      ["a", "a", "a"],
      ["h1", "h2", "h3"],
      allFns(3),
      led(["a", "a", "a"], ["h3", "h1", "h2"])
    );
    assert.deepStrictEqual(perm, [2, 0, 1]);
  });

  it("orders EFFECT-FREE DECLARATIONS to prior order too, not just functions", () => {
    // exp038: three independent pure declarations — nothing reads what another
    // writes, nothing has a load-time effect — so every permutation is legal and
    // the prior's order wins. Lever B v2 pinned all three (not function
    // declarations) and left the churn on disk.
    const perm = alignEmissionOrder(
      ["a", "a", "a"],
      ["h1", "h2", "h3"],
      [decl(["a"]), decl(["b"]), decl(["c"])],
      led(["a", "a", "a"], ["h3", "h1", "h2"])
    );
    assert.deepStrictEqual(perm, [2, 0, 1]);
  });

  it("moves hoisted functions freely but never breaks a data dependency", () => {
    // Bundle: h1 assigns `m` at load time and h2 reads it; h3,h4 are functions.
    // Prior reversed everything [h4,h3,h2,h1]. The functions take their prior
    // order (h4 before h3) and may cross anything (hoisted), but h1 must stay
    // ahead of h2 — the reader can never precede the writer.
    const perm = alignEmissionOrder(
      ["a", "a", "a", "a"],
      ["h1", "h2", "h3", "h4"],
      [decl(["m"]), decl(["n"], ["m"]), fn(), fn()],
      led(["a", "a", "a", "a"], ["h4", "h3", "h2", "h1"])
    );
    assert.deepStrictEqual(perm, [3, 2, 0, 1]);
  });

  it("never moves a statement across an effect barrier (the boot-crash rule)", () => {
    // h2 can observably do something at load time (`defineModuleExports(m, …)`),
    // so nothing may cross it — even though the prior wants the order reversed.
    // Reordering here is what crashed the runnable tree in exp037.
    const perm = alignEmissionOrder(
      ["a", "a", "a"],
      ["h1", "h2", "h3"],
      [decl(["m"]), barrier(["m"]), decl(["t"])],
      led(["a", "a", "a"], ["h3", "h2", "h1"])
    );
    assert.deepStrictEqual(perm, [0, 1, 2]);
  });

  it("never repositions an AMBIGUOUS (duplicate-hash) statement — precision guard", () => {
    // hStub appears twice on BOTH sides: two same-shaped stubs (noop / tiny
    // getter) that differ only in their names. The hash cannot tell them apart,
    // so claiming a prior position for them is a GUESS — it teleports their text
    // and manufactures churn where bundle order had none (the +2.3% regression
    // measured on 118->119). Ambiguous statements must anchor where they are;
    // only the unique-hash statements may claim prior positions.
    const perm = alignEmissionOrder(
      ["a", "a", "a", "a"],
      ["hStub", "hStub", "hUniqA", "hUniqB"],
      allFns(4),
      led(["a", "a", "a", "a"], ["hUniqA", "hUniqB", "hStub", "hStub"])
    );
    // Unguarded, the stubs would claim the prior's trailing stub slots and swap
    // to the back ([2,3,0,1]). Guarded, they stay; the uniques are already in
    // prior relative order, so nothing moves.
    assert.deepStrictEqual(perm, [0, 1, 2, 3]);
  });

  it("does nothing when fewer than two statements can be identified across versions", () => {
    // Only h1 has an unambiguous hash on both sides -> no order to align to.
    const perm = alignEmissionOrder(
      ["a", "a", "a"],
      ["h1", "hDup", "hDup"],
      allFns(3),
      led(["a", "a", "a"], ["hDup", "hDup", "h1"])
    );
    assert.deepStrictEqual(perm, [0, 1, 2]);
  });

  it("aligns each file independently and never moves a statement across files", () => {
    const assignment = ["a", "b", "a"];
    const perm = alignEmissionOrder(
      assignment,
      ["h1", "h2", "h3"],
      allFns(3),
      led(["a", "a", "b"], ["h3", "h1", "h2"])
    );
    assert.deepStrictEqual(perm, [2, 1, 0]);
    // invariant: the statement emitted at each slot belongs to that slot's file
    perm.forEach((bodyIdx, slot) => {
      assert.strictEqual(assignment[bodyIdx], assignment[slot]);
    });
  });

  it("is the identity when there is no prior or the hash version mismatches", () => {
    assert.deepStrictEqual(
      alignEmissionOrder(["a", "a"], ["h1", "h2"], allFns(2), undefined),
      [0, 1]
    );
    assert.deepStrictEqual(
      alignEmissionOrder(["a", "a"], ["h1", "h2"], allFns(2), {
        ...led(["a", "a"], ["h2", "h1"]),
        hashVersion: V + 1
      }),
      [0, 1]
    );
  });
});

describe("segmentStem", () => {
  it("falls back to 'stubs', never a minted name, when every binding is banned", async () => {
    const { parseFileAst } = await import("../babel-utils.js");
    const { referenceIndices, segmentStem } = await import("./stable-split.js");
    const ast = parseFileAst(
      "function noopFunction36() {}\nfunction noopFunction73() {}"
    );
    assert.ok(ast);
    const body = ast.program.body;
    const refs = referenceIndices(body);
    assert.strictEqual(segmentStem(body, refs, 0, 2), "stubs");
  });
});

describe("toKebabCase", () => {
  it("normalizes camel/Pascal/acronym/mixed to kebab", async () => {
    const { toKebabCase } = await import("./stable-split.js");
    assert.strictEqual(toKebabCase("authFlow"), "auth-flow");
    assert.strictEqual(toKebabCase("hostnameResolver"), "hostname-resolver");
    assert.strictEqual(toKebabCase("HTTPClient"), "http-client");
    assert.strictEqual(toKebabCase("user-input"), "user-input"); // already kebab
    assert.strictEqual(toKebabCase("app254Initializer"), "app254-initializer");
    assert.strictEqual(toKebabCase("agentColor"), "agent-color");
  });
});

describe("acceptProposedName grammar", () => {
  it("rejects a leading conjunction/article (andTaskPipeline)", () => {
    for (const bad of [
      "andTaskPipeline",
      "orElseHandler",
      "theTaskRunner",
      "aStarSearch",
      "anEntryPoint",
      "butThenWhat"
    ]) {
      assert.strictEqual(
        acceptProposedName(bad),
        null,
        `${bad} must be rejected`
      );
    }
  });
  it("keeps predicate and normal names that merely start with those letters", () => {
    // Tokens that only PREFIX-match a stopword are fine: input, offer, theme,
    // andrew, ... and predicate names (isX) are legit.
    for (const good of [
      "inputHandler",
      "offerManager",
      "themeEngine",
      "isReverseDirection",
      "andrewConfig",
      "toolExecutor"
    ]) {
      assert.ok(acceptProposedName(good), `${good} must be kept`);
    }
  });
});

describe("acceptProposedName", () => {
  it("bans minted numeric-disambiguator stems but keeps known unit tokens", () => {
    for (const bad of [
      "appInitializer17",
      "app254Initializer",
      "appInitializer309",
      "handler42"
    ]) {
      assert.strictEqual(
        acceptProposedName(bad),
        null,
        `${bad} must be banned`
      );
    }
    for (const good of [
      "float64Error",
      "base64Encode",
      "sha256Hasher",
      "utf8Decoder"
    ]) {
      assert.strictEqual(acceptProposedName(good), good, `${good} must pass`);
    }
  });

  it("bans the minted noop/stub families seen in real output", () => {
    // Real leaked dir names from the CC 2.1.89 tree.
    for (const bad of [
      "noopFunction36",
      "noopFunction73",
      "doNothing24",
      "emptyOperation29",
      "noOpHandlers",
      "silentNoop",
      "noOperation",
      "emptyCallback"
    ]) {
      assert.strictEqual(
        acceptProposedName(bad),
        null,
        `${bad} must be banned`
      );
    }
  });

  it("keeps real names that merely contain digits or 'empty'", () => {
    for (const good of [
      "float64Error",
      "base64UrlErrorBuilders",
      "emptyStateRenderer"
    ]) {
      assert.strictEqual(acceptProposedName(good), good, `${good} must pass`);
    }
  });
});

describe("stableSplitFromCode", () => {
  it("returns null for non-wrapper code (caller falls back)", async () => {
    const result = await stableSplitFromCode("var a = 1;\nvar b = 2;", {
      clusterConfig: SMALL
    });
    assert.strictEqual(result, null);
  });

  it("splits fresh: complete, parseable, name-preserving, deterministic", async () => {
    const result = await stableSplitFromCode(FIXTURE, { clusterConfig: SMALL });
    assert.ok(result);
    // Every wrapper-body statement assigned exactly once, in order.
    assert.strictEqual(result.ledger.order.length, 7 + PAD_COUNT);
    // Multiple files across a nested tree.
    assert.ok(
      result.stats.files > 1,
      `expected a split, got ${result.stats.files}`
    );
    for (const [file, content] of result.fileContents) {
      assert.ok(
        parseSync(content, { sourceType: "unambiguous", configFile: false }),
        `${file} must parse`
      );
      assert.match(file, /\.js$/);
    }
    // Declared names all survive somewhere in the tree.
    const all = [...result.fileContents.values()].join("\n");
    for (const name of [
      "alphaCore",
      "betaHelper",
      "alphaConfig",
      "gammaRender",
      "deltaFormat",
      "gammaState"
    ]) {
      assert.ok(all.includes(name), `${name} must be emitted`);
    }
    // Deterministic: same input, same tree.
    const again = await stableSplitFromCode(FIXTURE, { clusterConfig: SMALL });
    assert.ok(again);
    assert.deepStrictEqual(
      [...again.fileContents.keys()],
      [...result.fileContents.keys()]
    );
    assert.deepStrictEqual(again.ledger, result.ledger);
  });

  it("reconstructs the original statement sequence byte-identically", async () => {
    const result = await stableSplitFromCode(FIXTURE, { clusterConfig: SMALL });
    assert.ok(result);
    const rebuilt = reconstructBody(result.fileContents, result.ledger);
    // Oracle: slice the wrapper-body statements straight out of FIXTURE —
    // every statement exactly once, in order, byte-identical.
    const ast = parseSync(FIXTURE, {
      sourceType: "unambiguous",
      configFile: false
    }) as t.File | null;
    assert.ok(ast);
    const first = ast.program.body[0];
    assert.ok(t.isExpressionStatement(first));
    assert.ok(t.isFunctionExpression(first.expression));
    const expected = first.expression.body.body
      .map((s) => FIXTURE.slice(s.start ?? 0, s.end ?? 0))
      .join("\n");
    assert.strictEqual(rebuilt, expected);
  });

  it("realigning to a reversed prior is a PURE reorder — no statement lost or mangled (Lever B safety)", async () => {
    const r1 = await stableSplitFromCode(FIXTURE, { clusterConfig: SMALL });
    assert.ok(r1?.ledger.hashes);
    // A prior identical to r1's but with the whole emission sequence REVERSED —
    // the maximal reshuffle. The split must still reproduce every statement
    // exactly once (concat-equivalence, now order-free) and each file must be a
    // pure reorder of r1's — the load-order safety property (only movable
    // function declarations ever move).
    const prior: StableSplitLedger = {
      ...r1.ledger,
      order: [...r1.ledger.order].reverse(),
      hashes: [...(r1.ledger.hashes as string[])].reverse()
    };
    const r2 = await stableSplitFromCode(FIXTURE, {
      clusterConfig: SMALL,
      prior
    });
    assert.ok(r2, "concat-equivalence must hold (would throw otherwise)");
    assert.deepStrictEqual(
      [...r2.fileContents.keys()].sort(),
      [...r1.fileContents.keys()].sort()
    );
    for (const [file, before] of r1.fileContents) {
      const after = r2.fileContents.get(file);
      assert.ok(after, `${file} must still exist`);
      assert.deepStrictEqual(
        after.split("\n").sort(),
        before.split("\n").sort(),
        `${file} must be a pure reorder`
      );
    }
  });

  it("reconstruct throws when a file is short of the ledger's statements", async () => {
    const result = await stableSplitFromCode(FIXTURE, { clusterConfig: SMALL });
    assert.ok(result);
    const corrupted = new Map(result.fileContents);
    const [first] = corrupted.keys();
    corrupted.set(first, "\n");
    assert.throws(() => reconstructBody(corrupted, result.ledger), /short of/);
  });

  it("reconstruct accepts a file starting with a bare directive-like string", () => {
    const files = new Map([
      ["a.js", '"license: MIT";\nvar one = 1;\n'],
      ["b.js", "var two = 2;\n"]
    ]);
    const ledger: StableSplitLedger = {
      version: 1,
      files: ["a.js", "b.js"],
      nameToFiles: {},
      order: ["a.js", "a.js", "b.js"]
    };
    const rebuilt = reconstructBody(files, ledger);
    assert.strictEqual(rebuilt, '"license: MIT";\nvar one = 1;\nvar two = 2;');
  });

  it("reconstruct throws when a file holds statements beyond the ledger", () => {
    const files = new Map([["a.js", "var one = 1;\nvar extra = 2;\n"]]);
    const ledger: StableSplitLedger = {
      version: 1,
      files: ["a.js"],
      nameToFiles: {},
      order: ["a.js"]
    };
    assert.throws(() => reconstructBody(files, ledger), /beyond the ledger/);
  });

  it("reconstruct throws on files the ledger does not know", () => {
    const files = new Map([
      ["a.js", "var one = 1;\n"],
      ["rogue.js", "var r = 2;\n"]
    ]);
    const ledger: StableSplitLedger = {
      version: 1,
      files: ["a.js"],
      nameToFiles: {},
      order: ["a.js"]
    };
    assert.throws(() => reconstructBody(files, ledger), /beyond the ledger/);
  });

  it("names files/folders after real bindings in a nested tree", async () => {
    const result = await stableSplitFromCode(FIXTURE, { clusterConfig: SMALL });
    assert.ok(result);
    for (const file of result.fileContents.keys()) {
      assert.match(
        file,
        CLUSTERED_PATH,
        `path must be folder/sub/name.js, got ${file}`
      );
    }
  });

  it("inherits the prior file for a matched name, overriding fresh grouping", async () => {
    const prior: StableSplitLedger = {
      version: 1,
      files: ["zed/custom.js"],
      nameToFiles: {
        alphaCore: ["zed/custom.js"],
        betaHelper: ["zed/custom.js"],
        alphaConfig: ["zed/custom.js"],
        gammaRender: ["zed/custom.js"],
        deltaFormat: ["zed/custom.js"],
        gammaState: ["zed/custom.js"]
      },
      order: []
    };
    const result = await stableSplitFromCode(FIXTURE, {
      clusterConfig: SMALL,
      prior
    });
    assert.ok(result);
    // Everything matched -> everything lands in the prior file; the final
    // no-binding console.log + padding follow their neighbor (locality).
    assert.deepStrictEqual([...result.fileContents.keys()], ["zed/custom.js"]);
    assert.strictEqual(result.stats.inherited, 6);
    assert.strictEqual(result.stats.residueLocality, 1 + PAD_COUNT);
  });

  it("maps redeclared names by ordinal on equal counts, abstains on mismatch", async () => {
    const redeclared = wrap([
      "var sharedFlag = 1;",
      "function useOne(x) {",
      "  return sharedFlag + x;",
      "}",
      "var sharedFlag = 2;",
      "function useTwo(x) {",
      "  return sharedFlag * x;",
      "}"
    ]);
    const prior: StableSplitLedger = {
      version: 1,
      files: ["one/first.js", "two/second.js"],
      nameToFiles: {
        sharedFlag: ["one/first.js", "two/second.js"],
        useOne: ["one/first.js"],
        useTwo: ["two/second.js"]
      },
      order: []
    };
    const result = await stableSplitFromCode(redeclared, {
      clusterConfig: SMALL,
      prior
    });
    assert.ok(result);
    const first = result.fileContents.get("one/first.js") ?? "";
    const second = result.fileContents.get("two/second.js") ?? "";
    assert.match(first, /sharedFlag = 1/);
    assert.match(second, /sharedFlag = 2/);
    assert.strictEqual(result.stats.inheritedViaOrdinal, 2);

    const mismatch: StableSplitLedger = {
      ...prior,
      nameToFiles: {
        ...prior.nameToFiles,
        sharedFlag: ["one/first.js", "two/second.js", "one/first.js"]
      }
    };
    const fallback = await stableSplitFromCode(redeclared, {
      clusterConfig: SMALL,
      prior: mismatch
    });
    assert.ok(fallback);
    assert.strictEqual(fallback.stats.inheritedViaOrdinal, 0);
  });

  it("separates ledger IDENTITY (hashes) from ledger LAYOUT (emitHashes)", async () => {
    // `hashes` is the hash-inheritance tier's identity key and must be a pure
    // function of the bundle — bundle order, stable no matter how the file is
    // laid out. `emitHashes` is the layout record the next release aligns to.
    // Lever B conflated them by writing the permuted array into `hashes`, which
    // made the field path-dependent: it recorded a layout that was itself
    // derived from the previous release's layout, so a self-hop could shift 44
    // of 35,903 entries while the emitted tree was byte-identical.
    const A = "one/first.js";
    const B = "two/second.js";
    const src = wrap([
      "var sharedFlag = 1;",
      "var sharedFlag = 2;",
      "var alphaTwo = 3;",
      "var betaTwo = 4;"
    ]);
    const ast = parseSync(src, { sourceType: "unambiguous" });
    assert.ok(ast && ast.type === "File");
    let bodyStmts: t.Statement[] = [];
    for (const s of ast.program.body) {
      if (t.isExpressionStatement(s) && t.isFunctionExpression(s.expression)) {
        bodyStmts = s.expression.body.body;
      }
    }
    const h = (i: number) => statementHash(bodyStmts[i]);
    const prior: StableSplitLedger = {
      version: 1,
      files: [A, B],
      nameToFiles: { sharedFlag: [A, B], alphaTwo: [A], betaTwo: [B] },
      order: [A, A, B, B],
      emitHashes: [h(2), h(0), h(1), h(3)],
      hashes: [h(0), h(2), h(1), h(3)],
      hashVersion: STATEMENT_HASH_VERSION
    };
    const result = await stableSplitFromCode(src, {
      clusterConfig: SMALL,
      prior
    });
    assert.ok(result);
    const { ledger } = result;

    // identity: bundle order, so it equals the bundle's own hash sequence
    assert.deepStrictEqual(
      ledger.hashes,
      bodyStmts.map((s) => statementHash(s)).slice(0, ledger.hashes?.length),
      "hashes must be the bundle-order identity key"
    );
    // layout: a permutation of the same multiset, and here actually reordered
    assert.ok(ledger.emitHashes, "emitHashes must be recorded");
    assert.deepStrictEqual(
      [...(ledger.emitHashes as string[])].sort(),
      [...(ledger.hashes as string[])].sort(),
      "emitHashes must be a permutation of hashes"
    );
    assert.notDeepStrictEqual(
      ledger.emitHashes,
      ledger.hashes,
      "fixture must actually trigger a reorder"
    );
  });

  it("keeps nameToFiles in BUNDLE order — emit order must not move assignments", async () => {
    // The ledger mixes two kinds of data: IDENTITY (what inherits next release)
    // and LAYOUT (where statements were emitted). `nameToFiles` is identity: for
    // a name declared in several files, `voteFor` picks `files[ordinal]`, so the
    // ORDER of that list decides where the k-th redeclaration lands next time.
    // Building it from the emitted body made a within-file reorder flip the
    // cross-file interleaving and hand the ordinal a different file — 33 of
    // 35,903 statements changed file when 2.1.216 was re-split against its own
    // output, breaking the self-hop idempotence invariant.
    const A = "one/first.js";
    const B = "two/second.js";
    const src = wrap([
      "var sharedFlag = 1;", // -> A (ordinal 0)
      "var sharedFlag = 2;", // -> B (ordinal 1)
      "var alphaTwo = 3;", // -> A
      "var betaTwo = 4;" // -> B
    ]);
    // Real hashes of the wrapper's first four statements, so the aligner has a
    // prior sequence it can actually act on.
    const ast = parseSync(src, { sourceType: "unambiguous" });
    assert.ok(ast && ast.type === "File");
    let bodyStmts: t.Statement[] = [];
    for (const s of ast.program.body) {
      if (t.isExpressionStatement(s) && t.isFunctionExpression(s.expression)) {
        bodyStmts = s.expression.body.body;
      }
    }
    assert.ok(bodyStmts.length > 4);
    const h = (i: number) => statementHash(bodyStmts[i]);
    const prior: StableSplitLedger = {
      version: 1,
      files: [A, B],
      nameToFiles: { sharedFlag: [A, B], alphaTwo: [A], betaTwo: [B] },
      // A emitted alphaTwo BEFORE sharedFlag last release, so aligning to it
      // swaps A's two statements and flips their order relative to B's.
      order: [A, A, B, B],
      hashes: [h(2), h(0), h(1), h(3)],
      hashVersion: STATEMENT_HASH_VERSION
    };
    const run = async () =>
      await stableSplitFromCode(src, { clusterConfig: SMALL, prior });

    const aligned = await run();
    process.env.HUMANIFY_NO_EMIT_ALIGN = "1";
    const plain = await run();
    process.env.HUMANIFY_NO_EMIT_ALIGN = undefined;
    delete process.env.HUMANIFY_NO_EMIT_ALIGN;
    assert.ok(aligned && plain);

    // The aligner must actually have done something, or this proves nothing.
    assert.notDeepStrictEqual(
      aligned.fileContents.get(A),
      plain.fileContents.get(A),
      "fixture must trigger a reorder"
    );
    // ...but identity data is layout-independent.
    assert.deepStrictEqual(
      aligned.ledger.nameToFiles,
      plain.ledger.nameToFiles,
      "nameToFiles must not depend on emit order"
    );
    assert.deepStrictEqual(aligned.ledger.nameToFiles.sharedFlag, [A, B]);
  });

  describe("binding-identity tier (Lever B)", () => {
    // A binding that was RENAMED and whose content CHANGED misses both the
    // hash tier (content differs) and the name-vote tier (name flipped), so
    // it would fall to locality and follow its neighbor into the WRONG file.
    // The fingerprint matcher's new->prior identity lets it inherit the file
    // its matched prior counterpart lived in instead.
    const renamedBody = wrap([
      "function helperOne(x) {",
      "  return x + 1;",
      "}",
      "function helperTwo(x) {",
      "  return x + 2;",
      "}",
      "function taskRouter(cmd) {",
      "  return dispatchTable[cmd](cmd, extraArg, moreArgs);",
      "}",
      "function helperThree(x) {",
      "  return x + 3;",
      "}"
    ]);
    // Everything lived in core/main.js last release except the dispatcher,
    // which lived in its own file. No prior hashes -> hash tier stays off.
    const prior: StableSplitLedger = {
      version: 1,
      files: ["core/main.js", "tools/dispatch.js"],
      nameToFiles: {
        helperOne: ["core/main.js"],
        helperTwo: ["core/main.js"],
        commandDispatcher: ["tools/dispatch.js"],
        helperThree: ["core/main.js"]
      },
      order: []
    };

    it("without an identity map, the renamed binding drifts to its neighbor's file", async () => {
      const result = await stableSplitFromCode(renamedBody, {
        clusterConfig: SMALL,
        prior
      });
      assert.ok(result);
      // taskRouter has no name vote and no hash -> locality -> follows
      // helperTwo into core/main.js (the false-positive B removes).
      const core = result.fileContents.get("core/main.js") ?? "";
      assert.match(
        core,
        /dispatchTable/,
        "drifts into core/main.js by locality"
      );
      assert.strictEqual(result.stats.inheritedViaIdentity, 0);
    });

    it("with the identity map, the renamed binding inherits its matched prior file", async () => {
      const result = await stableSplitFromCode(renamedBody, {
        clusterConfig: SMALL,
        prior,
        priorMatchMap: new Map([["taskRouter", "commandDispatcher"]])
      });
      assert.ok(result);
      const dispatch = result.fileContents.get("tools/dispatch.js") ?? "";
      assert.match(
        dispatch,
        /dispatchTable/,
        "taskRouter inherits commandDispatcher's file via binding identity"
      );
      const core = result.fileContents.get("core/main.js") ?? "";
      assert.doesNotMatch(core, /dispatchTable/);
      assert.strictEqual(result.stats.inheritedViaIdentity, 1);
    });

    it("abstains to locality when the matched prior name spans multiple files", async () => {
      // commandDispatcher lived in TWO files last release -> not unanimous ->
      // the identity tier must refuse and fall back to locality.
      const split: StableSplitLedger = {
        ...prior,
        nameToFiles: {
          ...prior.nameToFiles,
          commandDispatcher: ["tools/dispatch.js", "core/main.js"]
        }
      };
      const result = await stableSplitFromCode(renamedBody, {
        clusterConfig: SMALL,
        prior: split,
        priorMatchMap: new Map([["taskRouter", "commandDispatcher"]])
      });
      assert.ok(result);
      assert.strictEqual(
        result.stats.inheritedViaIdentity,
        0,
        "ambiguous prior file must abstain, never guess"
      );
    });

    it("leaves assignments byte-identical when no identity map is given", async () => {
      const without = await stableSplitFromCode(renamedBody, {
        clusterConfig: SMALL,
        prior
      });
      const emptyMap = await stableSplitFromCode(renamedBody, {
        clusterConfig: SMALL,
        prior,
        priorMatchMap: new Map()
      });
      assert.ok(without && emptyMap);
      assert.deepStrictEqual(
        [...emptyMap.fileContents.keys()].sort(),
        [...without.fileContents.keys()].sort(),
        "an empty identity map must not change any assignment"
      );
      assert.strictEqual(emptyMap.stats.inheritedViaIdentity, 0);
    });
  });

  describe("binding-identity preempt tier (Lever A)", () => {
    // A binding renamed TO a name that already exists (unanimously) in the
    // prior ledger as a DIFFERENT binding gets a confident but WRONG name-vote
    // to that other binding's file (a "collision magnet"). Its matched prior
    // identity is the right file; the preempt tier overrides the collision.
    const collisionBody = wrap([
      "function helperOne(x) {",
      "  return x + 1;",
      "}",
      "function dataProcessor(cmd) {",
      "  return dispatchTable[cmd](cmd, extraArg, moreArgs);",
      "}",
      "function helperThree(x) {",
      "  return x + 3;",
      "}"
    ]);
    // dataProcessor is a magnet in utils/helpers.js (a different binding last
    // release); this binding's true prior identity is commandDispatcher in
    // tools/dispatch.js. helperOne/helperThree lived in core/main.js.
    const priorCollision: StableSplitLedger = {
      version: 1,
      files: ["core/main.js", "tools/dispatch.js", "utils/helpers.js"],
      nameToFiles: {
        helperOne: ["core/main.js"],
        helperThree: ["core/main.js"],
        dataProcessor: ["utils/helpers.js"],
        commandDispatcher: ["tools/dispatch.js"]
      },
      order: []
    };

    it("without the map, the collision magnet mis-files the binding", async () => {
      const result = await stableSplitFromCode(collisionBody, {
        clusterConfig: SMALL,
        prior: priorCollision
      });
      assert.ok(result);
      // dataProcessor name-votes for its magnet's file — the churn A removes.
      const helpers = result.fileContents.get("utils/helpers.js") ?? "";
      assert.match(helpers, /dispatchTable/, "drifts to the magnet's file");
      assert.strictEqual(result.stats.inheritedViaIdentityPreempt, 0);
    });

    it("preempts a disagreeing name-vote, inheriting the matched prior file", async () => {
      const result = await stableSplitFromCode(collisionBody, {
        clusterConfig: SMALL,
        prior: priorCollision,
        priorMatchMap: new Map([["dataProcessor", "commandDispatcher"]])
      });
      assert.ok(result);
      const dispatch = result.fileContents.get("tools/dispatch.js") ?? "";
      assert.match(
        dispatch,
        /dispatchTable/,
        "the identity home overrides the collision name-vote"
      );
      const helpers = result.fileContents.get("utils/helpers.js") ?? "";
      assert.doesNotMatch(helpers, /dispatchTable/);
      assert.strictEqual(result.stats.inheritedViaIdentityPreempt, 1);
    });

    it("abstains for a GENERIC new name — the least-reliable match", async () => {
      // Same collision, but the binding's new name is minted (noop4): the
      // match is untrustworthy, so the preempt refuses and the name-vote wins.
      const genericBody = wrap([
        "function helperOne(x) {",
        "  return x + 1;",
        "}",
        "function noop4(cmd) {",
        "  return dispatchTable[cmd](cmd, extraArg, moreArgs);",
        "}",
        "function helperThree(x) {",
        "  return x + 3;",
        "}"
      ]);
      const genericPrior: StableSplitLedger = {
        ...priorCollision,
        nameToFiles: {
          helperOne: ["core/main.js"],
          helperThree: ["core/main.js"],
          noop4: ["utils/helpers.js"],
          commandDispatcher: ["tools/dispatch.js"]
        }
      };
      const result = await stableSplitFromCode(genericBody, {
        clusterConfig: SMALL,
        prior: genericPrior,
        priorMatchMap: new Map([["noop4", "commandDispatcher"]])
      });
      assert.ok(result);
      assert.strictEqual(
        result.stats.inheritedViaIdentityPreempt,
        0,
        "a generic new name must never preempt"
      );
      const helpers = result.fileContents.get("utils/helpers.js") ?? "";
      assert.match(helpers, /dispatchTable/, "the name-vote stands");
    });

    it("does not fire when identity AGREES with the name-vote (no-op)", async () => {
      // dataProcessor's magnet is the SAME file as its matched prior identity,
      // so the name-vote is already right — the preempt must not double-count.
      const agreeingPrior: StableSplitLedger = {
        ...priorCollision,
        nameToFiles: {
          helperOne: ["core/main.js"],
          helperThree: ["core/main.js"],
          dataProcessor: ["tools/dispatch.js"],
          commandDispatcher: ["tools/dispatch.js"]
        }
      };
      const result = await stableSplitFromCode(collisionBody, {
        clusterConfig: SMALL,
        prior: agreeingPrior,
        priorMatchMap: new Map([["dataProcessor", "commandDispatcher"]])
      });
      assert.ok(result);
      assert.strictEqual(result.stats.inheritedViaIdentityPreempt, 0);
      const dispatch = result.fileContents.get("tools/dispatch.js") ?? "";
      assert.match(dispatch, /dispatchTable/);
    });
  });

  it("namer polishes NEW file/folder names, collapsing repeated levels", async () => {
    // The namer gives every folder the same name, so top === sub for every
    // segment; the redundant middle level must collapse to
    // src/apiClient/<file>.js, never src/apiClient/apiClient/<file>.js.
    const requests: string[] = [];
    const result = await stableSplitFromCode(FIXTURE, {
      clusterConfig: SMALL,
      namer: async (batch) =>
        batch.map((request) => {
          requests.push(`${request.kind}:${request.mechanicalStem}`);
          return request.kind === "folder" ? "apiClient" : "requestHandler";
        })
    });
    assert.ok(result);
    const stem = (s: string) => s.replace(/(-\d+)?(\.js)?$/, "");
    for (const p of result.fileContents.keys()) {
      const parts = p.split("/");
      assert.ok(
        parts.length === 2 || parts.length === 3,
        `repeated level collapsed to src/[folder/]file, got ${p}`
      );
      assert.strictEqual(parts[0], "src", `app code under src/, got ${p}`);
      if (parts.length === 3) {
        assert.strictEqual(
          stem(parts[1]),
          "api-client",
          `folder polished (kebab), got ${p}`
        );
      }
      assert.strictEqual(
        stem(parts[parts.length - 1]),
        "request-handler",
        `file polished (kebab), got ${p}`
      );
    }
    assert.ok(requests.some((r) => r.startsWith("file:")));
    assert.ok(requests.some((r) => r.startsWith("folder:")));
  });

  it("rejects generic/invalid namer proposals, keeping the mechanical stem", async () => {
    const result = await stableSplitFromCode(FIXTURE, {
      clusterConfig: SMALL,
      namer: async (batch) =>
        batch.map((request) =>
          request.kind === "folder" ? "utils" : "no spaces allowed"
        )
    });
    assert.ok(result);
    for (const p of result.fileContents.keys()) {
      assert.match(
        p,
        CLUSTERED_PATH,
        `rejected proposals keep valid stems, got ${p}`
      );
      assert.ok(
        !p.split("/").includes("utils"),
        `generic name rejected, got ${p}`
      );
    }
  });

  it("normalizes namer proposals to kebab-case for a consistent tree", async () => {
    const result = await stableSplitFromCode(FIXTURE, {
      clusterConfig: SMALL,
      namer: async (batch) =>
        batch.map((request) =>
          request.kind === "folder" ? "messageRendering" : "handleUserInput"
        )
    });
    assert.ok(result);
    const paths = [...result.fileContents.keys()];
    assert.ok(
      paths.some((p) => p.startsWith("src/message-rendering/")),
      `camelCase folder must normalize to kebab, got ${paths.join(", ")}`
    );
    assert.ok(
      paths.some((p) => stemOf(p) === "handle-user-input"),
      `camelCase file must normalize to kebab, got ${paths.join(", ")}`
    );
  });

  it("never calls the namer on the prior-carried path (renames are churn)", async () => {
    const fresh = await stableSplitFromCode(FIXTURE, { clusterConfig: SMALL });
    assert.ok(fresh);
    let called = 0;
    const result = await stableSplitFromCode(FIXTURE, {
      clusterConfig: SMALL,
      prior: fresh.ledger,
      namer: async (batch) => {
        called++;
        return batch.map(() => "shouldNeverAppear");
      }
    });
    assert.ok(result);
    assert.strictEqual(called, 0);
    assert.ok(
      ![...result.fileContents.keys()].some((p) =>
        p.includes("shouldNeverAppear")
      )
    );
  });

  it("sends disagreeing multi-name statements to their neighbor's file", async () => {
    const multi = wrap([
      "function anchorFn(x) {",
      "  return x;",
      "}",
      "var { pFlag, qFlag } = anchorFn(1);",
      "console.log(pFlag, qFlag);"
    ]);
    const prior: StableSplitLedger = {
      version: 1,
      files: ["a/a.js", "b/b.js"],
      nameToFiles: {
        anchorFn: ["a/a.js"],
        pFlag: ["a/a.js"],
        qFlag: ["b/b.js"]
      },
      order: []
    };
    const result = await stableSplitFromCode(multi, {
      clusterConfig: SMALL,
      prior
    });
    assert.ok(result);
    assert.strictEqual(result.stats.conflictDisagree, 1);
    assert.match(result.fileContents.get("a/a.js") ?? "", /pFlag, qFlag/);
  });
});

describe("hash-keyed inheritance", () => {
  // The walk's measured failure mode (85->86: upstream reordered 35% of the
  // bundle): a statement whose declared names ALL flipped (LLM rename noise)
  // has no name vote, and neighbor-following under a reorder scatters it
  // into whichever file its NEW neighbor lives in — byte-identical code
  // moving files. The rename-invariant statement hash must inherit the
  // prior file regardless of order and names.
  const V1_APP = [
    "function alphaCore(x) {",
    '  return betaHelper(x) + "alpha-marker";',
    "}",
    "function betaHelper(x) {",
    '  return x * 2 + "beta-marker".length;',
    "}",
    "function gammaRender(y) {",
    '  return "gamma-marker" + deltaFormat(y);',
    "}",
    "function deltaFormat(y) {",
    '  return String(y) + "delta-marker";',
    "}"
  ];
  // Same four statements: every identifier renamed, order REVERSED.
  const V2_APP = [
    "function iotaFormat(q) {",
    '  return String(q) + "delta-marker";',
    "}",
    "function thetaRender(q) {",
    '  return "gamma-marker" + iotaFormat(q);',
    "}",
    "function etaHelper(p) {",
    '  return p * 2 + "beta-marker".length;',
    "}",
    "function zetaCore(p) {",
    '  return etaHelper(p) + "alpha-marker";',
    "}"
  ];
  const MARKERS = [
    "alpha-marker",
    "beta-marker",
    "gamma-marker",
    "delta-marker"
  ];

  function fileOf(
    result: { fileContents: Map<string, string> },
    marker: string
  ): string {
    const hits = [...result.fileContents.entries()]
      .filter(([, content]) => content.includes(marker))
      .map(([file]) => file);
    assert.strictEqual(hits.length, 1, `${marker} must be in exactly one file`);
    return hits[0];
  }

  it("keeps renamed+reordered statements in their prior files", async () => {
    const v1 = await stableSplitFromCode(wrap(V1_APP), {
      clusterConfig: SMALL
    });
    assert.ok(v1);
    // Fixture guard: the four functions span >= 2 files, so a reorder CAN
    // scatter them — otherwise the test is vacuous.
    const v1Files = new Set(MARKERS.map((m) => fileOf(v1, m)));
    assert.ok(v1Files.size >= 2, "fixture must spread markers over 2+ files");

    const v2 = await stableSplitFromCode(wrap(V2_APP), {
      clusterConfig: SMALL,
      prior: v1.ledger
    });
    assert.ok(v2);
    for (const marker of MARKERS) {
      assert.strictEqual(
        fileOf(v2, marker),
        fileOf(v1, marker),
        `${marker} statement must stay in its prior file across rename+reorder`
      );
    }
    assert.ok(
      v2.stats.inheritedViaHash >= MARKERS.length,
      "the four moved statements must be hash-inherited"
    );
  });

  it("stays off (stats zero) when the prior ledger has no hashes", async () => {
    const v1 = await stableSplitFromCode(wrap(V1_APP), {
      clusterConfig: SMALL
    });
    assert.ok(v1);
    const { hashes: _h, hashVersion: _v, ...stripped } = v1.ledger;
    const v2 = await stableSplitFromCode(wrap(V2_APP), {
      clusterConfig: SMALL,
      prior: stripped
    });
    assert.ok(v2);
    assert.strictEqual(v2.stats.inheritedViaHash, 0);
  });

  it("writes hashes on both regimes so lineage chains inherit by content", async () => {
    const fresh = await stableSplitFromCode(wrap(V1_APP), {
      clusterConfig: SMALL
    });
    assert.ok(fresh);
    assert.strictEqual(fresh.ledger.hashVersion, STATEMENT_HASH_VERSION);
    assert.strictEqual(fresh.ledger.hashes?.length, fresh.ledger.order.length);
    const carried = await stableSplitFromCode(wrap(V2_APP), {
      clusterConfig: SMALL,
      prior: fresh.ledger
    });
    assert.ok(carried);
    assert.strictEqual(carried.ledger.hashVersion, STATEMENT_HASH_VERSION);
    assert.strictEqual(
      carried.ledger.hashes?.length,
      carried.ledger.order.length
    );
  });

  /** Wrapper-body statements of a fixture, for hand-built prior ledgers. */
  function bodyOf(code: string): t.Statement[] {
    const ast = parseSync(code, {
      sourceType: "unambiguous",
      configFile: false
    }) as t.File | null;
    assert.ok(ast);
    const first = ast.program.body[0];
    assert.ok(t.isExpressionStatement(first));
    assert.ok(t.isFunctionExpression(first.expression));
    return first.expression.body.body;
  }

  // Hand-built priors below: anchor lives in b/b.js, probes lived in
  // a/a.js. Probes are bare calls (no declared names — no name votes) so
  // the hash tier's count rules alone decide their fate; the neighbor
  // fallback would put them in the anchor's b/b.js.
  const ANCHOR = 'function anchorFn() { return "anchor-mark"; }';
  const PROBE = 'fireProbe("probe-mark");';
  /** Same probe content under a renamed callee — hash-equal by design. */
  const PROBE_RENAMED = 'firePulse("probe-mark");';

  function probePrior(probeFiles: string[]): StableSplitLedger {
    const stmts = bodyOf(wrap([ANCHOR, ...probeFiles.map(() => PROBE)])).slice(
      0,
      1 + probeFiles.length
    );
    return {
      version: 1,
      files: [...new Set(["b/b.js", ...probeFiles])].sort(),
      nameToFiles: { anchorFn: ["b/b.js"] },
      order: ["b/b.js", ...probeFiles],
      hashes: stmts.map(statementHash),
      hashVersion: STATEMENT_HASH_VERSION
    };
  }

  it("equal-count unanimous duplicates inherit their prior file", async () => {
    const result = await stableSplitFromCode(
      wrap([ANCHOR, PROBE_RENAMED, PROBE_RENAMED]),
      { clusterConfig: SMALL, prior: probePrior(["a/a.js", "a/a.js"]) }
    );
    assert.ok(result);
    assert.match(result.fileContents.get("a/a.js") ?? "", /probe-mark/);
    assert.doesNotMatch(result.fileContents.get("b/b.js") ?? "", /probe-mark/);
    // anchor + both probes
    assert.strictEqual(result.stats.inheritedViaHash, 3);
  });

  it("unequal counts refuse the hash vote (no teleporting new duplicates)", async () => {
    // Prior had TWO probes in a/a.js; this release has THREE. All three
    // must follow their neighbor (b/b.js), never get pulled into the old
    // cluster on a collided short-statement hash.
    const result = await stableSplitFromCode(
      wrap([ANCHOR, PROBE_RENAMED, PROBE_RENAMED, PROBE_RENAMED]),
      { clusterConfig: SMALL, prior: probePrior(["a/a.js", "a/a.js"]) }
    );
    assert.ok(result);
    assert.doesNotMatch(result.fileContents.get("a/a.js") ?? "", /probe-mark/);
    assert.match(result.fileContents.get("b/b.js") ?? "", /probe-mark/);
    assert.strictEqual(result.stats.inheritedViaHash, 1); // anchor only
  });

  it("equal counts split across prior files abstain (precision over recall)", async () => {
    const result = await stableSplitFromCode(
      wrap([ANCHOR, PROBE_RENAMED, PROBE_RENAMED]),
      { clusterConfig: SMALL, prior: probePrior(["a/a.js", "c/c.js"]) }
    );
    assert.ok(result);
    assert.match(result.fileContents.get("b/b.js") ?? "", /probe-mark/);
    assert.strictEqual(result.stats.inheritedViaHash, 1); // anchor only
  });

  it("content identity outranks a name vote", async () => {
    // The statement's CONTENT lived in a/a.js (under an old name); its NEW
    // name points at b/b.js. Content wins: a/a.js's diff becomes zero and
    // b/b.js loses nothing — the smaller diff on both sides.
    const V1 = 'function oldName() { return "content-c"; }';
    const V2 = 'function newName() { return "content-c"; }';
    const prior: StableSplitLedger = {
      version: 1,
      files: ["a/a.js", "b/b.js"],
      nameToFiles: { oldName: ["a/a.js"], newName: ["b/b.js"] },
      order: ["a/a.js"],
      hashes: bodyOf(wrap([V1]))
        .slice(0, 1)
        .map(statementHash),
      hashVersion: STATEMENT_HASH_VERSION
    };
    const result = await stableSplitFromCode(wrap([V2]), {
      clusterConfig: SMALL,
      prior
    });
    assert.ok(result);
    assert.match(result.fileContents.get("a/a.js") ?? "", /content-c/);
    assert.doesNotMatch(result.fileContents.get("b/b.js") ?? "", /content-c/);
  });
});

function stemOf(path: string): string {
  const file = path.split("/").pop() ?? "";
  return file.replace(/(-\d+)?\.js$/, "");
}
