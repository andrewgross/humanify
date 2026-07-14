import assert from "node:assert";
import { describe, it } from "node:test";
import { Command } from "commander";
import { configureEnvReadsCommand } from "./env-reads.js";

describe("configureEnvReadsCommand", () => {
  it("registers the env-reads subcommand with its options", () => {
    const program = new Command();
    program.name("humanify").enablePositionalOptions();
    configureEnvReadsCommand(program);

    const cmd = program.commands.find((c) => c.name() === "env-reads");
    assert.ok(cmd, "env-reads subcommand should be registered");
    assert.ok(
      cmd.options.find((o) => o.long === "--markdown"),
      "should have --markdown option"
    );
    const outputOpt = cmd.options.find((o) => o.long === "--output");
    assert.ok(outputOpt, "should have --output option");
    assert.strictEqual(outputOpt.short, "-o");
  });
});
