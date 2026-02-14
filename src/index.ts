#!/usr/bin/env -S npx tsx
import pkg from "../package.json" with { type: "json" };
import { download } from "./commands/download.js";
import { configureUnifiedCommand } from "./commands/unified.js";
import { cli } from "./cli.js";

const program = cli()
  .name("humanify")
  .description("Unminify JavaScript using an OpenAI-compatible API or a local LLM")
  .version(pkg.version);

configureUnifiedCommand(program);

program
  .addCommand(download())
  .parse(process.argv);
