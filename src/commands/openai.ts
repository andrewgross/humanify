import { cli } from "../cli.js";
import prettier from "../plugins/prettier.js";
import { unminify } from "../unminify.js";
import babel from "../plugins/babel/babel.js";
import { createRenamePlugin } from "../plugins/rename.js";
import { createOpenAIProvider } from "../llm/openai-compatible.js";
import { withRateLimit } from "../llm/rate-limiter.js";
import { withDebug } from "../llm/debug-wrapper.js";
import { verbose } from "../verbose.js";
import { debug } from "../debug.js";
import { env } from "../env.js";
import { parseNumber } from "../number-utils.js";
import { DEFAULT_CONCURRENCY } from "./default-args.js";

export const openai = cli()
  .name("openai")
  .description("Use OpenAI's API to unminify code")
  .option("-m, --model <model>", "The model to use", "gpt-4o-mini")
  .option("-o, --outputDir <output>", "The output directory", "output")
  .option(
    "-k, --apiKey <apiKey>",
    "The OpenAI API key. Alternatively use OPENAI_API_KEY environment variable"
  )
  .option(
    "--baseURL <baseURL>",
    "The OpenAI base server URL.",
    env("OPENAI_BASE_URL") ?? "https://api.openai.com/v1"
  )
  .option("--verbose", "Show verbose output")
  .option("--debug", "Show detailed debug output including prompts and responses")
  .option(
    "-c, --concurrency <concurrency>",
    "Maximum number of concurrent LLM requests",
    `${DEFAULT_CONCURRENCY}`
  )
  .argument("input", "The input minified Javascript file")
  .action(async (filename, opts) => {
    if (opts.verbose) {
      verbose.enabled = true;
    }
    if (opts.debug) {
      debug.enabled = true;
      verbose.enabled = true;
    }

    const apiKey = opts.apiKey ?? env("OPENAI_API_KEY");
    const concurrency = parseNumber(opts.concurrency);

    const baseProvider = createOpenAIProvider(apiKey, opts.model, {
      endpoint: opts.baseURL
    });

    // Wrap with debug logging if enabled
    const debugProvider = withDebug(baseProvider, opts.model);

    const provider = withRateLimit(debugProvider, {
      maxConcurrent: concurrency
    });

    await unminify(filename, opts.outputDir, [
      babel,
      createRenamePlugin({
        provider,
        concurrency,
        onProgress: console.log
      }),
      prettier
    ]);
  });
