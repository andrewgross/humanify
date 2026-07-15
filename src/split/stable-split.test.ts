import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  reconstructBody,
  type StableSplitLedger,
  stableSplitFromCode
} from "./stable-split.js";

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

/** Every clustered app path is `src/folder/subfolder/file.js`, or the
 * collapsed `src/folder/file.js` when a subfolder merely repeats its
 * parent's name (the src/ prefix plus one OR two folder levels). */
const CLUSTERED_PATH =
  /^src\/[A-Za-z_$][\w$-]*(\/[A-Za-z_$][\w$-]*)?\/[A-Za-z_$][\w$-]*\.js$/;

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
      namer: async (request) => {
        requests.push(`${request.kind}:${request.mechanicalStem}`);
        return request.kind === "folder" ? "apiClient" : "requestHandler";
      }
    });
    assert.ok(result);
    const stem = (s: string) => s.replace(/(-\d+)?(\.js)?$/, "");
    for (const p of result.fileContents.keys()) {
      const parts = p.split("/");
      assert.strictEqual(
        parts.length,
        3,
        `repeated level collapsed to src/folder/file, got ${p}`
      );
      const [prefix, top, file] = parts;
      assert.strictEqual(prefix, "src", `app code under src/, got ${p}`);
      assert.strictEqual(stem(top), "apiClient", `folder polished, got ${p}`);
      assert.strictEqual(
        stem(file),
        "requestHandler",
        `file polished, got ${p}`
      );
    }
    assert.ok(requests.some((r) => r.startsWith("file:")));
    assert.ok(requests.some((r) => r.startsWith("folder:")));
  });

  it("rejects generic/invalid namer proposals, keeping the mechanical stem", async () => {
    const result = await stableSplitFromCode(FIXTURE, {
      clusterConfig: SMALL,
      namer: async (request) =>
        request.kind === "folder" ? "utils" : "no spaces allowed"
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

  it("normalizes namer proposals to camelCase for a consistent tree", async () => {
    const result = await stableSplitFromCode(FIXTURE, {
      clusterConfig: SMALL,
      namer: async (request) =>
        request.kind === "folder" ? "message-rendering" : "handle-user-input"
    });
    assert.ok(result);
    const paths = [...result.fileContents.keys()];
    assert.ok(
      paths.some((p) => p.startsWith("src/messageRendering/")),
      `kebab folder must normalize to camelCase, got ${paths.join(", ")}`
    );
    assert.ok(
      paths.some((p) => stemOf(p) === "handleUserInput"),
      `kebab file must normalize to camelCase, got ${paths.join(", ")}`
    );
  });

  it("never calls the namer on the prior-carried path (renames are churn)", async () => {
    const fresh = await stableSplitFromCode(FIXTURE, { clusterConfig: SMALL });
    assert.ok(fresh);
    let called = 0;
    const result = await stableSplitFromCode(FIXTURE, {
      clusterConfig: SMALL,
      prior: fresh.ledger,
      namer: async () => {
        called++;
        return "shouldNeverAppear";
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

function stemOf(path: string): string {
  const file = path.split("/").pop() ?? "";
  return file.replace(/(-\d+)?\.js$/, "");
}
