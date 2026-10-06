/**
 * The `e2e` check stage: the RELEASE BINARY run end to end on the committed
 * e2e fixtures, and its output booted.
 *
 *   npx tsx scripts/e2e.ts          # needs target/release/humanify (rust:build)
 *
 * Since the cutover (docs/rust-port/19-cutover.md) the pipeline is the Rust
 * binary, so this replaces the TS `*.e2etest.ts` suite. Per fixture with a
 * committed build (test/e2e/fixtures/<name>/build/v<ver>/build/index.js) and
 * per version pair in its fixture.config.json:
 *
 *  1. FRESH — the binary humanifies v1 against a stub LLM endpoint. The stub
 *     answers every naming prompt with `<id>Renamed` for each identifier the
 *     prompt lists, so the whole LLM path runs — request, parse, validate,
 *     apply — and a run where no rename lands fails the stage.
 *  2. PRIOR — v2 with `--prior-version` = step 1's output: the cross-version
 *     path (matching, name transfer, reconcile).
 *  3. DETERMINISM — step 2 again into a second directory: the two trees must
 *     be byte-identical (the stub is deterministic, so any difference is the
 *     binary's own).
 *  4. BOOT — the input and every output are imported by Node; each must load,
 *     export the same names with the same types, and every exported function
 *     called with no arguments must return a value of the same shape. A
 *     rename that breaks a binding, a scope or the module surface fails here.
 *     STRICT: an export name is the module's API, so an output that exports
 *     under any other name fails (finding #55 — the naming stage renamed
 *     `export function createStore`; the `esm-exports` fixture holds every
 *     export form).
 *  5. SEQUENTIAL — the conservative naming schedule, `--sequential
 *     --batch-size 10` (docs/rust-port/20-fast-mode.md §defaults): fresh
 *     and prior each run twice, byte-identical, and BOTH trees must equal
 *     the committed legacy goldens (test/golden/legacy-default/) — the
 *     pre-2026-09-28 binary's default path (stashed at
 *     /work/preflip-humanify-00e171f1), captured at 00e171f1 and
 *     REGENERATED 2026-09-28: the original capture's bytes were not
 *     reproducible even by its own source binary (const-vs-let on two
 *     declarations; see /work/overnight-notes.md), so the goldens were
 *     re-captured with this file's exact stub + invocation, twice each,
 *     deterministic. The proof
 *     that `--sequential` still ships the old default's bytes — and the
 *     output boots (step 4). The DEFAULT schedule is the relaxed tier, so
 *     its determinism is step 3's assert (the old `--fast` legs are gone:
 *     `--fast` was deleted in the flip).
 *
 * What it cannot see: the split tree and its run scaffold (the fixtures are
 * single-module libraries, not bundles), and model quality. Both belong to
 * the eval (`npm run eval -- score`), which boots each split tree on four
 * real release pairs.
 *
 * BUNDLE fixtures (`"bundle": true` in fixture.config.json — e.g.
 * `esbuild-bundle`, a real esbuild 0.27.2 iife build) ARE split trees:
 * every leg runs with `--split`, and the boot step compares the input
 * bundle's observable behavior (stdout + exit of `node <input>`) with
 * `node run.cjs` in the emitted runnable graph (`bun-bundle`, a real Bun
 * CJS build, runs its input the way Bun's loader does —
 * BUN_CJS_LOADER). The legacy-golden step
 * only runs for pairs with committed goldens — a post-flip fixture has
 * no pre-flip golden to prove anything against, so the comparison is
 * skipped with a printed note instead of silently committed against
 * itself.
 *
 * DETECTION is recorded for every pair: `humanify detect --toolchain` on
 * the v1 input (bundler, minifier, unpack adapter, bundle layout) is
 * printed first, and a fixture's `expect` block (bundler, unpack adapter,
 * a non-empty vendor/) is checked LAST, after the run steps — so a fixture
 * whose detection is wrong is still run, split and booted end to end.
 *
 * THE SPLIT METHOD (`expect.splitMethod`, `expect.apart`): the fresh run
 * writes `--stats-json`, and its `splitMethod.method` must be the one the
 * fixture declares — `module-markers` (the bundle's lazy-init modules
 * describe it: `bun-lazy`), `fresh-grouping` (they do not: `bun-plain`,
 * `bun-mixed`, the one-planted-module fixtures), or `not-split` (a bundle
 * layout the split does not read — an ES module's or esbuild
 * `--format=cjs`'s top level: the run exits 0 with a WARNING and the named
 * output unsplit; there is no runnable tree to boot, so the boot step
 * says so and checks no tree was written). `apart` names marker strings
 * that must land in DIFFERENT files of the fresh tree's src/ — the
 * mixed bundle's eager modules used to pile into one src/index.js.
 *
 * KNOWN GAPS (`KNOWN_GAPS` below): a fixture that exposes a gap the
 * pipeline has today (a minified esbuild build detected as unknown — spec
 * I2) must fail EXACTLY as its entry declares. It is printed as a
 * KNOWN GAP and the
 * stage stays green; failing any other way, or PASSING, fails the stage —
 * so the fix that closes a gap is told to delete the entry, and the
 * fixture becomes a passing case.
 *
 * Pass/fail, never advisory. Exit 1 on the first failure, naming it.
 */
import { spawn, spawnSync } from "node:child_process";
import * as fs from "node:fs";
import type * as http from "node:http";
import type { AddressInfo } from "node:net";
import * as os from "node:os";
import * as path from "node:path";

import { startStubLlm, stubAnswer } from "./lib/stub-llm.js";

const REPO = path.resolve(import.meta.dirname, "..");
const BIN = path.join(REPO, "target/release/humanify");
const FIXTURES = path.join(REPO, "test/e2e/fixtures");

interface VersionPair {
  v1: string;
  v2: string;
}

/**
 * A fixture whose committed build is a real BUNDLE (not a single-module
 * library). Such fixtures run the pipeline WITH `--split`, so the output
 * is the runnable CJS module graph (`src/` + `vendor/` + `run.cjs`), not
 * one rewritten file:
 *
 *   - the "rename landed" check scans the whole output tree;
 *   - `--prior-version` points at the fresh run's
 *     `.humanify/humanified.js`, the tree the next release diffs against;
 *   - the boot step compares BEHAVIOR (stdout + exit of `node run.cjs`
 *     vs `node <input>`, both run in place) — an iife bundle exports
 *     nothing, so there is no import surface to compare (the export-name
 *     invariant keeps its own fixture: `esm-exports`);
 *   - step 5's legacy-golden comparison only runs when a golden for the
 *     pair is committed (the goldens predate bundle fixtures; a new
 *     fixture has none and can never prove the pre-flip default).
 */
interface FixtureConfig {
  versionPairs: VersionPair[];
  bundle?: boolean;
  expect?: FixtureExpectations;
}

/**
 * What a fixture's input MUST be recognised as, and what the run must do
 * with it — checked after every other step passed, so a fixture with a
 * known detection gap is still run, split and booted end to end first.
 *
 *   - `bundler` — `humanify detect`'s bundler verdict on the v1 input;
 *   - `unpackAdapter` — the toolchain's unpack adapter for it
 *     (`detect --toolchain`, the same resolution the run makes);
 *   - `vendor` — the fresh split tree's `vendor/` holds at least one
 *     extracted dependency (bundle fixtures);
 *   - `splitMethod` — the fresh run's `--stats-json` `splitMethod.method`
 *     (bundle fixtures);
 *   - `apart` — pairs of marker strings that must sit in DIFFERENT files
 *     of the fresh tree's src/ (each must be found);
 *   - `assets` — app text asset files the fresh tree must hold (finding
 *     #88: a module that is only app text is an app asset, not vendored);
 *   - `stays` — marker strings whose vendor/ file must keep the SAME path
 *     from the fresh tree to the prior leg's (finding #88: a vendor module
 *     whose string changed length is carried by content, file and all).
 */
interface FixtureExpectations {
  bundler?: string;
  unpackAdapter?: string;
  vendor?: boolean;
  splitMethod?: string;
  apart?: [string, string][];
  assets?: string[];
  stays?: string[];
}

/** What the fresh run did, as the expectations read it. */
interface ObservedRun {
  vendorFiles: number;
  splitMethod?: string;
}

/** `humanify detect --toolchain`'s verdict on one input, as recorded. */
interface Detection {
  bundler: string;
  bundlerTier?: string;
  minifier?: string;
  unpackAdapter: string;
  bundleLayout?: string;
}

/**
 * A gap the pipeline HAS today, recorded by a fixture that exposes it.
 * The fixture runs like every other; it must then fail, and fail with a
 * message containing `error` — that keeps the stage green while the gap
 * is open, and gives its fix a red test to turn green. The entry is the
 * contract, both ways (`judgeKnownGap`):
 *
 *   - the fixture fails some OTHER way → the stage fails (the gap moved;
 *     re-read it before re-declaring);
 *   - the fixture PASSES → the stage fails: the gap is fixed, so delete
 *     the entry and the fixture joins the gate as a passing case.
 *
 * Keyed by fixture: every pair of the fixture is held to the entry.
 */
export interface KnownGap {
  fixture: string;
  /** The spec / review / finding item the gap is filed under. */
  spec: string;
  reason: string;
  /** A substring of the failure, exact as the pipeline or harness says it. */
  error: string;
}

export const KNOWN_GAPS: KnownGap[] = [
  {
    fixture: "esbuild-minified",
    spec: "docs/plugin-spec.md I2; toolchain review R18",
    reason:
      "esbuild's detector reads its helper NAMES (`__commonJS`, `__toESM`, …), which --minify shortens to letters: the build detects as unknown, gets the do-nothing (passthrough) adapter and keeps every dependency in the app — no vendor/ (the split still runs and boots)",
    error: "detection: bundler is unknown, expected esbuild"
  }
];

/** How a fixture's outcome reads against its known-gap entry, if any. */
export function judgeKnownGap(
  gap: KnownGap | undefined,
  failure: string | null
): { verdict: "pass" | "known-gap" | "fail"; message: string } {
  if (!gap) {
    return failure === null
      ? { verdict: "pass", message: "" }
      : { verdict: "fail", message: failure };
  }
  if (failure === null) {
    return {
      verdict: "fail",
      message: `known gap "${gap.fixture}" (${gap.spec}) now PASSES — the gap is fixed: delete its KNOWN_GAPS entry in scripts/e2e.ts so the fixture gates as a passing case`
    };
  }
  if (!failure.includes(gap.error)) {
    return {
      verdict: "fail",
      message: `known gap "${gap.fixture}" (${gap.spec}) changed: expected a failure containing ${JSON.stringify(gap.error)}, got:\n${failure}`
    };
  }
  const quoted =
    failure.split("\n").find((l) => l.includes(gap.error)) ?? gap.error;
  return {
    verdict: "known-gap",
    message: `KNOWN GAP ${gap.fixture} (${gap.spec}): ${gap.reason}\n    fails as declared: ${quoted.trim()}`
  };
}

/** KNOWN_GAPS entries that name no fixture, or a fixture twice. */
export function staleKnownGaps(gaps: KnownGap[], fixtures: string[]): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const g of gaps) {
    if (seen.has(g.fixture)) {
      out.push(`KNOWN_GAPS declares "${g.fixture}" twice`);
    } else if (!fixtures.includes(g.fixture)) {
      out.push(`KNOWN_GAPS entry "${g.fixture}" names no fixture`);
    }
    seen.add(g.fixture);
  }
  return out;
}

/** Each expectation the observed run missed, as one line. */
export function expectationFailures(
  expect: FixtureExpectations | undefined,
  observed: Detection,
  run: ObservedRun
): string[] {
  const out: string[] = [];
  if (expect?.bundler !== undefined && observed.bundler !== expect.bundler) {
    out.push(
      `detection: bundler is ${observed.bundler}, expected ${expect.bundler}`
    );
  }
  if (
    expect?.unpackAdapter !== undefined &&
    observed.unpackAdapter !== expect.unpackAdapter
  ) {
    out.push(
      `toolchain: unpack adapter is ${observed.unpackAdapter}, expected ${expect.unpackAdapter}`
    );
  }
  if (expect?.vendor && run.vendorFiles === 0) {
    out.push(
      "unpack: the split tree's vendor/ is empty, expected the extracted dependencies"
    );
  }
  if (
    expect?.splitMethod !== undefined &&
    run.splitMethod !== expect.splitMethod
  ) {
    out.push(
      `split: the fresh run's split method is ${run.splitMethod}, expected ${expect.splitMethod}`
    );
  }
  return out;
}

/**
 * Each `apart` pair that is NOT in separate files: `files` maps a tree's
 * file (relative path) to its text.
 */
export function apartFailures(
  pairs: [string, string][],
  files: Map<string, string>
): string[] {
  const out: string[] = [];
  const holders = (marker: string) =>
    [...files].filter(([, text]) => text.includes(marker)).map(([f]) => f);
  for (const [a, b] of pairs) {
    const [inA, inB] = [holders(a), holders(b)];
    const missing = [a, b].filter((_, i) => [inA, inB][i].length === 0);
    for (const m of missing) out.push(`split: no file holds "${m}"`);
    if (missing.length > 0) continue;
    const shared = inA.find((f) => inB.includes(f));
    if (shared !== undefined) {
      out.push(
        `split: "${a}" and "${b}" share ${shared}, expected separate files`
      );
    }
  }
  return out;
}

/** Each expected app text asset the tree's file list lacks. */
export function assetFailures(assets: string[], files: string[]): string[] {
  return assets
    .filter((a) => !files.includes(a))
    .map((a) => `unpack: no app text asset ${a}`);
}

/**
 * Each `stays` marker whose vendor file moved between the two releases:
 * `fresh` and `prior` map each tree's vendor files to their text.
 */
export function staysFailures(
  markers: string[],
  fresh: Map<string, string>,
  prior: Map<string, string>
): string[] {
  const holder = (files: Map<string, string>, marker: string) =>
    [...files].find(([, text]) => text.includes(marker))?.[0];
  const out: string[] = [];
  for (const m of markers) {
    const [a, b] = [holder(fresh, m), holder(prior, m)];
    if (a === undefined || b === undefined) {
      out.push(`unpack: no vendor file holds "${m}" in both releases`);
    } else if (a !== b) {
      out.push(`unpack: "${m}" moved from ${a} to ${b} across the release`);
    }
  }
  return out;
}

/**
 * A check failed. Thrown, not exited on, so the stage can hold a
 * known-gap fixture to its declared failure (`judgeKnownGap`).
 */
class E2EFailure extends Error {}

function fail(msg: string): never {
  throw new E2EFailure(msg);
}

export { stubAnswer };

/** The e2e stub — the shared module's, on an ephemeral port. */
function startStub(): Promise<http.Server> {
  return startStubLlm().then(({ server }) => server);
}

/**
 * ASYNC on purpose: the stub LLM lives in this process, so a synchronous
 * spawn would block the event loop that has to answer the binary's requests
 * (the first draft hung exactly so).
 */
async function runBinary(
  args: string[],
  endpoint: string,
  label: string
): Promise<string> {
  const child = spawn(
    BIN,
    [...args, "--endpoint", endpoint, "--api-key", "e2e", "--retries", "0"],
    { stdio: ["ignore", "ignore", "pipe"] }
  );
  let stderr = "";
  child.stderr.on("data", (c) => {
    stderr += c;
  });
  const code = await new Promise<number | null>((resolve) =>
    child.on("close", resolve)
  );
  if (code !== 0) {
    fail(`${label}: the binary exited ${code}\n${stderr.slice(-2000)}`);
  }
  return stderr;
}

/**
 * The prior the next release diffs against, as the run names it on its
 * `Next release: --prior-version <path>` line: a split tree's
 * `.humanify/humanified.js`, or — a bundle the split does not read — the
 * named file written unsplit.
 */
function nextReleasePrior(stderr: string, label: string): string {
  const m = /^Next release: --prior-version (.+)$/m.exec(stderr);
  if (!m) fail(`${label}: the run named no prior for the next release`);
  return m[1];
}

/**
 * Every file of a split tree's app code, `src/` (path → text); empty when
 * the run wrote no tree.
 */
function appTexts(tree: string): Map<string, string> {
  return dirTexts(tree, "src");
}

/** Every file under `tree/<dir>`, keyed `<dir>/<relative path>`. */
function dirTexts(tree: string, dir: string): Map<string, string> {
  const root = path.join(tree, dir);
  if (!fs.existsSync(root)) return new Map();
  return new Map(
    treeFiles(root).map((f) => [
      `${dir}/${f}`,
      fs.readFileSync(path.join(root, f), "utf8")
    ])
  );
}

/** The fresh run's `--stats-json` `splitMethod.method`, if written. */
function splitMethodOf(statsPath: string): string | undefined {
  if (!fs.existsSync(statsPath)) return undefined;
  const stats = JSON.parse(fs.readFileSync(statsPath, "utf8")) as {
    splitMethod?: { method: string };
  };
  return stats.splitMethod?.method;
}

/** Every file under `dir`, relative, sorted. */
function treeFiles(dir: string): string[] {
  return (fs.readdirSync(dir, { recursive: true }) as string[])
    .filter((f) => fs.statSync(path.join(dir, f)).isFile())
    .sort();
}

function assertIdenticalTrees(a: string, b: string, label: string): void {
  const fa = treeFiles(a);
  const fb = treeFiles(b);
  if (fa.join("\n") !== fb.join("\n")) {
    fail(`${label}: the two runs wrote different file sets`);
  }
  for (const f of fa) {
    if (
      !fs.readFileSync(path.join(a, f)).equals(fs.readFileSync(path.join(b, f)))
    ) {
      fail(`${label}: ${f} differs between two identical runs`);
    }
  }
}

/**
 * The module's observable surface: export names → type, and for each
 * exported function its no-argument result's shape. Run in a child Node so
 * a module that throws on load fails this fixture, not the stage runner.
 */
function surfaceOf(file: string, label: string): string {
  const probe = `
    const m = await import(${JSON.stringify(`file://${file}`)});
    const shape = (v) => v === null ? "null" : typeof v === "object"
      ? "{" + Object.keys(v).sort().map((k) => k + ":" + typeof v[k]).join(",") + "}"
      : typeof v;
    const out = {};
    for (const k of Object.keys(m).sort()) {
      out[k] = typeof m[k];
      if (typeof m[k] === "function") {
        try { out[k + "()"] = shape(m[k]()); } catch (e) { out[k + "()"] = "throws " + e.constructor.name; }
      }
    }
    console.log(JSON.stringify(out));
  `;
  const r = spawnSync(process.execPath, ["--input-type=module", "-e", probe], {
    encoding: "utf8",
    cwd: path.dirname(file)
  });
  if (r.status !== 0) {
    fail(`${label}: ${file} does not load under Node:\n${r.stderr}`);
  }
  return r.stdout.trim();
}

/** Copy a file into an ESM scope so Node reads its `export`s as a module. */
function asModule(file: string, dir: string): string {
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "package.json"), '{"type":"module"}\n');
  const dest = path.join(dir, "index.js");
  fs.copyFileSync(file, dest);
  return dest;
}

/**
 * Bun's CommonJS output (`bun build --format=cjs`, first line
 * `// @bun @bun-cjs`) is ONE function expression,
 * `(function(exports, require, module, __filename, __dirname) {…})`, that
 * Bun's loader calls; Node evaluating the file as-is calls nothing and
 * prints nothing. This does what Bun's loader does: evaluate the file to
 * that function and call it with a real CommonJS module for the file.
 */
const BUN_CJS_LOADER = `
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const Module = require("node:module");
const file = path.resolve(process.argv[1]);
const mod = new Module(file, null);
mod.filename = file;
mod.paths = Module._nodeModulePaths(path.dirname(file));
const wrapper = vm.runInThisContext(fs.readFileSync(file, "utf8"), { filename: file });
wrapper.call(mod.exports, mod.exports, Module.createRequire(file), mod, file, path.dirname(file));
`;

/** True for Bun's CommonJS output (its `// @bun … @bun-cjs` first line). */
function isBunCjs(file: string): boolean {
  const first = fs.readFileSync(file, "utf8").split("\n", 1)[0] ?? "";
  return /^\/\/ @bun\b.*@bun-cjs\b/.test(first);
}

/**
 * A BUNDLE fixture's observable behavior: what the file prints and how it
 * exits when Node runs it AS-IS, in its own directory (a split tree's
 * run.cjs requires its sibling files — no copying it away). A Bun CJS
 * bundle runs through BUN_CJS_LOADER, as Bun would run it.
 */
function behaviorOf(
  file: string,
  label: string
): { stdout: string; status: number } {
  const args = isBunCjs(file)
    ? ["-e", BUN_CJS_LOADER, path.basename(file)]
    : [path.basename(file)];
  const r = spawnSync(process.execPath, args, {
    encoding: "utf8",
    cwd: path.dirname(file)
  });
  if (r.status === null) {
    fail(`${label}: ${file} could not be run by Node:\n${r.stderr}`);
  }
  return { stdout: r.stdout, status: r.status ?? -1 };
}

/** True when the committed legacy goldens cover this pair. */
function hasGolden(dir: string): boolean {
  return fs.existsSync(dir);
}

interface FixturePair {
  name: string;
  pair: VersionPair;
  bundle: boolean;
  expect?: FixtureExpectations;
}

function fixturePairs(): FixturePair[] {
  const out: FixturePair[] = [];
  for (const name of fs.readdirSync(FIXTURES).sort()) {
    const cfgPath = path.join(FIXTURES, name, "fixture.config.json");
    if (!fs.existsSync(cfgPath)) continue;
    const cfg = JSON.parse(fs.readFileSync(cfgPath, "utf8")) as FixtureConfig;
    for (const pair of cfg.versionPairs) {
      out.push({ name, pair, bundle: cfg.bundle === true, expect: cfg.expect });
    }
  }
  return out;
}

/**
 * `humanify detect --toolchain` on one input: its second line is the
 * resolved selection + toolchain, the same resolution a run makes.
 */
function detectionOf(file: string, label: string): Detection {
  const r = spawnSync(BIN, ["detect", "--toolchain", file], {
    encoding: "utf8"
  });
  if (r.status !== 0) fail(`${label}: detect --toolchain failed\n${r.stderr}`);
  const lines = r.stdout.trim().split("\n");
  const resolved = JSON.parse(lines[lines.length - 1]) as {
    selection: Detection;
    toolchain: Array<{ piece: string; choice: string }>;
  };
  return {
    ...resolved.selection,
    bundleLayout: resolved.toolchain.find((p) => p.piece === "bundleLayout")
      ?.choice
  };
}

function describeDetection(d: Detection): string {
  return `bundler=${d.bundler} (${d.bundlerTier}), minifier=${d.minifier}, unpack=${d.unpackAdapter}, layout=${d.bundleLayout}`;
}

/** The extracted dependencies a split tree's vendor/ holds. */
function vendorFileCount(tree: string): number {
  const dir = path.join(tree, "vendor");
  if (!fs.existsSync(dir)) return 0;
  return treeFiles(dir).filter((f) => f.endsWith(".js")).length;
}

function inputOf(name: string, version: string): string {
  const p = path.join(FIXTURES, name, "build", `v${version}`, "build/index.js");
  if (!fs.existsSync(p)) fail(`${name}: no committed build at ${p}`);
  return p;
}

/**
 * Steps 1: the fresh run — the binary humanifies v1 against the stub, and
 * a stub rename MUST land (it proves the whole LLM path ran). A bundle
 * fixture's output is a tree, so the rename check scans every file, and
 * its stats file records the split method. Returns the prior reference
 * the prior legs read (the single file, or the one the run names).
 */
async function freshLeg(
  name: string,
  pair: VersionPair,
  endpoint: string,
  bundle: boolean,
  fresh: string,
  splitArgs: string[]
): Promise<string> {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  const statsArgs = bundle ? ["--stats-json", `${fresh}.stats.json`] : [];
  const stderr = await runBinary(
    [inputOf(name, pair.v1), ...splitArgs, ...statsArgs, "-o", fresh],
    endpoint,
    `${label} fresh`
  );
  const freshOut = path.join(fresh, "index.js");
  if (!bundle && !fs.existsSync(freshOut)) {
    fail(`${label}: fresh run wrote no index.js`);
  }
  const renamedLanded = bundle
    ? treeFiles(fresh).some((f) =>
        fs.readFileSync(path.join(fresh, f), "utf8").includes("Renamed")
      )
    : fs.readFileSync(freshOut, "utf8").includes("Renamed");
  if (!renamedLanded) {
    fail(
      `${label}: no stub rename landed — the LLM path did not run end to end`
    );
  }
  return bundle ? nextReleasePrior(stderr, `${label} fresh`) : freshOut;
}

/**
 * Steps 2-3: v2 with `--prior-version` (the cross-version path), twice —
 * the two trees must be byte-identical.
 */
async function priorLeg(
  name: string,
  pair: VersionPair,
  endpoint: string,
  priorReference: string,
  prior: string,
  again: string,
  splitArgs: string[]
): Promise<void> {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  const priorArgs = [inputOf(name, pair.v2), "--prior-version", priorReference];
  await runBinary(
    [...priorArgs, ...splitArgs, "-o", prior],
    endpoint,
    `${label} prior`
  );
  await runBinary(
    [...priorArgs, ...splitArgs, "-o", again],
    endpoint,
    `${label} prior (again)`
  );
  assertIdenticalTrees(prior, again, label);
}

/**
 * One fixture pair, end to end: what detection and the toolchain say
 * about the input (always printed — the record of today's behaviour),
 * the run steps, and last the fixture's declared expectations.
 */
async function checkPair(
  { name, pair, bundle, expect }: FixturePair,
  endpoint: string,
  scratch: string
): Promise<void> {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  const detection = detectionOf(inputOf(name, pair.v1), label);
  console.log(
    `  ${label}: detect v${pair.v1}: ${describeDetection(detection)}`
  );
  const root = path.join(scratch, `${name}-${pair.v1}-${pair.v2}`);
  const fresh = path.join(root, "fresh");
  await runPair(name, pair, endpoint, root, bundle);
  const vendorFiles = bundle ? vendorFileCount(fresh) : 0;
  const splitMethod = bundle ? splitMethodOf(`${fresh}.stats.json`) : undefined;
  if (bundle)
    console.log(
      `  ${label}: fresh split method: ${splitMethod}; vendor/: ${vendorFiles} file(s)`
    );
  const missed = [
    ...expectationFailures(expect, detection, { vendorFiles, splitMethod }),
    ...apartFailures(expect?.apart ?? [], appTexts(fresh)),
    ...assetFailures(expect?.assets ?? [], [...appTexts(fresh).keys()]),
    ...staysFailures(
      expect?.stays ?? [],
      dirTexts(fresh, "vendor"),
      dirTexts(path.join(root, "prior-a"), "vendor")
    )
  ];
  if (missed.length > 0) fail(`${label}: ${missed.join("; ")}`);
}

async function runPair(
  name: string,
  pair: VersionPair,
  endpoint: string,
  root: string,
  bundle: boolean
): Promise<void> {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  const fresh = path.join(root, "fresh");
  const prior = path.join(root, "prior-a");
  const again = path.join(root, "prior-b");
  // A bundle fixture's output is the runnable split tree.
  const splitArgs = bundle ? ["--split"] : [];

  // The prior the next release diffs against: the split tree's
  // humanified source (or the named file a bundle the split does not read
  // was written to), or the single humanified file.
  const priorReference = await freshLeg(
    name,
    pair,
    endpoint,
    bundle,
    fresh,
    splitArgs
  );
  await priorLeg(name, pair, endpoint, priorReference, prior, again, splitArgs);
  const seqPriorA = await sequencedLeg(
    root,
    label,
    name,
    pair,
    endpoint,
    bundle,
    splitArgs
  );

  const legs = { fresh, prior, sequential: seqPriorA };
  if (!bundle) bootSurfaces(name, pair, root, legs);
  else if (fs.existsSync(path.join(fresh, "run.cjs"))) {
    bootTrees(name, pair, legs);
  } else checkUnsplit(label, legs);
}

/** The three output legs the boot step reads. */
interface Legs {
  fresh: string;
  prior: string;
  sequential: string;
}

/**
 * A bundle layout the split does not read: the named output was written
 * unsplit (the fixture's `splitMethod: "not-split"` is checked last). No
 * runnable tree exists to boot; none may have been written.
 */
function checkUnsplit(label: string, legs: Legs): void {
  for (const out of Object.values(legs)) {
    for (const f of ["run.cjs", path.join(".humanify", "split-ledger.json")]) {
      if (fs.existsSync(path.join(out, f))) {
        fail(`${label}: ${out} holds ${f} but no runnable tree`);
      }
    }
  }
  console.log(
    `  ${label}: fresh + prior (+ --sequential, twice) ran unsplit, deterministic — no runnable tree to boot`
  );
}

/**
 * A bundle's observable identity is its BEHAVIOR — same stdout, same exit
 * code — input bundle vs split tree, both run in place.
 */
function bootTrees(name: string, pair: VersionPair, legs: Legs): void {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  for (const [version, out, tag] of [
    [pair.v1, legs.fresh, "fresh"],
    [pair.v2, legs.prior, "prior"],
    [pair.v2, legs.sequential, "sequential"]
  ] as const) {
    const want = behaviorOf(
      inputOf(name, version),
      `${label} input v${version}`
    );
    const got = behaviorOf(
      path.join(out, "run.cjs"),
      `${label} ${tag} output v${version}`
    );
    if (got.stdout !== want.stdout || got.status !== want.status) {
      fail(
        `${label}: v${version}'s split tree behaves differently\n  input:  ${JSON.stringify(want)}\n  output: ${JSON.stringify(got)}`
      );
    }
  }
  console.log(
    `  ${label}: fresh + prior (+ --sequential, twice) split trees ran, deterministic, boots with the input's behavior`
  );
}

/** A single-module fixture boots with the input's module surface. */
function bootSurfaces(
  name: string,
  pair: VersionPair,
  root: string,
  legs: Legs
): void {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  for (const [version, out, tag] of [
    [pair.v1, legs.fresh, "fresh"],
    [pair.v2, legs.prior, "prior"],
    [pair.v2, legs.sequential, "sequential"]
  ] as const) {
    const want = surfaceOf(
      asModule(inputOf(name, version), path.join(root, `boot-in-${version}`)),
      `${label} input v${version}`
    );
    const got = surfaceOf(
      asModule(path.join(out, "index.js"), path.join(root, `boot-out-${tag}`)),
      `${label} ${tag} output v${version}`
    );
    if (got !== want) {
      fail(
        `${label}: v${version}'s output boots with a different surface\n  input:  ${want}\n  output: ${got}`
      );
    }
  }
  console.log(
    `  ${label}: fresh + prior (+ --sequential, twice, vs the legacy goldens) ran, deterministic, boots with the input's surface`
  );
}

/**
 * Step 5: the conservative schedule at the OLD batch size, fresh + prior
 * each run twice for byte-determinism, compared against the pre-flip
 * default's committed goldens when the pair has any (a post-flip fixture
 * has none — see the header). Returns the sequential-prior output dir (the
 * boot step's third leg).
 */
async function sequencedLeg(
  root: string,
  label: string,
  name: string,
  pair: VersionPair,
  endpoint: string,
  bundle: boolean,
  splitArgs: string[]
): Promise<string> {
  const seqArgs = ["--sequential", "--batch-size", "10"];
  const seqFreshA = path.join(root, "seq-fresh-a");
  const seqFreshB = path.join(root, "seq-fresh-b");
  const seqPriorA = path.join(root, "seq-prior-a");
  const seqPriorB = path.join(root, "seq-prior-b");
  const goldenDir = path.join(REPO, "test/golden/legacy-default");
  const freshGolden = path.join(goldenDir, `${name}-${pair.v1}-fresh`);
  const priorGolden = path.join(goldenDir, `${name}-${pair.v1}-${pair.v2}`);
  const seqFreshStderr = await runBinary(
    [inputOf(name, pair.v1), ...splitArgs, ...seqArgs, "-o", seqFreshA],
    endpoint,
    `${label} sequential fresh`
  );
  await runBinary(
    [inputOf(name, pair.v1), ...splitArgs, ...seqArgs, "-o", seqFreshB],
    endpoint,
    `${label} sequential fresh (again)`
  );
  assertIdenticalTrees(seqFreshA, seqFreshB, `${label} --sequential fresh`);
  compareGolden(seqFreshA, freshGolden, label, "--sequential fresh");
  const seqPriorReference = bundle
    ? nextReleasePrior(seqFreshStderr, `${label} sequential fresh`)
    : path.join(seqFreshA, "index.js");
  const seqPriorArgs = [
    inputOf(name, pair.v2),
    "--prior-version",
    seqPriorReference,
    ...seqArgs
  ];
  await runBinary(
    [...seqPriorArgs, ...splitArgs, "-o", seqPriorA],
    endpoint,
    `${label} sequential prior`
  );
  await runBinary(
    [...seqPriorArgs, ...splitArgs, "-o", seqPriorB],
    endpoint,
    `${label} sequential prior (again)`
  );
  assertIdenticalTrees(seqPriorA, seqPriorB, `${label} --sequential prior`);
  compareGolden(seqPriorA, priorGolden, label, "--sequential prior");
  return seqPriorA;
}

/** The byte-comparison against a committed legacy golden, when one exists. */
function compareGolden(
  actual: string,
  goldenDir: string,
  label: string,
  step: string
): void {
  if (hasGolden(goldenDir)) {
    assertIdenticalTrees(
      actual,
      goldenDir,
      `${label} ${step} vs the pre-flip default's golden`
    );
  } else {
    console.log(
      `  ${label}: no committed golden for this pair — golden comparison skipped (${step})`
    );
  }
}

/**
 * Runs one pair and holds it to its known-gap entry: the pair's failure
 * (an E2EFailure — anything else is a harness bug and propagates) or its
 * pass, read by `judgeKnownGap`. Returns true for a recorded known gap.
 */
async function judgedPair(
  fp: FixturePair,
  endpoint: string,
  scratch: string
): Promise<boolean> {
  let failure: string | null = null;
  try {
    await checkPair(fp, endpoint, scratch);
  } catch (e) {
    if (!(e instanceof E2EFailure)) throw e;
    failure = e.message;
  }
  const gap = KNOWN_GAPS.find((g) => g.fixture === fp.name);
  const judged = judgeKnownGap(gap, failure);
  if (judged.verdict === "fail") fail(judged.message);
  if (judged.verdict === "known-gap") console.log(`  ${judged.message}`);
  return judged.verdict === "known-gap";
}

async function main(): Promise<void> {
  if (!fs.existsSync(BIN)) {
    fail(
      `no binary at ${BIN} — the rust:build stage builds it (cargo build --release --locked -p humanify-cli)`
    );
  }
  const pairs = fixturePairs();
  if (pairs.length === 0) fail(`no fixture pairs under ${FIXTURES}`);
  const stale = staleKnownGaps(
    KNOWN_GAPS,
    pairs.map((p) => p.name)
  );
  if (stale.length > 0) fail(stale.join("\n"));
  const server = await startStub();
  const { port } = server.address() as AddressInfo;
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-e2e-"));
  let gaps = 0;
  try {
    for (const fp of pairs) {
      if (await judgedPair(fp, `http://127.0.0.1:${port}/v1`, scratch)) {
        gaps++;
      }
    }
  } finally {
    server.close();
    fs.rmSync(scratch, { recursive: true, force: true });
  }
  console.log(
    `e2e: ${pairs.length - gaps} fixture pair(s) passed, ${gaps} known gap(s) failed exactly as declared`
  );
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  main().catch((e: unknown) => {
    console.error(
      `E2E FAILED: ${e instanceof E2EFailure ? e.message : e instanceof Error ? e.stack : String(e)}`
    );
    process.exit(1);
  });
}
