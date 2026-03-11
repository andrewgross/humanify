#!/usr/bin/env -S npx tsx
import pkg from "../package.json" with { type: "json" };
import { configureUnifiedCommand } from "./commands/unified.js";
import { configureSplitCommand } from "./commands/split.js";
import { cli } from "./cli.js";

const program = cli()
  .name("humanify")
  .description("Unminify JavaScript using an OpenAI-compatible API")
  .version(pkg.version);

configureUnifiedCommand(program);
configureSplitCommand(program);

program.parse(process.argv);
