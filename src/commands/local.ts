import { cli } from "../cli.js";
import { DEFAULT_MODEL, getEnsuredModelPath } from "../local-models.js";
import { unminify } from "../unminify.js";
import prettier from "../plugins/prettier.js";
import babel from "../plugins/babel/babel.js";
import { createRenamePlugin } from "../plugins/rename.js";
import { createLocalProvider } from "../llm/local-llama.js";
import { withDebug } from "../llm/debug-wrapper.js";
import { verbose } from "../verbose.js";
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
  .option("-v, --verbose", "Increase verbosity (-v for info, -vv for debug)", (_, prev) => (prev || 0) + 1, 0)
  .option(
    "-c, --concurrency <concurrency>",
    "Maximum number of concurrent LLM requests",
    `${DEFAULT_CONCURRENCY}`
  )
  .argument("input", "The input minified Javascript file")
  .action(async (filename, opts) => {
    verbose.level = opts.verbose || 0;

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
    } finally {
      baseProvider.dispose();
    }
  });
