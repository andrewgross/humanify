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
import { createRenamePlugin } from "../rename/plugin.js";
import {
  formatProfileSummary,
  NULL_PROFILER,
  Profiler,
  toTraceEvents
} from "../profiling/index.js";
import { detectModules } from "../split/module-detect.js";
import { splitFromAst } from "../split/index.js";
import {
  SPLIT_LEDGER_FILENAME,
  type StableSplitLedger,
  stableSplitFromCode
} from "../split/stable-split.js";
import { createSplitNamer } from "../split/split-namer.js";
import { runnableEntryFile, tryEmitRunnableCjs } from "../split/cjs-emit.js";
import { relinkBunModules } from "../split/bun-relink.js";
import {
  detectExternalPackages,
  writeRunnableScaffold
} from "../split/runnable-scaffold.js";
import {
  BUN_MODULES_MANIFEST,
  type BunModulesManifest
} from "../unpack/adapters/bun.js";
import { createProgressRenderer } from "../ui/progress.js";
import { unminify } from "../unminify.js";
import { verbose } from "../verbose.js";
import { DEFAULT_CONCURRENCY } from "./default-args.js";

export interface CommandOptions {
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
  priorVersion?: string;
  reconcilePriorDiff?: boolean;
  namingFloor?: boolean;
  namingFloorSweep?: boolean;
  reasoningEffort?: string;
  splitLedger?: string;
  splitLlmNames?: boolean;
  splitRunnable?: boolean;
  renameLedger?: string;
}

/**
 * Flag preconditions, checked upfront. A flag whose behavior is gated
 * behind another flag is silently ignored when that prerequisite is
 * missing — but these flags are invariants for how a run is processed, so
 * an unmet precondition is an error, not a no-op. Returns one message per
 * violation (empty when every precondition holds), in flag-declaration
 * order.
 */
export function checkFlagInvariants(opts: CommandOptions): string[] {
  const rules: Array<{
    when: boolean;
    flag: string;
    needs: boolean;
    prereq: string;
  }> = [
    {
      when: !!opts.splitRunnable,
      flag: "--split-runnable",
      needs: opts.split,
      prereq: "--split"
    },
    {
      when: !!opts.splitLlmNames,
      flag: "--split-llm-names",
      needs: opts.split,
      prereq: "--split"
    },
    {
      when: !!opts.splitLedger,
      flag: "--split-ledger",
      needs: opts.split,
      prereq: "--split"
    },
    {
      when: !!opts.namingFloorSweep,
      flag: "--naming-floor-sweep",
      needs: !!opts.namingFloor,
      prereq: "--naming-floor"
    },
    {
      when: !!opts.reconcilePriorDiff,
      flag: "--reconcile-prior-diff",
      needs: !!opts.priorVersion,
      prereq: "--prior-version"
    }
  ];
  return rules
    .filter((r) => r.when && !r.needs)
    .map((r) => `${r.flag} requires ${r.prereq}`);
}

/** Crash upfront with a clear message when any flag precondition is unmet. */
function enforceFlagInvariants(opts: CommandOptions): void {
  const violations = checkFlagInvariants(opts);
  if (violations.length === 0) return;
  for (const message of violations) console.error(`Error: ${message}`);
  process.exit(1);
}

/** Validate the --reasoning-effort flag value; exits on an invalid level. */
function parseReasoningEffort(
  value: string | undefined
): "low" | "medium" | "high" | undefined {
  if (value === undefined) return undefined;
  if (value === "low" || value === "medium" || value === "high") return value;
  console.error(
    `Error: --reasoning-effort must be low, medium, or high (got "${value}")`
  );
  process.exit(1);
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

/** A self-contained (no humanify dependency) replay script emitted next to
 * the ledger, so the rename output can be regenerated with plain Node. */
const RENAME_LEDGER_APPLIER = `#!/usr/bin/env node
// Apply this humanify rename ledger to its source snapshot, reproducing the
// renamed output. Usage: node apply.mjs [outfile]  (stdout if no outfile).
import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const dir = path.dirname(fileURLToPath(import.meta.url));
const source = readFileSync(path.join(dir, "source.js"), "utf8");
const ledger = JSON.parse(
  readFileSync(path.join(dir, "rename-ledger.json"), "utf8")
);
if (createHash("sha256").update(source).digest("hex") !== ledger.sourceSha256) {
  throw new Error("source.js does not match the ledger's sourceSha256");
}
const edits = [];
for (const e of ledger.entries) {
  for (const [s, en] of e.occurrences) edits.push([s, en, e.finalName]);
}
edits.sort((a, b) => b[0] - a[0]);
let out = source;
for (const [s, en, name] of edits) out = out.slice(0, s) + name + out.slice(en);
const dest = process.argv[2];
if (dest) {
  writeFileSync(dest, out);
  console.error(\`wrote \${dest}\`);
} else {
  process.stdout.write(out);
}
`;

/** Emit the rename ledger, its source snapshot, and a standalone applier. */
function writeRenameLedger(
  dir: string,
  bundle: NonNullable<
    import("../rename/plugin.js").RenamePluginResult["renameLedger"]
  >
): void {
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, "rename-ledger.json"),
    JSON.stringify(bundle.ledger)
  );
  fs.writeFileSync(path.join(dir, "source.js"), bundle.source);
  fs.writeFileSync(path.join(dir, "apply.mjs"), RENAME_LEDGER_APPLIER);
}

/** Write a map of relative paths → contents under outputDir. */
function writeSplitTree(
  outputDir: string,
  fileContents: Map<string, string>
): void {
  fs.mkdirSync(outputDir, { recursive: true });
  for (const [fileName, content] of fileContents) {
    const filePath = path.join(outputDir, fileName);
    const fileDir = path.dirname(filePath);
    if (fileDir !== outputDir) {
      fs.mkdirSync(fileDir, { recursive: true });
    }
    fs.writeFileSync(filePath, content);
  }
}

/**
 * Prior split ledger for cross-release assignment inheritance:
 * --split-ledger wins, else auto-discovered next to the --prior-version
 * file (each split run writes its own ledger there, so a lineage chain
 * inherits automatically).
 */
function loadPriorSplitLedger(
  opts: CommandOptions,
  renderer: ReturnType<typeof createProgressRenderer>
): StableSplitLedger | undefined {
  const discovered = opts.priorVersion
    ? path.join(path.dirname(opts.priorVersion), SPLIT_LEDGER_FILENAME)
    : undefined;
  const ledgerPath =
    opts.splitLedger ??
    (discovered && fs.existsSync(discovered) ? discovered : undefined);
  if (!ledgerPath) return undefined;
  const parsed = JSON.parse(fs.readFileSync(ledgerPath, "utf-8"));
  if (parsed?.version !== 1) {
    throw new Error(`Unsupported split ledger version in ${ledgerPath}`);
  }
  renderer.message(`Split ledger: inheriting assignments from ${ledgerPath}`);
  return parsed as StableSplitLedger;
}

/** Re-link extracted Bun CJS factory modules into the runnable split graph
 * when a `_bun-modules.json` manifest is present (Bun bundles only).
 * Returns whether a re-link ran. */
async function relinkBunFactoriesIfPresent(
  outputDir: string,
  splitFiles: string[],
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<boolean> {
  const manifestPath = path.join(outputDir, BUN_MODULES_MANIFEST);
  if (!fs.existsSync(manifestPath)) return false;
  const manifest = JSON.parse(
    fs.readFileSync(manifestPath, "utf-8")
  ) as BunModulesManifest;
  if (manifest.adapter !== "bun" || manifest.factories.length === 0) {
    return false;
  }
  await relinkBunModules(outputDir, manifest, splitFiles);
  renderer.message(
    `Re-linked ${manifest.factories.length} Bun factory module(s) into the runnable graph`
  );
  return true;
}

/** Emit a self-contained runner (run.cjs), package.json (detected external
 * deps), and RUNNABLE.md into a runnable split tree so it can be
 * `npm install`ed and executed directly. */
async function emitRunnableScaffold(
  outputDir: string,
  runnable: Map<string, string>,
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<void> {
  const entry = runnableEntryFile(runnable);
  const externals = await detectExternalPackages(outputDir);
  await writeRunnableScaffold(outputDir, entry, externals);
  const deps = externals.length
    ? `${externals.length} external dep(s): ${externals.slice(0, 6).join(", ")}${externals.length > 6 ? ", …" : ""}`
    : "no external deps";
  renderer.message(
    `Runnable scaffold: run.cjs + package.json (${deps}) — \`npm install && node run.cjs --version\``
  );
}

/** Stable statement-level split (Bun wrapper bundles). Returns false when
 * the input is not wrapper-shaped or the pass fails — caller falls back
 * to the legacy adapter splitter; a completed run is never lost.
 *
 * With --split-llm-names AND no prior ledger (a fresh-grouping release),
 * new file/folder names are LLM-polished; inherited names never change
 * (a rename is cross-version churn), so the namer is skipped whenever a
 * prior ledger drives the assignment. */
async function tryStableSplit(
  opts: CommandOptions,
  renameResult: import("../rename/plugin.js").RenamePluginResult,
  provider: import("../llm/types.js").LLMProvider,
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<boolean> {
  try {
    const prior = loadPriorSplitLedger(opts, renderer);
    const namer =
      opts.splitLlmNames && !prior ? createSplitNamer(provider) : undefined;
    if (namer) renderer.message("Split naming: LLM-polishing new file names");
    const stable = await stableSplitFromCode(renameResult.code, {
      prior,
      namer
    });
    if (!stable) return false;
    // --split-runnable emits a live-binding CommonJS module graph instead
    // of the default byte-exact review slices. A decline or failure falls
    // back to the review tree LOUDLY — the stable tree and its ledger are
    // never sacrificed to the runnable emitter.
    const runnable = opts.splitRunnable
      ? tryEmitRunnableCjs(renameResult.code, stable.ledger, (reason) =>
          renderer.message(
            `--split-runnable declined: ${reason} — writing byte-exact review tree instead`
          )
        )
      : null;
    writeSplitTree(opts.outputDir, runnable ?? stable.fileContents);
    fs.writeFileSync(
      path.join(opts.outputDir, SPLIT_LEDGER_FILENAME),
      JSON.stringify(stable.ledger)
    );
    // A Bun bundle's library factories were extracted to their own files by
    // the unpack step; the runnable tree references them by free
    // identifier. Re-bind those into the executable graph so the split
    // tree actually loads and runs (no-op for non-Bun input).
    const relinked = runnable
      ? await relinkBunFactoriesIfPresent(
          opts.outputDir,
          [...runnable.keys()],
          renderer
        )
      : false;
    if (runnable) {
      await emitRunnableScaffold(opts.outputDir, runnable, renderer);
    }
    const { stats } = stable;
    renderer.message(
      `Stable split: ${stats.files} file(s) in ${stats.folders} folder(s)` +
        (runnable
          ? ` [runnable CJS module graph${relinked ? " + Bun re-link" : ""}]`
          : "") +
        (prior
          ? ` — inherited ${stats.inherited}/${stats.statements} ` +
            `(${stats.inheritedViaOrdinal} via ordinals, ` +
            `${stats.residueLocality} residue by locality)`
          : ` (fresh grouping, ${stats.statements} statements)`)
    );
    return true;
  } catch (err) {
    renderer.message(
      `Stable split failed (${err instanceof Error ? err.message : String(err)}); falling back to adapter split`
    );
    return false;
  }
}

async function runSplit(
  filename: string,
  opts: CommandOptions,
  renameResult: import("../rename/plugin.js").RenamePluginResult,
  originalSource: string,
  provider: import("../llm/types.js").LLMProvider,
  profiler: import("../profiling/index.js").Profiler | typeof NULL_PROFILER,
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<void> {
  const splitSpan = profiler.startSpan("split", "pipeline");
  if (await tryStableSplit(opts, renameResult, provider, renderer)) {
    splitSpan.end({ stable: true });
    renderer.message(`Split complete: written to ${opts.outputDir}`);
    return;
  }
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
  writeSplitTree(opts.outputDir, fileContents);

  renderer.message(
    `Split complete: ${fileContents.size} file(s) written to ${opts.outputDir}`
  );
}

/**
 * Resolve the LLM provider from CLI flags + env (endpoint/model/key/timeout,
 * HUMANIFY_MAX_TOKENS, HUMANIFY_REASONING_EFFORT), then wrap it with debug and
 * rate limiting. The rate-limit cap spans both lanes so it never throttles the
 * module lane below its configured size.
 */
function buildProvider(
  opts: CommandOptions,
  concurrency: number,
  moduleConcurrency: number | undefined
): import("../llm/types.js").LLMProvider {
  const apiKey =
    opts.apiKey ?? env("HUMANIFY_API_KEY") ?? env("OPENAI_API_KEY");
  if (!apiKey) {
    console.error(
      "Error: API key required. Provide --api-key, or set HUMANIFY_API_KEY or OPENAI_API_KEY environment variable."
    );
    process.exit(1);
  }
  const maxTokensEnv = env("HUMANIFY_MAX_TOKENS");
  const baseProvider = new OpenAICompatibleProvider({
    endpoint: opts.endpoint,
    apiKey,
    model: opts.model,
    timeout: parseNumber(opts.timeout),
    maxTokens: maxTokensEnv ? parseNumber(maxTokensEnv) : undefined,
    reasoningEffort: parseReasoningEffort(
      opts.reasoningEffort ?? env("HUMANIFY_REASONING_EFFORT")
    )
  });
  return withRateLimit(withDebug(baseProvider, opts.model), {
    maxConcurrent: concurrency + (moduleConcurrency ?? 40),
    retryAttempts: parseNumber(opts.retries)
  });
}

/**
 * Load and validate the --prior-version file. An empty file would flow
 * through as "no prior" and silently become a full-cost zero-transfer run,
 * and --reconcile-prior-diff without a prior would silently no-op — both
 * fail loudly instead.
 */
function loadPriorVersionCode(
  opts: CommandOptions,
  renderer: ReturnType<typeof createProgressRenderer>
): string | undefined {
  const priorVersionCode = opts.priorVersion
    ? fs.readFileSync(opts.priorVersion, "utf-8")
    : undefined;
  if (priorVersionCode !== undefined && !priorVersionCode.trim()) {
    throw new Error(`--prior-version file is empty: ${opts.priorVersion}`);
  }
  if (opts.reconcilePriorDiff && !priorVersionCode) {
    throw new Error("--reconcile-prior-diff requires --prior-version");
  }
  if (priorVersionCode) {
    renderer.message(`Prior version: loaded from ${opts.priorVersion}`);
  }
  return priorVersionCode;
}

async function runPipeline(
  filename: string,
  opts: CommandOptions,
  provider: import("../llm/types.js").LLMProvider,
  renderer: ReturnType<typeof createProgressRenderer>,
  profiler: import("../profiling/index.js").Profiler | typeof NULL_PROFILER,
  concurrency: number,
  moduleConcurrency: number | undefined
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

  // 2. Load prior version code if --prior-version was specified.
  const priorVersionCode = loadPriorVersionCode(opts, renderer);

  // 3. Build plugins with config available upfront — no callbacks
  const rename = createRenamePlugin({
    provider,
    concurrency,
    moduleConcurrency,
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
    bundlerType: config.bundlerType,
    priorVersionCode,
    reconcilePriorDiff: opts.reconcilePriorDiff,
    namingFloor: opts.namingFloor,
    namingFloorSweep: opts.namingFloorSweep,
    emitRenameLedger: !!opts.renameLedger
  });
  let lastRenameResult:
    | import("../rename/plugin.js").RenamePluginResult
    | undefined;
  const parseFailures: Array<{
    filePath: string;
    failure: import("../output-validation.js").OutputParseFailure;
  }> = [];
  const semanticFailures: Array<{
    filePath: string;
    failure: import("../output-validation.js").OutputSemanticFailure;
  }> = [];

  // When --split, capture original source for module detection
  let originalSource = "";
  const isSplit = opts.split;

  // Output is formatted by babel-generator (compact: false) inside the rename
  // plugin — no prettier pass. Prettier on a 14MB file builds a Doc IR that
  // exceeds Node's default 4GB heap.
  const babelPlugin = createBabelPlugin({ profiler });

  let totalInternalErrors = 0;
  const plugins: ((code: string, context: FileContext) => Promise<string>)[] = [
    (code, _ctx) => babelPlugin(code),
    async (code, ctx) => {
      const result = await rename(code, ctx);
      lastRenameResult = result;
      totalInternalErrors += result.internalErrors;
      if (result.parseFailure) {
        parseFailures.push({
          filePath: ctx.filePath ?? "<unknown>",
          failure: result.parseFailure
        });
      }
      if (result.semanticFailure) {
        semanticFailures.push({
          filePath: ctx.filePath ?? "<unknown>",
          failure: result.semanticFailure
        });
      }
      if (result.coverageSummary) {
        renderer.message(result.coverageSummary);
        debug.log("summary", result.coverageSummary);
      }
      return result.code;
    }
  ];

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
      provider,
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
      lastRenameResult.coverageData,
      lastRenameResult.transferStats,
      lastRenameResult.thirdPartyClassification
    );
    writeDiagnosticsFile(diagReport, opts.diagnostics);
    renderer.message(`Diagnostics written to ${opts.diagnostics}`);
  }

  if (opts.renameLedger && lastRenameResult?.renameLedger) {
    writeRenameLedger(opts.renameLedger, lastRenameResult.renameLedger);
    renderer.message(
      `Rename ledger: ${lastRenameResult.renameLedger.ledger.entries.length} ` +
        `rename(s) → ${opts.renameLedger}/ (apply: node ${opts.renameLedger}/apply.mjs)`
    );
  }

  reportParseFailures(parseFailures, renderer);
  reportSemanticFailures(semanticFailures, renderer);
  reportInternalErrors(totalInternalErrors, renderer);
}

/**
 * Reports output files that failed to re-parse after renaming and marks the
 * run as failed. Files are still written so a long run's output can be
 * inspected, but the process exits non-zero.
 */
function reportParseFailures(
  parseFailures: Array<{
    filePath: string;
    failure: import("../output-validation.js").OutputParseFailure;
  }>,
  renderer: ReturnType<typeof createProgressRenderer>
): void {
  if (parseFailures.length === 0) return;

  for (const { filePath, failure } of parseFailures) {
    const location =
      failure.line !== undefined
        ? ` (line ${failure.line}${failure.column !== undefined ? `, column ${failure.column}` : ""})`
        : "";
    renderer.message(
      `ERROR: Generated output for ${filePath} is not valid JavaScript${location}: ${failure.message}` +
        (failure.excerpt ? `\n${failure.excerpt}` : "")
    );
  }
  renderer.message(
    `ERROR: ${parseFailures.length} output file${parseFailures.length > 1 ? "s" : ""} failed to parse — output was written for inspection, but this run is marked failed.`
  );
  process.exitCode = 1;
}

/**
 * Reports output files whose renames violated a semantic invariant
 * (free-name capture, left-behind reference, or a split declaration).
 * The output parses, so this comparison is the only gate that catches
 * these — same failure semantics as parse failures.
 */
function reportSemanticFailures(
  semanticFailures: Array<{
    filePath: string;
    failure: import("../output-validation.js").OutputSemanticFailure;
  }>,
  renderer: ReturnType<typeof createProgressRenderer>
): void {
  if (semanticFailures.length === 0) return;

  for (const { filePath, failure } of semanticFailures) {
    renderer.message(`ERROR: ${filePath}: ${failure.message}`);
  }
  renderer.message(
    `ERROR: ${semanticFailures.length} output file${semanticFailures.length > 1 ? "s" : ""} violated rename invariants — output was written for inspection, but this run is marked failed.`
  );
  process.exitCode = 1;
}

/**
 * Reports internal per-function pipeline errors. LLM provider errors are
 * contained (they yield unrenamed outcomes) and never reach this count —
 * a nonzero value is a programming error, so the run is marked failed
 * even though output was written.
 */
function reportInternalErrors(
  internalErrors: number,
  renderer: ReturnType<typeof createProgressRenderer>
): void {
  if (internalErrors === 0) return;
  renderer.message(
    `ERROR: ${internalErrors} function${internalErrors > 1 ? "s" : ""} hit an internal error during renaming (see debug log) — output was written, but this run is marked failed.`
  );
  process.exitCode = 1;
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
      "Max concurrent function-lane LLM requests " +
        "(flag > HUMANIFY_CONCURRENCY env). Module-lane size is set separately " +
        "via HUMANIFY_MODULE_CONCURRENCY; the global in-flight cap is their sum.",
      env("HUMANIFY_CONCURRENCY") ?? `${DEFAULT_CONCURRENCY}`
    )
    .option(
      "--retries <n>",
      "Number of retry attempts for failed API calls",
      "3"
    )
    .option("--timeout <ms>", "LLM request timeout in milliseconds", "300000")
    .option(
      "--reasoning-effort <level>",
      "Reasoning effort for reasoning models: low, medium, or high " +
        "(flag > HUMANIFY_REASONING_EFFORT env; default: server-side default). " +
        "'low' is ~8x faster on gpt-oss at equal name quality; only set it for " +
        "reasoning models — non-reasoning models (e.g. gpt-4o-mini) reject it."
    )
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
    .option(
      "--max-retries <n>",
      "Per-identifier LLM call limit, initial + retries (default: 2; further conflicts resolve by suffixing)"
    )
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
      "--prior-version <path>",
      "Path to a prior humanified file for cross-version rename reuse"
    )
    .option(
      "--reconcile-prior-diff",
      "After generation, snap rename-noise diff hunks back to the prior version's names (requires --prior-version)"
    )
    .option(
      "--naming-floor",
      "Close minted-token coverage gaps deterministically (class/function-expression inner-id derivation + decoration retry)"
    )
    .option(
      "--naming-floor-sweep",
      "With --naming-floor, also LLM-name the remaining minted survivors (params/decls/vars). " +
        "Prior-aware with --prior-version + --reconcile-prior-diff: prior names transfer deterministically and the LLM names only the residue"
    )
    .option(
      "--split-ledger <path>",
      "Prior split ledger for cross-release file-assignment inheritance " +
        "(default: auto-discovered next to --prior-version)"
    )
    .option(
      "--split-llm-names",
      "LLM-polish NEW split file/folder names (fresh-grouping releases only; " +
        "inherited names never change). Requires --split"
    )
    .option(
      "--split-runnable",
      "Emit a runnable CommonJS module graph (require/exports, live " +
        "cross-file bindings) instead of byte-exact review slices. Requires --split"
    )
    .option(
      "--rename-ledger <dir>",
      "Write a replayable rename ledger (every rename keyed by byte position) " +
        "+ source snapshot + a standalone apply.mjs, so the LLM-rename output " +
        "can be reproduced without re-running the model"
    )
    .option(
      "--profile <path>",
      "Write performance profile to JSON file (Chrome Trace Event format, viewable at chrome://tracing or ui.perfetto.dev)"
    )
    .action(async (filename: string, opts: CommandOptions) => {
      // Reject unusable flag combinations before doing any work, so a flag
      // that could not take effect crashes loudly instead of being ignored.
      enforceFlagInvariants(opts);
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
      // Module-lane size: HUMANIFY_MODULE_CONCURRENCY env, else the processor's
      // bundler-aware default (20, or 40 for esbuild). Env-tunable without code
      // changes for high-throughput servers.
      const moduleConcurrencyEnv = env("HUMANIFY_MODULE_CONCURRENCY");
      const moduleConcurrency = moduleConcurrencyEnv
        ? parseNumber(moduleConcurrencyEnv)
        : undefined;
      const profiler = opts.profile ? new Profiler(true) : NULL_PROFILER;
      const provider = buildProvider(opts, concurrency, moduleConcurrency);

      try {
        await runPipeline(
          filename,
          opts,
          provider,
          renderer,
          profiler,
          concurrency,
          moduleConcurrency
        );
      } finally {
        await finalizeProfile(opts, filename, profiler, renderer);
        renderer.finish();
        await finalizeLogStream(logStream);
      }
    });
}
