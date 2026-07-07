import assert from "node:assert";
import { describe, it } from "node:test";
import { detectBundle } from "./detect.js";

// Inline fixture snippets representing the first ~200 bytes of real bundler output
const FIXTURES = {
  webpack: `
/******/ (function(modules) { // webpackBootstrap
/******/   var installedModules = {};
/******/   function __webpack_require__(moduleId) {
/******/     if(installedModules[moduleId]) return installedModules[moduleId].exports;
`,
  browserify: `
(function(){function r(e,n,t){function o(i,f){if(!n[i]){if(!e[i]){var c="function"==typeof require&&require;if(!f&&c)return c(i,!0);if(u)return u(i,!0);var a=new Error("Cannot find module '"+i+"'");throw a.code="MODULE_NOT_FOUND",a}var p=n[i]={exports:{}};e[i][0].call(p.exports,function(r){var n=e[i][1][r];return o(n||r)},p,p.exports,r,e,n,t)}return n[i].exports}for(var u="function"==typeof require&&require,i=0;i<t.length;i++)o(t[i]);return o})()({1:[function(require,module,exports){
var installedModules = {};
`,
  esbuild: `
var __defProp = Object.defineProperty;
var __export = (target, all) => { for (var name in all) __defProp(target, name, { get: all[name], enumerable: true }); };
var __commonJS = (cb, mod) => function() { return mod || (0, cb[Object.keys(cb)[0]])(mod = { exports: {} }), mod.exports; };
var __toESM = (mod) => __defProp(mod, "__esModule", { value: true });
`,
  parcel: `
parcelRequire = (function (modules, cache, entry, globalName) {
  var previousRequire = typeof parcelRequire === 'function' && parcelRequire;
  function newRequire(name, jumped) {
    if (!cache[name]) {
`,
  bun: `import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);var L=(I,A,q)=>(q=I!=null?Object.create(null):A,Object.defineProperty(q,"default",{enumerable:!0,value:I}));`,
  plain: `
function greet(name) {
  console.log("Hello, " + name + "!");
}
greet("world");
`,
  // Minifier-only fixtures (bundler-agnostic): used to assert minifier.type.
  // Each distinctive fixture also contains `void 0` to prove the distinctive
  // signal outranks the generic terser fallback.
  swcMinified: `function _class_call_check(instance, Constructor) {
  if (!(instance instanceof Constructor)) throw new TypeError("Cannot call a class as a function");
}
var _lib = _interop_require_default(require("lib"));
var _default = void 0;`,
  bunMinified: `var $a0=1,$b1=2,$c2=3,$d3=4,$e4=5,$f5=6,$g6=7,$h7=8,$i8=9,$j9=10,$kA=11,$lB=12;var u=void 0,t=!0;`,
  esbuildMinified: `// app.js
var a = void 0, b = !0, c = !1;`,
  genericMinified: `var a=void 0,b=!0,c=!1;return a?b:c;`
};

describe("detectBundle", () => {
  it("detects webpack bundles", () => {
    const result = detectBundle(FIXTURES.webpack);
    assert.strictEqual(result.bundler?.type, "webpack");
    assert.strictEqual(result.bundler?.tier, "definitive");
  });

  it("detects browserify bundles", () => {
    const result = detectBundle(FIXTURES.browserify);
    assert.strictEqual(result.bundler?.type, "browserify");
    assert.strictEqual(result.bundler?.tier, "definitive");
  });

  it("detects esbuild bundles", () => {
    const result = detectBundle(FIXTURES.esbuild);
    assert.strictEqual(result.bundler?.type, "esbuild");
    assert.strictEqual(result.bundler?.tier, "definitive");
  });

  it("detects parcel bundles", () => {
    const result = detectBundle(FIXTURES.parcel);
    assert.strictEqual(result.bundler?.type, "parcel");
    assert.strictEqual(result.bundler?.tier, "definitive");
  });

  it("detects bun CJS bundles", () => {
    const result = detectBundle(FIXTURES.bun);
    assert.strictEqual(result.bundler?.type, "bun");
    assert.strictEqual(result.bundler?.tier, "definitive");
  });

  it("returns unknown for plain JS", () => {
    const result = detectBundle(FIXTURES.plain);
    assert.strictEqual(result.bundler?.type, "unknown");
    assert.strictEqual(result.bundler?.tier, "unknown");
  });

  it("collects all matching signals", () => {
    const result = detectBundle(FIXTURES.webpack);
    assert.ok(result.signals.length >= 1);
    // All bundler signals should be webpack (minifier signals are allowed too)
    const bundlerSignals = result.signals.filter((s) => s.bundler);
    assert.ok(bundlerSignals.every((s) => s.bundler === "webpack"));
  });

  describe("no cross-contamination for definitive signals", () => {
    const cases: [string, string][] = [
      ["webpack", FIXTURES.webpack],
      ["browserify", FIXTURES.browserify],
      ["esbuild", FIXTURES.esbuild],
      ["parcel", FIXTURES.parcel],
      ["bun", FIXTURES.bun]
    ];

    for (const [name, fixture] of cases) {
      it(`${name} fixture only detects ${name}`, () => {
        const result = detectBundle(fixture);
        const bundlerSignals = result.signals.filter(
          (s) => s.bundler && s.tier === "definitive"
        );
        const uniqueBundlers = new Set(bundlerSignals.map((s) => s.bundler));
        assert.strictEqual(
          uniqueBundlers.size,
          1,
          `Expected 1 bundler type but got: ${[...uniqueBundlers].join(", ")}`
        );
        assert.ok(uniqueBundlers.has(name as import("./types.js").BundlerType));
      });
    }
  });

  describe("minifier classification", () => {
    it("classifies swc bundles by their snake_case helper names, not terser", () => {
      const result = detectBundle(FIXTURES.swcMinified);
      assert.strictEqual(result.minifier?.type, "swc");
    });

    it("classifies bun-minified bundles as bun despite the universal void 0", () => {
      const result = detectBundle(FIXTURES.bunMinified);
      assert.strictEqual(result.minifier?.type, "bun");
    });

    it("classifies esbuild-minified bundles as esbuild despite the universal void 0", () => {
      const result = detectBundle(FIXTURES.esbuildMinified);
      assert.strictEqual(result.minifier?.type, "esbuild");
    });

    it("falls back to terser when only universal minification tokens are present", () => {
      const result = detectBundle(FIXTURES.genericMinified);
      assert.strictEqual(result.minifier?.type, "terser");
      // Pinned: the universal-token fallback is deliberately the lowest tier so
      // any distinctive esbuild/bun/swc signal always outranks it.
      assert.strictEqual(result.minifier?.tier, "unknown");
    });

    it("reports no minifier for non-minified code", () => {
      const result = detectBundle(FIXTURES.plain);
      assert.strictEqual(result.minifier?.type, "unknown");
      assert.strictEqual(result.minifier?.tier, "unknown");
    });
  });
});
