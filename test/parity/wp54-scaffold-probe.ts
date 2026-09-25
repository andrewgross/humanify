// WP5.4 probe: the runnable scaffold's real-TS outputs, frozen as
// test/parity/wp54-scaffold.json for the Rust unit test
// (crates/humanify-core/src/finish/scaffold/scaffold_test.rs).
//
//   npx tsx test/parity/wp54-scaffold-probe.ts test/parity/wp54-scaffold.json
//
// The resolve cases run against a scratch tree the Rust test rebuilds
// byte-for-byte: <root>/node_modules/{ajv,@scope/pkg}/package.json and the
// lookup starts two directories down (<root>/a/b), so the walk up is real.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  externalPackagesFrom,
  resolveExternalVersions,
  writeRunnableScaffold
} from "../../src/split/runnable-scaffold.js";

export const SPECIFIER_TEXTS = [
  `require("ajv"); require('ws/lib/x'); require("fs"); require("node:fs"); require("./a.js"); require("/abs")`,
  `import("@scope/pkg/deep"); require ( "bun:jsc" ) ; xrequire("zzz"); require("fs/promises"); require("data:x")`,
  `require("a"b"); require(""); require("stream/web2"); require("_http_agent"); require('mixed")`,
  `require(\n"multi\nline"\n)`,
  `require( "nbsp"﻿)`
];

export const INSTALLED: [string, string][] = [
  ["ajv", "8.1.0"],
  ["@scope/pkg", "1.2.3"]
];

async function main(outPath: string): Promise<void> {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "wp54-scaffold-"));
  for (const [p, v] of INSTALLED) {
    fs.mkdirSync(path.join(root, "node_modules", p), { recursive: true });
    fs.writeFileSync(
      path.join(root, "node_modules", p, "package.json"),
      JSON.stringify({ version: v })
    );
  }
  const deep = path.join(root, "a", "b");
  fs.mkdirSync(deep, { recursive: true });
  const scaffolds: Record<string, unknown>[] = [];
  const cases: [string, string, string[], boolean][] = [
    ["none", "_index.js", [], false],
    ["some", "index.js", ["@scope/pkg", "ajv", "ws"], true],
    ["all", "index.js", ["ajv"], true],
    ["star", "index.js", ["ws"], false]
  ];
  for (const [name, entry, externals, resolve] of cases) {
    const out = fs.mkdtempSync(path.join(os.tmpdir(), "wp54-scaffold-out-"));
    await writeRunnableScaffold(
      out,
      entry,
      externals,
      resolve ? deep : undefined
    );
    const read = (f: string) => fs.readFileSync(path.join(out, f), "utf-8");
    scaffolds.push({
      name,
      entry,
      externals,
      resolve,
      packageJson: read("package.json"),
      readme: read("RUNNABLE.md"),
      runner: read("run.cjs")
    });
    fs.rmSync(out, { recursive: true, force: true });
  }
  const result = {
    specifierTexts: SPECIFIER_TEXTS,
    externals: externalPackagesFrom(SPECIFIER_TEXTS),
    installed: INSTALLED,
    resolved: resolveExternalVersions(["@scope/pkg", "ajv", "ws"], deep),
    scaffolds
  };
  fs.rmSync(root, { recursive: true, force: true });
  fs.writeFileSync(outPath, `${JSON.stringify(result, null, 2)}\n`);
}

const outPath = process.argv[2];
if (!outPath) throw new Error("usage: see header");
main(outPath).catch((err) => {
  console.error(err);
  process.exit(1);
});
