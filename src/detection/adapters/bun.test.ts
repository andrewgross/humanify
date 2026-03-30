import assert from "node:assert";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { BunUnpackAdapter } from "./bun.js";

const BUN_BUNDLE = [
  `import{createRequire as Glq}from"node:module";`,
  `var m6=Glq(import.meta.url);`,
  `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
  `var mod_a=x((exports,module)=>{`,
  `  var dep=m6("node:path");`,
  `  function helper(){return dep.join("a","b")}`,
  `  module.exports=helper;`,
  `});`,
  `var mod_b=x((exports)=>{`,
  `  exports.value=42;`,
  `});`,
  `var main=mod_a();`
].join("\n");

describe("BunUnpackAdapter", () => {
  const adapter = new BunUnpackAdapter();
  let tmpDir: string;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "bun-unpack-"));
  });

  afterEach(async () => {
    await fs.rm(tmpDir, { recursive: true, force: true });
  });

  it("supports bun detection", () => {
    assert.strictEqual(
      adapter.supports({
        bundler: { type: "bun", tier: "definitive" },
        signals: []
      }),
      true
    );
  });

  it("does not support non-bun detection", () => {
    assert.strictEqual(
      adapter.supports({
        bundler: { type: "webpack", tier: "definitive" },
        signals: []
      }),
      false
    );
  });

  it("extracts factory bodies into separate files", async () => {
    const result = await adapter.unpack(BUN_BUNDLE, tmpDir);

    const fileNames = result.files.map((f) => path.basename(f.path)).sort();
    assert.ok(
      fileNames.includes("mod_a.js"),
      `Expected mod_a.js in ${fileNames}`
    );
    assert.ok(
      fileNames.includes("mod_b.js"),
      `Expected mod_b.js in ${fileNames}`
    );
    assert.ok(
      fileNames.includes("runtime.js"),
      `Expected runtime.js in ${fileNames}`
    );
  });

  it("rewrites require variable to require()", async () => {
    await adapter.unpack(BUN_BUNDLE, tmpDir);

    const modA = await fs.readFile(path.join(tmpDir, "mod_a.js"), "utf-8");
    assert.ok(
      modA.includes('require("node:path")'),
      `Expected rewritten require call, got: ${modA}`
    );
    assert.ok(
      !modA.includes("m6("),
      `Should not contain original require var, got: ${modA}`
    );
  });

  it("collects runtime code outside factories", async () => {
    await adapter.unpack(BUN_BUNDLE, tmpDir);

    const runtime = await fs.readFile(path.join(tmpDir, "runtime.js"), "utf-8");
    assert.ok(
      runtime.includes("mod_a()"),
      `Expected runtime to contain entry point call`
    );
  });

  it("handles code without factory helper gracefully", async () => {
    const plainCode = 'console.log("hello");';
    const result = await adapter.unpack(plainCode, tmpDir);

    assert.strictEqual(result.files.length, 1);
    assert.strictEqual(path.basename(result.files[0].path), "index.js");
  });

  it("works with different helper names", async () => {
    const bundle = [
      `import{createRequire as OBq}from"node:module";`,
      `var r5=OBq(import.meta.url);`,
      `var C=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `var foo=C((exports)=>{`,
      `  exports.x=r5("node:fs");`,
      `});`,
      `var bar=C((exports)=>{`,
      `  exports.y=2;`,
      `});`
    ].join("\n");

    const result = await adapter.unpack(bundle, tmpDir);
    const fileNames = result.files.map((f) => path.basename(f.path)).sort();
    assert.ok(fileNames.includes("foo.js"));
    assert.ok(fileNames.includes("bar.js"));

    const foo = await fs.readFile(path.join(tmpDir, "foo.js"), "utf-8");
    assert.ok(foo.includes('require("node:fs")'));
  });
});
