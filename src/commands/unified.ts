import type { Command } from "commander";
import fs from "node:fs";
import path from "node:path";
import { debug } from "../debug.js";
import { detectBundle } from "../detection/index.js";
import type { BundlerType, MinifierType } from "../detection/types.js";
import { env } from "../env.js";
import { ensureFileExists } from "../file-utils.js";
import { buildPipelineConfig } from "../pipeline/config.js";
import type { FileContext } from "../pipeline/types.js";
import { withDebug } from "../llm/debug-wrapper.js";
import { OpenAICompatibleProvider } from "../llm/openai-compatible.js";
import { withRateLimit } from "../llm/rate-limiter.js";
import { parseNumber } from "../number-utils.js";
import { createBabelPlugin } from "../plugins/babel/babel.js";
import { createPrettierPlugin } from "../plugins/prettier.js";
import { createRenamePlugin } from "../rename/plugin.js";
import {
  formatProfileSummary,
  NULL_PROFILER,
  Profiler,
  toTraceEvents
} from "../profiling/index.js";
import { detectModules } from "../split/module-detect.js";
import { splitFromAst } from "../split/index.js";
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
  split: boolean;
  logFile?: string;
  diagnostics?: string;
  bundler?: string;
  minifier?: string;
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

async function runSplit(
  filename: string,
  opts: CommandOptions,
  renameResult: import("../rename/plugin.js").RenamePluginResult,
  originalSource: string,
  profiler: import("../profiling/index.js").Profiler | typeof NULL_PROFILER,
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<void> {
  const splitSpan = profiler.startSpan("split", "pipeline");
  const detection = detectModules(originalSource);
  const fileContents = splitFromAst(
    renameResult.ast,
    filename,
    originalSource,
    {
      detection
    }
  );
  splitSpan.end({ fileCount: fileContents.size });

  renderer.message(`Splitting into ${fileContents.size} file(s)...`);

  const prettify = createPrettierPlugin({ profiler });
  fs.mkdirSync(opts.outputDir, { recursive: true });
  for (const [fileName, content] of fileContents) {
    const prettified = await prettify(content);
    const filePath = path.join(opts.outputDir, fileName);
    const fileDir = path.dirname(filePath);
    if (fileDir !== opts.outputDir) {
      fs.mkdirSync(fileDir, { recursive: true });
    }
    fs.writeFileSync(filePath, prettified);
  }

  renderer.message(
    `Split complete: ${fileContents.size} file(s) written to ${opts.outputDir}`
  );
}

async function runPipeline(
  filename: string,
  opts: CommandOptions,
  provider: import("../llm/types.js").LLMProvider,
  renderer: ReturnType<typeof createProgressRenderer>,
  profiler: import("../profiling/index.js").Profiler | typeof NULL_PROFILER,
  concurrency: number
): Promise<void> {
  // 1. Read input and detect bundler/minifier
  ensureFileExists(filename);
  const bundledCode = fs.readFileSync(filename, "utf-8");
  const detectionSpan = profiler.startSpan("detection", "pipeline");
  const detection = detectBundle(bundledCode);
  const config = buildPipelineConfig(detection, {
    bundlerOverride: opts.bundler as BundlerType | undefined,
    minifierOverride: opts.minifier as MinifierType | undefined
  });
  detectionSpan.end({
    bundler: config.bundlerType,
    adapter: config.unpackAdapterName
  });
  verbose.log(
    `Bundle detection: bundler=${config.bundlerType} (${config.bundlerTier}), ` +
      `minifier=${config.minifierType}, adapter=${config.unpackAdapterName}`
  );
  if (detection.signals.length > 0) {
    verbose.debug(
      `Detection signals: ${detection.signals.map((s) => `${s.source}:${s.pattern}`).join(", ")}`
    );
  }

  // 2. Build plugins with config available upfront — no callbacks
  const rename = createRenamePlugin({
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
    skipLibraries: opts.skipLibraries,
    minifierType: config.minifierType,
    bundlerType: config.bundlerType
  });
  let lastRenameResult:
    | import("../rename/plugin.js").RenamePluginResult
    | undefined;

  // When --split, capture original source for module detection
  let originalSource = "";
  const isSplit = opts.split;

  // Build plugin chain: babel → rename, and prettier only if not splitting
  const babelPlugin = createBabelPlugin({ profiler });
  const prettierPlugin = !isSplit ? createPrettierPlugin({ profiler }) : null;

  const plugins: ((code: string, context: FileContext) => Promise<string>)[] = [
    (code, _ctx) => babelPlugin(code),
    async (code, ctx) => {
      const result = await rename(code, ctx);
      lastRenameResult = result;
      if (result.coverageSummary) {
        renderer.message(result.coverageSummary);
        debug.log("summary", result.coverageSummary);
      }
      return result.code;
    }
  ];
  if (prettierPlugin) {
    plugins.push((code, _ctx) => prettierPlugin(code));
  }

  // 3. Run pipeline
  await unminify(bundledCode, opts.outputDir, config, plugins, {
    skipLibraries: opts.skipLibraries,
    log: (msg) => renderer.message(msg),
    profiler,
    onOriginalSource: isSplit
      ? (_filePath, code) => {
          originalSource = code;
        }
      : undefined,
    skipFileWrite: isSplit
  });

  if (isSplit && lastRenameResult) {
    await runSplit(
      filename,
      opts,
      lastRenameResult,
      originalSource,
      profiler,
      renderer
    );
  }

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
    .option(
      "--minifier <type>",
      "Force minifier type (terser, esbuild, swc, bun, none)"
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
      "--split",
      "Split output into multiple files based on detected module boundaries"
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
