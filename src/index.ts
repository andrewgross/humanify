#!/usr/bin/env -S npx tsx
import pkg from "../package.json" with { type: "json" };
import { download } from "./commands/download.js";
import { local } from "./commands/local.js";
import { openai } from "./commands/openai.js";
import { cli } from "./cli.js";
import { gemini } from "./commands/gemini.js";

cli()
  .name("humanify")
  .description("Unminify code using OpenAI's API or a local LLM")
  .version(pkg.version)
  .addCommand(local)
  .addCommand(openai)
  .addCommand(gemini)
  .addCommand(download())
  .parse(process.argv);
