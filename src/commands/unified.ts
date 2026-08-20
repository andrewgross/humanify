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
import { ensureFileExists } from "../file-utils.js";
import { buildPipelineConfig } from "../pipeline/config.js";
import type { FileContext } from "../pipeline/types.js";
import { CachedLLMProvider } from "../llm/cached-provider.js";
import { withDebug } from "../llm/debug-wrapper.js";
import { OpenAICompatibleProvider } from "../llm/openai-compatible.js";
import { withRateLimit } from "../llm/rate-limiter.js";
import { createBabelPlugin } from "../plugins/babel/babel.js";
import { createRenamePlugin } from "../rename/plugin.js";
import { stageFingerprint } from "../stage-fingerprint.js";
import { pipelineSelectionRecord } from "../pipeline/selection-record.js";
import { renameClaimStats } from "../rename/validated-rename.js";
import { FAILED_OUTPUT_DIR, preserveFailedOutput } from "../failed-output.js";
import {
  formatProfileSummary,
  NULL_PROFILER,
  Profiler,
  toTraceEvents
} from "../profiling/index.js";
import {
  HUMANIFIED_SOURCE_PATH,
  PLACEMENT_STATS_PATH,
  STAGE_HASHES_PATH,
  SPLIT_LEDGER_PATH,
  findSplitLedgerPath,
  splitTreeRootOf
} from "../split/layout.js";
import { carryRenamesIntoBundle } from "../split/bundle-carry.js";
import {
  type PostSplitRename,
  postSplitReconcile
} from "../split/post-split-reconcile.js";
import {
  createIsEligible,
  type IsEligibleFn
} from "../rename/rename-eligibility.js";
import {
  type StableSplitLedger,
  placementSummary,
  stableSplitFromCode
} from "../split/stable-split.js";
import { createSplitNamer, createTreeReviser } from "../split/split-namer.js";
import {
  type VendorNamingStats,
  createVendorNamer
} from "../unpack/vendor-namer.js";
import { runnableEntryFile, tryEmitRunnableCjs } from "../split/cjs-emit.js";
import { relinkBunModules } from "../split/bun-relink.js";
import { desugarSummary, desugarUsingInTree } from "../split/using-desugar.js";
import {
  detectExternalPackages,
  writeRunnableScaffold
} from "../split/runnable-scaffold.js";
import {
  type BunModulesManifest,
  type BunModulesManifestEntry,
  bunManifestPath,
  findPriorTreeRoot,
  loadPriorManifestFactories,
  loadPriorVendorNames
} from "../unpack/adapters/bun.js";
import { createProgressRenderer } from "../ui/progress.js";
import { setAmbiguityProbePath } from "../prior-version/ambiguity-probe.js";
import { configureKillSwitches, switchOn } from "../kill-switches.js";
import { selectUnpackAdapter } from "../unpack/index.js";
import { placementTrail } from "../split/placement-trail.js";
import { strategyTrail } from "../rename/strategy-trail.js";
import { nameContention } from "../rename/name-contention.js";
import { unminify } from "../unminify.js";
import { verbose } from "../verbose.js";
import {
  DEFAULT_CONCURRENCY,
  DEFAULT_LLM_TIMEOUT_MS,
  MAX_DEFAULT_MODULE_CONCURRENCY
} from "./default-args.js";
import { type Settings, resolveSettings } from "./settings.js";

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
  disable?: string;
  probe?: string;
  maxTokens?: string;
  moduleConcurrency?: string;
  llmCache?: string;
  ambiguityProbe?: string;
  splitLedger?: string;
  splitPure?: boolean;
  renameLedger?: string;
  statsJson?: string;
}

/**
 * Which default-on flags the user actually typed. Default-on booleans make
 * a plain options object ambiguous — `namingFloorSweep: true` is both the
 * default state and the explicit `--naming-floor-sweep` — so intent rules
 * need commander's option-value sources. Callers without sources omit
 * this; a true value then counts as explicit.
 */
export interface FlagExplicitness {
  namingFloorSweep?: boolean;
}

/**
 * Flag preconditions, checked upfront. A flag whose behavior is gated
 * behind another flag is silently ignored when that prerequisite is
 * missing — but these flags are invariants for how a run is processed, so
 * an unmet precondition is an error, not a no-op. Returns one message per
 * violation (empty when every precondition holds), in flag-declaration
 * order.
 */
export function checkFlagInvariants(
  opts: CommandOptions,
  explicit?: FlagExplicitness
): string[] {
  const sweepExplicitlyOn =
    explicit?.namingFloorSweep ?? opts.namingFloorSweep === true;
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
    },
    {
      // An explicit `--naming-floor-sweep --no-naming-floor` is a typed
      // contradiction — crash loudly. The DEFAULT sweep under
      // --no-naming-floor silently gates off in resolveSettings' levers.
      when: sweepExplicitlyOn,
      flag: "--naming-floor-sweep",
      needs: opts.namingFloor !== false,
      prereq: "--naming-floor"
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

/**
 * Apply --disable/--probe ONCE, upfront, validated against the registry —
 * an unknown switch exits here with the valid list, before any work.
 */
function applySwitchFlags(opts: CommandOptions): void {
  try {
    configureKillSwitches({
      disable: opts.disable?.split(","),
      probe: opts.probe?.split(",")
    });
  } catch (e) {
    console.error(`Error: ${e instanceof Error ? e.message : e}`);
    process.exit(1);
  }
}

/** Crash upfront with a clear message when any flag precondition is unmet. */
function enforceFlagInvariants(
  opts: CommandOptions,
  explicit?: FlagExplicitness
): void {
  const violations = checkFlagInvariants(opts, explicit);
  if (violations.length === 0) return;
  for (const message of violations) console.error(`Error: ${message}`);
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

/**
 * The prior release's manifest entries in that release's emitted order, so the
 * fresh manifest can follow it instead of bundle order (exp047: 4,780 lines of
 * entry-block reshuffling across the four gate hops). Ordering only -- no name
 * is derived from this, because vendor names feed `src/` require paths.
 */
function loadPriorManifestFactoriesIfPresent(
  opts: CommandOptions
): BunModulesManifestEntry[] | undefined {
  if (!opts.priorVersion) return undefined;
  return loadPriorManifestFactories(opts.priorVersion);
}

/**
 * Record stage-boundary fingerprints so a future divergence is attributable.
 *
 * Two runs of identical code diverged once in 34 null controls, and pinning it
 * to "downstream of naming" required deducing from cache-write counts that both
 * legs had asked identical prompts. These hashes answer that directly: equal
 * `afterNaming` with differing trees means the split or a post-pass; differing
 * `afterNaming` means naming or earlier.
 */
function writeStageHashes(
  outputDir: string,
  hashes: { afterNaming: string; afterPlacement: string }
): void {
  const dest = path.join(outputDir, STAGE_HASHES_PATH);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.writeFileSync(dest, `${JSON.stringify(hashes, null, 2)}\n`);
}

/** Persist the split ledger into the output tree's metadata folder. */
/**
 * Persist the per-tier placement counts as structured data.
 *
 * They were already computed for every split and then rendered only into a
 * prose log line, so answering "did more statements fall through to locality
 * this run?" meant grepping two multi-GB logs and parsing English. Keyed by
 * the placement registry, so a new tier records itself here with no edit.
 */
function writePlacementStats(
  outputDir: string,
  stats: import("../split/stable-split.js").StableSplitStats
): void {
  const dest = path.join(outputDir, PLACEMENT_STATS_PATH);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.writeFileSync(
    dest,
    JSON.stringify(
      {
        statements: stats.statements,
        files: stats.files,
        folders: stats.folders,
        inherited: stats.inherited,
        residueLocality: stats.residueLocality,
        byTier: stats.byTier
      },
      null,
      2
    )
  );
}

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

/** Compact deterministic match/rename breakdown for the eval harness
 * (experiments/034-eval-harness): the stable counts only — coverage
 * (functions/module-bindings cached vs close-match vs LLM), transfer stats,
 * prior-match totals, and naming-floor — NOT the per-identifier dump the full
 * `--diagnostics` report carries. Small enough to store one per model per pair. */
/** Did the vendor namer receive any requests at all this run? */
function vendorNamingAttempted(s?: VendorNamingStats): boolean {
  if (!s) return false;
  return s.named + s.declined + s.echoed + s.batchesFailed > 0;
}

function writeEvalStats(
  destPath: string,
  result: import("../rename/plugin.js").RenamePluginResult,
  vendorNamingStats?: VendorNamingStats,
  selection?: import("../pipeline/selection-record.js").PipelineSelectionRecord
): void {
  const stats = {
    coverage: result.coverageData,
    transferStats: result.transferStats,
    priorVersionApplied: result.priorVersionApplied,
    priorVersionAlreadyNamed: result.priorVersionAlreadyNamed,
    priorVersionBindingsApplied: result.priorVersionBindingsApplied,
    namingFloor: result.namingFloor,
    // Cascade success rates that previously reached only a -vv log line, so a
    // committed run could not be asked how well its tiers did. `undefined`
    // when the stage did not run — absent is not zero.
    closeMatchStats: result.closeMatchStats,
    // The matching cascade's per-tier decomposition. Written on every
    // prior-carrying run: docs/pipeline-stages.md records that MOST
    // cross-version noise is a matching failure, and exp053 measured
    // enclosingStatement resolving 21.1% against shingle's 0.1% — a spread no
    // committed artifact could show until now.
    resolutionStats: result.resolutionStats,
    // The BINDING cascade's own tier counters — the unguarded-singleton
    // surface. Print-only until 2026-08-10; a verdict a gate cannot read
    // is advisory text.
    bindingResolutionStats: result.bindingResolutionStats ?? null,
    // Recorded only when the namer was actually ASKED something. All-zero
    // counters would be ambiguous between "it ran and named nothing" and "it
    // was never invoked" — and on a prior-carrying run the second is what
    // happens, because every vendor name is carried and only hash-named
    // fallback factories ever reach the namer.
    vendorNaming: vendorNamingAttempted(vendorNamingStats)
      ? vendorNamingStats
      : undefined,
    // ALWAYS written, including all-zero — unlike vendorNaming above. The
    // ledger sits on the guard path of every rename, so it always ran, and a
    // zero is the finding: it means the cross-era condition never arose on
    // this input, so a clean exit says nothing about the fix (exp059's bug
    // fires ~20% of the time). Omitting it would make "did not fire"
    // indistinguishable from "not instrumented".
    renameClaims: renameClaimStats(),
    // WHICH PATH this run took. Deterministic from the input, so not a
    // determinism check — but it differs across pairs silently, and without it
    // a committed run cannot say which unpack adapter processed the bundle.
    selection
  };
  fs.mkdirSync(path.dirname(destPath), { recursive: true });
  fs.writeFileSync(destPath, JSON.stringify(stats, null, 2));
}

/** Debug provenance (`-vv` only): persist the split's binding-identity map
 * `{final -> prior}` beside the ledger so a cross-version relocation study can
 * re-split with the tier ON vs OFF deterministically. Never load-bearing — a
 * missing file just means no study is possible. */
function writePriorMatchMapDebug(
  outputDir: string,
  priorMatchMap: ReadonlyMap<string, string> | undefined
): void {
  if (!debug.enabled || !priorMatchMap || priorMatchMap.size === 0) return;
  const dest = path.join(outputDir, ".humanify", "prior-match-map.json");
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.writeFileSync(dest, JSON.stringify(Object.fromEntries(priorMatchMap)));
}

/**
 * Release the large post-rename ASTs once the split tree is written to disk:
 * `stable.wrapper` (a full bundle parse + crawled scope graph) and — for
 * callers that still hold one — `renameResult.ast`. The Bun re-link that runs
 * next reads the tree from disk and needs neither; leaving them reachable
 * makes every GC the re-link triggers trace the multi-GB graph, turning the
 * pass from seconds into tens of minutes. runSplit already drops
 * `renameResult.ast` at entry (the stable split parses renameResult.code
 * privately, and the adapter fallback re-parses it), so `stable.wrapper` is
 * normally the one live graph released here.
 */
export function releaseSplitSourceState(
  renameResult: { ast?: unknown },
  stable: { wrapper?: unknown }
): void {
  renameResult.ast = undefined;
  stable.wrapper = undefined;
}

/**
 * How a stable-split attempt ended, from the caller's point of view:
 * everything worked, or a step AFTER the tree+ledger were committed to
 * disk failed — the tree exists and must not be overwritten. A failure
 * BEFORE anything was written throws: there is one splitter, and an input
 * it cannot handle is reported, never silently re-split some other way.
 */
type StableSplitOutcome = "complete" | "tree-written-post-failure";

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
  renderer: ReturnType<typeof createProgressRenderer>,
  priorRoot: string | undefined
): Promise<void> {
  debug.log(
    "split",
    `re-linking ${manifest.factories.length} factories + ${splitFiles.length} tree files`
  );
  await relinkBunModules(outputDir, manifest, splitFiles, { priorRoot });
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
      renderer,
      // An unchanged vendored library keeps the prior release's bytes rather
      // than the minifier's freshly-rerolled locals (exp046 Task C).
      opts.priorVersion ? findPriorTreeRoot(opts.priorVersion) : undefined
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
    // Last content pass: compile `using`/`await using` away so the tree
    // loads under Bun (which cannot require CJS+using, bun#11100) and
    // Node < 24. Runs after the re-link so it sees final file contents;
    // the review tree and .humanify/ metadata are never touched.
    const desugared = await desugarUsingInTree(opts.outputDir);
    renderer.message(desugarSummary(opts.outputDir, desugared));
    await emitRunnableScaffold(
      opts.outputDir,
      runnable,
      renderer,
      path.dirname(inputFile)
    );
  }
  return Boolean(runnable && manifest);
}

/**
 * Post-split prior-diff reconciliation over the emitted tree (exp054). Scoped
 * per file, so a name-masked statement is compared against the ~20 candidates
 * in its own file instead of the ~60,000 in the bundle — the identity evidence
 * the split computed and the pipeline used to discard.
 *
 * Runs LAST, on the tree as it stands on disk. `writeSplitTree` is not the end:
 * `finishSplitOutput` then re-links Bun vendor requires and desugars `using`,
 * both of which rewrite `src/` files. Reconciling before that diffs text that
 * is not what ships against a prior that has already been through both, and it
 * cost 1,058 git lines across the four gate hops — concentrated on the hops
 * with the heaviest vendor rotation, which is the mechanism (exp054).
 *
 * Best-effort like every post-output pass: no prior tree, no reconciliation.
 * The rename trail is logged in full because a pass with an empty trail cannot
 * have moved a KPI however the KPI reads (measurement-pitfalls rule 11).
 */
function reconcilePostSplit(
  opts: CommandOptions,
  ledger: StableSplitLedger,
  isEligible: IsEligibleFn,
  renderer: ReturnType<typeof createProgressRenderer>
): void {
  if (!opts.priorVersion) return;
  const priorRoot = splitTreeRootOf(opts.priorVersion);
  const read = (root: string, file: string): string | undefined => {
    try {
      return fs.readFileSync(path.join(root, file), "utf-8");
    } catch {
      return undefined; // absent on that side
    }
  };
  const result = postSplitReconcile({
    ledger,
    readFresh: (file) => read(opts.outputDir, file),
    readPrior: (file) => read(priorRoot, file),
    isEligible
  });
  if (result.changed.size === 0) {
    // Report the zero rather than returning silently. A pass that changed
    // nothing and a pass that never ran produced IDENTICAL output before this
    // — no line either way — so "did the reconcile do anything?" could not be
    // answered from a run log, only by instrumenting the source. That is the
    // same absent-is-not-zero failure this repo has already paid for in
    // `vendorNaming` and in the ledger counters: a silent zero reads as "not
    // applicable" when it may mean "considered N files and restored nothing",
    // which is a finding.
    renderer.message(
      `Post-split reconcile: no changes (considered ${result.stats.considered} file(s))`
    );
    return;
  }
  for (const [file, text] of result.changed) {
    fs.writeFileSync(path.join(opts.outputDir, file), text);
  }
  // The ledger was written before the tree was finished; rewrite it so the
  // names it records are the names now on disk.
  writeSplitLedger(opts.outputDir, ledger);
  carryIntoBundle(opts, ledger, result.renames, renderer);
  renderer.message(
    `Post-split reconcile: restored ${result.renames.length} prior name(s) ` +
      `across ${result.stats.changed} of ${result.stats.considered} file(s)` +
      (result.stats.discarded > 0
        ? ` (${result.stats.discarded} discarded)`
        : "")
  );
  if (result.stats.incoherent > 0) {
    // Loud, not fatal: the tree on disk is correct either way, but the NEXT
    // release would align on a name that is no longer in it.
    renderer.message(
      `Post-split reconcile: WARNING — ${result.stats.incoherent} ledger ` +
        `entr(ies) still name a binding this pass renamed away`
    );
  }
  for (const rename of result.renames) {
    debug.log(
      "post-split-reconcile",
      `${rename.file}: ${rename.fromName} -> ${rename.toName} ` +
        `[${rename.kind}, ${rename.votes} votes]`
    );
  }
}

/**
 * Give `.humanify/humanified.js` the names the tree just shipped.
 *
 * That file is what the NEXT release points `--prior-version` at — it is the
 * lineage a forward walk inherits through. Without this the tree and the bundle
 * disagree by exactly the post-split renames, and each hop has to re-earn the
 * restoration from the prior tree rather than carrying it through the ordinary
 * matcher. Abstains per rename rather than guessing; an abstention leaves that
 * binding named as before, which is the status quo.
 */
function carryIntoBundle(
  opts: CommandOptions,
  ledger: StableSplitLedger,
  renames: PostSplitRename[],
  renderer: ReturnType<typeof createProgressRenderer>
): void {
  const bundlePath = path.join(opts.outputDir, HUMANIFIED_SOURCE_PATH);
  let bundleCode: string;
  try {
    bundleCode = fs.readFileSync(bundlePath, "utf-8");
  } catch {
    return;
  }
  let carry: ReturnType<typeof carryRenamesIntoBundle>;
  try {
    carry = carryRenamesIntoBundle(bundleCode, ledger, renames);
  } catch (err) {
    debug.log(
      "post-split-reconcile",
      `bundle carry skipped: ${err instanceof Error ? err.message : String(err)}`
    );
    return;
  }
  const missed = [...carry.abstained.values()].reduce((n, v) => n + v, 0);
  if (carry.code) fs.writeFileSync(bundlePath, carry.code);
  // The REASONS, not just the count. They were debug-only, so a normal run
  // reported "(10 abstained)" — a number that cannot be acted on, because the
  // reasons mean opposite things. `top-level-would-move-an-export-key` is a
  // deliberate refusal (carrying it drifted 238/238 export keys, exp054);
  // `no-locator` or `slot-out-of-range` are limitations that lose a name the
  // pass had already decided. Same count, different work.
  const byReason = [...carry.abstained]
    .sort((a, b) => b[1] - a[1])
    .map(([reason, n]) => `${reason} x${n}`)
    .join(", ");
  renderer.message(
    `Post-split reconcile: carried ${carry.carried}/${renames.length} ` +
      `name(s) into the bundle for the next release` +
      (missed > 0 ? ` (${missed} abstained: ${byReason})` : "")
  );
  for (const [reason, count] of carry.abstained) {
    debug.log(
      "post-split-reconcile",
      `bundle carry abstained: ${reason} x${count}`
    );
  }
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
  renderer: ReturnType<typeof createProgressRenderer>,
  isEligible: IsEligibleFn,
  /** Assign by the bundle's module fossils (exp070): the selected unpack
   * adapter declared `providesModuleFossils` and the `fossil-split` kill
   * switch is not thrown. Decided once at detection, threaded down. */
  fossil: boolean
): Promise<StableSplitOutcome> {
  // Set once the tree + ledger + humanified source are on disk: a failure
  // after this point must not trigger the adapter fallback (it would
  // clobber the committed tree with a cruder re-split).
  let committed = false;
  try {
    // AST-cache hygiene is owned by the parse funnel (babel-utils
    // maybeResetAstCaches): stableSplitFromCode's full-bundle parseFileAst
    // starts the split phase's fresh cache era on its own.
    const prior = loadPriorSplitLedger(opts, renderer);
    // LLM-name folders/files on the fresh release; inherited layout is kept.
    const namer = prior ? undefined : createSplitNamer(provider);
    const reviser = prior ? undefined : createTreeReviser(provider);
    // exp087 (Andrew): fossil-path FRESH MINTS are named from their contents
    // on WARM hops. Cold start keeps mechanical stems — a one-time event
    // judged by steady state, and the blast radius of LLM-naming 4,800
    // cold files at once is a separate decision.
    const mintNamer = prior ? createSplitNamer(provider) : undefined;
    if (namer) renderer.message("Split naming: LLM-naming folders and files");
    if (mintNamer) {
      renderer.message("Split naming: LLM-naming fresh module mints");
    }
    // Lever B: the rename pipeline captured {final name → matched prior name}
    // for every module binding whose name flipped across versions (built in
    // plugin.ts before the naming-era AST is released). It drives the
    // binding-identity tier in assignWithPrior — a relocated binding inherits
    // its prior file even when both the hash and name tiers miss it. The tier
    // gates hard (unique + unanimous → inherit, else abstain to locality), and
    // abstains on any name absent from the split input, so a stale key is a
    // harmless no-op.
    const stable = await stableSplitFromCode(renameResult.code, {
      fossil,
      prior,
      namer,
      mintNamer,
      reviser,
      // Also carries the content-anchor tier's prior statement texts, which
      // zip with `prior.order` into (text, file) pairs — captured during prior
      // matching, never re-parsed.
      priorCarry: renameResult.priorCarry
    });
    if (!stable) {
      // One splitter. An input the stable split cannot handle (no wrapper
      // IIFE / fewer than two statements) is a detection problem to fix
      // upfront, not something to paper over mid-run with a cruder tree.
      throw new Error(
        "input is not stable-splittable (no recognizable bundle wrapper) — " +
          "no split performed; run without --split or fix detection"
      );
    }
    removeConsumedSourceFile(opts.outputDir, processedSourcePath, inputFile);
    // --split emits the runnable live-binding CommonJS module graph by
    // default; --split-pure keeps the byte-exact review slices. A runnable
    // decline or failure falls back to the review tree LOUDLY — the stable
    // tree and its ledger are never sacrificed to the runnable emitter.
    if (!opts.splitPure) debug.log("split", "emitting runnable CJS graph");
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
          stable.wrapper,
          // Prior aliases: a still-legal import name is kept rather than
          // re-derived, so an unrelated naming draw cannot rewrite import lines
          // across every file that imports the module.
          prior
        );
    writeSplitTree(opts.outputDir, runnable ?? stable.fileContents);
    writeSplitLedger(opts.outputDir, stable.ledger);
    writePriorMatchMapDebug(opts.outputDir, renameResult.priorCarry?.matchMap);
    // The full humanified single file, beside the ledger, is what the NEXT
    // release points --prior-version at (rename reuse + ledger inheritance).
    writeHumanifiedSource(opts.outputDir, renameResult.code);
    // Fingerprint the NAMING boundary before the post-split passes and the
    // bundle carry rewrite `humanified.js` — the file on disk is post-carry, so
    // without this there is no record of what naming actually produced.
    writeStageHashes(opts.outputDir, {
      afterNaming: stageFingerprint(renameResult.code),
      afterPlacement: stageFingerprint(JSON.stringify(stable.ledger))
    });
    committed = true;
    // The tree, ledger, and source are on disk now. Drop the big in-memory ASTs
    // before the Bun re-link — it reads the tree from disk, and holding the
    // multi-GB scope graph live makes its every GC trace the whole thing.
    releaseSplitSourceState(renameResult, stable);
    debug.log("split", "tree committed; released the wrapper parse");
    const relinked = await finishSplitOutput(
      opts,
      inputFile,
      runnable,
      renderer
    );
    // Phase 3.6 (exp054): the phase-3.3 reconcile tiers, scoped to the tree the
    // split decided, run against the FINAL on-disk text — after the re-link and
    // the `using` desugar, which are the last passes to rewrite `src/`.
    reconcilePostSplit(opts, stable.ledger, isEligible, renderer);
    const { stats } = stable;
    writePlacementStats(opts.outputDir, stats);
    renderer.message(
      `Stable split: ${stats.files} file(s) in ${stats.folders} folder(s)` +
        (runnable
          ? ` [runnable CJS module graph${relinked ? " + Bun re-link" : ""}]`
          : "") +
        (prior
          ? ` — inherited ${stats.inherited}/${stats.statements} ` +
            // Rendered FROM the placement registry, so a new tier appears here
            // without anyone remembering to add it.
            `(${placementSummary(stats)})`
          : ` (fresh grouping, ${stats.statements} statements)`)
    );
    renderer.message(
      `Next release: --prior-version ${path.join(opts.outputDir, HUMANIFIED_SOURCE_PATH)}`
    );
    return "complete";
  } catch (err) {
    return stableSplitFailureOutcome(err, committed, renderer);
  }
}

/** Convert a stable-split throw into the caller-facing outcome: a tree
 * already committed to disk is never re-split; a pre-commit failure is
 * FATAL — there is one splitter, and a run that cannot split must say so
 * rather than ship a tree built some other way. (Until 2026-08-12 this
 * fell back to a second, cruder clustering splitter; that path executed
 * zero times outside its own tests and is deleted.) */
function stableSplitFailureOutcome(
  err: unknown,
  committed: boolean,
  renderer: ReturnType<typeof createProgressRenderer>
): StableSplitOutcome {
  const failure = err instanceof Error ? err.message : String(err);
  if (committed) {
    renderer.message(
      `Post-split step failed (${failure}); the split tree is already written`
    );
    return "tree-written-post-failure";
  }
  throw new Error(
    `stable split failed before any tree was written: ${failure}`
  );
}

async function runSplit(
  filename: string,
  opts: CommandOptions,
  renameResult: import("../rename/plugin.js").RenamePluginResult,
  original: { source: string; path: string },
  provider: import("../llm/types.js").LLMProvider,
  profiler: import("../profiling/index.js").Profiler | typeof NULL_PROFILER,
  renderer: ReturnType<typeof createProgressRenderer>,
  /** The rename pipeline's own skip predicate, so the post-split reconcile
   * refuses exactly the names the rest of the pipeline refuses. */
  isEligible: IsEligibleFn,
  /** See tryStableSplit — decided once at detection, threaded down. */
  fossil: boolean
): Promise<void> {
  const splitSpan = profiler.startSpan("split", "pipeline");
  // Nothing on the stable path reads the post-rename AST — the split parses
  // renameResult.code privately — and holding it through the split's own
  // full-bundle parse + wrapper scope crawl keeps two multi-GB graphs live
  // at once (docs/analysis-two-version-memory-flow.md §2, window #3). Drop
  // it now; the rare adapter fallback below re-parses the same code.
  renameResult.ast = undefined;
  const outcome = await tryStableSplit(
    opts,
    filename,
    renameResult,
    original.path,
    provider,
    renderer,
    isEligible,
    fossil
  );
  if (outcome === "complete") {
    splitSpan.end({ stable: true });
    renderer.message(`Split complete: written to ${opts.outputDir}`);
    return;
  }
  // tree-written-post-failure: the tree is already committed to disk —
  // report and keep it. (Every pre-commit failure throws inside
  // tryStableSplit; the deleted adapter fallback used to re-split here.)
  splitSpan.end({ stable: false });
  renderer.message(
    "Split tree already written; a post-split step failed after commit"
  );
}

/**
 * Resolve the LLM provider from CLI flags (+ API-key env fallback only), then wrap it with debug and
 * rate limiting. The rate-limit cap spans both lanes so it never throttles the
 * module lane below its configured size.
 */
function buildProvider(
  settings: Settings
): import("../llm/types.js").LLMProvider {
  // Every value here is already resolved — CLI over env over default, parsed
  // once. This function used to re-derive six of them, including parsing
  // `reasoningEffort` twice within its own body.
  const baseProvider = new OpenAICompatibleProvider({
    endpoint: settings.endpoint,
    apiKey: settings.apiKey,
    model: settings.model,
    timeout: settings.timeout,
    maxTokens: settings.maxTokens,
    reasoningEffort: settings.reasoningEffort
  });
  const limited = withRateLimit(withDebug(baseProvider, settings.model), {
    // OUTER bound over both of the processor's limiters. Deliberately not
    // bundler-aware: this runs before detectBundle, and the constant is the
    // widest lane the default can be, so the bound never binds.
    maxConcurrent:
      settings.concurrency +
      (settings.moduleConcurrency ?? MAX_DEFAULT_MODULE_CONCURRENCY),
    retryAttempts: settings.retryAttempts
  });
  // Cache OUTERMOST: hits bypass the rate limiter and debug wrapper
  // entirely; misses flow through the full stack and get recorded.
  if (!settings.llmCacheDir) return limited;
  return new CachedLLMProvider(limited, settings.llmCacheDir, {
    model: settings.model,
    temperature: 0,
    maxTokens: settings.maxTokens,
    reasoningEffort: settings.reasoningEffort
  });
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
  settings: Settings,
  provider: import("../llm/types.js").LLMProvider,
  renderer: ReturnType<typeof createProgressRenderer>,
  profiler: import("../profiling/index.js").Profiler | typeof NULL_PROFILER
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
  // exp070: the fossil-split decision is made HERE, once, from the selected
  // adapter's declared capability — detection gates the path, the split
  // exercises it. `--disable fossil-split` restores the pre-fossil layout
  // machinery for A/B and rollback.
  const fossilSplit =
    selectUnpackAdapter(config).providesModuleFossils === true &&
    !switchOn("fossil-split");
  if (fossilSplit) {
    verbose.log("Fossil split: module fossils will drive statement assignment");
  }
  if (detection.signals.length > 0) {
    verbose.debug(
      `Detection signals: ${detection.signals.map((s) => `${s.source}:${s.pattern}`).join(", ")}`
    );
  }

  // 2. Load prior version code if --prior-version was specified.
  const priorVersionCode = loadPriorVersionCode(opts, renderer);

  // Per-identifier strategy attempt trails, drained into the diagnostics
  // report. Debug-only: enabled exactly when --diagnostics is set.
  strategyTrail.reset(Boolean(opts.diagnostics));
  // Same switch for the split's placement trail: which tier put each statement
  // in which file, and what evidence the tiers that abstained had.
  placementTrail.reset(Boolean(opts.diagnostics));
  // Contention events (a requested name already held) — exp063's standing
  // error detector: each one is a wrong holder, a duplicate heir, or a
  // corrupted vote somewhere upstream.
  nameContention.reset(Boolean(opts.diagnostics));

  // 3. Build plugins with config available upfront — no callbacks
  const rename = createRenamePlugin({
    provider,
    concurrency: settings.concurrency,
    moduleConcurrency: settings.moduleConcurrency,
    onProgress: (m) => renderer.update(m),
    batchSize: settings.batchSize,
    maxRetriesPerIdentifier: settings.maxRetriesPerIdentifier,
    maxFreeRetries: settings.maxFreeRetries,
    laneThreshold: settings.laneThreshold,
    profiler,
    skipLibraries: settings.skipLibraries,
    minifierType: config.minifierType,
    bundlerType: config.bundlerType,
    priorVersionCode,
    ...settings.levers,
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
    /** The file BEFORE the rename pass — a capture is not readable from the
     *  output alone, since `b !== b` is also a legitimate NaN check. */
    originalCode: string;
    /** The code the CHECK examined. Later passes (reconcile, sweep, family
     *  permutation) replace the output before it reaches disk, so the file is
     *  a different artifact and diffing it answers a question nobody asked. */
    validatedCode?: string;
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
          failure: result.semanticFailure,
          originalCode: code,
          validatedCode: result.validatedCode
        });
      }
      if (result.coverageSummary) {
        renderer.message(result.coverageSummary);
        debug.log("summary", result.coverageSummary);
      }
      return result.code;
    }
  ];

  // Vendor is the surface that went UNSCORED for thirteen experiments while
  // carrying 2.4x the entire measured src/ noise (rule 8). Counting why its
  // names were or were not produced is the cheapest guard against that
  // repeating.
  const vendorNaming: VendorNamingStats = {
    named: 0,
    declined: 0,
    echoed: 0,
    batchesFailed: 0
  };

  // 3. Run pipeline
  await unminify(bundledCode, opts.outputDir, config, plugins, {
    skipLibraries: settings.skipLibraries,
    log: (msg) => renderer.message(msg),
    profiler,
    vendorNamer: createVendorNamer(provider, vendorNaming),
    priorVendorNames: loadPriorVendorNamesIfPresent(opts, renderer),
    priorManifestFactories: loadPriorManifestFactoriesIfPresent(opts),
    onOriginalSource: isSplit
      ? (filePath, code) => {
          original.source = code;
          original.path = filePath;
        }
      : undefined,
    skipFileWrite: isSplit
  });

  reportVendorNaming(vendorNaming, renderer);

  // BEFORE the split: it consumes and deletes the processed source, so this is
  // the only window in which a rejected file still exists to be preserved.
  if (semanticFailures.length > 0) {
    preserveFailedOutput(opts.outputDir, semanticFailures);
    renderer.message(
      `Preserved ${semanticFailures.length} rejected file(s) for inspection under ${FAILED_OUTPUT_DIR}/`
    );
  }

  if (isSplit && lastRenameResult) {
    await runSplit(
      filename,
      opts,
      lastRenameResult,
      original,
      provider,
      profiler,
      renderer,
      createIsEligible(config.bundlerType, config.minifierType),
      fossilSplit
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
      lastRenameResult.thirdPartyClassification,
      strategyTrail.report(),
      placementTrail.report(),
      nameContention.report()
    );
    writeDiagnosticsFile(diagReport, opts.diagnostics);
    renderer.message(`Diagnostics written to ${opts.diagnostics}`);
  }

  if (opts.statsJson && lastRenameResult?.coverageData) {
    writeEvalStats(
      opts.statsJson,
      lastRenameResult,
      vendorNaming,
      pipelineSelectionRecord(config)
    );
    renderer.message(`Eval stats written to ${opts.statsJson}`);
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
/**
 * What the vendor namer actually did. Silent when it never ran, because a
 * bundle with no vendor files should not print a line of zeros — but a run
 * that named nothing DOES say so, since "0 named of 812" is the finding.
 */
function reportVendorNaming(
  stats: VendorNamingStats,
  renderer: ReturnType<typeof createProgressRenderer>
): void {
  const attempted =
    stats.named + stats.declined + stats.echoed + stats.batchesFailed;
  if (attempted === 0) return;
  const parts = [`${stats.named} named`];
  if (stats.declined > 0) parts.push(`${stats.declined} declined`);
  if (stats.echoed > 0) parts.push(`${stats.echoed} echoed the key`);
  if (stats.batchesFailed > 0) {
    parts.push(`${stats.batchesFailed} batch(es) failed`);
  }
  renderer.message(`Vendor naming: ${parts.join(", ")}`);
}

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
    `ERROR: ${semanticFailures.length} output file${semanticFailures.length > 1 ? "s" : ""} violated rename invariants — the rejected file(s) and their pre-rename sources are preserved under ${FAILED_OUTPUT_DIR}/; this run is marked failed.`
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
      "https://api.openai.com/v1"
    )
    .option(
      "--api-key <key>",
      "API key (flag > HUMANIFY_API_KEY > OPENAI_API_KEY env vars)"
    )
    .option("-m, --model <model>", "Model identifier", "gpt-4o-mini")
    .option("-o, --output-dir <output>", "Output directory", "output")
    .option(
      "-v, --verbose",
      "Increase verbosity (-v for info, -vv for debug)",
      (_, prev) => (prev || 0) + 1,
      0
    )
    .option(
      "-c, --concurrency <n>",
      "Max concurrent function-lane LLM requests. Module-lane size is set " +
        "separately via --module-concurrency; the global in-flight cap is their sum.",
      `${DEFAULT_CONCURRENCY}`
    )
    .option(
      "--module-concurrency <n>",
      "Max concurrent module-lane LLM requests (default derived from -c)"
    )
    .option("--max-tokens <n>", "Per-request completion token budget")
    .option(
      "--ambiguity-probe <path>",
      "Write the matcher ambiguity probe JSON to this path (instrumentation)"
    )
    .option(
      "--disable <passes>",
      "Comma-separated pass switches to turn OFF for ablation (registry: " +
        "src/kill-switches.ts; unknown names are fatal and list the valid set)"
    )
    .option(
      "--probe <probes>",
      "Comma-separated instrumentation probes to turn ON (same registry)"
    )
    .option(
      "--retries <n>",
      "Number of retry attempts for failed API calls",
      "3"
    )
    .option(
      "--timeout <ms>",
      "LLM request timeout in milliseconds",
      `${DEFAULT_LLM_TIMEOUT_MS}`
    )
    .option(
      "--llm-cache <dir>",
      "Cache LLM responses on disk keyed by request content. " +
        "Repeated prompts become deterministic " +
        "across sessions and reruns are nearly free — the serving-drift " +
        "countermeasure the 034 eval README describes."
    )
    .option(
      "--reasoning-effort <level>",
      "Reasoning effort for reasoning models: low, medium, or high " +
        "(no env fallback; default: server-side default). " +
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
      "--stats-json <path>",
      "Write the deterministic match/rename breakdown as compact JSON " +
        "(coverage + transfer stats + prior-match counts) for the eval harness"
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
    .action(async (filename: string, opts: CommandOptions, cmd: Command) => {
      // Reject unusable flag combinations before doing any work, so a flag
      // that could not take effect crashes loudly instead of being ignored.
      // Default-on booleans need commander's sources to tell an explicit
      // flag from its default.
      enforceFlagInvariants(opts, {
        namingFloorSweep:
          cmd.getOptionValueSource("namingFloorSweep") === "cli" &&
          opts.namingFloorSweep === true
      });
      setAmbiguityProbePath(opts.ambiguityProbe);
      applySwitchFlags(opts);
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

      // ONE resolution of every setting: CLI over env over default, parsed
      // once, frozen. Everything downstream reads a field.
      let settings: Settings;
      try {
        settings = resolveSettings(opts);
      } catch (e) {
        console.error(`Error: ${e instanceof Error ? e.message : String(e)}`);
        process.exit(1);
      }
      const profiler = opts.profile ? new Profiler(true) : NULL_PROFILER;
      const provider = buildProvider(settings);

      try {
        await runPipeline(
          filename,
          opts,
          settings,
          provider,
          renderer,
          profiler
        );
      } finally {
        await finalizeProfile(opts, filename, profiler, renderer);
        renderer.finish();
        await finalizeLogStream(logStream);
      }
    });
}
