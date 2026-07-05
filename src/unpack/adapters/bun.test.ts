import assert from "node:assert";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import {
  BUN_MODULES_MANIFEST,
  BunUnpackAdapter,
  type BunModulesManifest
} from "./bun.js";

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

async function readManifest(tmpDir: string): Promise<BunModulesManifest> {
  const raw = await fs.readFile(
    path.join(tmpDir, BUN_MODULES_MANIFEST),
    "utf-8"
  );
  return JSON.parse(raw) as BunModulesManifest;
}

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

  it("extracts factory bodies into separate files with stable names", async () => {
    const result = await adapter.unpack(BUN_BUNDLE, tmpDir);
    const manifest = await readManifest(tmpDir);

    assert.strictEqual(manifest.adapter, "bun");
    assert.strictEqual(
      manifest.factories.length,
      2,
      "expected one manifest entry per factory"
    );
    assert.strictEqual(manifest.runtimeFile, "runtime.js");

    // Filenames come from the cascade. With no banner/URL/prior-name signal,
    // every factory falls back to `lib_<structuralHash[:8]>`.
    for (const entry of manifest.factories) {
      assert.strictEqual(entry.nameSource, "fallback");
      assert.match(
        entry.fileName,
        /^lib_[0-9a-f]{8}\.js$/,
        `expected lib_<hash>.js, got ${entry.fileName}`
      );
      assert.match(entry.structuralHash, /^[0-9a-f]{16}$/);
    }

    // Each manifest entry corresponds to a real file on disk.
    const writtenNames = result.files.map((f) => path.basename(f.path));
    for (const entry of manifest.factories) {
      assert.ok(
        writtenNames.includes(entry.fileName),
        `manifest entry ${entry.fileName} missing from emitted files`
      );
    }
    assert.ok(writtenNames.includes("runtime.js"));
  });

  it("rewrites require variable to require()", async () => {
    await adapter.unpack(BUN_BUNDLE, tmpDir);
    const manifest = await readManifest(tmpDir);

    const modA = manifest.factories.find((f) => f.factoryVar === "mod_a");
    assert.ok(modA, "expected mod_a entry");
    const body = await fs.readFile(path.join(tmpDir, modA.fileName), "utf-8");
    assert.ok(
      body.includes('require("node:path")'),
      `Expected rewritten require call, got: ${body}`
    );
    assert.ok(
      !body.includes("m6("),
      `Should not contain original require var, got: ${body}`
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

    await adapter.unpack(bundle, tmpDir);
    const manifest = await readManifest(tmpDir);
    assert.strictEqual(manifest.factories.length, 2);

    const foo = manifest.factories.find((e) => e.factoryVar === "foo");
    assert.ok(foo, "expected foo entry");
    const fooBody = await fs.readFile(path.join(tmpDir, foo.fileName), "utf-8");
    assert.ok(fooBody.includes('require("node:fs")'));
  });

  it("uses banner package as the filename when one is present", async () => {
    const bundle = [
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `/*! axios v1.2.3 */`,
      `var foo=x((exports,module)=>{`,
      `  module.exports=function axios(){};`,
      `});`,
      `var main=foo();`
    ].join("\n");

    await adapter.unpack(bundle, tmpDir);
    const manifest = await readManifest(tmpDir);
    assert.strictEqual(manifest.factories.length, 1);

    const entry = manifest.factories[0];
    assert.strictEqual(entry.nameSource, "banner");
    assert.strictEqual(entry.bannerPackage, "axios");
    assert.strictEqual(entry.bannerVersion, "1.2.3");
    assert.strictEqual(entry.fileName, "axios@1.2.3.js");
  });

  it("disambiguates colliding cascade names with a -N counter", async () => {
    const bundle = [
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `/*! axios v1.0.0 */`,
      `var a=x((exports)=>{ exports.value = function f(a) { return a + 1; }; });`,
      `/*! axios v1.0.0 */`,
      `var b=x((exports)=>{ exports.value = function f(a,b) { return a * b; }; });`,
      `/*! axios v1.0.0 */`,
      `var c=x((exports)=>{ exports.value = function f(a,b,c) { return a * b * c; }; });`,
      `var main=a();`
    ].join("\n");

    await adapter.unpack(bundle, tmpDir);
    const manifest = await readManifest(tmpDir);
    assert.strictEqual(manifest.factories.length, 3);

    // First factory keeps the unsuffixed name; subsequent ones get -2, -3, ...
    assert.strictEqual(manifest.factories[0].fileName, "axios@1.0.0.js");
    assert.strictEqual(manifest.factories[1].fileName, "axios@1.0.0-2.js");
    assert.strictEqual(manifest.factories[2].fileName, "axios@1.0.0-3.js");
  });
});
