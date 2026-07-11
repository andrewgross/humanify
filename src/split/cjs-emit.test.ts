import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import { emitRunnableCjs } from "./cjs-emit.js";
import type { StableSplitLedger } from "./stable-split.js";

/** Two files, cross-file read and a cross-file write, so both accessor
 * forms (get-only and get/set) and the reference rewrite are exercised. */
function fixture(): { code: string; ledger: StableSplitLedger } {
  const body = [
    "function sharedHelper(x) {", // decl in a.js, read from b.js
    "  return x + 1;",
    "}",
    "var counter = 0;", // decl in a.js, WRITTEN from b.js
    "function useHelper(y) {", // decl in b.js, reads sharedHelper + writes counter
    "  counter = counter + 1;",
    "  return sharedHelper(y);",
    "}",
    "function localOnly(z) {", // decl in b.js, no cross-file refs
    "  var counter = z;", // shadows the module `counter` — must NOT rewrite
    "  return counter * 2;",
    "}"
  ];
  const code = [
    "(function (exports, require, module, __filename, __dirname) {",
    ...body.map((l) => `  ${l}`),
    "});"
  ].join("\n");
  // a.js: sharedHelper (stmt 0), counter (stmt 1). b.js: useHelper (2),
  // localOnly (3).
  const order = ["core/a.js", "core/a.js", "core/b.js", "core/b.js"];
  const ledger: StableSplitLedger = {
    version: 1,
    files: ["core/a.js", "core/b.js"],
    nameToFiles: {
      sharedHelper: ["core/a.js"],
      counter: ["core/a.js"],
      useHelper: ["core/b.js"],
      localOnly: ["core/b.js"]
    },
    order
  };
  return { code, ledger };
}

describe("emitRunnableCjs", () => {
  it("exports cross-file bindings as live accessors and rewrites references", () => {
    const { code, ledger } = fixture();
    const files = emitRunnableCjs(code, ledger);
    assert.ok(files);
    const a = files.get("core/a.js") ?? "";
    const b = files.get("core/b.js") ?? "";

    // a.js exports sharedHelper (get-only) and counter (get + set).
    assert.match(
      a,
      /Object\.defineProperty\(module\.exports,\s*"sharedHelper",\s*\{\s*get:/,
      `a.js must export sharedHelper as an accessor:\n${a}`
    );
    assert.match(
      a,
      /"counter",[^}]*set:\s*v\s*=>/,
      `counter is written cross-file → must have a setter:\n${a}`
    );
    assert.doesNotMatch(
      a,
      /"sharedHelper",[^}]*set:/,
      "sharedHelper is never written cross-file → get-only"
    );

    // b.js requires a.js once and reaches across via the namespace.
    assert.match(
      b,
      /const \S+ = require\("\.\/a\.js"\);/,
      `b.js must require a.js:\n${b}`
    );
    assert.match(b, /\.sharedHelper\(y\)/, `cross-file read rewritten:\n${b}`);
    assert.match(b, /\.counter =/, `cross-file write rewritten:\n${b}`);
  });

  it("does not rewrite a shadowing local of the same name", () => {
    const { code, ledger } = fixture();
    const files = emitRunnableCjs(code, ledger);
    assert.ok(files);
    const b = files.get("core/b.js") ?? "";
    // localOnly's inner `var counter = z; return counter * 2;` must stay
    // bare — it is a local, not the module binding.
    assert.match(
      b,
      /var counter = z;/,
      `local counter must be untouched:\n${b}`
    );
    assert.match(
      b,
      /return counter \* 2;/,
      `local counter read untouched:\n${b}`
    );
  });

  it("emits files that parse", () => {
    const { code, ledger } = fixture();
    const files = emitRunnableCjs(code, ledger);
    assert.ok(files);
    for (const [file, content] of files) {
      assert.ok(
        parseSync(content, { sourceType: "unambiguous", configFile: false }),
        `${file} must parse:\n${content}`
      );
    }
  });

  it("returns null-free content for every ledger file", () => {
    const { code, ledger } = fixture();
    const files = emitRunnableCjs(code, ledger);
    assert.ok(files);
    for (const f of ledger.files) {
      assert.ok(files.has(f), `missing file ${f}`);
    }
  });
});
