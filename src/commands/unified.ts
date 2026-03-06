import fs from "fs";
import type { Command } from "commander";
import prettier from "../plugins/prettier.js";
import { unminify } from "../unminify.js";
import babel from "../plugins/babel/babel.js";
import { createRenamePlugin } from "../plugins/rename.js";
import { OpenAICompatibleProvider } from "../llm/openai-compatible.js";
import { withRateLimit } from "../llm/rate-limiter.js";
import { withDebug } from "../llm/debug-wrapper.js";
import { createLocalProvider } from "../llm/local-llama.js";
import { DEFAULT_MODEL, getEnsuredModelPath } from "../local-models.js";
import { verbose } from "../verbose.js";
import { env } from "../env.js";
import { parseNumber } from "../number-utils.js";
import { DEFAULT_CONCURRENCY } from "./default-args.js";
import { createSourceMapWriter } from "../source-map-writer.js";
import { createProgressRenderer } from "../ui/progress.js";
import { debug } from "../debug.js";

export function configureUnifiedCommand(program: Command): void {
  program
    .argument("<input>", "The input minified JavaScript file")
    .option(
      "--endpoint <url>",
      "OpenAI-compatible API endpoint",
      env("HUMANIFY_ENDPOINT") ?? env("OPENAI_BASE_URL") ?? "https://api.openai.com/v1"
    )
    .option(
      "--api-key <key>",
      "API key (flag > HUMANIFY_API_KEY > OPENAI_API_KEY env vars)"
    )
    .option("-m, --model <model>", "Model identifier", env("HUMANIFY_MODEL") ?? "gpt-4o-mini")
    .option("--local", "Use local llama.cpp model instead of API")
    .option("--local-model <name>", "Local model name (e.g. 2b, 8b)", DEFAULT_MODEL)
    .option("-s, --seed <seed>", "Seed for reproducible results (local only)")
    .option("--disable-gpu", "Disable GPU acceleration (local only)")
    .option("-o, --output-dir <output>", "Output directory", "output")
    .option("-v, --verbose", "Increase verbosity (-v for info, -vv for debug)", (_, prev) => (prev || 0) + 1, 0)
    .option(
      "-c, --concurrency <n>",
      "Maximum number of concurrent LLM requests",
      `${DEFAULT_CONCURRENCY}`
    )
    .option("--retries <n>", "Number of retry attempts for failed API calls", "3")
    .option("--timeout <ms>", "LLM request timeout in milliseconds", "300000")
    .option("--source-map", "Generate source map files alongside output")
    .option("--no-skip-libraries", "Process library code instead of skipping it")
    .option("--log-file <path>", "Write debug logs to file (implies -vv)")
    .action(async (filename: string, opts) => {
      verbose.level = opts.verbose || 0;

      // --log-file implies -vv and redirects debug output to the file
      let logStream: fs.WriteStream | null = null;
      if (opts.logFile) {
        logStream = fs.createWriteStream(opts.logFile, { flags: "a" });
        const writeToLog = (text: string) => { logStream!.write(text + "\n"); };
        debug.setOutput(writeToLog);
        // Also redirect verbose output to the log file instead of stdout
        verbose.setOutput(writeToLog);
        verbose.level = Math.max(verbose.level, 2);
      }

      // Decide renderer mode:
      // Use TTY renderer when stderr is a TTY and either not -vv or debug is going to a file
      const isTTY = !!(process.stderr as any).isTTY;
      const useRichUI = isTTY && (verbose.level < 2 || !!opts.logFile);
      const renderer = createProgressRenderer({ tty: useRichUI });

      const concurrency = parseNumber(opts.concurrency);
      const sourceMapEnabled = !!opts.sourceMap;
      const smWriter = sourceMapEnabled ? createSourceMapWriter() : null;

      const runPipeline = async (provider: import("../llm/types.js").LLMProvider) => {
        const renameOptions: Parameters<typeof createRenamePlugin>[0] = {
          provider,
          concurrency,
          onProgress: (m) => renderer.update(m),
          sourceMap: sourceMapEnabled
        };
        const rename = createRenamePlugin(renameOptions);
        try {
          await unminify(filename, opts.outputDir, [
            babel,
            async (code) => {
              const result = await rename(code);
              smWriter?.capture(result.sourceMap);
              if (result.coverageSummary) {
                renderer.message(result.coverageSummary);
              }
              return result.code;
            },
            ...(sourceMapEnabled ? [] : [prettier])
          ], {
            skipLibraries: opts.skipLibraries,
            onCommentRegions: (regions) => {
              renameOptions.commentRegions = regions ?? undefined;
            },
            ...(smWriter ? { afterFileWrite: (fp: string) => smWriter.write(fp) } : {}),
            log: (msg) => renderer.message(msg),
          });
        } finally {
          renderer.finish();
          if (logStream) {
            debug.resetOutput();
            verbose.resetOutput();
            await new Promise<void>((resolve) => logStream!.end(() => resolve()));
          }
        }
      };

      if (opts.local) {
        verbose.log("Starting local inference with options: ", opts);
        const modelPath = getEnsuredModelPath(opts.localModel);
        const baseProvider = await createLocalProvider(modelPath, {
          modelName: opts.localModel,
          disableGpu: opts.disableGpu,
          seed: opts.seed ? parseInt(opts.seed) : undefined
        });
        const provider = withDebug(baseProvider, opts.localModel);
        try {
          await runPipeline(provider);
        } finally {
          baseProvider.dispose();
        }
      } else {
        const apiKey = opts.apiKey ?? env("HUMANIFY_API_KEY") ?? env("OPENAI_API_KEY");
        if (!apiKey) {
          console.error(
            "Error: API key required. Provide --api-key, or set HUMANIFY_API_KEY or OPENAI_API_KEY environment variable."
          );
          process.exit(1);
        }
        const baseProvider = new OpenAICompatibleProvider({
          endpoint: opts.endpoint,
          apiKey,
          model: opts.model,
          timeout: parseNumber(opts.timeout)
        });
        const debugProvider = withDebug(baseProvider, opts.model);
        const retries = parseNumber(opts.retries);
        const provider = withRateLimit(debugProvider, { maxConcurrent: concurrency, retryAttempts: retries });
        await runPipeline(provider);
      }
    });
}
