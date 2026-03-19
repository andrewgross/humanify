import assert from "node:assert";
import { describe, it } from "node:test";
import { Command } from "commander";
import { configureSplitCommand } from "./split.js";

/**
 * Build a program that mirrors the real CLI structure:
 * parent command with -o (unified) + split subcommand with -o.
 */
function buildTestProgram(): {
  program: Command;
  captured: { input?: string; output?: string; verbose?: boolean };
} {
  const captured: { input?: string; output?: string; verbose?: boolean } = {};

  const program = new Command();
  program
    .name("humanify")
    .enablePositionalOptions()
    // Parent command has -o and -v (like the unified/humanify command)
    .argument("[input]")
    .option("-o, --output-dir <dir>", "Parent output", "output")
    .option("-v, --verbose", "Parent verbose");

  // Override the split action to capture parsed opts instead of running the pipeline
  const splitCmd = program.command("split");
  splitCmd
    .description("Split a file")
    .argument("<input>")
    .option("-o, --output <dir>", "Output directory", "split-output")
    .option("--dry-run", "Dry run")
    .option("-v, --verbose", "Verbose")
    .option("--min-cluster-size <n>", "Min cluster size", "0")
    .option("--proximity", "Proximity fallback")
    .action((input: string, opts: Record<string, unknown>) => {
      captured.input = input;
      captured.output = opts.output as string;
      captured.verbose = opts.verbose as boolean;
    });

  return { program, captured };
}

describe("split subcommand option parsing", () => {
  it("receives -o value when passed after 'split'", () => {
    const { program, captured } = buildTestProgram();
    program.parse(["node", "humanify", "split", "input.js", "-o", "/tmp/out"]);
    assert.strictEqual(captured.output, "/tmp/out");
  });

  it("receives --output value when passed after 'split'", () => {
    const { program, captured } = buildTestProgram();
    program.parse([
      "node",
      "humanify",
      "split",
      "input.js",
      "--output",
      "/abs/path/to/split"
    ]);
    assert.strictEqual(captured.output, "/abs/path/to/split");
  });

  it("uses default when -o is not passed", () => {
    const { program, captured } = buildTestProgram();
    program.parse(["node", "humanify", "split", "input.js"]);
    assert.strictEqual(captured.output, "split-output");
  });

  it("receives -v flag when passed after 'split'", () => {
    const { program, captured } = buildTestProgram();
    program.parse(["node", "humanify", "split", "input.js", "-v"]);
    assert.strictEqual(captured.verbose, true);
  });

  it("handles -o with absolute path", () => {
    const { program, captured } = buildTestProgram();
    program.parse([
      "node",
      "humanify",
      "split",
      "/Users/me/input.js",
      "-o",
      "/Users/me/output/split"
    ]);
    assert.strictEqual(captured.input, "/Users/me/input.js");
    assert.strictEqual(captured.output, "/Users/me/output/split");
  });

  it("handles all split options together", () => {
    const { program, captured } = buildTestProgram();
    program.parse([
      "node",
      "humanify",
      "split",
      "input.js",
      "-o",
      "/tmp/out",
      "-v",
      "--min-cluster-size",
      "5",
      "--proximity"
    ]);
    assert.strictEqual(captured.output, "/tmp/out");
    assert.strictEqual(captured.verbose, true);
  });
});

describe("configureSplitCommand", () => {
  it("registers split subcommand with -o option", () => {
    const program = new Command();
    program.name("humanify").enablePositionalOptions();
    configureSplitCommand(program);

    const splitCmd = program.commands.find((c) => c.name() === "split");
    assert.ok(splitCmd, "split subcommand should be registered");

    const outputOpt = splitCmd.options.find((o) => o.long === "--output");
    assert.ok(outputOpt, "split should have --output option");
    assert.strictEqual(outputOpt.short, "-o");
  });
});
