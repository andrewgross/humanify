/**
 * 051 task 1 — the naming residual attributed PER GIT LINE.
 *
 *   npx tsx experiments/051-naming-residual/line-ledger.ts <priorSrc> <freshSrc> [label]
 *
 * The instance-level classifiers in this directory answer "what kind of thing is
 * this statement", and that is the wrong unit: `diff-composition` charges an
 * instance the LINES a line diff would print, and those lines can be anywhere in
 * a 492-line statement. Reading a statement's biggest identifier change tells
 * you nothing about which lines were billed. (Measured: 520 lines on 85->86 sit
 * in statements whose init-call lists are byte-identical.)
 *
 * So this walks the line diff of every naming instance and attributes each
 * charged line to what changed ON THAT LINE. The buckets, one sentence each:
 *
 *   ALIAS-ONLY      every identifier that differs on the line is require-bound
 *                   on both sides to the SAME module path — pure alias churn,
 *                   noise by construction.
 *   MOVED-DECL      the differing identifier is require-bound on both sides to
 *                   DIFFERENT paths, but the member read off it is unchanged and
 *                   exported by both — the declaration moved between split
 *                   files and the alias followed it.
 *   TARGET-CHANGED  require-bound both sides, different paths, different member:
 *                   the line references different code. Upstream reordering a
 *                   dependency list lands here, and so does a mispairing.
 *   PRIVATE         a `#x` differs; no pass reaches these.
 *   LOCAL-DRIFT     nothing on the line is require-bound: the pipeline chose a
 *                   different name for the same local thing.
 *   UNPAIRED        a line present on one side only inside a naming instance.
 *
 * Ranking within a line is deliberately conservative: TARGET-CHANGED wins over
 * MOVED-DECL wins over ALIAS-ONLY, so the two reducible buckets are LOWER
 * bounds. That is the safe direction for a decision to close the arc.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";

const [PRIOR, FRESH, LABEL = ""] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: line-ledger.ts <priorSrc> <freshSrc> [label]");
  process.exit(1);
}

const requireCache = new Map<string, Map<string, string>>();
function requiresOf(root: string, file: string): Map<string, string> {
  const key = `${root} ${file}`;
  let m = requireCache.get(key);
  if (!m) {
    m = new Map<string, string>();
    try {
      const code = fs.readFileSync(path.join(root, file), "utf8");
      for (const r of code.matchAll(
        /(?:const|var|let)\s+([A-Za-z_$][\w$]*)\s*=\s*require\(\s*(["'])(.*?)\2\s*\)/g
      )) {
        m.set(r[1], r[3]);
      }
    } catch {
      // absent on this side
    }
    requireCache.set(key, m);
  }
  return m;
}

const exportCache = new Map<string, Set<string>>();
function exportsOf(root: string, file: string): Set<string> {
  const key = `${root} ${file}`;
  let s = exportCache.get(key);
  if (!s) {
    s = new Set<string>();
    try {
      const code = fs.readFileSync(path.join(root, file), "utf8");
      for (const m of code.matchAll(
        /defineProperty\(module\.exports,\s*"([^"]+)"/g
      )) {
        s.add(m[1]);
      }
    } catch {
      // absent on this side
    }
    exportCache.set(key, s);
  }
  return s;
}

const resolveFrom = (consumer: string, spec: string) =>
  path.normalize(path.join(path.dirname(consumer), spec));

/** Changed-line pairs of two texts, plus the lines present on one side only.
 * Rolling-DP LCS, then paired positionally inside each changed region — the
 * same shape a line diff prints. */
function lineDiff(
  a: string[],
  b: string[]
): { pairs: [string, string][]; unpaired: number } {
  const n = a.length;
  const m = b.length;
  const pairs: [string, string][] = [];
  if (n * m > 4_000_000) {
    // Too big to align exactly; pair positionally, which is what these
    // statements look like anyway (same hash, same shape).
    let unpaired = Math.abs(n - m);
    for (let i = 0; i < Math.min(n, m); i++) {
      if (a[i] !== b[i]) pairs.push([a[i], b[i]]);
    }
    return { pairs, unpaired };
  }
  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    new Array(m + 1).fill(0)
  );
  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      dp[i][j] =
        a[i - 1] === b[j - 1]
          ? dp[i - 1][j - 1] + 1
          : Math.max(dp[i - 1][j], dp[i][j - 1]);
    }
  }
  const delA: string[] = [];
  const addB: string[] = [];
  let i = n;
  let j = m;
  while (i > 0 && j > 0) {
    if (a[i - 1] === b[j - 1]) {
      i--;
      j--;
    } else if (dp[i - 1][j] >= dp[i][j - 1]) {
      delA.push(a[--i]);
    } else {
      addB.push(b[--j]);
    }
  }
  while (i > 0) delA.push(a[--i]);
  while (j > 0) addB.push(b[--j]);
  const k = Math.min(delA.length, addB.length);
  for (let x = 0; x < k; x++) pairs.push([delA[x], addB[x]]);
  return { pairs, unpaired: delA.length + addB.length - 2 * k };
}

const TOKEN = /#?[A-Za-z_$][\w$]*/g;

type Bucket =
  | "aliasOnly"
  | "movedDecl"
  | "targetChanged"
  | "private"
  | "localDrift"
  | "unpaired"
  | "unaligned";

const NAMES: Record<Bucket, string> = {
  aliasOnly: "ALIAS-ONLY (same module)",
  movedDecl: "MOVED-DECL (same export, new file)",
  targetChanged: "TARGET-CHANGED (different code)",
  private: "PRIVATE FIELD (#x)",
  localDrift: "LOCAL-DRIFT (no imports on the line)",
  unpaired: "UNPAIRED (one side only)",
  unaligned: "UNALIGNED (token counts differ)"
};
const ORDER: Bucket[] = [
  "aliasOnly",
  "movedDecl",
  "targetChanged",
  "private",
  "localDrift",
  "unpaired",
  "unaligned"
];

/** Which bucket a single changed line pair belongs to. */
function classifyLine(
  file: string,
  la: string,
  lb: string
): { bucket: Bucket; detail: string } {
  const ta = la.match(TOKEN) ?? [];
  const tb = lb.match(TOKEN) ?? [];
  if (ta.length !== tb.length) return { bucket: "unaligned", detail: "" };
  const pReq = requiresOf(PRIOR, file);
  const fReq = requiresOf(FRESH, file);
  let alias = false;
  let moved = false;
  let target = false;
  let priv = false;
  let detail = "";
  for (let j = 0; j < ta.length; j++) {
    const x = ta[j];
    const y = tb[j];
    if (x === y) continue;
    if (x.startsWith("#") || y.startsWith("#")) {
      priv = true;
      continue;
    }
    const px = pReq.get(x);
    const fy = fReq.get(y);
    if (!px || !fy) {
      if (!px && !fy && !detail) detail = `${x} -> ${y}`;
      continue;
    }
    if (px === fy) {
      alias = true;
      if (!detail) detail = `${x} -> ${y}  [${px}]`;
    } else {
      const member = ta[j + 1];
      const same = member && member === tb[j + 1];
      if (
        same &&
        exportsOf(PRIOR, resolveFrom(file, px)).has(member) &&
        exportsOf(FRESH, resolveFrom(file, fy)).has(member)
      ) {
        moved = true;
        if (!detail) detail = `${member}: ${x}(${px}) -> ${y}(${fy})`;
      } else {
        target = true;
        detail = `${x}.${ta[j + 1] ?? ""}(${px}) -> ${y}.${tb[j + 1] ?? ""}(${fy})`;
      }
    }
  }
  const bucket: Bucket = target
    ? "targetChanged"
    : moved
      ? "movedDecl"
      : alias
        ? "aliasOnly"
        : priv
          ? "private"
          : "localDrift";
  return { bucket, detail };
}

const samples: NoiseSample[] = [];
const tally = composeDiff(PRIOR, FRESH, { samples, cap: 400_000 });

const ln: Record<Bucket, number> = {
  aliasOnly: 0,
  movedDecl: 0,
  targetChanged: 0,
  private: 0,
  localDrift: 0,
  unpaired: 0,
  unaligned: 0
};
const examples: Record<string, { detail: string; n: number }> = {};
const byFile = new Map<string, number>();

for (const s of samples.filter((x) => x.kind === "naming")) {
  const { pairs, unpaired } = lineDiff(
    (s.priorText ?? "").split("\n"),
    (s.freshText ?? "").split("\n")
  );
  ln.unpaired += unpaired;
  for (const [la, lb] of pairs) {
    const { bucket, detail } = classifyLine(s.file, la, lb);
    ln[bucket] += 2; // a changed line costs a delete and an add
    if (detail) {
      const k = `${bucket} ${detail}`;
      examples[k] = {
        detail: `${bucket}  ${detail}`,
        n: (examples[k]?.n ?? 0) + 2
      };
    }
    if (bucket === "aliasOnly" || bucket === "movedDecl") {
      byFile.set(s.file, (byFile.get(s.file) ?? 0) + 2);
    }
  }
}

const total = ORDER.reduce((n, b) => n + ln[b], 0);
const pad = (n: number, w = 6) => String(n).padStart(w);
const pct = (n: number) =>
  total ? `${((100 * n) / total).toFixed(1)}%`.padStart(6) : "   n/a";

console.log(`=== NAMING, PER LINE — ${LABEL || `${PRIOR} -> ${FRESH}`} ===`);
console.log(
  `  naming charged by diff-composition: ${tally.naming} git lines` +
    `   (this ledger accounts ${total})`
);
console.log("");
for (const b of ORDER) {
  console.log(`  ${NAMES[b].padEnd(36)} ${pad(ln[b])} ${pct(ln[b])}`);
  console.log(`ROW|${LABEL}|${b}|${ln[b]}`);
}
console.log(
  `\n  REDUCIBLE by a naming/placement lever (alias-only + moved-decl):` +
    ` ${ln.aliasOnly + ln.movedDecl} ln`
);
console.log(`ROW|${LABEL}|reducible|${ln.aliasOnly + ln.movedDecl}`);

console.log(`\n  most expensive single substitutions:`);
for (const e of Object.values(examples)
  .sort((a, b) => b.n - a.n)
  .slice(0, 60)) {
  console.log(`    ${pad(e.n, 5)} ln  ${e.detail}`);
}

console.log(`\n  files carrying the most reducible lines:`);
for (const [f, n] of [...byFile.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 8)) {
  console.log(`    ${pad(n, 5)} ln  ${f}`);
}
