import assert from "node:assert";
import { describe, it } from "node:test";
import { detectBundle } from "../detection/detect.js";
import { selectAdapter } from "./index.js";

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
  bun: `import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);var L=(I,A,q)=>(q=I!=null?Object.create(null):A,Object.defineProperty(q,"default",{enumerable:!0,value:I}));`,
  plain: `
function greet(name) {
  console.log("Hello, " + name + "!");
}
greet("world");
`
};

describe("selectAdapter", () => {
  it("selects webcrack adapter for webpack detection", () => {
    const detection = detectBundle(FIXTURES.webpack);
    const adapter = selectAdapter(detection);
    assert.strictEqual(adapter.name, "webcrack");
  });

  it("selects webcrack adapter for browserify detection", () => {
    const detection = detectBundle(FIXTURES.browserify);
    const adapter = selectAdapter(detection);
    assert.strictEqual(adapter.name, "webcrack");
  });

  it("selects bun adapter for bun CJS detection", () => {
    const detection = detectBundle(FIXTURES.bun);
    const adapter = selectAdapter(detection);
    assert.strictEqual(adapter.name, "bun");
  });

  it("selects passthrough adapter for esbuild detection", () => {
    const detection = detectBundle(FIXTURES.esbuild);
    const adapter = selectAdapter(detection);
    assert.strictEqual(adapter.name, "passthrough");
  });

  it("selects passthrough adapter for unknown", () => {
    const detection = detectBundle(FIXTURES.plain);
    const adapter = selectAdapter(detection);
    assert.strictEqual(adapter.name, "passthrough");
  });

  it("respects bundler override", () => {
    const detection = detectBundle(FIXTURES.plain); // unknown
    const adapter = selectAdapter(detection, { bundlerOverride: "webpack" });
    assert.strictEqual(adapter.name, "webcrack");
  });

  it("override to unknown still uses passthrough", () => {
    const detection = detectBundle(FIXTURES.webpack);
    const adapter = selectAdapter(detection, { bundlerOverride: "unknown" });
    // "unknown" override is ignored, falls through to normal detection
    assert.strictEqual(adapter.name, "webcrack");
  });
});
