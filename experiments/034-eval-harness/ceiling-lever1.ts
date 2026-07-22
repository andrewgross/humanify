/**
 * Lever 1 ceiling measurement (no LLM, no pipeline run).
 *
 * Of the NOISE statements in a version transition (rename-invariant
 * statementHash present in both versions, text differs — i.e. structurally
 * unchanged code the pipeline re-named), how many have a UNIQUE prior
 * hash-twin: hash count 1 on both sides, so the prior statement's names could
 * transfer positionally with zero ambiguity? That fraction is the safe
 * deterministic ceiling for the statement-level wholesale name-inheritance
 * tier (roadmap Lever 1).
 *
 * Buckets per noise statement (counts of its hash on fresh/prior side):
 *  - uniqueTwin   1:1        — unambiguous transfer, the ceiling
 *  - equalCount   n:n (n>1)  — transferable only with a positional/unanimity
 *                              rule (the split's equal-count discipline); NOT
 *                              counted into the ceiling
 *  - unequal      n:m (n!=m) — ambiguous, abstain
 *
 * Multi-line unique twins are the near-certain true twins (a rich structure
 * colliding by chance is implausible); single-line ones include the
 * `foo();`-style generic shapes where the hash can twin different code —
 * exactly what the bindingRolesAgree gate must prune in the build. Both are
 * reported so the go/no-go can weigh them separately.
 *
 * Runs directly on the archive outputs — pass nothing:
 *   NODE_OPTIONS=--max-old-space-size=14336 \
 *     npx tsx experiments/034-eval-harness/ceiling-lever1.ts
 * Writes results/lever1-ceiling/ceiling.json and prints a table.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { type Stmt, statementsOf } from "./statements.js";

const HERE = path.dirname(new URL(import.meta.url).pathname);

interface PairCfg {
  from: string;
  to: string;
  kind: string;
}

interface Config {
  priorsBase: string;
  pairs: PairCfg[];
}

interface SamplePair {
  hash: string;
  lines: number;
  prior: string;
  fresh: string;
}

interface Bucket {
  stmts: number;
  lines: number;
}

interface PairResult {
  pair: string;
  statements: number;
  noise: Bucket;
  uniqueTwin: Bucket;
  uniqueTwinMultiline: Bucket;
  uniqueTwinSingleline: Bucket;
  /** Unique twins whose first line (declaration head) is byte-identical —
   * the flip is purely in internal locals, the LLM-floor signature. */
  uniqueTwinSameHead: Bucket;
  equalCount: Bucket;
  unequal: Bucket;
  pctUniqueOfNoiseStmts: number;
  pctUniqueOfNoiseLines: number;
  samples: SamplePair[];
}

function counts(stmts: Stmt[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const s of stmts) m.set(s.hash, (m.get(s.hash) ?? 0) + 1);
  return m;
}

function firstLine(text: string, max = 110): string {
  const line = text.split("\n", 1)[0];
  return line.length > max ? `${line.slice(0, max)}…` : line;
}

function add(b: Bucket, s: Stmt): void {
  b.stmts++;
  b.lines += s.lines;
}

function emptyBucket(): Bucket {
  return { stmts: 0, lines: 0 };
}

function analyzePair(
  pairLabel: string,
  freshCode: string,
  priorCode: string
): PairResult {
  const fresh = statementsOf(freshCode);
  const prior = statementsOf(priorCode);
  const freshCounts = counts(fresh);
  const priorCounts = counts(prior);
  const priorByHash = new Map<string, Stmt[]>();
  for (const s of prior) {
    let list = priorByHash.get(s.hash);
    if (!list) priorByHash.set(s.hash, (list = []));
    list.push(s);
  }

  const noise = emptyBucket();
  const uniqueTwin = emptyBucket();
  const uniqueTwinMultiline = emptyBucket();
  const uniqueTwinSingleline = emptyBucket();
  const uniqueTwinSameHead = emptyBucket();
  const equalCount = emptyBucket();
  const unequal = emptyBucket();
  const samples: SamplePair[] = [];

  for (const s of fresh) {
    const twins = priorByHash.get(s.hash);
    if (!twins) continue; // novel — real change, not our target
    if (twins.some((t) => t.text === s.text)) continue; // clean
    add(noise, s); // structurally unchanged, text differs
    const fc = freshCounts.get(s.hash) ?? 0;
    const pc = priorCounts.get(s.hash) ?? 0;
    if (fc === 1 && pc === 1) {
      add(uniqueTwin, s);
      add(s.lines > 1 ? uniqueTwinMultiline : uniqueTwinSingleline, s);
      const headSame = firstLine(twins[0].text) === firstLine(s.text);
      if (headSame) add(uniqueTwinSameHead, s);
      if (samples.length < 8 && (!headSame || samples.length < 4)) {
        samples.push({
          hash: s.hash,
          lines: s.lines,
          prior: firstLine(twins[0].text),
          fresh: firstLine(s.text)
        });
      }
    } else if (fc === pc) {
      add(equalCount, s);
    } else {
      add(unequal, s);
    }
  }

  return {
    pair: pairLabel,
    statements: fresh.length,
    noise,
    uniqueTwin,
    uniqueTwinMultiline,
    uniqueTwinSingleline,
    uniqueTwinSameHead,
    equalCount,
    unequal,
    pctUniqueOfNoiseStmts: pct(uniqueTwin.stmts, noise.stmts),
    pctUniqueOfNoiseLines: pct(uniqueTwin.lines, noise.lines),
    samples
  };
}

function pct(part: number, whole: number): number {
  return whole ? +((100 * part) / whole).toFixed(1) : 0;
}

function sumBucket(results: PairResult[], key: keyof PairResult): Bucket {
  const out = emptyBucket();
  for (const r of results) {
    const b = r[key] as Bucket;
    out.stmts += b.stmts;
    out.lines += b.lines;
  }
  return out;
}

function printTable(results: PairResult[], totals: Record<string, Bucket>) {
  const cols =
    "pair             noise(st/ln)    unique(st/ln)   uniq-multi(st/ln) uniq-sameHead   eqCount(st/ln)  unequal(st/ln)  uniq% st/ln";
  console.log(cols);
  const row = (
    label: string,
    n: Bucket,
    u: Bucket,
    um: Bucket,
    sh: Bucket,
    eq: Bucket,
    ne: Bucket
  ) => {
    const f = (b: Bucket) => `${b.stmts}/${b.lines}`.padEnd(15);
    console.log(
      `${label.padEnd(17)}${f(n)} ${f(u)} ${f(um)}   ${f(sh)} ${f(eq)} ${f(ne)} ${pct(u.stmts, n.stmts)}%/${pct(u.lines, n.lines)}%`
    );
  };
  for (const r of results) {
    row(
      r.pair,
      r.noise,
      r.uniqueTwin,
      r.uniqueTwinMultiline,
      r.uniqueTwinSameHead,
      r.equalCount,
      r.unequal
    );
  }
  row(
    "TOTAL",
    totals.noise,
    totals.uniqueTwin,
    totals.uniqueTwinMultiline,
    totals.uniqueTwinSameHead,
    totals.equalCount,
    totals.unequal
  );
}

function main() {
  const cfg: Config = JSON.parse(
    fs.readFileSync(path.join(HERE, "pairs.json"), "utf8")
  );
  const results: PairResult[] = [];
  for (const p of cfg.pairs) {
    const priorPath = path.join(
      cfg.priorsBase,
      `claude-code-${p.from}/.humanify/humanified.js`
    );
    const freshPath = path.join(
      cfg.priorsBase,
      `claude-code-${p.to}/.humanify/humanified.js`
    );
    if (!fs.existsSync(priorPath) || !fs.existsSync(freshPath)) {
      console.error(`SKIP ${p.from}->${p.to}: archive output missing`);
      continue;
    }
    console.error(`analyzing ${p.from}->${p.to} ...`);
    results.push(
      analyzePair(
        `${p.from}->${p.to}`,
        fs.readFileSync(freshPath, "utf8"),
        fs.readFileSync(priorPath, "utf8")
      )
    );
  }

  const totals = {
    noise: sumBucket(results, "noise"),
    uniqueTwin: sumBucket(results, "uniqueTwin"),
    uniqueTwinMultiline: sumBucket(results, "uniqueTwinMultiline"),
    uniqueTwinSingleline: sumBucket(results, "uniqueTwinSingleline"),
    uniqueTwinSameHead: sumBucket(results, "uniqueTwinSameHead"),
    equalCount: sumBucket(results, "equalCount"),
    unequal: sumBucket(results, "unequal")
  };
  printTable(results, totals);

  const outDir = path.join(HERE, "results", "lever1-ceiling");
  fs.mkdirSync(outDir, { recursive: true });
  const outPath = path.join(outDir, "ceiling.json");
  fs.writeFileSync(
    outPath,
    `${JSON.stringify({ results, totals }, null, 2)}\n`
  );
  console.error(`\nwrote ${outPath}`);
}

main();
