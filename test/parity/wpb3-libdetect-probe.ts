// probe: WPB.3's gate — library-detection verdicts, TS vs Rust, byte-exact.
//
// For every input: the REAL TS chain — detectBundle → selectAdapter →
// adapter.unpack (bun: no namer, no prior) → selectLibraryDetector →
// detectLibraries — serialized as one JSON line (paths relative to the
// unpack dir, CommentRegion offsets as the TS reports them: UTF-16 code
// units), plus `findCommentRegions` over the whole input. Then the Rust verb
// `humanify libdetect` on the same input, compared byte-for-byte:
//   - non-Bun inputs: the Rust unpacks ITSELF (passthrough; webcrack through
//     scripts/webcrack-shim.ts) into its own dir;
//   - Bun inputs: the Rust detects on the TS's unpack file list (`--files`),
//     because Bun vendor file names derive from the structural hash BYTES,
//     which differ by design (docs/rust-port/00-control.md §3).
// For the four oracle pairs' fresh bundles it also checks the Rust's
// runtime.js regions against the dump's regions.json `commentRegions`.
//
// Inputs: every *.js under test/e2e/fixtures/<fixture>/{minified,build,source},
// the eight oracle bundles (READ ONLY), and synthetic vectors under
// <out>/synthetic/ (the TS unit tests' fixtures + the V8 regex edges).
//
// Usage: npx tsx test/parity/wpb3-libdetect-probe.ts <rust-humanify-bin> <out-dir>
// Exit 0 iff every input's verdict is identical.

import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { detectBundle } from "../../src/detection/detect.js";
import {
  type CommentRegion,
  findCommentRegions
} from "../../src/library-detection/comment-regions.js";
import { selectLibraryDetector } from "../../src/library-detection/index.js";
import type { LibraryDetectionResult } from "../../src/library-detection/types.js";
import type { PipelineConfig } from "../../src/pipeline/types.js";
import { selectAdapter } from "../../src/unpack/index.js";

const REPO = resolve(import.meta.dirname, "../..");
const FIXTURES = join(REPO, "test/e2e/fixtures");
const SHIM = join(REPO, "scripts/webcrack-shim.ts");
const PAIRS = [
  ["85", "86"],
  ["118", "119"],
  ["197", "198"],
  ["215", "216"]
];
const MAIN_REPO = dirname(
  execFileSync(
    "git",
    ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    { cwd: REPO, encoding: "utf-8" }
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
    if (statSync(p).isDirectory()) walkJs(p, out);
    else if (name.endsWith(".js")) out.push(p);
  }
}

const WEBPACK = readFileSync(
  "/work/rust-port/gates/wpb2/webpack-synthetic.js",
  "utf-8"
);
const BUN_HEAD =
  "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n";
const pad = (n: number) => "x".repeat(n);
const SYNTHETIC: Record<string, string> = {
  // comment-regions.test.ts / default.test.ts fixtures, as whole inputs
  "single.js": "/*! React v18.2.0 */\nfunction a() {}",
  "multi.js":
    "/*! React v18.2.0 */\nfunction reactInternal() {}\n/*! zustand v4.0.0 */\nfunction zustandStore() {}",
  "license.js": "/** @license lodash */\nvar _ = {};",
  "module.js": "/** @module underscore */\nvar _ = {};",
  "star.js": "/**\n * axios v1.6.0\n */\nfunction send() {}",
  "deep.js": `${pad(2000)}\n/*! React v18.2.0 */\nfunction a() {}`,
  "dash.js": "/*! moment - v2.29.4 */\nfunction m() {}",
  "punct.js": "/*! jQuery, v3.6.0 */\nvar $;",
  "dedupe.js": "/*! lodash v4.17.21 */\nvar _ = {};",
  "mixed.js": `var appCode = ${JSON.stringify(pad(1100))};\n\n/*! React v18.2.0 */\nfunction reactInternal() { return 2; }\n/*! zustand v4.0.0 */\nfunction zustandStore() { return 3; }`,
  "late.js": "var appVar = 1;\n/*! React v18.2.0 */\nfunction a() {}",
  // V8 regex edges: \s is WhiteSpace+LineTerminator (U+FEFF yes, U+0085 no)
  "ws-feff.js": "var a;\n/*!\u{feff}Lib\u{2028}v1.0 */",
  "ws-nel.js": "var a;\n/*! a\u{85}b v1.0 */",
  "ws-nbsp.js": "var a;\n/*!\u{a0}Nb\u{3000}-\u{2029}v2.0 */",
  "dash-nospace.js": "var a;\n/*! foo -v1.0 */",
  "vdot.js": "var a;\n/*! foo v.x */ /*! bar vx */",
  "license-star.js":
    "/*@license a */ /**@license b */ /* *@license c */ /** @licensed d */",
  "star-semver.js": " * pkg v1.22.333\n * short v1.2\n *nospace v1.2.3",
  "normalize.js": "/*! @Babel/Runtime,;:! v7.0.0 */ /*! @@x v1.0 */",
  "unicode-name.js": "/*! ÉMILE! v1.0.0 */ /*! caf\u{e9}\u{1F600} v2.0 */",
  // offsets after astral chars are UTF-16 indexes in the TS
  "astral-offsets.js": `var s = "\u{1F600}\u{1F600}é";\n/*! one v1.0 */\n"\u{1F601}";\n/*! two v2.0 */`,
  // the 1KB header window counts UTF-16 units: astral chars before a banner
  "header-window.js": `${"\u{1F600}".repeat(505)}/*! edge v1.0 */`,
  "header-window-miss.js": `${"\u{1F600}".repeat(508)}/*! edge v1.0 */`,
  // Bun: factories with banners (manifest path), and a bun-detected input
  // without the factory helper (index.js floor → no manifest → banner scan)
  "bun-factories.js": `import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);\n${BUN_HEAD}/*! axios v1.2.3 */\nvar a=x((exports,module)=>{module.exports=function axios(){}});\nvar b=x((exports)=>{exports.v=1});\nvar main=a();`,
  "bun-no-helper.js":
    'import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);var L=(I,A,q)=>(q=I!=null?Object.create(null):A,Object.defineProperty(q,"default",{enumerable:!0,value:I}));\n/*! late v9.9.9 */\nvar z=1;',
  // bun-detected, helper present, no factory → index.js floor → the Bun
  // detector's no-manifest banner scan (whole file, first PATTERN wins)
  "bun-no-factories.js": `import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);\n${BUN_HEAD}var q=${JSON.stringify(pad(2000))};\n/** @license MIT deep */\n/*! later v1.0.0 */`,
  // webpack through the webcrack adapter (the shim on the Rust side)
  "webpack.js": WEBPACK,
  "webpack-banner.js": `/*! wp-lib v1.0.0 */\n${WEBPACK}`
};

function regionsJson(regions: CommentRegion[]): object[] {
  return regions.map((r) => ({
    libraryName: r.libraryName,
    startOffset: r.startOffset,
    endOffset: r.endOffset
  }));
}

function serialize(
  adapter: string,
  detector: string,
  result: LibraryDetectionResult,
  dir: string,
  code: string
): string {
  const rel = (p: string) => relative(dir, p).split("\\").join("/");
  return JSON.stringify({
    adapter,
    detector,
    libraryFiles: [...result.libraryFiles].map(([p, d]) => [rel(p), d]),
    novelFiles: result.novelFiles.map(rel),
    mixedFiles: [...result.mixedFiles].map(([p, m]) => [
      rel(p),
      { regions: regionsJson(m.regions), libraryNames: m.libraryNames }
    ]),
    inputRegions: regionsJson(findCommentRegions(code))
  });
}

interface Outcome {
  id: string;
  same: boolean;
  ts: string;
  rust: string;
}

async function probe(
  bin: string,
  out: string,
  id: string,
  input: string
): Promise<Outcome> {
  const code = readFileSync(input, "utf-8");
  const adapter = selectAdapter(detectBundle(code));
  const tsDir = join(out, "ts", id);
  const rustDir = join(out, "rust", id);
  rmSync(tsDir, { recursive: true, force: true });
  rmSync(rustDir, { recursive: true, force: true });
  const { files } = await adapter.unpack(code, tsDir);
  const detector = selectLibraryDetector({
    unpackAdapterName: adapter.name
  } as PipelineConfig);
  const result = await detector.detectLibraries(files);
  const ts = serialize(adapter.name, detector.name, result, tsDir, code);

  const args = ["libdetect", input];
  if (adapter.name === "bun") {
    const listPath = join(out, "ts", `${id}.files.json`);
    writeFileSync(listPath, JSON.stringify(files));
    args.push(tsDir, "--files", listPath);
  } else {
    args.push(rustDir, "--webcrack-shim", SHIM);
  }
  let rust: string;
  try {
    rust = execFileSync(bin, args, {
      encoding: "utf-8",
      maxBuffer: 1 << 30
    }).trimEnd();
  } catch (err) {
    rust = `ERROR ${(err as Error).message.split("\n")[0]}`;
  }
  // A non-Bun Rust unpack wrote its own tree: it must be the TS's, byte
  // for byte (passthrough's index.js; webcrack's files through the shim).
  if (adapter.name !== "bun" && !treesEqual(tsDir, rustDir)) {
    rust = `TREE DIFFERS ${rust}`;
  }
  return { id, same: ts === rust, ts, rust };
}

function treeFiles(root: string, out = new Map<string, string>(), dir = root) {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir).sort()) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) treeFiles(root, out, p);
    else out.set(relative(root, p), readFileSync(p, "latin1"));
  }
  return out;
}

function treesEqual(a: string, b: string): boolean {
  const fa = treeFiles(a);
  const fb = treeFiles(b);
  return (
    fa.size > 0 &&
    fa.size === fb.size &&
    [...fa].every(([k, v]) => fb.get(k) === v)
  );
}

/** The dump's commentRegions for a pair vs the Rust verdict's runtime.js
 * regions (bytes in the dump — its span unit — vs UTF-16 in the verdict:
 * compared as start/end/library, both empty on every Bun pair so far). */
function regionsDumpCheck(rustJson: string, pair: string): string {
  const dump = JSON.parse(
    readFileSync(
      `/work/oracle/oracle-f7a707d/dumps/${pair}/regions.json`,
      "utf-8"
    )
  ) as {
    commentRegions: { span: { start: number; end: number }; library: string }[];
  };
  const verdict = JSON.parse(rustJson) as {
    mixedFiles: [string, { regions: CommentRegion[] }][];
  };
  const runtime = verdict.mixedFiles.find(([p]) => p === "runtime.js");
  const rustRegions = (runtime?.[1].regions ?? []).map((r) => ({
    span: { start: r.startOffset, end: r.endOffset ?? -1 },
    library: r.libraryName
  }));
  const same =
    JSON.stringify(rustRegions) === JSON.stringify(dump.commentRegions);
  return `${pair} regions.json commentRegions=${dump.commentRegions.length} rust=${rustRegions.length} ${same ? "EQUAL" : "DIFFER"}`;
}

/** Every input: the fixture set, the eight bundles, the synthetic vectors. */
function collectInputs(out: string): [string, string][] {
  const inputs: [string, string][] = [];
  for (const fx of readdirSync(FIXTURES).sort()) {
    const found: string[] = [];
    for (const sub of ["minified", "build", "source"]) {
      const d = join(FIXTURES, fx, sub);
      if (existsSync(d)) walkJs(d, found);
    }
    for (const p of found) {
      inputs.push([relative(FIXTURES, p).replace(/[/\\]/g, "__"), p]);
    }
  }
  for (const [from, to] of PAIRS) {
    for (const v of [from, to]) inputs.push([`bundle-2.1.${v}`, bundlePath(v)]);
  }
  for (const [name, text] of Object.entries(SYNTHETIC)) {
    const p = join(out, "synthetic", name);
    writeFileSync(p, text);
    inputs.push([`synthetic__${name}`, p]);
  }
  return inputs;
}

function report(outcomes: Outcome[], out: string): number {
  const byAdapter = new Map<string, number>();
  for (const o of outcomes) {
    const adapter = (JSON.parse(o.ts) as { adapter: string }).adapter;
    byAdapter.set(adapter, (byAdapter.get(adapter) ?? 0) + 1);
    if (!o.same) {
      console.log(
        `DIFF ${o.id}\n  ts:   ${o.ts.slice(0, 600)}\n  rust: ${o.rust.slice(0, 600)}`
      );
    }
  }
  for (const [from, to] of PAIRS) {
    const rust = outcomes.find((o) => o.id === `bundle-2.1.${to}`)?.rust;
    if (rust && !rust.startsWith("ERROR")) {
      console.log(regionsDumpCheck(rust, `2.1.${from}-2.1.${to}`));
    }
  }
  const same = outcomes.filter((o) => o.same).length;
  console.log(
    `adapters: ${[...byAdapter].map(([a, n]) => `${a}=${n}`).join(" ")}`
  );
  console.log(`libdetect: ${same}/${outcomes.length} identical`);
  writeFileSync(
    join(out, "verdicts.jsonl"),
    `${outcomes.map((o) => JSON.stringify({ id: o.id, same: o.same, ts: o.ts })).join("\n")}\n`
  );
  return outcomes.length - same;
}

async function main(): Promise<void> {
  const [bin, outArg] = process.argv.slice(2);
  if (!bin || !outArg) {
    throw new Error("usage: wpb3-libdetect-probe.ts <rust-bin> <out-dir>");
  }
  const out = resolve(outArg);
  mkdirSync(join(out, "synthetic"), { recursive: true });
  mkdirSync(join(out, "ts"), { recursive: true });
  const outcomes: Outcome[] = [];
  for (const [id, input] of collectInputs(out)) {
    outcomes.push(await probe(bin, out, id, input));
  }
  if (report(outcomes, out) > 0) process.exit(1);
}

void main();
