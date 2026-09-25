/**
 * WPB.4 probe: the pipeline's COMMAND SURFACE, recorded from the REAL
 * commander program (src/index.ts's construction, the real
 * configureUnifiedCommand / configureEnvReadsCommand), not re-derived:
 *
 *   commands — per command: name, description, usage, arguments, and every
 *     option as commander holds it (flags, short, long, negate, required,
 *     optional, variadic, attributeName, defaultValue, description), plus
 *     the rendered help text (non-TTY: helpWidth 80, no colors).
 *   cases — argv vectors run through `parse` with the actions replaced by a
 *     recorder: a parse that reaches an action records the command, its
 *     processed arguments, the opts object and every option's value SOURCE
 *     (default / cli / absent); a parse that exits records commander's exit
 *     code, error code and the exact stdout/stderr bytes (usage errors print
 *     `error: ...` + the help, the harness-invisible lowercase family).
 *
 * The Rust side (crates/humanify-cli/src/surface_test.rs) replays every
 * case through its commander-semantics parser and requires equality.
 *
 *   npx tsx test/parity/wpb4-cli-probe.ts > test/parity/wpb4-cli-surface.json
 */
import { type Command, CommanderError } from "commander";
import pkg from "../../package.json" with { type: "json" };
import { cli } from "../../src/cli.js";
import { configureEnvReadsCommand } from "../../src/commands/env-reads.js";
import { configureUnifiedCommand } from "../../src/commands/unified.js";

interface Captured {
  out: string;
  err: string;
}

/** src/index.ts's program, with output captured and exits thrown. The
 * output/exit configuration must precede configure*Command: a subcommand
 * copies its parent's settings when `.command()` creates it. */
function buildProgram(captured: Captured): Command {
  const program = cli()
    .name("humanify")
    .description("Unminify JavaScript using an OpenAI-compatible API")
    .version(pkg.version)
    .enablePositionalOptions();
  program.exitOverride();
  program.configureOutput({
    writeOut: (s) => {
      captured.out += s;
    },
    writeErr: (s) => {
      captured.err += s;
    },
    getOutHelpWidth: () => undefined as unknown as number,
    getErrHelpWidth: () => undefined as unknown as number,
    getOutHasColors: () => false,
    getErrHasColors: () => false
  });
  configureUnifiedCommand(program);
  configureEnvReadsCommand(program);
  return program;
}

type Outcome =
  | {
      kind: "action";
      command: string;
      args: unknown[];
      opts: Record<string, unknown>;
      sources: Record<string, string | null>;
    }
  | {
      kind: "exit";
      exitCode: number;
      code: string;
      stdout: string;
      stderr: string;
    };

function sourcesOf(cmd: Command): Record<string, string | null> {
  const out: Record<string, string | null> = {};
  for (const o of cmd.options) {
    const key = o.attributeName();
    out[key] = cmd.getOptionValueSource(key) ?? null;
  }
  return out;
}

function run(argv: string[]): Outcome {
  const captured: Captured = { out: "", err: "" };
  const program = buildProgram(captured);
  let outcome: Outcome | undefined;
  const record =
    (command: string) =>
    (...params: unknown[]): void => {
      const cmd = params[params.length - 1] as Command;
      const opts = params[params.length - 2] as Record<string, unknown>;
      outcome = {
        kind: "action",
        command,
        args: params.slice(0, -2),
        opts: { ...opts },
        sources: sourcesOf(cmd)
      };
    };
  program.action(record("pipeline"));
  const envReads = program.commands.find((c) => c.name() === "env-reads");
  envReads?.action(record("env-reads"));
  try {
    program.parse(["node", "humanify", ...argv]);
  } catch (e) {
    if (!(e instanceof CommanderError)) throw e;
    return {
      kind: "exit",
      exitCode: e.exitCode,
      code: e.code,
      stdout: captured.out,
      stderr: captured.err
    };
  }
  if (!outcome) throw new Error(`no action and no exit for ${argv.join(" ")}`);
  return outcome;
}

function describeCommand(cmd: Command) {
  const help = cmd.createHelp();
  help.prepareContext({ helpWidth: undefined as unknown as number });
  return {
    name: cmd.name(),
    description: cmd.description(),
    usage: cmd.usage(),
    arguments: cmd.registeredArguments.map((a) => ({
      name: a.name(),
      required: a.required,
      variadic: a.variadic,
      description: a.description
    })),
    options: cmd.options.map((o) => ({
      flags: o.flags,
      short: o.short ?? null,
      long: o.long ?? null,
      negate: o.negate,
      required: o.required,
      optional: o.optional,
      variadic: o.variadic,
      attributeName: o.attributeName(),
      defaultValue: o.defaultValue ?? null,
      description: o.description
    })),
    help: cmd.helpInformation()
  };
}

const IN = "in.js";
/** The harness's scored-leg argv, verbatim flag order
 * (experiments/034-eval-harness/run.sh:228-233, contract §7). */
const HARNESS = [
  IN,
  "--split",
  "--endpoint",
  "http://127.0.0.1:9/v1",
  "--model",
  "openai/gpt-oss-20b",
  "--api-key",
  "k",
  "--reasoning-effort",
  "low",
  "-c",
  "32",
  "-o",
  "/tmp/out",
  "--llm-cache",
  "/tmp/cache",
  "--prior-version",
  "/tmp/prior.js",
  "--stats-json",
  "/tmp/s.json",
  "-vv",
  "--log-file",
  "/tmp/log",
  "--diagnostics",
  "/tmp/d.json"
];

const CASES: string[][] = [
  [],
  [IN],
  ["--help"],
  ["-h"],
  [IN, "--help"],
  [IN, "--bogus", "--help"],
  ["-vh"],
  ["-V"],
  ["--version"],
  [IN, "-V"],
  [IN, "--bogus"],
  ["--bogus"],
  ["-x"],
  [IN, "-x"],
  [IN, "extra"],
  [IN, "extra", "more"],
  [IN, "--model"],
  [IN, "-m"],
  [IN, "-o"],
  [IN, "--endpoint"],
  [IN, "-m", "x"],
  [IN, "-mx"],
  [IN, "--model=x"],
  [IN, "--model="],
  [IN, "--model", "-x"],
  [IN, "--model", "--split"],
  [IN, "--model", "a", "--model", "b"],
  ["-m", "x", IN],
  [IN, "-v"],
  [IN, "-vv"],
  [IN, "-vvv"],
  [IN, "-v", "-v"],
  [IN, "--verbose", "--verbose"],
  [IN, "-vc5"],
  [IN, "-vm", "x"],
  [IN, "-c", "7"],
  [IN, "-c7"],
  [IN, "--concurrency=7"],
  [IN, "-c", "abc"],
  [IN, "--skip-libraries"],
  [IN, "--no-skip-libraries"],
  [IN, "--skip-libraries", "--no-skip-libraries"],
  [IN, "--reconcile-prior-diff"],
  [IN, "--no-reconcile-prior-diff"],
  [IN, "--no-reconcile-prior-diff", "--reconcile-prior-diff"],
  [IN, "--naming-floor"],
  [IN, "--no-naming-floor"],
  [IN, "--naming-floor-sweep"],
  [IN, "--no-naming-floor-sweep"],
  [IN, "--naming-floor-sweep", "--no-naming-floor"],
  [IN, "--split"],
  [IN, "--split=yes"],
  [IN, "--split", "--split-pure"],
  [IN, "--split-pure"],
  [IN, "--split-ledger", "l.json"],
  [IN, "--disable", "family-permute,fossil-split"],
  [IN, "--probe", "shingle-probe"],
  [IN, "--bundler", "foo", "--minifier", "gzip"],
  [IN, "--max-tokens", "100", "--module-concurrency", "3"],
  [IN, "--batch-size", "5", "--max-retries", "1", "--max-free-retries", "9"],
  [IN, "--lane-threshold", "4", "--retries", "0", "--timeout", "10"],
  [IN, "--ambiguity-probe", "p.json", "--rename-ledger", "rl"],
  [IN, "--profile", "p.json", "--dump-artifacts", "d"],
  [IN, "--split", "--split-ledger", "l.json", "--split-pure"],
  ["--", "-x"],
  [IN, "--", "-x"],
  ["-"],
  [IN, "-"],
  HARNESS,
  ["env-reads"],
  ["env-reads", "p"],
  ["env-reads", "p", "--markdown", "-o", "f"],
  ["env-reads", "p", "--output=f"],
  ["env-reads", "--help"],
  ["env-reads", "p", "--help"],
  ["env-reads", "p", "extra"],
  ["env-reads", "p", "--bogus"],
  ["env-reads", "p", "-o"],
  ["env-reads", "-V"],
  ["-m", "x", "env-reads", "p"],
  ["--split", "env-reads", "p"],
  [IN, "env-reads"],
  ["env-reads", "p", "-m", "x"]
];

const captured: Captured = { out: "", err: "" };
const program = buildProgram(captured);
const surface = {
  version: pkg.version,
  commands: [program, ...program.commands].map(describeCommand),
  cases: CASES.map((argv) => ({ argv, outcome: run(argv) }))
};
process.stdout.write(`${JSON.stringify(surface, null, 2)}\n`);
