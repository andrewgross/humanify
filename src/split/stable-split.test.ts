import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  acceptProposedName,
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
