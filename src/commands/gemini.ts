import { cli } from "../cli.js";
import prettier from "../plugins/prettier.js";
import { unminify } from "../unminify.js";
import babel from "../plugins/babel/babel.js";
import { createRenamePlugin } from "../plugins/rename.js";
import { createGeminiProvider } from "../llm/gemini.js";
import { withRateLimit } from "../llm/rate-limiter.js";
import { withDebug } from "../llm/debug-wrapper.js";
import { verbose } from "../verbose.js";
import { env } from "../env.js";
import { DEFAULT_CONCURRENCY } from "./default-args.js";
import { parseNumber } from "../number-utils.js";

export const gemini = cli()
  .name("gemini")
  .description("Use Google Gemini/AIStudio API to unminify code")
  .option("-m, --model <model>", "The model to use", "gemini-1.5-flash")
  .option("-o, --outputDir <output>", "The output directory", "output")
  .option(
    "-c, --concurrency <concurrency>",
    "Maximum number of concurrent LLM requests",
    `${DEFAULT_CONCURRENCY}`
  )
  .option(
    "-k, --apiKey <apiKey>",
    "The Google Gemini/AIStudio API key. Alternatively use GEMINI_API_KEY environment variable"
  )
  .option("-v, --verbose", "Increase verbosity (-v for info, -vv for debug)", (_, prev) => (prev || 0) + 1, 0)
  .argument("input", "The input minified Javascript file")
  .action(async (filename, opts) => {
    verbose.level = opts.verbose || 0;

    const apiKey = opts.apiKey ?? env("GEMINI_API_KEY");
    const concurrency = parseNumber(opts.concurrency);

    const baseProvider = createGeminiProvider(apiKey, opts.model);

    // Wrap with debug logging if enabled
    const debugProvider = withDebug(baseProvider, opts.model);

    const provider = withRateLimit(debugProvider, {
      maxConcurrent: concurrency
    });

    const rename = createRenamePlugin({
        provider,
        concurrency,
        onProgress: console.log
      });
    await unminify(filename, opts.outputDir, [
      babel,
      async (code) => (await rename(code)).code,
      prettier
    ]);
  });
