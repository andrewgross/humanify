import assert from "node:assert";
import { describe, it } from "node:test";
import { detectBrowserify } from "./browserify.js";
import { detectBunBundler } from "./bun.js";
import { detectEsbuild } from "./esbuild.js";
import {
  detectBunMinifier,
  detectEsbuildMinifier,
  detectSwcMinifier,
  detectTerser
} from "./minifier.js";
import { detectParcel } from "./parcel.js";
import { detectWebpack } from "./webpack.js";

describe("webpack signal detection", () => {
  it("detects __webpack_require__", () => {
    const signals = detectWebpack("var m = __webpack_require__(1);");
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "webpack");
    assert.strictEqual(signals[0].tier, "definitive");
  });

  it("detects __webpack_modules__", () => {
    const signals = detectWebpack("var __webpack_modules__ = {};");
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "webpack");
  });

  it("detects webpackChunk", () => {
    const signals = detectWebpack(
      "(self.webpackChunkapp = self.webpackChunkapp || [])"
    );
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].pattern, "webpackChunk");
  });

  it("returns multiple signals when multiple patterns match", () => {
    const code = "var __webpack_modules__ = {}; __webpack_require__(0);";
    const signals = detectWebpack(code);
    assert.ok(signals.length >= 2);
  });

  it("returns empty for non-webpack code", () => {
    assert.strictEqual(detectWebpack('console.log("hello")').length, 0);
  });
});

describe("browserify signal detection", () => {
  it("detects browserify module call pattern", () => {
    const code =
      "e[i][0].call(p.exports,function(r){var n=e[i][1][r];return o(n||r)},p,p.exports}";
    const signals = detectBrowserify(code);
    assert.ok(signals.length >= 1);
    assert.ok(signals.some((s) => s.bundler === "browserify"));
  });

  it("detects installedModules without webpack", () => {
    const signals = detectBrowserify("var installedModules = {};");
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "browserify");
    assert.strictEqual(signals[0].tier, "definitive");
  });

  it("does not match installedModules when __webpack_require__ present", () => {
    const code =
      "var installedModules = {}; function __webpack_require__(id) {}";
    assert.strictEqual(detectBrowserify(code).length, 0);
  });

  it("returns empty for non-browserify code", () => {
    assert.strictEqual(detectBrowserify('console.log("hello")').length, 0);
  });
});

describe("esbuild bundler signal detection", () => {
  it("detects __commonJS", () => {
    const signals = detectEsbuild("var init_foo = __commonJS({");
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "esbuild");
    assert.strictEqual(signals[0].tier, "definitive");
  });

  it("detects __toESM", () => {
    const signals = detectEsbuild('var react = __toESM(require("react"));');
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "esbuild");
  });

  it("detects __toCommonJS", () => {
    const signals = detectEsbuild("module.exports = __toCommonJS(exports);");
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "esbuild");
  });

  it("detects __export definition", () => {
    const signals = detectEsbuild(
      "var __export = (target, all) => { for (var name in all) {} };"
    );
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "esbuild");
  });

  it("returns empty for non-esbuild code", () => {
    assert.strictEqual(detectEsbuild('console.log("hello")').length, 0);
  });
});

describe("parcel signal detection", () => {
  it("detects parcelRequire", () => {
    const signals = detectParcel("var parcelRequire;");
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "parcel");
    assert.strictEqual(signals[0].tier, "definitive");
  });

  it("detects require(_bundle_loader)", () => {
    const signals = detectParcel('var loader = require("_bundle_loader");');
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "parcel");
  });

  it("returns empty for non-parcel code", () => {
    assert.strictEqual(detectParcel('console.log("hello")').length, 0);
  });
});

describe("bun bundler signal detection", () => {
  const BUN_PREAMBLE = `import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`;

  it("detects bun CJS bundle preamble", () => {
    const signals = detectBunBundler(BUN_PREAMBLE);
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "bun");
    assert.strictEqual(signals[0].tier, "definitive");
  });

  it("detects the @bun banner on a bytecode CJS bundle (no ESM createRequire import)", () => {
    // Real Bun `@bun-cjs` bytecode bundles are a `(function(exports, require,
    // module, …){…})` wrapper with NO `import{createRequire}from"node:module"`
    // — createRequire comes from `require("module")` instead. The banner Bun
    // emits verbatim is the definitive marker.
    const bytecodeCjs = `// @bun @bytecode @bun-cjs\n(function(exports, require, module, __filename, __dirname) {var vGc=require("module"),K7h=vGc.createRequire("/");var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);});`;
    const signals = detectBunBundler(bytecodeCjs);
    assert.strictEqual(signals.length, 1);
    assert.strictEqual(signals[0].bundler, "bun");
    assert.strictEqual(signals[0].tier, "definitive");
  });

  it("does not treat a stray @bun mention mid-file as the banner", () => {
    // The banner is a leading line comment; a string/comment elsewhere is not.
    assert.strictEqual(
      detectBunBundler('var doc = "see @bun docs";\nfunction f(){}').length,
      0
    );
  });

  it("requires both patterns to match", () => {
    // Only {exports:{}} without createRequire
    assert.strictEqual(
      detectBunBundler(
        "var x = (I, A) => () => (A || I((A = {exports: {}}).exports, A), A.exports);"
      ).length,
      0
    );
    // Only createRequire import without {exports:{}}
    assert.strictEqual(
      detectBunBundler(
        'import{createRequire as X}from"node:module";var r=X(import.meta.url);var y=r("fs");'
      ).length,
      0
    );
  });

  it("returns empty for esbuild code", () => {
    const esbuildCode =
      "var __commonJS = (cb, mod) => function() { return mod || (0, cb[Object.keys(cb)[0]])(mod = { exports: {} }), mod.exports; };";
    assert.strictEqual(detectBunBundler(esbuildCode).length, 0);
  });

  it("returns empty for plain JS", () => {
    assert.strictEqual(detectBunBundler('console.log("hello")').length, 0);
  });
});

describe("minifier signal detection", () => {
  describe("terser", () => {
    it("detects void 0 as the low-confidence minification fallback", () => {
      const signals = detectTerser("if (x === void 0) return;");
      assert.strictEqual(signals.length, 1);
      assert.strictEqual(signals[0].minifier, "terser");
      // `void 0` is universal across minifiers, so it is the lowest tier: it
      // marks code as minified and defaults to terser, but any distinctive
      // esbuild/bun/swc signal must outrank it.
      assert.strictEqual(signals[0].tier, "unknown");
    });

    it("detects !0 and !1 boolean coercion at the fallback tier", () => {
      const signals = detectTerser("return !0;");
      assert.strictEqual(signals.length, 1);
      assert.strictEqual(signals[0].pattern, "!0/!1 boolean coercion");
      assert.strictEqual(signals[0].tier, "unknown");
    });

    it("returns empty for clean code", () => {
      assert.strictEqual(
        detectTerser("const x = true; return undefined;").length,
        0
      );
    });
  });

  describe("esbuild minifier", () => {
    it("detects esbuild banner comment", () => {
      const signals = detectEsbuildMinifier("// index.js\nvar x = 1;");
      assert.strictEqual(signals.length, 1);
      assert.strictEqual(signals[0].minifier, "esbuild");
      assert.strictEqual(signals[0].tier, "likely");
    });

    it("returns empty without banner", () => {
      assert.strictEqual(detectEsbuildMinifier("var x = 1;").length, 0);
    });
  });

  describe("bun minifier", () => {
    it("detects $-prefixed identifiers", () => {
      const code =
        "$a0 = 1; $bC = 2; $cD = 3; $dE = 4; $eF = 5; $fG = 6; $gH = 7; $hI = 8; $iJ = 9; $jK = 10; $kL = 11;";
      const signals = detectBunMinifier(code);
      assert.strictEqual(signals.length, 1);
      assert.strictEqual(signals[0].minifier, "bun");
      assert.strictEqual(signals[0].tier, "likely");
    });

    it("returns empty for few $-prefixed vars", () => {
      assert.strictEqual(detectBunMinifier("$a0 = 1; $bC = 2;").length, 0);
    });
  });

  describe("swc minifier", () => {
    it("detects snake_case helper names", () => {
      const signals = detectSwcMinifier(
        "function _class_call_check(instance, Constructor) {}"
      );
      assert.strictEqual(signals.length, 1);
      assert.strictEqual(signals[0].minifier, "swc");
      assert.strictEqual(signals[0].tier, "likely");
    });

    it("detects _interop_require_default", () => {
      const signals = detectSwcMinifier(
        'var _lib = _interop_require_default(require("lib"));'
      );
      assert.strictEqual(signals.length, 1);
      assert.strictEqual(signals[0].minifier, "swc");
    });

    it("does not misfire on Babel's camelCase helpers (precision)", () => {
      // Babel emits `_classCallCheck` / `_toConsumableArray`; must NOT match swc.
      assert.strictEqual(
        detectSwcMinifier("function _classCallCheck(i, C) {}").length,
        0
      );
      assert.strictEqual(
        detectSwcMinifier("var a = _toConsumableArray(b);").length,
        0
      );
    });

    it("does not misfire on the Babel-colliding single-word helpers", () => {
      // `_extends` / `_inherits` are shared with Babel and intentionally excluded.
      assert.strictEqual(
        detectSwcMinifier("var x = _extends({}, y);").length,
        0
      );
      assert.strictEqual(detectSwcMinifier("_inherits(Sub, Super);").length, 0);
    });

    it("returns empty for clean code", () => {
      assert.strictEqual(detectSwcMinifier("const x = 1; return x;").length, 0);
    });
  });
});
