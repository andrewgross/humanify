import assert from "node:assert";
import { describe, it } from "node:test";
import {
  identifyBunCjsFactory,
  identifyBunLazyInit,
  identifyBunRequire
} from "./bun-helpers.js";

// Bun preambles from different Claude Code versions (names change every build)
const PREAMBLE_V281 = `import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);var L=(I,A,q)=>(q=I!=null?Object.create(null):A,Object.defineProperty(q,"default",{enumerable:!0,value:I}));`;
const PREAMBLE_V267 = `import{createRequire as OBq}from"node:module";var r5=OBq(import.meta.url);var C=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);var E=(I,A,q)=>(q=I!=null?Object.create(null):A,Object.defineProperty(q,"default",{enumerable:!0,value:I}));`;

describe("identifyBunCjsFactory", () => {
  it("identifies factory helper from v2.1.81 preamble", () => {
    const result = identifyBunCjsFactory(PREAMBLE_V281);
    assert.ok(result);
    assert.strictEqual(result.name, "x");
  });

  it("identifies factory helper from v2.1.67 preamble", () => {
    const result = identifyBunCjsFactory(PREAMBLE_V267);
    assert.ok(result);
    assert.strictEqual(result.name, "C");
  });

  it("returns null for esbuild code", () => {
    const esbuild =
      "var __commonJS = (cb, mod) => function() { return mod || (0, cb[Object.keys(cb)[0]])(mod = { exports: {} }), mod.exports; };";
    assert.strictEqual(identifyBunCjsFactory(esbuild), null);
  });

  it("returns null for plain JS", () => {
    assert.strictEqual(identifyBunCjsFactory('console.log("hello")'), null);
  });
});

describe("identifyBunRequire", () => {
  it("identifies require variable from v2.1.81 preamble", () => {
    assert.strictEqual(identifyBunRequire(PREAMBLE_V281), "m6");
  });

  it("identifies require variable from v2.1.67 preamble", () => {
    assert.strictEqual(identifyBunRequire(PREAMBLE_V267), "r5");
  });

  it("returns null without createRequire import", () => {
    assert.strictEqual(
      identifyBunRequire('var x = require("node:path")'),
      null
    );
  });

  it("returns null without import.meta.url call", () => {
    assert.strictEqual(
      identifyBunRequire(
        'import{createRequire as X}from"node:module";var r=X("./foo");'
      ),
      null
    );
  });
});

describe("identifyBunLazyInit", () => {
  it("identifies lazy init helper", () => {
    // Simplified lazy init pattern
    const source = `var L=(I,A,q)=>(A&&(q=A(A=0)),q);`;
    assert.strictEqual(identifyBunLazyInit(source), "L");
  });

  it("returns null for non-lazy-init code", () => {
    assert.strictEqual(identifyBunLazyInit("var x = () => 1;"), null);
  });
});
