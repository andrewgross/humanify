import { cli } from "../cli.js";
import { DEFAULT_MODEL, getEnsuredModelPath } from "../local-models.js";
import { unminify } from "../unminify.js";
import prettier from "../plugins/prettier.js";
import babel from "../plugins/babel/babel.js";
import { createRenamePlugin } from "../plugins/rename.js";
import { createLocalProvider } from "../llm/local-llama.js";
import { withDebug } from "../llm/debug-wrapper.js";
import { verbose } from "../verbose.js";
import { debug } from "../debug.js";
import { DEFAULT_CONCURRENCY } from "./default-args.js";
import { parseNumber } from "../number-utils.js";

export const local = cli()
  .name("local")
  .description("Use a local LLM to unminify code")
  .showHelpAfterError(true)
  .option("-m, --model <model>", "The model to use", DEFAULT_MODEL)
  .option("-o, --outputDir <output>", "The output directory", "output")
  .option(
    "-s, --seed <seed>",
    "Seed for the model to get reproduceable results (leave out for random seed)"
  )
  .option("--disableGpu", "Disable GPU acceleration")
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

    verbose.log("Starting local inference with options: ", opts);

    const concurrency = parseNumber(opts.concurrency);
    const modelPath = getEnsuredModelPath(opts.model);

    const baseProvider = await createLocalProvider(modelPath, {
      modelName: opts.model,
      disableGpu: opts.disableGpu,
      seed: opts.seed ? parseInt(opts.seed) : undefined
    });

    // Wrap with debug logging if enabled
    const provider = withDebug(baseProvider, opts.model);

    try {
      await unminify(filename, opts.outputDir, [
        babel,
        createRenamePlugin({
          provider,
          concurrency,
          onProgress: console.log
        }),
        prettier
      ]);
    } finally {
      baseProvider.dispose();
    }
  });
