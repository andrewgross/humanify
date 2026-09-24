// probe: WPB.1's gate — bundle-detection verdicts, TS vs Rust, byte-exact.
//
// Runs the REAL TS `detectBundle` (src/detection/detect.ts) on every input,
// writes `JSON.stringify(verdict)` per input, then runs the Rust verb
// `humanify detect <input>` on the same file and compares the two JSON
// strings byte-for-byte (key order included: the Rust serializes the TS
// object-literal order).
//
// Inputs (the WPB.1 fixture set):
//   1. every *.js under test/e2e/fixtures/<fixture>/{minified,build,source}
//   2. the four oracle pairs' eight minified bundles (READ ONLY):
//      ../claude-code-versions/inputs/claude-code-2.1.<v>/binary-decompiled/src/entrypoints/index.js
//   3. synthetic vectors written under <out>/synthetic/: the TS unit tests'
//      fixtures plus the JS-regex-semantics edges the corpus never reaches
//      (ECMAScript \s vs Unicode White_Space, ASCII \b, `.` vs U+2028, the
//      16K / 200 windows counted in UTF-16 units, invalid UTF-8, a BOM).
//
// Usage: npx tsx test/parity/wpb1-detect-probe.ts <rust-humanify-bin> <out-dir>
// Exit 0 iff every input's verdict is identical.

import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { detectBundle } from "../../src/detection/detect.js";

const REPO = resolve(import.meta.dirname, "../..");
const FIXTURES = join(REPO, "test/e2e/fixtures");
const VERSIONS = ["85", "86", "118", "119", "197", "198", "215", "216"];
// The bundles sit beside the MAIN checkout (run-oracle-pair-recut.sh's
// $REPO/..), which a worktree's own path does not reach — resolve it
// through git's common dir.
const MAIN_REPO = dirname(
  execFileSync(
    "git",
    ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    {
      cwd: REPO,
      encoding: "utf-8"
    }
  ).trim()
);
const bundlePath = (v: string) =>
  join(
    MAIN_REPO,
    `../claude-code-versions/inputs/claude-code-2.1.${v}/binary-decompiled/src/entrypoints/index.js`
  );

function walkJs(dir: string, out: string[]): void {
  for (const name of readdirSync(dir).sort()) {
    if (name === "node_modules" || name.startsWith(".tmp-clone")) continue;
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walkJs(p, out);
    else if (name.endsWith(".js")) out.push(p);
  }
}

function fixtureInputs(): string[] {
  const out: string[] = [];
  for (const fx of readdirSync(FIXTURES).sort()) {
    for (const sub of ["minified", "build", "source"]) {
      const d = join(FIXTURES, fx, sub);
      if (existsSync(d)) walkJs(d, out);
    }
  }
  return out;
}

const astral = "\u{1F600}";
const SYNTHETIC: Record<string, string | Buffer> = {
  "ws-feff.js": "var\u{feff}__export = 1",
  "ws-nbsp-ideographic.js": "var\u{a0}\u{3000}__export = 1",
  "ws-nel.js": "var\u{85}__export = 1",
  "browserify-ls.js": "x[0].call(m.exports }",
  "browserify-nel.js": "x[0].call(m.exports\u{85}}",
  "wb-nonascii-before.js": "é__commonJS(",
  "wb-word-before.js": "a__commonJS(",
  "wb-nonascii-after.js": "__commonJSé(",
  "wb-bool-nonascii.js": "x=!0é",
  "wb-bool-word.js": "x=!0a",
  "dot-ls.js": "// a b.js\nvar x=void 0;",
  "dot-cr.js": "// a\rb.js\nvar x=void 0;",
  "dot-crlf.js": "// a.js\r\nvar x=void 0;",
  "dot-empty.js": "// .js\nvar x=void 0;",
  "dot-astral.js": `// é${astral}.js\nvar x=void 0;`,
  "banner-window-out.js": `${astral.repeat(97)}// a.js\n!0`,
  "banner-window-in.js": `${astral.repeat(96)}// a.js\n!0`,
  "scan-window-out.js": `${astral.repeat(8190)}parcelRequire`,
  "scan-window-bool.js": `${astral.repeat(8190)}!0`,
  "scan-window-split-pair.js": `${"a".repeat(16381)}!0${astral}__webpack_require__`,
  "create-require-brace.js":
    'import{a}x{createRequire}from"node:module";var q={exports:{}};',
  "create-require-spaced.js":
    "import \u{feff}{ x, createRequire as C } from 'node:module';var q={exports:\t{}};",
  "bun-banner-bom.js": "\u{feff}\n  //\u{a0}@bun @bytecode",
  "bun-banner-late.js": "x\n// @bun",
  "bun-banner-word.js": "// @bunx",
  "bun-banner-dash.js": "// @bun-cjs",
  "parcel-loader.js": "var l = require \t( '_bundle_loader\" ) ;",
  "bun-dollars-10.js": "$a0 $b1 $c2 $d3 $e4 $f5 $g6 $h7 $i8 $j9 $$ $1a",
  "bun-dollars-11.js": "$a0 $b1 $c2 $d3 $e4 $f5 $g6 $h7 $i8 $j9 $kk",
  "swc-props.js": "_object_spread_props(a)",
  "swc-suffix.js": "_object_spreadX(a) x_class_call_check()",
  "tie-likely.js": `// a.js\n_class_call_check();${"$ab ".repeat(11)}`,
  "invalid-utf8.js": Buffer.concat([
    Buffer.from("var \xff"),
    Buffer.from([0xc3, 0x28, 0xe2, 0x82, 0x20]),
    Buffer.from(" __commonJS( void 0")
  ]),
  "bom-webpack.js": "\u{feff}__webpack_modules__",
  "empty.js": ""
};

function syntheticInputs(dir: string): string[] {
  mkdirSync(dir, { recursive: true });
  return Object.entries(SYNTHETIC).map(([name, body]) => {
    const p = join(dir, name);
    writeFileSync(p, body);
    return p;
  });
}

function rustVerdict(bin: string, input: string): string {
  try {
    return execFileSync(bin, ["detect", input], {
      encoding: "utf-8",
      stdio: ["ignore", "pipe", "pipe"]
    }).trimEnd();
  } catch (e) {
    return `<rust failed: ${(e as Error).message.split("\n")[0]}>`;
  }
}

function main(): void {
  const [bin, outDir] = process.argv.slice(2);
  if (!bin || !outDir) {
    console.error("usage: wpb1-detect-probe.ts <rust-humanify-bin> <out-dir>");
    process.exit(2);
  }
  mkdirSync(outDir, { recursive: true });
  const bundles = VERSIONS.map(bundlePath).filter((p) => existsSync(p));
  const inputs = [
    ...fixtureInputs(),
    ...bundles,
    ...syntheticInputs(join(outDir, "synthetic"))
  ];
  const tsLines: string[] = [];
  const rustLines: string[] = [];
  const divergent: string[] = [];
  const tally = new Map<string, number>();
  for (const input of inputs) {
    const ts = JSON.stringify(detectBundle(readFileSync(input, "utf-8")));
    const rust = rustVerdict(bin, input);
    tsLines.push(JSON.stringify({ input, verdict: ts }));
    rustLines.push(JSON.stringify({ input, verdict: rust }));
    if (ts !== rust) divergent.push(input);
    const v = JSON.parse(ts);
    const key = `${v.bundler.type}/${v.minifier.type}(${v.minifier.tier})`;
    tally.set(key, (tally.get(key) ?? 0) + 1);
  }
  writeFileSync(join(outDir, "ts-verdicts.jsonl"), `${tsLines.join("\n")}\n`);
  writeFileSync(
    join(outDir, "rust-verdicts.jsonl"),
    `${rustLines.join("\n")}\n`
  );
  const fixtureCount =
    inputs.length - bundles.length - Object.keys(SYNTHETIC).length;
  console.log(
    `inputs: ${inputs.length} (fixtures ${fixtureCount}, oracle bundles ${bundles.length}/8, synthetic ${Object.keys(SYNTHETIC).length})`
  );
  for (const [k, n] of [...tally].sort()) console.log(`  verdict ${k}: ${n}`);
  for (const d of divergent) console.log(`DIVERGENT: ${d}`);
  console.log(
    `identical: ${inputs.length - divergent.length}/${inputs.length}${divergent.length === 0 ? "  -> IDENTICAL" : "  -> DIVERGENT"}`
  );
  process.exit(divergent.length === 0 && bundles.length === 8 ? 0 : 1);
}

main();
