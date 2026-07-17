import assert from "node:assert";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { VENDOR_DIR } from "../../split/layout.js";
import {
  BUN_MODULES_MANIFEST,
  BunUnpackAdapter,
  loadPriorVendorNames,
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
    path.join(tmpDir, VENDOR_DIR, BUN_MODULES_MANIFEST),
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
    // every factory falls back to `lib_<structuralHash[:8]>`, vendored
    // under vendor/.
    for (const entry of manifest.factories) {
      assert.strictEqual(entry.nameSource, "fallback");
      assert.match(
        entry.fileName,
        /^vendor\/lib_[0-9a-f]{8}\.js$/,
        `expected vendor/lib_<hash>.js, got ${entry.fileName}`
      );
      assert.match(entry.structuralHash, /^[0-9a-f]{16}$/);
    }

    // Each manifest entry corresponds to a real file on disk.
    const writtenNames = result.files.map((f) => path.relative(tmpDir, f.path));
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

  it("collects runtime code outside factories with stable factory references", async () => {
    await adapter.unpack(BUN_BUNDLE, tmpDir);
    const manifest = await readManifest(tmpDir);

    const modA = manifest.factories.find((f) => f.factoryVar === "mod_a");
    assert.ok(modA?.runtimeIdentifier, "mod_a must expose a runtimeIdentifier");
    const runtime = await fs.readFile(path.join(tmpDir, "runtime.js"), "utf-8");
    assert.ok(
      runtime.includes(`${modA.runtimeIdentifier}()`),
      `Expected runtime to call the stable identifier, got:\n${runtime}`
    );
    assert.ok(
      !runtime.includes("mod_a()"),
      "the rerollable minified factory var must not survive in runtime"
    );
  });

  describe("stable factory identifiers", () => {
    // Factory var tokens are minted by Bun's minifier and re-roll between
    // builds; their declarations are stripped during extraction, leaving
    // FREE identifiers nothing can ever rename. Rewriting every reference
    // to a content-derived identifier (the extracted file's name) makes
    // runtime.js and the lib files byte-stable across versions.

    it("derives the identifier from the extracted file name", async () => {
      await adapter.unpack(BUN_BUNDLE, tmpDir);
      const manifest = await readManifest(tmpDir);

      for (const entry of manifest.factories) {
        assert.ok(
          entry.runtimeIdentifier,
          `entry ${entry.factoryVar} missing runtimeIdentifier`
        );
        // Derived from the bare file name — the vendor/ folder is a
        // layout concern and must not leak into identifiers.
        const base = path.basename(entry.fileName).replace(/\.js$/, "");
        assert.strictEqual(
          entry.runtimeIdentifier,
          base.replace(/[^A-Za-z0-9_$]/g, "_"),
          "identifier is the sanitized file name"
        );
      }
    });

    it("gives the same identifier across versions with rerolled tokens", async () => {
      // Same module content; every minified token re-minted and layout
      // shifted — exactly what a new Bun build does.
      const v2 = [
        `import{createRequire as Wq9}from"node:module";`,
        `var n7=Wq9(import.meta.url);`,
        `var y=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
        `var extra=y((exports)=>{`,
        `  exports.brandNew=true;`,
        `});`,
        `var q7=y((exports,module)=>{`,
        `  var w2=n7("node:path");`,
        `  function p9(){return w2.join("a","b")}`,
        `  module.exports=p9;`,
        `});`,
        `var r2=y((exports)=>{`,
        `  exports.value=42;`,
        `});`,
        `var z9=q7();`
      ].join("\n");

      const tmpDir2 = await fs.mkdtemp(path.join(os.tmpdir(), "bun-unpack2-"));
      try {
        await adapter.unpack(BUN_BUNDLE, tmpDir);
        await adapter.unpack(v2, tmpDir2);
        const m1 = await readManifest(tmpDir);
        const m2 = await readManifest(tmpDir2);

        const v1ModA = m1.factories.find((f) => f.factoryVar === "mod_a");
        assert.ok(v1ModA?.structuralHash);
        const v2ModA = m2.factories.find(
          (f) => f.structuralHash === v1ModA.structuralHash
        );
        assert.ok(v2ModA, "same-content factory must share a structural hash");
        assert.strictEqual(
          v2ModA.runtimeIdentifier,
          v1ModA.runtimeIdentifier,
          "same content must yield the same identifier in both versions"
        );

        const runtime2 = await fs.readFile(
          path.join(tmpDir2, "runtime.js"),
          "utf-8"
        );
        assert.ok(
          runtime2.includes(`${v1ModA.runtimeIdentifier}()`),
          `v2 runtime must call the SAME stable identifier, got:\n${runtime2}`
        );
      } finally {
        await fs.rm(tmpDir2, { recursive: true, force: true });
      }
    });

    it("rewrites cross-factory references inside extracted bodies", async () => {
      const bundle = [
        `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
        `var mod_a=x((exports,module)=>{`,
        `  module.exports=function base(){return 7};`,
        `});`,
        `var mod_c=x((exports)=>{`,
        `  exports.wrapped=mod_a()();`,
        `});`,
        `var main=mod_c();`
      ].join("\n");

      await adapter.unpack(bundle, tmpDir);
      const manifest = await readManifest(tmpDir);
      const modA = manifest.factories.find((f) => f.factoryVar === "mod_a");
      const modC = manifest.factories.find((f) => f.factoryVar === "mod_c");
      assert.ok(modA?.runtimeIdentifier && modC);

      const bodyC = await fs.readFile(
        path.join(tmpDir, modC.fileName),
        "utf-8"
      );
      assert.ok(
        bodyC.includes(`${modA.runtimeIdentifier}()`),
        `mod_c's extracted body must reference mod_a's stable identifier, got:\n${bodyC}`
      );
      assert.ok(
        !bodyC.includes("mod_a"),
        "the rerollable token must not survive inside extracted bodies"
      );
    });

    it("leaves shadowing local bindings and their references untouched", async () => {
      const bundle = [
        `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
        `var mod_a=x((exports)=>{`,
        `  exports.value=1;`,
        `});`,
        `function shadow(){var mod_a=5;return mod_a+1}`,
        `var main=mod_a();`,
        `console.log(shadow());`
      ].join("\n");

      await adapter.unpack(bundle, tmpDir);
      const manifest = await readManifest(tmpDir);
      const modA = manifest.factories.find((f) => f.factoryVar === "mod_a");
      assert.ok(modA?.runtimeIdentifier);

      const runtime = await fs.readFile(
        path.join(tmpDir, "runtime.js"),
        "utf-8"
      );
      assert.ok(
        runtime.includes("var mod_a=5") && runtime.includes("return mod_a+1"),
        `the LOCAL mod_a binding and its references must stay untouched, got:\n${runtime}`
      );
      assert.ok(
        runtime.includes(`${modA.runtimeIdentifier}()`),
        "the factory reference must still be rewritten"
      );
    });
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
    assert.strictEqual(entry.fileName, "vendor/axios@1.2.3.js");
  });

  it("LLM-names fallback factories via the vendorNamer option", async () => {
    // Two hash-named factories and one banner-named. The namer sees ONLY
    // the fallback ones (cascade priority intact) and its accepted
    // proposals become vendor/<name>.js with nameSource "llm"; a rejected
    // proposal keeps the lib_<hash> fallback.
    const bundle = [
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `/*! axios v1.2.3 */`,
      `var withBanner=x((exports,module)=>{ module.exports=function axios(){}; });`,
      `var unknownOne=x((exports)=>{ exports.load=function load(s){return s+"YAMLException";}; });`,
      `var unknownTwo=x((exports)=>{ exports.render=function render(t){return t+"template";}; });`,
      `var main=withBanner();`
    ].join("\n");

    const seenKeys: string[][] = [];
    await adapter.unpack(bundle, tmpDir, {
      vendorNamer: async (requests) => {
        seenKeys.push(requests.map((r) => r.key));
        return requests.map((r) =>
          r.evidence.includes("YAMLException") ? "js-yaml" : "not a name!!"
        );
      }
    });
    const manifest = await readManifest(tmpDir);
    assert.equal(manifest.factories.length, 3);

    assert.equal(seenKeys.length, 1, "one batch for the fallback factories");
    assert.equal(seenKeys[0].length, 2, "banner-named factory excluded");
    assert.ok(seenKeys[0].every((k) => /^lib_[0-9a-f]{8}$/.test(k)));

    const banner = manifest.factories.find((f) => f.nameSource === "banner");
    assert.ok(banner);
    assert.equal(banner.fileName, "vendor/axios@1.2.3.js");

    const llm = manifest.factories.find((f) => f.nameSource === "llm");
    assert.ok(llm, "accepted proposal becomes an llm-sourced name");
    assert.equal(llm.fileName, "vendor/js-yaml.js");
    assert.equal(llm.name, "js-yaml");

    const fallback = manifest.factories.find(
      (f) => f.nameSource === "fallback"
    );
    assert.ok(fallback, "rejected proposal keeps the hash fallback");
    assert.match(fallback.fileName, /^vendor\/lib_[0-9a-f]{8}\.js$/);
  });

  it("strips a trailing .js from banner package names (no highlight.js.js)", async () => {
    const bundle = [
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `/*! highlight.js */`,
      `var hl=x((exports,module)=>{`,
      `  module.exports=function highlight(){};`,
      `});`,
      `var main=hl();`
    ].join("\n");

    await adapter.unpack(bundle, tmpDir);
    const manifest = await readManifest(tmpDir);
    assert.strictEqual(manifest.factories.length, 1);
    assert.strictEqual(manifest.factories[0].nameSource, "banner");
    assert.strictEqual(
      manifest.factories[0].fileName,
      "vendor/highlight.js",
      "the stem must drop its own .js before the extension is appended"
    );
  });

  it("groups a package's multiple modules into vendor/<package>/ (not -N siblings)", async () => {
    // Three modules the cascade identifies as the SAME package are that
    // package's internal modules — a human puts them in one folder, not
    // axios@1.0.0.js / -2 / -3 scattered in vendor/'s root.
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

    // Every module lands under one folder, named by its stable structural
    // hash (cross-version stable, unique within the folder) — no -N.
    for (const f of manifest.factories) {
      assert.match(
        f.fileName,
        /^vendor\/axios@1\.0\.0\/lib_[0-9a-f]{8}\.js$/,
        `expected vendor/axios@1.0.0/lib_<hash>.js, got ${f.fileName}`
      );
    }
    const stems = manifest.factories.map((f) => f.fileName);
    assert.strictEqual(new Set(stems).size, 3, "distinct files in the folder");
    // The files are really written under that folder.
    const grpcDir = await fs.readdir(
      path.join(tmpDir, "vendor", "axios@1.0.0")
    );
    assert.strictEqual(grpcDir.length, 3);
  });

  it("keeps a package with a single module flat (no needless folder)", async () => {
    const bundle = [
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `/*! axios v2.0.0 */`,
      `var a=x((exports)=>{ exports.value = 1; });`,
      `/*! lodash v4.0.0 */`,
      `var b=x((exports)=>{ exports.value = 2; });`,
      `var main=a();`
    ].join("\n");

    await adapter.unpack(bundle, tmpDir);
    const manifest = await readManifest(tmpDir);
    const names = manifest.factories.map((f) => f.fileName).sort();
    assert.deepStrictEqual(names, [
      "vendor/axios@2.0.0.js",
      "vendor/lodash@4.0.0.js"
    ]);
  });

  it("the runtime identifier stays stable when a package folder is introduced", async () => {
    // Grouping is a DISPLAY concern; the free identifier every reference is
    // rewritten to must not gain the folder prefix (that would churn
    // runtime.js the moment a package's module count crosses 1).
    const bundle = [
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `/*! axios v1.0.0 */`,
      `var a=x((exports)=>{ exports.value = function f(a){return a+1}; });`,
      `/*! axios v1.0.0 */`,
      `var b=x((exports)=>{ exports.value = function f(a,b){return a*b}; });`,
      `var main=a()+b();`
    ].join("\n");

    await adapter.unpack(bundle, tmpDir);
    const manifest = await readManifest(tmpDir);
    const runtime = await fs.readFile(path.join(tmpDir, "runtime.js"), "utf-8");
    for (const f of manifest.factories) {
      assert.ok(f.runtimeIdentifier, "each grouped module keeps an identifier");
      // Identifier is the module's own stable stem, not the folder path.
      assert.doesNotMatch(
        f.runtimeIdentifier,
        /axios/,
        `identifier must not carry the package folder, got ${f.runtimeIdentifier}`
      );
      assert.ok(
        runtime.includes(`${f.runtimeIdentifier}(`),
        "runtime calls the stable identifier"
      );
    }
  });

  it("disambiguates names that differ only in case (case-insensitive FS safe)", async () => {
    // Two libraries whose banner package names collide under case-folding.
    // A case-sensitive uniquify would emit vendor/Ab@1.js and vendor/aB@1.js,
    // which macOS/Windows collapse to one file; the shared case-folding
    // uniquify must suffix the second.
    const bundle = [
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `/*! Ab v1.0.0 */`,
      `var a=x((exports)=>{ exports.value = 1; });`,
      `/*! aB v1.0.0 */`,
      `var b=x((exports)=>{ exports.value = 2; });`,
      `var main=a();`
    ].join("\n");

    await adapter.unpack(bundle, tmpDir);
    const manifest = await readManifest(tmpDir);
    assert.strictEqual(manifest.factories.length, 2);

    const names = manifest.factories.map((f) => f.fileName);
    const lowered = names.map((n) => n.toLowerCase());
    assert.strictEqual(
      new Set(lowered).size,
      2,
      `case-collision on disk: ${names.join(", ")}`
    );
    // First writer keeps its casing; the second is suffixed.
    assert.strictEqual(names[0], "vendor/Ab@1.0.0.js");
    assert.strictEqual(names[1], "vendor/aB@1.0.0-2.js");
  });

  describe("cross-version vendor name carry-over", () => {
    // LLM vendor names are not reproducible run-to-run, so an unchanged
    // library gets a different vendor/<name>.js each release. src/ imports
    // vendor BY PATH, so that drift rewrites require() lines in app code —
    // the churn dominates a cross-version diff even when vendor/ itself is
    // not committed. structuralHash is the cross-version join key, so the
    // prior release's names carry over onto unchanged libraries.
    const UNKNOWN_BUNDLE = [
      `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
      `var unknownOne=x((exports)=>{ exports.load=function load(s){return s+"YAMLException";}; });`,
      `var main=unknownOne();`
    ].join("\n");

    it("reuses the prior name for an unchanged library, ahead of the LLM", async () => {
      // Pass 1: no prior — the LLM names it.
      await adapter.unpack(UNKNOWN_BUNDLE, tmpDir, {
        vendorNamer: async (requests) => requests.map(() => "js-yaml")
      });
      const first = await readManifest(tmpDir);
      assert.strictEqual(first.factories.length, 1);
      assert.strictEqual(first.factories[0].nameSource, "llm");
      const hash = first.factories[0].structuralHash;

      // Pass 2: the same library, a release later. The LLM would now answer
      // differently (naming is not stable); the prior name must win.
      const second = await fs.mkdtemp(path.join(os.tmpdir(), "bun-unpack-2-"));
      const asked: string[][] = [];
      await adapter.unpack(UNKNOWN_BUNDLE, second, {
        priorVendorNames: new Map([[hash, ["js-yaml"]]]),
        vendorNamer: async (requests) => {
          asked.push(requests.map((r) => r.key));
          return requests.map(() => "yaml-parser");
        }
      });
      const manifest = JSON.parse(
        await fs.readFile(
          path.join(second, VENDOR_DIR, BUN_MODULES_MANIFEST),
          "utf-8"
        )
      ) as BunModulesManifest;
      await fs.rm(second, { recursive: true, force: true });

      assert.strictEqual(manifest.factories[0].name, "js-yaml");
      assert.strictEqual(manifest.factories[0].nameSource, "carry-over");
      assert.strictEqual(manifest.factories[0].fileName, "vendor/js-yaml.js");
      assert.deepStrictEqual(
        asked,
        [],
        "a carried-over factory is not fallback-named, so the LLM never sees it"
      );
    });

    it("falls back to the LLM for a library the prior did not have", async () => {
      await adapter.unpack(UNKNOWN_BUNDLE, tmpDir, {
        priorVendorNames: new Map([["0".repeat(16), ["some-other-lib"]]]),
        vendorNamer: async (requests) => requests.map(() => "js-yaml")
      });
      const manifest = await readManifest(tmpDir);
      assert.strictEqual(manifest.factories[0].nameSource, "llm");
      assert.strictEqual(manifest.factories[0].name, "js-yaml");
    });

    it("keeps a carried-over hash name flat, like a freshly minted one", async () => {
      // Grouping asks "does this name identify a package?" — a lib_<hash>
      // fallback identifies nothing, so its modules stay flat. That test used
      // to read nameSource !== "fallback", which a carried name silently
      // defeats: it arrives as "carry-over", so hash-named modules suddenly
      // grew a pointless vendor/lib_x/lib_x.js folder ONE HOP after being
      // minted flat — a rename of every reference to them.
      const twins = [
        `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
        `var one=x((exports)=>{ exports.v = function pick(a){ return a[0]; }; });`,
        `var two=x((exports)=>{ exports.v = function pick(a){ return a[0]; }; });`,
        `var main=one();`
      ].join("\n");

      // Pass 1: no prior — both hash-named, both flat.
      await adapter.unpack(twins, tmpDir, {
        vendorNamer: async (rs) => rs.map(() => null)
      });
      const first = await readManifest(tmpDir);
      const flatNames = first.factories.map((f) => f.fileName).sort();
      assert.ok(
        flatNames.every((n) => /^vendor\/lib_[0-9a-f]{8}(-2)?\.js$/.test(n)),
        `minted flat: ${flatNames.join(", ")}`
      );

      // Pass 2: carry those same names over. Layout must not move.
      const prior = new Map<string, string[]>();
      for (const f of first.factories) {
        const g = prior.get(f.structuralHash) ?? [];
        g.push(f.name);
        prior.set(f.structuralHash, g);
      }
      const second = await fs.mkdtemp(path.join(os.tmpdir(), "bun-carry-"));
      await adapter.unpack(twins, second, {
        priorVendorNames: prior,
        vendorNamer: async (rs) => rs.map(() => null)
      });
      const manifest = JSON.parse(
        await fs.readFile(
          path.join(second, VENDOR_DIR, BUN_MODULES_MANIFEST),
          "utf-8"
        )
      ) as BunModulesManifest;
      await fs.rm(second, { recursive: true, force: true });

      assert.deepStrictEqual(
        manifest.factories.map((f) => f.fileName).sort(),
        flatNames,
        "carried hash names keep the layout they were minted with"
      );
    });

    it("keeps hash-colliding shims on their own distinct prior names", async () => {
      // Structurally identical re-export shims proxying different deps: one
      // structuralHash, several legitimately different names. Collapsing them
      // onto one name piles every shim into that library's folder and
      // misnames all but one.
      const shimBundle = [
        `var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);`,
        `var depOne=x((exports,module)=>{ module.exports=function one(a){return a+1}; });`,
        `var depTwo=x((exports,module)=>{ module.exports=function two(a,b,c){return a*b*c}; });`,
        `var shimOne=x((exports,module)=>{ module.exports=depOne(); });`,
        `var shimTwo=x((exports,module)=>{ module.exports=depTwo(); });`,
        `var main=shimOne();`
      ].join("\n");

      // Pass 1: learn the shims' shared hash and their bundle order.
      await adapter.unpack(shimBundle, tmpDir, {
        vendorNamer: async (rs) => rs.map(() => null)
      });
      const first = await readManifest(tmpDir);
      const counts = new Map<string, number>();
      for (const f of first.factories) {
        counts.set(f.structuralHash, (counts.get(f.structuralHash) ?? 0) + 1);
      }
      const shared = [...counts.entries()].find(([, n]) => n === 2)?.[0];
      assert.ok(shared, "the two shims must share a structuralHash");

      // Pass 2: the prior named them differently. Both must be honoured.
      const second = await fs.mkdtemp(path.join(os.tmpdir(), "bun-shim-"));
      await adapter.unpack(shimBundle, second, {
        priorVendorNames: new Map([[shared, ["retry", "lodash"]]]),
        vendorNamer: async (rs) => rs.map(() => null)
      });
      const manifest = JSON.parse(
        await fs.readFile(
          path.join(second, VENDOR_DIR, BUN_MODULES_MANIFEST),
          "utf-8"
        )
      ) as BunModulesManifest;
      await fs.rm(second, { recursive: true, force: true });

      const carried = manifest.factories
        .filter((f) => f.structuralHash === shared)
        .map((f) => f.name);
      assert.deepStrictEqual(
        carried,
        ["retry", "lodash"],
        "each colliding shim keeps its own prior name"
      );
    });

    it("loads prior vendor names from a prior release's tree", async () => {
      // --prior-version points at <root>/.humanify/humanified.js; the vendor
      // manifest is its sibling tree's vendor/_bun-modules.json.
      await adapter.unpack(UNKNOWN_BUNDLE, tmpDir, {
        vendorNamer: async (requests) => requests.map(() => "js-yaml")
      });
      const written = await readManifest(tmpDir);
      const hash = written.factories[0].structuralHash;

      await fs.mkdir(path.join(tmpDir, ".humanify"), { recursive: true });
      const priorFile = path.join(tmpDir, ".humanify", "humanified.js");
      await fs.writeFile(priorFile, "// prior");

      const names = loadPriorVendorNames(priorFile);
      assert.ok(names, "manifest discovered from the prior-version file");
      assert.deepStrictEqual(names.get(hash), ["js-yaml"]);
    });

    it("returns undefined when the prior tree has no vendor manifest", async () => {
      const bare = await fs.mkdtemp(path.join(os.tmpdir(), "bun-bare-"));
      await fs.writeFile(path.join(bare, "humanified.js"), "// prior");
      const names = loadPriorVendorNames(path.join(bare, "humanified.js"));
      await fs.rm(bare, { recursive: true, force: true });
      assert.strictEqual(names, undefined);
    });
  });
});
