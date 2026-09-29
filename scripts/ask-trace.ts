/**
 * The ask-trace runner — the stub side of the `--dump-asks` instrument.
 *
 *   npx tsx scripts/ask-trace.ts <input.js> [options]
 *
 * Boots the deterministic stub LLM (scripts/lib/stub-llm.ts — the same
 * server the e2e gate runs on) and runs the RELEASE BINARY on any input,
 * cold or `--prior-version`, with `--dump-asks` on. Emits:
 *
 *   <out>/asks.jsonl   — the reason-labeled ask log (one row per LLM ask)
 *   <out>/tree/        — the humanified output tree
 *
 * Deterministic end to end: the pipeline is completion-order-independent
 * by design and the stub's answers derive only from the request bytes, so
 * the same input + flags give the same log byte for byte (the diff-asks
 * comparator's use (a) depends on exactly this).
 *
 * Options:
 *   --out <dir>        output directory (default: a fresh temp dir, printed)
 *   --prior <path>     run with `--prior-version <path>`
 *   --collide <name>   make the stub name EVERY identifier <name> — forces
 *                      the collision-retry paths (the lane round-2 and the
 *                      barrier's disclosed re-ask); the default policy
 *                      answers `<id>Renamed`
 *   --sequential       the conservative naming schedule
 *   -- <flags…>        anything after `--` passes to the binary verbatim
 *                      (e.g. `-- --split --batch-size 10`)
 *
 * Prints the per-reason ask summary (TOTAL first) and exits with the
 * binary's exit code. Needs target/release/humanify (rust:build).
 */
import { spawn } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

import { collideAnswer, startStubLlm, stubAnswer } from "./lib/stub-llm.js";

const BIN = path.join(
  path.resolve(import.meta.dirname, ".."),
  "target/release/humanify"
);

/** One parsed ask row (the ask-trace writer's schema). */
export interface AskRow {
  seq: number;
  site: string;
  scope: string;
  scopeKind: string;
  reason: string;
  isRetry: boolean;
  retryCause?: string;
  retryCauseDetail?: string;
  priorContext: boolean;
  wave?: number;
  phase?: number;
  round: number;
  identifiers: string[];
  usedNamesCount: number;
  promptVariant: string;
}

export function usage(): string {
  return [
    "usage: npx tsx scripts/ask-trace.ts <input.js> [--out <dir>]",
    "         [--prior <path>] [--collide <name>] [--sequential] [-- <pipeline flags>]"
  ].join("\n");
}

export interface AskTraceArgs {
  input: string;
  out?: string;
  prior?: string;
  collide?: string;
  sequential: boolean;
  passthrough: string[];
}

function fail(msg: string): never {
  console.error(`ASK-TRACE FAILED: ${msg}`);
  process.exit(1);
}

function takeValue(flag: string, rest: string[], i: number): string {
  const v = rest[i + 1];
  if (!v) fail(`missing value for ${flag}`);
  return v;
}

function setValue(a: AskTraceArgs, flag: string, v: string): void {
  if (flag === "--out") a.out = v;
  else if (flag === "--prior") a.prior = v;
  else a.collide = v;
}

/** Parse the CLI the way the harness's own flags parse (fail loud). */
export function parseArgs(argv: string[]): AskTraceArgs {
  const [input, ...rest] = argv;
  if (!input || input.startsWith("-")) {
    fail(usage());
  }
  const args: AskTraceArgs = { input, sequential: false, passthrough: [] };
  for (let i = 0; i < rest.length; i++) {
    const a = rest[i];
    if (a === "--") {
      args.passthrough.push(...rest.slice(i + 1));
      break;
    }
    if (a === "--sequential") {
      args.sequential = true;
    } else if (a === "--out" || a === "--prior" || a === "--collide") {
      setValue(args, a, takeValue(a, rest, i));
      i += 1;
    } else {
      fail(`unknown option ${a}\n${usage()}`);
    }
  }
  return args;
}

/** The per-reason summary, totals first. */
export function summarize(rows: AskRow[]): string {
  const byReason = new Map<string, number>();
  for (const r of rows) {
    byReason.set(r.reason, (byReason.get(r.reason) ?? 0) + 1);
  }
  const keys = [...byReason.keys()].sort();
  const pad = Math.max(...keys.map((k) => k.length), "TOTAL".length);
  const lines = keys.map(
    (k) => `  ${k.padEnd(pad)}  ${String(byReason.get(k)).padStart(5)}`
  );
  return [
    `TOTAL ${rows.length} ask(s)`,
    ...lines,
    `  ${"REMAINING".padEnd(pad)}  ${String(rows.filter((r) => r.isRetry).length).padStart(5)} re-ask(s)`
  ].join("\n");
}

async function main(): Promise<void> {
  if (!fs.existsSync(BIN)) {
    fail(
      "target/release/humanify is missing — run `cargo build --release --locked -p humanify-cli` (rust:build)."
    );
  }
  const args = parseArgs(process.argv.slice(2));
  const input = path.resolve(args.input);
  if (!fs.existsSync(input)) fail(`no such input: ${input}`);
  const outDir = args.out
    ? path.resolve(args.out)
    : fs.mkdtempSync(path.join(os.tmpdir(), "ask-trace-"));
  fs.mkdirSync(outDir, { recursive: true });

  const answer = args.collide ? collideAnswer(args.collide) : stubAnswer;
  const { server, url } = await startStubLlm(answer);

  const binArgs = [
    input,
    "-o",
    path.join(outDir, "tree"),
    "--dump-asks",
    path.join(outDir, "asks.jsonl"),
    "--api-key",
    "ask-trace",
    "--retries",
    "0",
    "--endpoint",
    url
  ];
  if (args.prior) binArgs.push("--prior-version", path.resolve(args.prior));
  if (args.sequential) binArgs.push("--sequential");
  binArgs.push(...args.passthrough);

  // Async on purpose: the stub lives in this process (see scripts/e2e.ts).
  const child = spawn(BIN, binArgs, { stdio: ["ignore", "ignore", "pipe"] });
  child.stderr.on("data", (c) => {
    process.stderr.write(c);
  });
  const code: number = await new Promise((resolve) =>
    child.on("close", (c) => resolve(c ?? -1))
  );
  server.close();

  const asksPath = path.join(outDir, "asks.jsonl");
  if (fs.existsSync(asksPath)) {
    const rows: AskRow[] = fs
      .readFileSync(asksPath, "utf8")
      .split("\n")
      .filter((l) => l.trim())
      .map((l) => JSON.parse(l) as AskRow);
    console.log(summarize(rows));
  } else {
    console.error("ASK-TRACE FAILED: the run wrote no ask log");
  }
  console.log(`ask log:  ${asksPath}`);
  console.log(`tree:     ${path.join(outDir, "tree")}`);
  if (code !== 0) {
    fail(`the binary exited ${code}`);
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  await main();
}
