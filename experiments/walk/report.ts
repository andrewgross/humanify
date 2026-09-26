/**
 * Per-hop report over one or more version walks (walk.sh), TS vs Rust.
 *
 *   npx tsx experiments/walk/report.ts --walk rust=<dir> --walk ts=<dir> \
 *       --out <dir> [--examples rust] [--example-hops 4] [--jobs 4]
 *   npx tsx experiments/walk/report.ts card <walk-dir> <from> <to>
 *
 * NO NEW METRIC. Every number is produced by an existing owner:
 *   - the eval scorecard, `034-eval-harness/analyze.ts`, run on
 *     (hop N-1 tree, hop N tree) exactly as the eval runs it on a pair —
 *     noise / noiseLn / novel / realLn / relocSt / reloc / mints and the
 *     on-disk `layout` (real / naming / alias / reorder / nameOnlyLines) and
 *     `vendor` decompositions;
 *   - changed lines per top-level dir from `lib/diff.ts` (the one counter:
 *     `diff -rN`, a modified line counts twice = `git diff --numstat
 *     --no-renames` on the walk's history repo).
 * Cards are cached in <walk>/cards/, so re-running the report is cheap and
 * walk.sh can score each hop in the background while the next one runs.
 *
 * What a walk hop is NOT: an eval pair. The eval rebases v-1 with the scored
 * pipeline; a walk hop's prior is the SAME walk's previous output, so the two
 * pipelines' hops start from different priors — exactly what each would face
 * in production, and exactly why a per-hop TS-vs-Rust delta also contains
 * divergence inherited from earlier hops. Read the totals.
 */
import { spawn, spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { caveatLines, KPIS, type Scorecard } from "../034-eval-harness/kpis.js";
import { changedLines, changedLinesInTree, readOrEmpty } from "../lib/diff.js";
import {
  type HunkKind,
  parseHunks,
  pickExamples,
  tallyKinds
} from "./hunks.js";

const HERE = import.meta.dirname;
const REPO = path.resolve(HERE, "../..");
const HEAP_MB = 65536;
/** Never part of the reviewable tree (history.sh excludes the same set). */
const NOT_SOURCE = new Set([".humanify", "node_modules", "package-lock.json"]);

interface HopRecord {
  version: string;
  priorVersion: string | null;
  cold: boolean;
  exitCode: number;
  wallSeconds: number | null;
  peakRssMb: number | null;
  cacheWritten: number | null;
  boot: { ok: boolean; versionOk: boolean; promptOk: boolean };
}

interface WalkManifest {
  pipeline: string;
  versions: string[];
  pipelineRecord: Record<string, unknown>;
  llm: { endpoint: string; model: string };
}

interface TreeLines {
  total: number;
  byTop: Record<string, number>;
}

interface HopRow {
  from: string | null;
  to: string;
  hop: HopRecord;
  card: Scorecard | null;
  cardError?: string;
  lines: TreeLines | null;
}

const readJson = <T>(p: string): T => JSON.parse(fs.readFileSync(p, "utf8"));

function hopRecords(walk: string): HopRecord[] {
  const m = readJson<WalkManifest>(path.join(walk, "manifest.json"));
  return m.versions
    .map((v) => path.join(walk, "runs", `${v}.hop.json`))
    .filter((p) => fs.existsSync(p))
    .map((p) => readJson<HopRecord>(p));
}

const cardPath = (walk: string, from: string, to: string) =>
  path.join(walk, "cards", `${from}__${to}.json`);

function analyzeArgs(walk: string, from: string, to: string): string[] {
  const t = (v: string, ...rest: string[]) =>
    path.join(walk, "trees", v, ...rest);
  return [
    path.join(REPO, "experiments/034-eval-harness/analyze.ts"),
    t(to, ".humanify/humanified.js"),
    t(from, ".humanify/humanified.js"),
    t(to, ".humanify/split-ledger.json"),
    t(from, ".humanify/split-ledger.json"),
    path.join(walk, "runs", `${to}.stats.json`),
    `${from}->${to}`,
    t(to, "src"),
    t(from, "src"),
    t(to, "vendor"),
    t(from, "vendor")
  ];
}

/** Score one hop with the eval's own analyzer; cached. Resolves to an error string or null. */
function scoreHop(
  walk: string,
  from: string,
  to: string
): Promise<string | null> {
  const dest = cardPath(walk, from, to);
  if (fs.existsSync(dest)) return Promise.resolve(null);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  const tmp = `${dest}.partial`;
  const out = fs.openSync(tmp, "w");
  const err = fs.openSync(`${dest}.stderr`, "w");
  return new Promise((resolve) => {
    const child = spawn("npx", ["tsx", ...analyzeArgs(walk, from, to)], {
      cwd: REPO,
      stdio: ["ignore", out, err],
      env: { ...process.env, NODE_OPTIONS: `--max-old-space-size=${HEAP_MB}` }
    });
    child.on("close", (code) => {
      fs.closeSync(out);
      fs.closeSync(err);
      if (code === 0) {
        fs.renameSync(tmp, dest);
        resolve(null);
      } else resolve(`analyze.ts exited ${code} (see ${dest}.stderr)`);
    });
  });
}

/** Changed lines per top-level entry of the two trees (lib/diff.ts), cached. */
function treeLines(walk: string, from: string, to: string): TreeLines {
  const dest = cardPath(walk, from, to).replace(/\.json$/, ".lines.json");
  if (fs.existsSync(dest)) return readJson<TreeLines>(dest);
  const a = path.join(walk, "trees", from);
  const b = path.join(walk, "trees", to);
  const tops = new Set(
    [...fs.readdirSync(a), ...fs.readdirSync(b)].filter(
      (n) => !NOT_SOURCE.has(n)
    )
  );
  const byTop: Record<string, number> = {};
  for (const top of [...tops].sort()) {
    const pa = path.join(a, top);
    const pb = path.join(b, top);
    const isDir =
      (fs.existsSync(pa) && fs.statSync(pa).isDirectory()) ||
      (fs.existsSync(pb) && fs.statSync(pb).isDirectory());
    // A dir present on one side only: diff -rN against an empty dir.
    const empty = path.join(walk, "cards", ".empty");
    fs.mkdirSync(empty, { recursive: true });
    byTop[top] = isDir
      ? changedLinesInTree(
          fs.existsSync(pa) ? pa : empty,
          fs.existsSync(pb) ? pb : empty
        ).total
      : changedLines(readOrEmpty(pa), readOrEmpty(pb));
  }
  const total = Object.values(byTop).reduce((s, n) => s + n, 0);
  const res = { total, byTop };
  fs.writeFileSync(dest, JSON.stringify(res));
  return res;
}

async function pool<T>(items: T[], jobs: number, fn: (t: T) => Promise<void>) {
  const queue = [...items];
  const workers = Array.from({ length: Math.max(1, jobs) }, async () => {
    for (let it = queue.shift(); it !== undefined; it = queue.shift()) {
      await fn(it);
    }
  });
  await Promise.all(workers);
}

async function walkRows(walk: string, jobs: number): Promise<HopRow[]> {
  const hops = hopRecords(walk);
  const rows: HopRow[] = hops.map((hop, i) => ({
    from: i > 0 ? hops[i - 1].version : null,
    to: hop.version,
    hop,
    card: null,
    lines: null
  }));
  const scored = rows.filter((r) => r.from !== null);
  await pool(scored, jobs, async (r) => {
    const from = r.from as string;
    const err = await scoreHop(walk, from, r.to);
    if (err) r.cardError = err;
    else r.card = readJson<Scorecard>(cardPath(walk, from, r.to));
    r.lines = treeLines(walk, from, r.to);
  });
  return rows;
}

// ── presentation ────────────────────────────────────────────────────────

/** The headline noise: src on-disk noise (naming+alias+reorder) + vendor noise. */
function noiseLines(c: Scorecard | null): number | undefined {
  const l = c?.churn.layout;
  if (!l) return undefined;
  return l.noise + (c?.churn.vendor?.noise ?? 0);
}

const fmt = (n: number | null | undefined): string =>
  n === null || n === undefined ? "-" : n.toLocaleString("en-US");

const sum = (xs: Array<number | null | undefined>) =>
  xs.every((x) => x === null || x === undefined)
    ? undefined
    : xs.reduce<number>((s, x) => s + (x ?? 0), 0);

interface Column {
  head: string;
  get: (r: HopRow) => number | null | undefined;
  /** Summed in the totals row. */
  total?: boolean;
}

const DETAIL: Column[] = [
  { head: "exit", get: (r) => r.hop.exitCode },
  { head: "wall s", get: (r) => r.hop.wallSeconds, total: true },
  { head: "RSS MB", get: (r) => r.hop.peakRssMb },
  { head: "cache+", get: (r) => r.hop.cacheWritten, total: true },
  { head: "diffLn", get: (r) => r.lines?.total, total: true },
  { head: "src", get: (r) => r.lines?.byTop.src, total: true },
  { head: "vendor", get: (r) => r.lines?.byTop.vendor, total: true },
  {
    head: "noiseLines",
    get: (r) => noiseLines(r.card),
    total: true
  },
  { head: "naming", get: (r) => r.card?.churn.layout?.naming, total: true },
  { head: "alias", get: (r) => r.card?.churn.layout?.alias, total: true },
  { head: "reorderLn", get: (r) => r.card?.churn.layout?.reorder, total: true },
  { head: "vendorLn", get: (r) => r.card?.churn.vendor?.noise, total: true },
  {
    head: "nameOnly",
    get: (r) => r.card?.churn.layout?.nameOnlyLines,
    total: true
  },
  { head: "treeLn", get: (r) => r.card?.churn.layout?.churnLines, total: true },
  {
    head: "realExBuild",
    get: (r) => r.card?.churn.layout?.realExBuild,
    total: true
  },
  { head: "vendorReal", get: (r) => r.card?.churn.vendor?.real, total: true },
  ...["noise", "noiseLn", "novel", "realLn", "relocSt", "reloc", "mints"].map(
    (key): Column => {
      const kpi = KPIS.find((k) => k.key === key);
      if (!kpi) throw new Error(`unknown KPI ${key}`);
      return {
        head: key,
        get: (r) => (r.card ? kpi.fromCard(r.card) : undefined),
        total: true
      };
    }
  )
];

function bootCell(h: HopRecord): string {
  if (h.boot.ok) return "OK";
  return `FAIL(${h.boot.versionOk ? "" : "ver"}${h.boot.promptOk ? "" : " -p"})`;
}

function detailTable(name: string, rows: HopRow[]): string[] {
  const head = ["hop", ...DETAIL.map((c) => c.head), "boot"];
  const out = [
    `### ${name}`,
    "",
    `| ${head.join(" | ")} |`,
    `|${head.map(() => "---").join("|")}|`
  ];
  for (const r of rows) {
    const hop = r.from ? `${r.from}→${r.to}` : `${r.to} (cold)`;
    const cells = DETAIL.map((c) =>
      r.from || c.head.match(/exit|wall|RSS|cache/) ? fmt(c.get(r)) : ""
    );
    const note = r.cardError ? ` ⚠ ${r.cardError}` : "";
    out.push(`| ${hop} | ${cells.join(" | ")} | ${bootCell(r.hop)}${note} |`);
  }
  const hopsOnly = rows.filter((r) => r.from);
  const totals = DETAIL.map((c) =>
    c.total
      ? fmt(sum((c.head.match(/wall|cache/) ? rows : hopsOnly).map(c.get)))
      : ""
  );
  out.push(
    `| **total** | ${totals.join(" | ")} | ${rows.filter((r) => r.hop.boot.ok).length}/${rows.length} OK |`
  );
  return out;
}

const HEADLINE: Array<{
  head: string;
  get: (r: HopRow) => number | null | undefined;
}> = [
  { head: "diffLn", get: (r) => r.lines?.total },
  { head: "noiseLines", get: (r) => noiseLines(r.card) },
  { head: "nameOnly", get: (r) => r.card?.churn.layout?.nameOnlyLines },
  { head: "realExBuild", get: (r) => r.card?.churn.layout?.realExBuild },
  { head: "novel", get: (r) => r.card?.churn.statements.novel },
  { head: "wall s", get: (r) => r.hop.wallSeconds }
];

function comparisonTable(walks: Map<string, HopRow[]>): string[] {
  const names = [...walks.keys()];
  const byHop = new Map<string, Map<string, HopRow>>();
  for (const [name, rows] of walks) {
    for (const r of rows) {
      if (!r.from) continue;
      const key = `${r.from}→${r.to}`;
      if (!byHop.has(key)) byHop.set(key, new Map());
      byHop.get(key)?.set(name, r);
    }
  }
  const head = ["hop"];
  for (const h of HEADLINE) for (const n of names) head.push(`${h.head} ${n}`);
  const out = [
    `| ${head.join(" | ")} |`,
    `|${head.map(() => "---").join("|")}|`
  ];
  const common = [...byHop.entries()].filter(
    ([, m]) => m.size === names.length
  );
  for (const [key, m] of common) {
    const cells = HEADLINE.flatMap((h) =>
      names.map((n) => fmt(h.get(m.get(n) as HopRow)))
    );
    out.push(`| ${key} | ${cells.join(" | ")} |`);
  }
  const totals = HEADLINE.flatMap((h) =>
    names.map((n) => fmt(sum(common.map(([, m]) => h.get(m.get(n) as HopRow)))))
  );
  out.push(
    `| **total (${common.length} common hops)** | ${totals.join(" | ")} |`
  );
  return out;
}

// ── examples ────────────────────────────────────────────────────────────

function ensureHistory(walk: string): string {
  const repo = path.join(walk, "history.git");
  const r = spawnSync(path.join(HERE, "history.sh"), [walk], {
    encoding: "utf8"
  });
  if (r.status !== 0) throw new Error(`history.sh failed: ${r.stderr}`);
  return repo;
}

function gitDiff(
  repo: string,
  from: string,
  to: string,
  scope: string
): string {
  const r = spawnSync(
    "git",
    [
      "--git-dir",
      repo,
      "diff",
      "-U2",
      "--no-renames",
      `v${from}`,
      `v${to}`,
      "--",
      scope
    ],
    { encoding: "utf8", maxBuffer: 1 << 30 }
  );
  if (r.status !== 0) throw new Error(`git diff failed: ${r.stderr}`);
  return r.stdout;
}

/** Hops worth reading: the biggest real change, the calmest, the median, the noisiest. */
function exampleHops(rows: HopRow[], n: number): HopRow[] {
  const scored = rows.filter((r) => r.from && r.card?.churn.layout);
  const byReal = [...scored].sort(
    (a, b) =>
      (b.card?.churn.layout?.realExBuild ?? 0) -
      (a.card?.churn.layout?.realExBuild ?? 0)
  );
  const byNoise = [...scored].sort(
    (a, b) => (noiseLines(b.card) ?? 0) - (noiseLines(a.card) ?? 0)
  );
  const picks = [
    byReal[0],
    byNoise[0],
    byReal[Math.floor(byReal.length / 2)],
    byReal[byReal.length - 1],
    byReal[1]
  ];
  const seen = new Set<string>();
  const out: HopRow[] = [];
  for (const p of picks) {
    if (p && !seen.has(p.to) && out.length < n) {
      seen.add(p.to);
      out.push(p);
    }
  }
  return out.sort((a, b) => scored.indexOf(a) - scored.indexOf(b));
}

const KIND_LABEL: Record<HunkKind, string> = {
  real: "real change (edited code)",
  added: "real change (new code)",
  removed: "real change (deleted code)",
  "name-only": "NOISE: same code, different local names",
  moved: "NOISE: identical lines moved"
};

function examplesSection(
  name: string,
  walk: string,
  rows: HopRow[],
  nHops: number
): string[] {
  const repo = ensureHistory(walk);
  const out = [
    `## Example diffs — ${name} walk`,
    "",
    `From \`${repo}\` (one commit + tag per version; \`.humanify/\` excluded).`,
    "Reproduce any hunk with `git --git-dir <repo> diff vA vB -- <file>`.",
    "Hunk labels are for READING only (identifier-masked comparison, 034's",
    "`maskIdentifiers`); every number above comes from the eval scorer.",
    ""
  ];
  for (const r of exampleHops(rows, nHops)) {
    const from = r.from as string;
    const src = parseHunks(gitDiff(repo, from, r.to, "src"));
    const vendor = parseHunks(gitDiff(repo, from, r.to, "vendor"));
    const t = tallyKinds(src);
    const l = r.card?.churn.layout;
    out.push(
      `### ${from} → ${r.to}`,
      "",
      `Scorecard: src realExBuild ${fmt(l?.realExBuild)}, noise ${fmt(l?.noise)} ` +
        `(naming ${fmt(l?.naming)}, alias ${fmt(l?.alias)}, reorder ${fmt(l?.reorder)}), ` +
        `nameOnly ${fmt(l?.nameOnlyLines)}, vendor noise ${fmt(r.card?.churn.vendor?.noise)}.`,
      `src hunks by reading label (changed lines): real ${fmt(t.real)}, added ${fmt(t.added)}, ` +
        `removed ${fmt(t.removed)}, name-only ${fmt(t["name-only"])}, moved ${fmt(t.moved)}.`,
      ""
    );
    const picks = [
      ...pickExamples(src, ["added", "real"], 2),
      ...pickExamples(src, ["name-only", "moved"], 2),
      ...pickExamples(vendor, ["name-only"], 1)
    ];
    if (picks.length === 0) out.push("_(no readable hunks on this hop)_", "");
    for (const { kind, hunk } of picks) {
      out.push(
        `**${KIND_LABEL[kind]}** — \`${hunk.file}\``,
        "",
        "```diff",
        hunk.header,
        ...hunk.lines,
        "```",
        ""
      );
    }
  }
  return out;
}

// ── main ────────────────────────────────────────────────────────────────

const DEFINITIONS = [
  "## What each column is",
  "",
  "All per-hop numbers score (hop N-1 tree → hop N tree) of the SAME walk.",
  "",
  "- **diffLn / src / vendor** — changed lines between the two trees (`lib/diff.ts`, `diff -rN`; a modified line counts twice; = `git diff --numstat --no-renames` on `history.git`). `.humanify/` excluded. Includes REAL change.",
  "- **noiseLines** — the headline noise: src on-disk noise (`layout.noise` = naming + alias + reorder, git lines) + `vendor.noise`. Lower is better.",
  "- **naming / alias / reorderLn** — composeDiff's src components. `naming` only sees renames in statements whose hash did NOT flip.",
  "- **nameOnly** — `layout.nameOnlyLines`: LINE-level name-only churn (identical once local names are masked), incl. renames inside edited statements. Not additive with `naming`.",
  "- **treeLn** — `layout.churnLines`, total src tree churn, real INCLUDED; **realExBuild** — src real change minus the build-constant inlining lines.",
  "- **vendorLn / vendorReal** — vendor/ reducible noise vs genuine dependency change.",
  "- **noise / noiseLn / novel / realLn / relocSt / reloc / mints** — the eval KPIs (`kpis.ts`) on the BUNDLE; noise/novel in statements.",
  "- **cache+** — LLM cache entries written; must be 0 (no cache, rule 10). **wall s / RSS MB** — from run-pipeline.ts (wall excludes the heavy-lock wait; RSS = peak of the whole process tree).",
  "",
  "Caveats (from `kpis.ts`):",
  "",
  ...caveatLines(
    KPIS.filter((k) =>
      ["noiseLn", "realLn", "reloc", "treeLn", "vendorLn"].includes(k.key)
    )
  ).map((l) => `- ${l.trim()}`),
  "- Per-hop TS-vs-Rust deltas are NOT an A/B: each walk's hop N inherits its own hop N-1, so divergence compounds. Cold LLM draws vary run to run (rule 11); `noise-bands.json` bands are for 4-pair eval TOTALS, not walk hops. `novel`/`realLn` are statement-hash based and should nearly agree between pipelines; a large gap there means the trees differ in structure, not naming.",
  ""
];

async function main(): Promise<void> {
  const argv = process.argv.slice(2);
  if (argv[0] === "card") {
    const [walk, from, to] = argv.slice(1);
    const err = await scoreHop(walk, from, to);
    treeLines(walk, from, to);
    if (err) {
      console.error(err);
      process.exit(1);
    }
    return;
  }
  const walks = new Map<string, string>();
  let out = "";
  let examples = "";
  let exampleHopCount = 4;
  let jobs = 4;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[++i];
    if (a === "--walk") {
      const [name, dir] = v.split("=");
      walks.set(name, path.resolve(dir));
    } else if (a === "--out") out = path.resolve(v);
    else if (a === "--examples") examples = v;
    else if (a === "--example-hops") exampleHopCount = Number(v);
    else if (a === "--jobs") jobs = Number(v);
    else throw new Error(`unknown arg ${a}`);
  }
  if (walks.size === 0 || !out) {
    console.error(
      "usage: report.ts --walk <name>=<dir> [--walk …] --out <dir> [--examples <name>]"
    );
    process.exit(2);
  }
  if (!examples) examples = walks.has("rust") ? "rust" : [...walks.keys()][0];
  fs.mkdirSync(out, { recursive: true });

  const rows = new Map<string, HopRow[]>();
  for (const [name, dir] of walks) {
    console.error(`scoring ${name} walk (${dir}) …`);
    rows.set(name, await walkRows(dir, jobs));
  }

  const md: string[] = ["# Version walk report", ""];
  for (const [name, dir] of walks) {
    const m = readJson<WalkManifest>(path.join(dir, "manifest.json"));
    md.push(
      `- **${name}**: \`${dir}\` — ${JSON.stringify(m.pipelineRecord)}; LLM ${m.llm.model} @ ${m.llm.endpoint}; ` +
        `${rows.get(name)?.length ?? 0}/${m.versions.length} hops finished`
    );
  }
  md.push("", "## Per hop, side by side", "", ...comparisonTable(rows), "");
  md.push("## Per walk, every column", "");
  for (const [name, r] of rows) md.push(...detailTable(name, r), "");
  md.push(...DEFINITIONS);
  const exDir = walks.get(examples);
  const exRows = rows.get(examples);
  if (exDir && exRows)
    md.push(...examplesSection(examples, exDir, exRows, exampleHopCount));

  fs.writeFileSync(path.join(out, "report.md"), md.join("\n"));
  const json = Object.fromEntries(
    [...rows].map(([name, rs]) => [
      name,
      rs.map((r) => ({
        from: r.from,
        to: r.to,
        hop: r.hop,
        lines: r.lines,
        noiseLines: noiseLines(r.card),
        cardError: r.cardError,
        card: r.card
      }))
    ])
  );
  fs.writeFileSync(
    path.join(out, "report.json"),
    JSON.stringify(json, null, 2)
  );
  console.log(`report: ${path.join(out, "report.md")}`);
  // Not a verdict: say which hops are unscored rather than pass silently.
  for (const [name, rs] of rows) {
    for (const r of rs)
      if (r.cardError)
        console.log(`  !! ${name} ${r.from}->${r.to}: ${r.cardError}`);
  }
}

main().catch((e) => {
  console.error(`report: ${e instanceof Error ? e.stack : e}`);
  process.exit(1);
});
