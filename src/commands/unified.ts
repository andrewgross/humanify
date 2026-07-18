import type { Command } from "commander";
import fs from "node:fs";
import path from "node:path";
import { debug } from "../debug.js";
import { detectBundle } from "../detection/index.js";
import type { BundlerType, MinifierType } from "../detection/types.js";
import {
  SELECTABLE_BUNDLERS,
  SELECTABLE_MINIFIERS
} from "../detection/types.js";
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
  HUMANIFIED_SOURCE_PATH,
  SPLIT_LEDGER_PATH,
  findSplitLedgerPath
} from "../split/layout.js";
import {
  type StableSplitLedger,
  stableSplitFromCode
} from "../split/stable-split.js";
import { createSplitNamer, createTreeReviser } from "../split/split-namer.js";
import { createVendorNamer } from "../unpack/vendor-namer.js";
import { runnableEntryFile, tryEmitRunnableCjs } from "../split/cjs-emit.js";
import { relinkBunModules } from "../split/bun-relink.js";
import {
  detectExternalPackages,
  writeRunnableScaffold
} from "../split/runnable-scaffold.js";
import {
  type BunModulesManifest,
  bunManifestPath,
  loadPriorVendorNames
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
  splitPure?: boolean;
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
      when: !!opts.splitPure,
      flag: "--split-pure",
      needs: opts.split,
      prereq: "--split"
    },
    {
      when: !!opts.splitLedger,
      flag: "--split-ledger",
      needs: opts.split,
      prereq: "--split"
    }
  ];
  const preconditionViolations = rules
    .filter((r) => r.when && !r.needs)
    .map((r) => `${r.flag} requires ${r.prereq}`);
  const valueViolations = [
    checkEnumFlag("--bundler", opts.bundler, SELECTABLE_BUNDLERS),
    checkEnumFlag("--minifier", opts.minifier, SELECTABLE_MINIFIERS)
  ].filter((v): v is string => v !== null);
  return [...preconditionViolations, ...valueViolations];
}

/**
 * Reject a flag whose value is not one of `allowed`. Returns a violation
 * message (mirroring the "no silent no-op" principle: a value that could not
 * take effect crashes) or null when the flag is absent or valid.
 */
function checkEnumFlag(
  flag: string,
  value: string | undefined,
  allowed: readonly string[]
): string | null {
  if (value === undefined || allowed.includes(value)) return null;
  return `${flag} must be one of: ${allowed.join(", ")} (got "${value}")`;
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

const sha = (s) => createHash("sha256").update(s).digest("hex");
// Apply one stage's entries to src (right-to-left splices), verifying the
// snapshot hash first. The base ledger is stage 0; each post stage renames
// the prior stage's output (reconcile / deferred-sweep coordinate spaces).
function applyStage(src, stage) {
  if (sha(src) !== stage.sourceSha256) {
    throw new Error("source does not match the stage's sourceSha256");
  }
  const edits = [];
  for (const e of stage.entries) {
    for (const [s, en] of e.occurrences) edits.push([s, en, e.finalName]);
  }
  edits.sort((a, b) => b[0] - a[0]);
  let out = src;
  for (const [s, en, name] of edits) out = out.slice(0, s) + name + out.slice(en);
  return out;
}

let out = applyStage(source, ledger);
for (const stage of ledger.post ?? []) out = applyStage(out, stage);
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
 * --split-ledger wins, else auto-discovered from the --prior-version file
 * (findSplitLedgerPath: the ledger sits beside the prior release's
 * .humanify/humanified.js, so a lineage chain inherits automatically;
 * older tree-root and pre-.humanify flat layouts are still discovered).
 */
function loadPriorSplitLedger(
  opts: CommandOptions,
  renderer: ReturnType<typeof createProgressRenderer>
): StableSplitLedger | undefined {
  const discovered = opts.priorVersion
    ? findSplitLedgerPath(opts.priorVersion)
    : undefined;
  const ledgerPath = opts.splitLedger ?? discovered;
  if (!ledgerPath) return undefined;
  const parsed = JSON.parse(fs.readFileSync(ledgerPath, "utf-8"));
  if (parsed?.version !== 1) {
    throw new Error(`Unsupported split ledger version in ${ledgerPath}`);
  }
  renderer.message(`Split ledger: inheriting assignments from ${ledgerPath}`);
  return parsed as StableSplitLedger;
}

/**
 * Prior vendor names for cross-release carry-over, discovered from the
 * --prior-version file the same way the split ledger is. Vendor names are
 * LLM-derived and unstable run-to-run, and src/ imports vendor by path, so
 * without this an unchanged library rewrites require() lines across app code
 * every release.
 */
function loadPriorVendorNamesIfPresent(
  opts: CommandOptions,
  renderer: ReturnType<typeof createProgressRenderer>
): Map<string, string[]> | undefined {
  if (!opts.priorVersion) return undefined;
  const names = loadPriorVendorNames(opts.priorVersion);
  if (!names) return undefined;
  const factories = [...names.values()].reduce((n, g) => n + g.length, 0);
  renderer.message(
    `Vendor names: carrying ${factories} over from the prior release ` +
      `(${names.size} structural groups)`
  );
  return names;
}

/** Persist the split ledger into the output tree's metadata folder. */
function writeSplitLedger(outputDir: string, ledger: StableSplitLedger): void {
  const ledgerPath = path.join(outputDir, SPLIT_LEDGER_PATH);
  fs.mkdirSync(path.dirname(ledgerPath), { recursive: true });
  fs.writeFileSync(ledgerPath, JSON.stringify(ledger));
}

/** Persist the full single-file humanified output beside the ledger. It is
 * the canonical `--prior-version` target for the NEXT release: the rename
 * reuse pass diffs against its `.code`, and the split ledger it inherits
 * sits in the same folder (findSplitLedgerPath). */
function writeHumanifiedSource(outputDir: string, code: string): void {
  const dest = path.join(outputDir, HUMANIFIED_SOURCE_PATH);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.writeFileSync(dest, code);
}

/**
 * Release the large post-rename ASTs once the split tree is written to disk:
 * `renameResult.ast` (the whole bundle's scope-resolved NodePath/Scope graph)
 * and `stable.wrapper` (a full bundle parse). The Bun re-link that runs next
 * reads the tree from disk and needs neither; leaving them reachable makes every
 * GC the re-link triggers trace the multi-GB graph, turning the pass from
 * seconds into tens of minutes. Both fields are optional precisely so they can
 * be dropped here. `renameResult.ast`'s only reader is the adapter-split
 * fallback, which runSplit skips once the stable tree exists.
 */
export function releaseSplitSourceState(
  renameResult: { ast?: unknown },
  stable: { wrapper?: unknown }
): void {
  renameResult.ast = undefined;
  stable.wrapper = undefined;
}

/** The unpack step's on-disk copy of the processed source (e.g. the Bun
 * passthrough index.js) is fully superseded once the split tree exists —
 * its statements live in the tree. Remove it BEFORE the tree is written
 * so the runnable entry can claim the same index.js name. Two paths are
 * never touched: anything outside outputDir, and the run's own input file
 * — with `-o <input's dir>` the passthrough copy resolves to the input
 * itself, and deleting it would destroy the user's source. */
export function removeConsumedSourceFile(
  outputDir: string,
  sourcePath: string,
  inputFile: string
): void {
  if (!sourcePath) return;
  const resolved = path.resolve(sourcePath);
  if (resolved === path.resolve(inputFile)) return;
  const rel = path.relative(path.resolve(outputDir), resolved);
  if (rel.startsWith("..") || path.isAbsolute(rel)) return;
  fs.rmSync(sourcePath, { force: true });
}

/** The Bun unpack manifest, written next to the extracted factory files
 * in vendor/ — or null when this run extracted no factories. */
function loadBunManifest(outputDir: string): BunModulesManifest | null {
  const manifestPath = bunManifestPath(outputDir);
  if (!fs.existsSync(manifestPath)) return null;
  const manifest = JSON.parse(
    fs.readFileSync(manifestPath, "utf-8")
  ) as BunModulesManifest;
  if (manifest.adapter !== "bun" || manifest.factories.length === 0) {
    return null;
  }
  return manifest;
}

/** Re-link extracted Bun CJS factory modules into the runnable split
 * graph (Bun bundles only). */
async function relinkBunFactories(
  outputDir: string,
  manifest: BunModulesManifest,
  splitFiles: string[],
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<void> {
  await relinkBunModules(outputDir, manifest, splitFiles);
  renderer.message(
    `Re-linked ${manifest.factories.length} Bun factory module(s) into the runnable graph`
  );
}

/** Emit a self-contained runner (run.cjs), package.json (detected external
 * deps), and RUNNABLE.md into a runnable split tree so it can be
 * `npm install`ed and executed directly. */
async function emitRunnableScaffold(
  outputDir: string,
  runnable: Map<string, string>,
  renderer: ReturnType<typeof createProgressRenderer>,
  resolveFromDir: string | undefined
): Promise<void> {
  const entry = runnableEntryFile(runnable);
  const externals = await detectExternalPackages(outputDir);
  await writeRunnableScaffold(outputDir, entry, externals, resolveFromDir);
  const deps = externals.length
    ? `${externals.length} external dep(s): ${externals.slice(0, 6).join(", ")}${externals.length > 6 ? ", …" : ""}`
    : "no external deps";
  renderer.message(
    `Runnable scaffold: run.cjs + package.json (${deps}) — \`npm install && node run.cjs --version\``
  );
}

/** Post-tree finishing: re-link extracted Bun factories into the runnable
 * graph, drop the unpack step's superseded runtime file, and emit the
 * runnable scaffold. Returns whether a Bun re-link ran. */
async function finishSplitOutput(
  opts: CommandOptions,
  inputFile: string,
  runnable: Map<string, string> | null,
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<boolean> {
  // A Bun bundle's library factories were extracted to vendor/ by the
  // unpack step; the runnable tree references them by free identifier.
  // Re-bind those into the executable graph so the split tree actually
  // loads and runs (no-op for non-Bun input).
  const manifest = loadBunManifest(opts.outputDir);
  if (runnable && manifest) {
    await relinkBunFactories(
      opts.outputDir,
      manifest,
      [...runnable.keys()],
      renderer
    );
  }
  // The unpack runtime file is fully superseded by the split tree; the
  // re-link removes it on the runnable path, this covers the pure tree.
  if (!runnable && manifest?.runtimeFile) {
    fs.rmSync(path.join(opts.outputDir, manifest.runtimeFile), {
      force: true
    });
  }
  if (runnable) {
    await emitRunnableScaffold(
      opts.outputDir,
      runnable,
      renderer,
      path.dirname(inputFile)
    );
  }
  return Boolean(runnable && manifest);
}

/** Stable statement-level split (Bun wrapper bundles). Returns false when
 * the input is not wrapper-shaped or the pass fails — caller falls back
 * to the legacy adapter splitter; a completed run is never lost.
 *
 * On a fresh-grouping release (no prior ledger) folders and files are named
 * by the LLM, the same model that renamed the functions — inherited names
 * never change (a rename is cross-version churn), so naming is skipped
 * whenever a prior ledger drives the assignment. */
async function tryStableSplit(
  opts: CommandOptions,
  inputFile: string,
  renameResult: import("../rename/plugin.js").RenamePluginResult,
  processedSourcePath: string,
  provider: import("../llm/types.js").LLMProvider,
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<boolean> {
  try {
    const prior = loadPriorSplitLedger(opts, renderer);
    // LLM-name folders/files on the fresh release; inherited layout is kept.
    const namer = prior ? undefined : createSplitNamer(provider);
    const reviser = prior ? undefined : createTreeReviser(provider);
    if (namer) renderer.message("Split naming: LLM-naming folders and files");
    const stable = await stableSplitFromCode(renameResult.code, {
      prior,
      namer,
      reviser
    });
    if (!stable) return false;
    removeConsumedSourceFile(opts.outputDir, processedSourcePath, inputFile);
    // --split emits the runnable live-binding CommonJS module graph by
    // default; --split-pure keeps the byte-exact review slices. A runnable
    // decline or failure falls back to the review tree LOUDLY — the stable
    // tree and its ledger are never sacrificed to the runnable emitter.
    const runnable = opts.splitPure
      ? null
      : tryEmitRunnableCjs(
          renameResult.code,
          stable.ledger,
          (reason) =>
            renderer.message(
              `Runnable emit declined: ${reason} — writing byte-exact review tree instead`
            ),
          // Reuse the wrapper stableSplitFromCode parsed from the same string,
          // skipping a redundant parse + scope crawl of the whole bundle.
          stable.wrapper
        );
    writeSplitTree(opts.outputDir, runnable ?? stable.fileContents);
    writeSplitLedger(opts.outputDir, stable.ledger);
    // The full humanified single file, beside the ledger, is what the NEXT
    // release points --prior-version at (rename reuse + ledger inheritance).
    writeHumanifiedSource(opts.outputDir, renameResult.code);
    // The tree, ledger, and source are on disk now. Drop the big in-memory ASTs
    // before the Bun re-link — it reads the tree from disk, and holding the
    // multi-GB scope graph live makes its every GC trace the whole thing.
    releaseSplitSourceState(renameResult, stable);
    const relinked = await finishSplitOutput(
      opts,
      inputFile,
      runnable,
      renderer
    );
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
    renderer.message(
      `Next release: --prior-version ${path.join(opts.outputDir, HUMANIFIED_SOURCE_PATH)}`
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
  original: { source: string; path: string },
  provider: import("../llm/types.js").LLMProvider,
  profiler: import("../profiling/index.js").Profiler | typeof NULL_PROFILER,
  renderer: ReturnType<typeof createProgressRenderer>
): Promise<void> {
  const splitSpan = profiler.startSpan("split", "pipeline");
  if (
    await tryStableSplit(
      opts,
      filename,
      renameResult,
      original.path,
      provider,
      renderer
    )
  ) {
    splitSpan.end({ stable: true });
    renderer.message(`Split complete: written to ${opts.outputDir}`);
    return;
  }
  if (!renameResult.ast) {
    // tryStableSplit released the source AST (releaseSplitSourceState) only
    // AFTER committing the stable tree to disk, so a false return here means a
    // post-commit step failed, not that the split never ran. The tree is
    // already written — don't discard it with a cruder adapter re-split.
    splitSpan.end({ stable: false });
    renderer.message(
      "Split tree already written; skipping adapter fallback after post-split failure"
    );
    return;
  }
  const detection = detectModules(original.source);
  const fileContents = splitFromAst(
    renameResult.ast,
    filename,
    original.source,
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
 * Effective values of the shipped noise levers, all defaulting ON: the
 * naming floor (exp021, pure win, no LLM cost), the LLM sweep of the
 * remaining minted survivors (exp022, prior-aware — leftovers it removes
 * are exactly the names that churn as one-time corrections on later
 * hops), and the prior-diff reconcile whenever a prior is present (the
 * pass self-discards when it cannot hold the pure-rename invariant). All
 * three were flag-gated and silently dormant in every production walk
 * run. Disabling the floor implicitly disables the sweep.
 */
export function effectiveLeverConfig(
  opts: CommandOptions,
  hasPrior: boolean
): {
  namingFloor: boolean;
  namingFloorSweep: boolean;
  reconcilePriorDiff: boolean;
} {
  const namingFloor = opts.namingFloor ?? true;
  return {
    namingFloor,
    namingFloorSweep: (opts.namingFloorSweep ?? true) && namingFloor,
    reconcilePriorDiff: (opts.reconcilePriorDiff ?? true) && hasPrior
  };
}

/**
 * Load and validate the --prior-version file. An empty file would flow
 * through as "no prior" and silently become a full-cost zero-transfer
 * run — fail loudly instead.
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
    ...effectiveLeverConfig(opts, !!priorVersionCode),
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

  // When --split, capture the processed file's original source (for module
  // detection) and its on-disk path (removed once the split supersedes it).
  const original = { source: "", path: "" };
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
    vendorNamer: createVendorNamer(provider),
    priorVendorNames: loadPriorVendorNamesIfPresent(opts, renderer),
    onOriginalSource: isSplit
      ? (filePath, code) => {
          original.source = code;
          original.path = filePath;
        }
      : undefined,
    skipFileWrite: isSplit
  });

  if (isSplit && lastRenameResult) {
    await runSplit(
      filename,
      opts,
      lastRenameResult,
      original,
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
      `Force bundler type (${SELECTABLE_BUNDLERS.join(", ")})`
    )
    .option(
      "--minifier <type>",
      `Force minifier type (${SELECTABLE_MINIFIERS.join(", ")})`
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
      "Split output into a multi-file tree (src/ + vendor/ + run scaffold), " +
        "emitted as a runnable CommonJS module graph by default"
    )
    .option(
      "--prior-version <path>",
      "Path to a prior humanified file for cross-version rename reuse"
    )
    .option(
      "--reconcile-prior-diff",
      "After generation, snap rename-noise diff hunks back to the prior version's names (default with --prior-version)"
    )
    .option(
      "--no-reconcile-prior-diff",
      "Disable the prior-diff reconcile pass"
    )
    .option(
      "--naming-floor",
      "Close minted-token coverage gaps deterministically (class/function-expression inner-id derivation + decoration retry; default on)"
    )
    .option("--no-naming-floor", "Disable the deterministic naming floor")
    .option(
      "--naming-floor-sweep",
      "LLM-name the minted survivors the naming floor cannot derive (params/decls/vars; default on). " +
        "Prior-aware with a prior version: prior names transfer deterministically and the LLM names only the residue"
    )
    .option(
      "--no-naming-floor-sweep",
      "Disable the LLM sweep of minted survivors"
    )
    .option(
      "--split-ledger <path>",
      "Prior split ledger for cross-release file-assignment inheritance " +
        "(default: auto-discovered next to --prior-version)"
    )
    .option(
      "--split-pure",
      "Emit the byte-exact review tree instead of the runnable CommonJS " +
        "module graph (the --split default). Requires --split"
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
