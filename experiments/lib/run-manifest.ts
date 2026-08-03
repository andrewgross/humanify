/**
 * One file that says what actually happened in one eval run.
 *
 * ## Why
 *
 * Reconstructing a run used to mean opening `commit.txt`, `<v>.stats.json`,
 * `<v>-boot.json`, `<v>.stdout`, `<v>-run-status.json` and a `.log` that is not
 * even committed — and several facts were recorded in NONE of them. The missing
 * ones are not incidental: they are precisely the facts whose absence has
 * produced wrong published numbers here.
 *
 * | fact                       | what its absence cost                          |
 * | -------------------------- | ---------------------------------------------- |
 * | which prior (archive/rebased) | ~3.7x worse KPIs for no reason; a re-score  |
 * | cache on + entries written | rule 10 — exp047 replayed 8 legs, 0 prompts    |
 * | kill switches active       | a run silently not-the-default, unmarked        |
 * | peak RSS                   | 14336 MB OOMed cold; 65536 is still a guess     |
 * | exit code                  | KPIs published from trees the pipeline rejected |
 *
 * ## The design rule
 *
 * A manifest that only stores facts is a file nobody opens. `manifestWarnings`
 * turns the dangerous COMBINATIONS into sentences, and the harness prints them.
 * A fact nobody reads is worth what a fact nobody recorded is worth.
 */
import * as fs from "node:fs";
import * as path from "node:path";

/** Which base the run was scored against. */
export type PriorKind = "rebased" | "archive" | "unknown";

export interface RunManifest {
  /** "2.1.85->2.1.86" */
  pair: string;
  /** The eval label this run belongs to. */
  label: string;
  startedAt: string;
  wallSeconds: number;
  provenance: {
    commit: string;
    /** True when the working tree had uncommitted changes: the run then
     *  corresponds to no commit and cannot be reproduced from the sha. */
    dirty: boolean;
    node: string;
    bun: string;
  };
  inputs: {
    input: string;
    prior: string;
    priorKind: PriorKind;
  };
  config: {
    endpoint: string;
    model: string;
    reasoningEffort: string;
    concurrency: number;
    heapMb: number;
    waveScheduling: boolean;
    /** Kill switches set in the environment for THIS run. */
    killSwitches: string[];
    cache: {
      enabled: boolean;
      entriesBefore: number;
      entriesAfter: number;
      /** after - before. Zero with the cache enabled means every prompt was
       *  replayed and the run cannot settle anything about naming. */
      written: number;
    };
  };
  outcome: {
    exitCode: number;
    errors: string[];
    /** Peak resident set in MB, or undefined when no sample landed. */
    peakRssMb?: number;
    artifacts: Array<{ path: string; bytes: number }>;
  };
  /**
   * WHERE the split put things, and on what evidence — per placement tier.
   * Absent for a run that produced no split, and for every run predating
   * `.humanify/placement-stats.json`. Absent is not zero.
   */
  placement?: {
    statements: number;
    files: number;
    folders: number;
    inherited: number;
    residueLocality: number;
    byTier: Record<string, number>;
  };
}

const MANIFEST_SUFFIX = "-run.json";
/** Warn above this fraction of the heap: an OOM is close, not hypothetical. */
const HEAP_HEADROOM_WARN = 0.85;

/**
 * Whether a prior path is a REBASED tree or the committed archive.
 *
 * `run.sh` defaults to the archive prior, and scoring against it reads roughly
 * 3.7x worse for no reason at all. Nothing recorded which one a run used, so
 * the mistake was invisible until someone re-scored by hand.
 */
export function priorKindOf(priorPath: string): PriorKind {
  if (!priorPath) return "unknown";
  if (/-rebased(\/|$)/.test(priorPath)) return "rebased";
  return "archive";
}

/**
 * Peak resident set in MB from the contents of `/proc/<pid>/status`.
 *
 * GNU `time` is not installed in this environment and
 * `process.resourceUsage()` only reports the CURRENT process, so a parent that
 * spawns the pipeline has to sample the child's `VmHWM` (high-water mark).
 *
 * Returns undefined, never 0, when the field is absent: 0 would assert that the
 * run used no memory, which is a claim. Undefined says the sample never landed.
 */
export function peakRssMbFromStatus(status: string): number | undefined {
  const m = /^VmHWM:\s+(\d+)\s*kB$/m.exec(status);
  if (!m) return undefined;
  return Math.round(Number(m[1]) / 1024);
}

/** One sampled process: its parent, and its peak resident set in kB. */
export interface ProcSample {
  pid: number;
  ppid: number;
  vmHwmKb?: number;
}

/**
 * The largest peak RSS among a process and all its descendants, in MB.
 *
 * MAX, not SUM, and the distinction is the whole point: `--max-old-space-size`
 * is a PER-PROCESS limit, so what decides whether the run OOMs is the biggest
 * single process, not the total across a tree.
 *
 * Walking the tree at all is required because the harness launches the pipeline
 * as `npx tsx src/index.ts`, and `npx` immediately spawns the process that does
 * the work. Sampling only the direct child measured the npx wrapper — it
 * reported 97 MB for a tree that had just allocated 600.
 */
export function peakRssMbOfTree(
  samples: readonly ProcSample[],
  rootPid: number
): number | undefined {
  const byParent = groupByParent(samples);
  const byPid = new Map(samples.map((s) => [s.pid, s]));
  let best: number | undefined;
  for (const pid of descendants(byParent, rootPid)) {
    const kb = byPid.get(pid)?.vmHwmKb;
    if (kb === undefined) continue;
    const mb = Math.round(kb / 1024);
    if (best === undefined || mb > best) best = mb;
  }
  return best;
}

function groupByParent(
  samples: readonly ProcSample[]
): Map<number, ProcSample[]> {
  const byParent = new Map<number, ProcSample[]>();
  for (const s of samples) {
    const list = byParent.get(s.ppid);
    if (list) list.push(s);
    else byParent.set(s.ppid, [s]);
  }
  return byParent;
}

/** `root` and everything under it. `seen` is not an optimisation: /proc is
 *  sampled live and races with process exit, so a pid appearing as its own
 *  ancestor would otherwise spin forever. */
function descendants(
  byParent: Map<number, ProcSample[]>,
  root: number
): number[] {
  const seen = new Set<number>();
  const stack = [root];
  while (stack.length > 0) {
    const pid = stack.pop() as number;
    if (seen.has(pid)) continue;
    seen.add(pid);
    for (const child of byParent.get(pid) ?? []) stack.push(child.pid);
  }
  return [...seen];
}

export function writeManifest(
  resultsDir: string,
  version: string,
  manifest: RunManifest
): void {
  fs.writeFileSync(
    path.join(resultsDir, `${version}${MANIFEST_SUFFIX}`),
    JSON.stringify(manifest, null, 2)
  );
}

/** The manifest for a pair, or null when none was recorded. Never synthesised:
 *  a run that predates manifests has no manifest, which is not the same as a
 *  run that had nothing worth recording. */
export function loadManifest(
  resultsDir: string,
  version: string
): RunManifest | null {
  const p = path.join(resultsDir, `${version}${MANIFEST_SUFFIX}`);
  if (!fs.existsSync(p)) return null;
  try {
    return JSON.parse(fs.readFileSync(p, "utf8")) as RunManifest;
  } catch {
    return null;
  }
}

/** Every manifest in a results directory, sorted by version. */
export function loadManifests(resultsDir: string): RunManifest[] {
  if (!fs.existsSync(resultsDir)) return [];
  const out: RunManifest[] = [];
  for (const f of fs.readdirSync(resultsDir)) {
    if (!f.endsWith(MANIFEST_SUFFIX)) continue;
    const m = loadManifest(resultsDir, f.slice(0, -MANIFEST_SUFFIX.length));
    if (m) out.push(m);
  }
  return out.sort((a, b) =>
    a.pair.localeCompare(b.pair, undefined, { numeric: true })
  );
}

/**
 * The sentences a reader must see. Empty for a clean, cold, default run —
 * a warning that always fires is a warning nobody reads, which is how the
 * `ERROR:` lines already in every stdout went unnoticed for thirteen result
 * sets.
 */
export function manifestWarnings(m: RunManifest): string[] {
  return WARNING_CHECKS.filter((c) => c.fires(m)).map(
    (c) => `${m.pair}: ${c.say(m)}`
  );
}

/**
 * One entry per way a run can be quietly untrustworthy. A table rather than a
 * chain of `if`s so that adding a warning is one entry and the whole list can
 * be read at once — the same reason `PLACEMENT_TIERS` is a registry.
 *
 * Every entry cites the incident that earned it. A warning without one is a
 * warning nobody will dare delete when it stops being true.
 */
interface WarningCheck {
  name: string;
  fires: (m: RunManifest) => boolean;
  say: (m: RunManifest) => string;
}

const WARNING_CHECKS: readonly WarningCheck[] = [
  {
    name: "cache-replayed",
    fires: (m) => m.config.cache.enabled && m.config.cache.written === 0,
    say: () =>
      "LLM CACHE ON and 0 entries written — every prompt was REPLAYED, not " +
      "asked. This run cannot settle anything about naming " +
      "(measurement-pitfalls rule 10, born from exactly this)."
  },
  {
    name: "archive-prior",
    fires: (m) => m.inputs.priorKind === "archive",
    say: () =>
      "scored against the ARCHIVE prior, not a rebased one. Expect KPIs to " +
      "read far worse than a like-for-like base would give."
  },
  {
    name: "kill-switch-active",
    fires: (m) => m.config.killSwitches.length > 0,
    say: (m) =>
      `${m.config.killSwitches.length} kill switch(es) ACTIVE — this run is ` +
      `not the default pipeline: ${m.config.killSwitches.join(", ")}`
  },
  {
    name: "dirty-tree",
    fires: (m) => m.provenance.dirty,
    say: (m) =>
      `the working tree was DIRTY at ${m.provenance.commit} — this run ` +
      `corresponds to no commit and cannot be reproduced from the sha.`
  },
  {
    name: "heap-headroom",
    fires: (m) =>
      m.outcome.peakRssMb !== undefined &&
      m.config.heapMb > 0 &&
      m.outcome.peakRssMb >= m.config.heapMb * HEAP_HEADROOM_WARN,
    say: (m) =>
      `peak RSS ${m.outcome.peakRssMb} MB of a ${m.config.heapMb} MB heap ` +
      `(${Math.round((100 * (m.outcome.peakRssMb ?? 0)) / m.config.heapMb)}%) ` +
      `— the next bigger input OOMs. Raise EVAL_HEAP.`
  },
  {
    name: "nonzero-exit",
    fires: (m) => m.outcome.exitCode !== 0,
    say: (m) =>
      `pipeline EXIT ${m.outcome.exitCode} — it declared its own output ` +
      `invalid. Any KPI below is computed from a rejected tree.`
  }
];

/**
 * Is this parsed JSON a per-pair SCORECARD, as opposed to one of the other
 * files that sit beside it (run manifest, run status, boot verdict, stats)?
 *
 * Tests `churn`, the field the summarizer actually reads — not `pair`, which
 * the scorecard and the run manifest both carry. Discriminating on `pair` is
 * what made `summarize.ts` read a manifest as a scorecard and crash the whole
 * eval after the 421-second pipeline run had already succeeded.
 *
 * Duck-typing on the CONSUMED field means the next artifact written into a
 * results directory cannot break the summary just by having a common key.
 */
export function isScorecardShape(j: unknown): boolean {
  if (typeof j !== "object" || j === null) return false;
  const card = j as { pair?: unknown; churn?: unknown };
  return (
    typeof card.pair === "string" &&
    typeof card.churn === "object" &&
    card.churn !== null
  );
}
