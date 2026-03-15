import type { Command } from "commander";
import fs from "node:fs";
import { debug } from "../debug.js";
import type { BundlerType } from "../detection/index.js";
import { env } from "../env.js";
import { withDebug } from "../llm/debug-wrapper.js";
import { OpenAICompatibleProvider } from "../llm/openai-compatible.js";
import { withRateLimit } from "../llm/rate-limiter.js";
import { parseNumber } from "../number-utils.js";
import { createBabelPlugin } from "../plugins/babel/babel.js";
import { createPrettierPlugin } from "../plugins/prettier.js";
import { createRenamePlugin } from "../plugins/rename.js";
import {
  formatProfileSummary,
  NULL_PROFILER,
  Profiler,
  toTraceEvents
} from "../profiling/index.js";
import { createProgressRenderer } from "../ui/progress.js";
import { unminify } from "../unminify.js";
import { verbose } from "../verbose.js";
import { DEFAULT_CONCURRENCY } from "./default-args.js";

interface CommandOptions {
  endpoint: string;
  apiKey?: string;
  model: string;
  outputDir: string;
  verbose: number;
  concurrency: string;
  retries: string;
  timeout: string;
  skipLibraries: boolean;
  logFile?: string;
  diagnostics?: string;
  bundler?: string;
  batchSize?: string;
  maxRetries?: string;
  maxFreeRetries?: string;
  laneThreshold?: string;
  profile?: string;
}

async function finalizeLogStream(
  logStream: fs.WriteStream | null
): Promise<void> {
  if (logStream) {
    debug.resetOutput();
    verbose.resetOutput();
    await new Promise<void>((resolve) => logStream.end(() => resolve()));
  }
}

async function runPipeline(
  filename: string,
  opts: CommandOptions,
  provider: import("../llm/types.js").LLMProvider,
  renderer: ReturnType<typeof createProgressRenderer>,
  profiler: import("../profiling/index.js").Profiler | typeof NULL_PROFILER,
  concurrency: number
): Promise<void> {
  const renameOptions: Parameters<typeof createRenamePlugin>[0] = {
    provider,
    concurrency,
    onProgress: (m) => renderer.update(m),
    batchSize: opts.batchSize ? parseNumber(opts.batchSize) : undefined,
    maxRetriesPerIdentifier: opts.maxRetries
      ? parseNumber(opts.maxRetries)
      : undefined,
    maxFreeRetries: opts.maxFreeRetries
      ? parseNumber(opts.maxFreeRetries)
      : undefined,
    laneThreshold: opts.laneThreshold
      ? parseNumber(opts.laneThreshold)
      : undefined,
    profiler,
    skipLibraries: opts.skipLibraries
  };
  const rename = createRenamePlugin(renameOptions);
  let lastRenameResult:
    | import("../plugins/rename.js").RenamePluginResult
    | undefined;

  await unminify(
    filename,
    opts.outputDir,
    [
      createBabelPlugin({ profiler }),
      async (code) => {
        const result = await rename(code);
        lastRenameResult = result;
        if (result.coverageSummary) {
          renderer.message(result.coverageSummary);
          debug.log("summary", result.coverageSummary);
        }
        return result.code;
      },
      createPrettierPlugin({ profiler })
    ],
    {
      skipLibraries: opts.skipLibraries,
      bundler: opts.bundler as BundlerType | undefined,
      onCommentRegions: (regions) => {
        renameOptions.commentRegions = regions ?? undefined;
      },
      onDetection: (detection) => {
        renameOptions.minifierType = detection.minifier?.type;
      },
      log: (msg) => renderer.message(msg),
      profiler
    }
  );

  if (opts.diagnostics && lastRenameResult?.coverageData) {
    const { buildDiagnosticsReport, writeDiagnosticsFile } = await import(
      "../rename/diagnostics.js"
    );
    const diagReport = buildDiagnosticsReport(
      lastRenameResult.reports,
      lastRenameResult.coverageData
    );
    writeDiagnosticsFile(diagReport, opts.diagnostics);
    renderer.message(`Diagnostics written to ${opts.diagnostics}`);
  }
}

async function finalizeProfile(
  opts: CommandOptions,
  filename: string,
  profiler: import("../profiling/index.js").Profiler | typeof NULL_PROFILER,
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<void> {
  if (opts.profile) {
    const report = (
      profiler as import("../profiling/index.js").Profiler
    ).finalize({ inputFile: filename });
    const traceData = toTraceEvents(report);
    fs.writeFileSync(opts.profile, JSON.stringify(traceData, null, 2));
    const summary = formatProfileSummary(report);
    renderer.message(summary);
    renderer.message(`Profile written to ${opts.profile}`);
  }
}

export function configureUnifiedCommand(program: Command): void {
  program
    .argument("<input>", "The input minified JavaScript file")
    .option(
      "--endpoint <url>",
      "OpenAI-compatible API endpoint",
      env("HUMANIFY_ENDPOINT") ??
        env("OPENAI_BASE_URL") ??
        "https://api.openai.com/v1"
    )
    .option(
      "--api-key <key>",
      "API key (flag > HUMANIFY_API_KEY > OPENAI_API_KEY env vars)"
    )
    .option(
      "-m, --model <model>",
      "Model identifier",
      env("HUMANIFY_MODEL") ?? "gpt-4o-mini"
    )
    .option("-o, --output-dir <output>", "Output directory", "output")
    .option(
      "-v, --verbose",
      "Increase verbosity (-v for info, -vv for debug)",
      (_, prev) => (prev || 0) + 1,
      0
    )
    .option(
      "-c, --concurrency <n>",
      "Maximum number of concurrent LLM requests",
      `${DEFAULT_CONCURRENCY}`
    )
    .option(
      "--retries <n>",
      "Number of retry attempts for failed API calls",
      "3"
    )
    .option("--timeout <ms>", "LLM request timeout in milliseconds", "300000")
    .option(
      "--skip-libraries, --no-skip-libraries",
      "Skip library code instead of processing it with the LLM (default: true)"
    )
    .option("--log-file <path>", "Write debug logs to file (implies -vv)")
    .option(
      "--diagnostics <path>",
      "Write detailed rename diagnostics to JSON file"
    )
    .option(
      "--bundler <type>",
      "Force bundler type (webpack, browserify, rollup, esbuild, parcel, bun)"
    )
    .option("--batch-size <n>", "Identifiers per LLM batch (default: 10)")
    .option("--max-retries <n>", "Per-identifier retry limit (default: 3)")
    .option(
      "--max-free-retries <n>",
      "Cross-lane collision retry limit (default: 100)"
    )
    .option(
      "--lane-threshold <n>",
      "Min bindings to enable parallel lanes (default: 25)"
    )
    .option(
      "--profile <path>",
      "Write performance profile to JSON file (Chrome Trace Event format, viewable at chrome://tracing or ui.perfetto.dev)"
    )
    .action(async (filename: string, opts) => {
      verbose.level = opts.verbose || 0;

      // --log-file implies -vv and redirects debug output to the file
      let logStream: fs.WriteStream | null = null;
      if (opts.logFile) {
        logStream = fs.createWriteStream(opts.logFile, { flags: "a" });
        const writeToLog = (text: string) => {
          logStream?.write(`${text}\n`);
        };
        debug.setOutput(writeToLog);
        // Also redirect verbose output to the log file instead of stdout
        verbose.setOutput(writeToLog);
        verbose.level = Math.max(verbose.level, 2);
      }

      // Decide renderer mode:
      // Use TTY renderer when stderr is a TTY and either not -vv or debug is going to a file
      const isTTY = !!process.stderr.isTTY;
      const useRichUI = isTTY && (verbose.level < 2 || !!opts.logFile);
      const renderer = createProgressRenderer({ tty: useRichUI });

      const concurrency = parseNumber(opts.concurrency);
      const profiler = opts.profile ? new Profiler(true) : NULL_PROFILER;

      const apiKey =
        opts.apiKey ?? env("HUMANIFY_API_KEY") ?? env("OPENAI_API_KEY");
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
      const provider = withRateLimit(debugProvider, {
        maxConcurrent: concurrency,
        retryAttempts: retries
      });

      try {
        await runPipeline(
          filename,
          opts,
          provider,
          renderer,
          profiler,
          concurrency
        );
      } finally {
        await finalizeProfile(opts, filename, profiler, renderer);
        renderer.finish();
        await finalizeLogStream(logStream);
      }
    });
}
