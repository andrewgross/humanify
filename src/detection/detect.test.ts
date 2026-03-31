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
`
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
});
