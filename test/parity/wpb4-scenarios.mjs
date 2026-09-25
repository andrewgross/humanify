#!/usr/bin/env node
// WPB.4 gate part 2: exit codes + error output, TS binary vs Rust binary,
// on every failure the pipeline can reach before its first unported stage.
//
//   node test/parity/wpb4-scenarios.mjs <humanify-binary> [--record]
//
// Each scenario runs in a FRESH temp directory (its own files, a `.env`
// when the scenario wants one) with a minimal environment (PATH, HOME, and
// only the API-key variables the scenario sets). Both legs are compared on:
//   - the exit code;
//   - stdout, byte for byte;
//   - stderr, byte for byte, after one declared normalization: where the TS
//     dies with an uncaught exception (Node prints a source frame, the
//     `Error: <msg>` headline, a stack and `Node.js vX`), only the text
//     before the dump plus the headline are kept — the Rust binary prints
//     that headline as a documented failure (contract 14 §2);
//   - the files the run left in its directory.
// `compare: "headline"` scenarios run stages 3-6 on the TS side (unpack,
// format) whose prose the Rust side cannot print (its formatted text comes
// from --beautified-input); for those only the exit code and the final
// `Error:` line are compared.
//
// --record writes the TS outcomes to test/parity/wpb4-scenarios.json, the
// fixture the cargo integration test (crates/humanify-cli/tests/
// scenarios.rs) replays against the binary without Node.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const here = path.dirname(new URL(import.meta.url).pathname);
const repo = path.resolve(here, "../..");
const bin = path.resolve(process.argv[2] ?? "");
const record = process.argv.includes("--record");
if (!process.argv[2]) {
  console.error("usage: wpb4-scenarios.mjs <humanify-binary> [--record]");
  process.exit(2);
}

const DEAD = ["--endpoint", "http://127.0.0.1:9/v1"];
const fresh60 = Array.from(
  { length: 60 },
  (_, i) => `function f${i}(a, b) {\n  return a * ${i} + b;\n}\n`
).join("");
const other60 = Array.from(
  { length: 60 },
  (_, i) =>
    `function g${i}(x) {\n  if (x) {\n    throw new Error("e${i}");\n  }\n  return [x, x];\n}\n`
).join("");

const IN = { "in.js": "var x = 1;\n" };
const S = [
  // flag invariants (before anything else, no key needed)
  { name: "invariant-split-pure", argv: ["in.js", "--split-pure"], files: IN },
  {
    name: "invariant-all-at-once",
    argv: [
      "in.js",
      "--split-pure",
      "--split-ledger",
      "l.json",
      "--naming-floor-sweep",
      "--no-naming-floor",
      "--bundler",
      "foo",
      "--minifier",
      "gzip"
    ],
    files: IN
  },
  {
    name: "invariant-bundler-unknown",
    argv: ["in.js", "--bundler", "unknown"],
    files: IN
  },
  {
    name: "invariant-before-switch",
    argv: ["in.js", "--split-pure", "--disable", "nope"],
    files: IN
  },
  {
    name: "default-sweep-under-no-floor-passes",
    argv: ["missing.js", "--no-naming-floor", "--api-key", "k"]
  },
  // kill switches
  {
    name: "switch-unknown-disable",
    argv: ["in.js", "--disable", "nope"],
    files: IN
  },
  {
    name: "switch-probe-name-under-disable",
    argv: ["in.js", "--disable", "shingle-probe"],
    files: IN
  },
  {
    name: "switch-disable-name-under-probe",
    argv: ["in.js", "--probe", "family-permute"],
    files: IN
  },
  {
    name: "switch-blank-segments-ok",
    argv: ["missing.js", "--disable", ",,", "--api-key", "k"]
  },
  {
    name: "switch-before-settings",
    argv: ["in.js", "--disable", "nope"],
    files: IN
  },
  // settings
  { name: "settings-missing-api-key", argv: ["in.js"], files: IN },
  {
    name: "settings-empty-api-key-flag",
    argv: ["in.js", "--api-key", ""],
    files: IN,
    env: { HUMANIFY_API_KEY: "h" }
  },
  {
    name: "settings-key-from-humanify-env",
    argv: ["missing.js"],
    env: { HUMANIFY_API_KEY: "h" }
  },
  {
    name: "settings-key-from-openai-env",
    argv: ["missing.js"],
    env: { OPENAI_API_KEY: "o" }
  },
  {
    name: "settings-key-from-dotenv",
    argv: ["missing.js"],
    files: { ".env": "HUMANIFY_API_KEY=fromdotenv\n" }
  },
  {
    name: "settings-bad-reasoning-effort",
    argv: ["in.js", "--api-key", "k", "--reasoning-effort", "extreme"],
    files: IN
  },
  {
    name: "settings-number-order",
    argv: ["in.js", "--api-key", "k", "--timeout", "x", "-c", "abc"],
    files: IN
  },
  {
    name: "settings-bad-concurrency",
    argv: ["in.js", "--api-key", "k", "-c", "abc"],
    files: IN
  },
  {
    name: "settings-bad-max-tokens",
    argv: ["in.js", "--api-key", "k", "--max-tokens", "lots"],
    files: IN
  },
  {
    name: "settings-parseint-lenient",
    argv: ["missing.js", "--api-key", "k", "-c", "12abc"]
  },
  {
    name: "settings-empty-model",
    argv: ["in.js", "--api-key", "k", "--model", ""],
    files: IN
  },
  {
    name: "settings-empty-endpoint",
    argv: ["in.js", "--api-key", "k", "--endpoint", ""],
    files: IN
  },
  // the provider step: the cache dir is created before the input is checked
  {
    name: "llm-cache-created-before-input-check",
    argv: ["missing.js", "--api-key", "k", "--llm-cache", "cache/dir"]
  },
  // input
  { name: "input-missing", argv: ["missing.js", "--api-key", "k"] },
  {
    name: "input-is-a-directory",
    argv: ["dir", "--api-key", "k"],
    files: { "dir/.keep": "" }
  },
  // prior version
  {
    name: "prior-missing",
    argv: ["in.js", "--api-key", "k", "--prior-version", "nope.js"],
    files: IN
  },
  {
    name: "prior-empty",
    argv: ["in.js", "--api-key", "k", "--prior-version", "prior.js"],
    files: { ...IN, "prior.js": "  \n\t\n" }
  },
  {
    name: "prior-not-the-same-program",
    argv: [
      "fresh.js",
      "--api-key",
      "k",
      ...DEAD,
      "--prior-version",
      "other.js",
      "-o",
      "out"
    ],
    files: { "fresh.js": fresh60, "other.js": other60 },
    rustExtraArgs: ["--beautified-input", "fresh.js"],
    compare: "headline"
  },
  // commander (end to end through the real binaries)
  { name: "usage-no-arguments", argv: [] },
  { name: "usage-unknown-option", argv: ["in.js", "--bogus"], files: IN },
  { name: "usage-option-missing-value", argv: ["in.js", "--model"], files: IN },
  { name: "usage-too-many-arguments", argv: ["a.js", "b.js"] },
  { name: "usage-help", argv: ["--help"] },
  { name: "usage-version", argv: ["-V"] },
  { name: "env-reads-missing-path", argv: ["env-reads", "nope"] },
  { name: "env-reads-help", argv: ["env-reads", "--help"] }
];

function listFiles(dir) {
  const out = [];
  const walk = (d) => {
    for (const e of fs
      .readdirSync(d, { withFileTypes: true })
      .sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) {
        out.push(`${path.relative(dir, p)}/`);
        walk(p);
      } else out.push(path.relative(dir, p));
    }
  };
  walk(dir);
  return out;
}

function runLeg(scenario, argv0, extra) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "wpb4-scn-"));
  for (const [rel, content] of Object.entries(scenario.files ?? {})) {
    fs.mkdirSync(path.dirname(path.join(dir, rel)), { recursive: true });
    fs.writeFileSync(path.join(dir, rel), content);
  }
  const before = new Set(listFiles(dir));
  const env = {
    PATH: process.env.PATH,
    HOME: process.env.HOME,
    ...(scenario.env ?? {})
  };
  const r = spawnSync(
    argv0[0],
    [...argv0.slice(1), ...scenario.argv, ...extra],
    {
      cwd: dir,
      env,
      encoding: "utf8",
      timeout: 120_000
    }
  );
  const created = listFiles(dir).filter((f) => !before.has(f));
  fs.rmSync(dir, { recursive: true, force: true });
  return { exitCode: r.status, stdout: r.stdout, stderr: r.stderr, created };
}

/** The TS crash dump reduced to its headline (the declared normalization). */
function normalizeStderr(stderr) {
  const lines = stderr.split("\n");
  const dumpStart = lines.findIndex((l) => /^(\/|node:|file:).*:\d+$/.test(l));
  if (dumpStart === -1) return stderr;
  const headline = lines
    .slice(dumpStart)
    .find((l) => /^[A-Za-z]*Error: /.test(l));
  return `${lines.slice(0, dumpStart).join("\n")}${dumpStart > 0 ? "\n" : ""}${headline ?? "<no headline>"}\n`;
}

const lastErrorLine = (s) =>
  s
    .split("\n")
    .filter((l) => /^Error: /.test(l))
    .pop() ?? null;

const tsCmd = [
  path.join(repo, "node_modules/.bin/tsx"),
  path.join(repo, "src/index.ts")
];
const results = [];
let identical = 0;
for (const s of S) {
  const ts = runLeg(s, tsCmd, []);
  ts.stderr = normalizeStderr(ts.stderr);
  const rs = runLeg(s, [bin], s.rustExtraArgs ?? []);
  const same =
    s.compare === "headline"
      ? ts.exitCode === rs.exitCode &&
        lastErrorLine(ts.stderr) === lastErrorLine(rs.stderr)
      : ts.exitCode === rs.exitCode &&
        ts.stdout === rs.stdout &&
        ts.stderr === rs.stderr &&
        JSON.stringify(ts.created) === JSON.stringify(rs.created);
  if (same) identical++;
  console.log(
    `${same ? "IDENTICAL" : "DIVERGES "}  ${s.name} (exit ts=${ts.exitCode} rust=${rs.exitCode})`
  );
  if (!same) {
    console.log(`  ts   stdout ${JSON.stringify(ts.stdout).slice(0, 300)}`);
    console.log(`  rust stdout ${JSON.stringify(rs.stdout).slice(0, 300)}`);
    console.log(`  ts   stderr ${JSON.stringify(ts.stderr).slice(0, 600)}`);
    console.log(`  rust stderr ${JSON.stringify(rs.stderr).slice(0, 600)}`);
    console.log(
      `  ts   created ${JSON.stringify(ts.created)} rust ${JSON.stringify(rs.created)}`
    );
  }
  results.push({ ...s, ts });
}
console.log(`scenarios: ${identical}/${S.length} identical`);
if (record) {
  fs.writeFileSync(
    path.join(here, "wpb4-scenarios.json"),
    `${JSON.stringify(results, null, 2)}\n`
  );
}
process.exit(identical === S.length ? 0 : 1);
