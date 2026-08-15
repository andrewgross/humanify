/**
 * Run the pipeline for one eval pair and record a full manifest of it.
 *
 *   npx tsx experiments/lib/run-pipeline.ts <config.json>
 *
 * The config is a JSON file (not a long argv) because the harness already
 * knows every value and passing twenty flags through bash quoting is how a
 * setting silently goes missing.
 *
 * ## Why a Node runner rather than more bash
 *
 * Three things the shell could not do here:
 *
 *  1. PEAK MEMORY. GNU `time` is not installed in this environment, and
 *     `process.resourceUsage()` reports only the calling process. The child's
 *     high-water mark lives in `/proc/<pid>/status` as `VmHWM` and has to be
 *     sampled while it is alive — after it exits, the file is gone. `EVAL_HEAP`
 *     has been a guess since 14336 MB OOMed cold; this makes it a measurement.
 *
 *  2. CACHE ACCOUNTING. The entry count has to be taken immediately before and
 *     after the run to mean anything. Rule 10 exists because eight legs ran
 *     through a warm cache without one prompt reaching the model.
 *
 *  3. ONE PLACE. Exit code, wall time, errors, config, provenance and
 *     artifacts were previously written by six different steps, and several
 *     were not written at all.
 */
import { execFileSync, spawn } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import {
  type RunManifest,
  manifestWarnings,
  type ProcSample,
  peakRssMbOfTree,
  priorKindOf,
  writeManifest
} from "./run-manifest.js";

/**
 * How often to sample the child's RSS — one small file read, so a 20-minute
 * pipeline costs a few thousand of them and nothing measurable.
 *
 * Deliberately short, and the FIRST sample is taken immediately rather than
 * after one interval: at 2000 ms with no leading sample, a run that finished
 * in under two seconds recorded no memory at all, which is how the first
 * version of this reported "?" for a child that had just allocated 600 MB.
 */
const RSS_SAMPLE_MS = 250;
/** Cap on recorded ERROR lines, so a pathological run cannot write a novel. */
const MAX_ERRORS = 10;

interface RunConfig {
  pair: string;
  version: string;
  label: string;
  resultsDir: string;
  input: string;
  prior: string;
  outputDir: string;
  repo: string;
  /** Full argv for the pipeline, after `tsx src/index.ts`. */
  args: string[];
  stdoutPath: string;
  endpoint: string;
  model: string;
  reasoningEffort: string;
  concurrency: number;
  heapMb: number;
  cacheDir?: string;
  /** Artifacts the run promised, checked and sized afterwards. */
  artifacts: string[];
}

function countFiles(dir: string | undefined): number {
  if (!dir || !fs.existsSync(dir)) return 0;
  let n = 0;
  const walk = (d: string): void => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      if (e.isDirectory()) walk(path.join(d, e.name));
      else n++;
    }
  };
  walk(dir);
  return n;
}

/** A command's stdout, or "" if it fails — provenance must never abort a run,
 *  and its stderr must not pollute the harness log. */
function sh(cmd: string, args: string[], cwd: string): string {
  try {
    return String(
      execFileSync(cmd, args, { cwd, encoding: "utf8", stdio: "pipe" })
    ).trim();
  } catch {
    return "";
  }
}

/**
 * Every live process, with its parent and peak RSS. Read fresh each sample —
 * the pipeline's real worker is a GRANDCHILD (`npx` spawns it), so the tree has
 * to be re-derived rather than resolved once at spawn time.
 */
function readProcTree(): ProcSample[] {
  const out: ProcSample[] = [];
  let entries: string[];
  try {
    entries = fs.readdirSync("/proc");
  } catch {
    return out; // not Linux, or /proc unavailable: peak RSS stays undefined
  }
  for (const e of entries) {
    const pid = Number(e);
    if (!Number.isInteger(pid) || pid <= 0) continue;
    try {
      const status = fs.readFileSync(`/proc/${pid}/status`, "utf8");
      const ppid = /^PPid:\s+(\d+)$/m.exec(status);
      if (!ppid) continue;
      const hwm = /^VmHWM:\s+(\d+)\s*kB$/m.exec(status);
      out.push({
        pid,
        ppid: Number(ppid[1]),
        vmHwmKb: hwm ? Number(hwm[1]) : undefined
      });
    } catch {
      /* the process exited between readdir and read — normal, skip it */
    }
  }
  return out;
}

/**
 * Kill switches set in the environment we are about to hand to the child.
 *
 * Delegates to the registry's owner rather than matching on the `HUMANIFY_`
 * prefix, which over-reported: `HUMANIFY_MAX_TOKENS` is a token budget,
 * `HUMANIFY_AMBIGUITY_PROBE` is deliberately excluded from the registry, and
 * `HUMANIFY_STRIP_USING` is read by the emitted tree — none is a switch the
 * pipeline honours, and all three match the prefix. Under rule 10 a provenance
 * field naming switches the run did not honour is exactly the kind of lie the
 * manifest exists to prevent.
 */
function switchesFromArgv(args: string[]): string[] {
  // Since 2026-08-12 switches are CLI flags, and the pipeline runs as a
  // CHILD process — the only true record of its switches is the argv this
  // runner hands it. (In-process activeKillSwitches() would always read
  // empty here; recording that would be the manifest lie rule 10 bans.)
  const out: string[] = [];
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--disable" || args[i] === "--probe") {
      out.push(...(args[i + 1]?.split(",") ?? []));
    }
  }
  return out.sort();
}

/**
 * Bun's version. It is NOT on PATH in this devcontainer — it lives in
 * `~/.bun/bin` — and a bare `bun --version` therefore returns "" and silently
 * records "no bun". The boot gate was already bitten by exactly this
 * (`run.sh` printed "BOOT GATE SKIPPED" and scored on), so look where it
 * actually is before believing it is absent.
 */
function bunVersion(cwd: string): string {
  const direct = sh("bun", ["--version"], cwd);
  if (direct) return direct;
  const home = process.env.HOME ?? "";
  return home ? sh(path.join(home, ".bun/bin/bun"), ["--version"], cwd) : "";
}

async function main(): Promise<void> {
  const cfgPath = process.argv[2];
  if (!cfgPath) {
    console.error("usage: run-pipeline.ts <config.json>");
    process.exit(2);
  }
  const cfg: RunConfig = JSON.parse(fs.readFileSync(cfgPath, "utf8"));

  const startedAt = new Date().toISOString();
  const t0 = Date.now();
  const entriesBefore = countFiles(cfg.cacheDir);

  const out = fs.openSync(cfg.stdoutPath, "w");
  const child = spawn(
    "npx",
    ["tsx", path.join(cfg.repo, "src/index.ts"), ...cfg.args],
    {
      cwd: cfg.repo,
      stdio: ["ignore", out, out],
      env: {
        ...process.env,
        NODE_OPTIONS: `--max-old-space-size=${cfg.heapMb}`
      }
    }
  );

  // Sample the child's high-water mark while it is alive; after it exits,
  // /proc/<pid>/status no longer exists and the number is unrecoverable.
  let peakRssMb: number | undefined;
  const sampleRss = (): void => {
    const mb = peakRssMbOfTree(readProcTree(), child.pid ?? -1);
    if (mb !== undefined && (peakRssMb === undefined || mb > peakRssMb)) {
      peakRssMb = mb;
    }
  };
  sampleRss(); // immediately, so a short run is not recorded as using nothing
  const sampler = setInterval(sampleRss, RSS_SAMPLE_MS);

  const exitCode = await new Promise<number>((resolve) => {
    child.on("close", (code) => resolve(code ?? -1));
    child.on("error", () => resolve(-1));
  });
  clearInterval(sampler);
  fs.closeSync(out);

  const entriesAfter = countFiles(cfg.cacheDir);

  // Where the split put things. Written by the pipeline beside the ledger;
  // read here so one manifest answers "what happened" including placement,
  // rather than leaving it to be grepped out of a multi-GB log.
  let placement: RunManifest["placement"];
  const placementPath = path.join(
    cfg.outputDir,
    ".humanify/placement-stats.json"
  );
  try {
    if (fs.existsSync(placementPath)) {
      placement = JSON.parse(fs.readFileSync(placementPath, "utf8"));
    }
  } catch {
    /* a malformed sidecar must not fail the run it describes */
  }

  const errors = fs.existsSync(cfg.stdoutPath)
    ? fs
        .readFileSync(cfg.stdoutPath, "utf8")
        .split("\n")
        .filter((l) => l.startsWith("ERROR:"))
        .slice(0, MAX_ERRORS)
    : [];

  const manifest: RunManifest = {
    pair: cfg.pair,
    label: cfg.label,
    startedAt,
    wallSeconds: Math.round((Date.now() - t0) / 1000),
    provenance: {
      commit: sh("git", ["rev-parse", "--short", "HEAD"], cfg.repo),
      dirty:
        sh("git", ["status", "--porcelain", "--untracked-files=no"], cfg.repo)
          .length > 0,
      node: process.version,
      bun: bunVersion(cfg.repo)
    },
    inputs: {
      input: cfg.input,
      prior: cfg.prior,
      priorKind: priorKindOf(cfg.prior)
    },
    config: {
      endpoint: cfg.endpoint,
      model: cfg.model,
      reasoningEffort: cfg.reasoningEffort,
      concurrency: cfg.concurrency,
      heapMb: cfg.heapMb,
      killSwitches: switchesFromArgv(cfg.args),
      // Counted ONCE: two separate walks could disagree and produce a
      // `written` that matches neither endpoint.
      cache: {
        enabled: Boolean(cfg.cacheDir),
        entriesBefore,
        entriesAfter,
        written: entriesAfter - entriesBefore
      }
    },
    placement,
    outcome: {
      exitCode,
      errors,
      peakRssMb,
      artifacts: cfg.artifacts.map((p) => ({
        path: p,
        bytes: fs.existsSync(p) ? fs.statSync(p).size : 0
      }))
    }
  };

  writeManifest(cfg.resultsDir, cfg.version, manifest);

  const rss = peakRssMb === undefined ? "?" : `${peakRssMb} MB`;
  console.log(
    `  run: exit ${exitCode}, ${manifest.wallSeconds}s, peak RSS ${rss}` +
      ` of ${cfg.heapMb} MB heap, cache +${manifest.config.cache.written}`
  );
  for (const line of manifestWarnings(manifest)) console.log(`  !! ${line}`);

  // Exit code is passed through: the harness still decides what to do about it.
  process.exit(exitCode === 0 ? 0 : exitCode);
}

main().catch((e) => {
  console.error(`run-pipeline: ${e}`);
  process.exit(2);
});
