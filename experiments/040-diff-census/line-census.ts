/**
 * Diff-noise CENSUS: classify every line of a real `git diff` between two split
 * trees by what PRODUCED it, so the noise can be ranked and attacked.
 *
 * This reads git's own output (`git diff --no-index -U0`) rather than
 * re-deriving a diff, so the categories describe exactly the lines a human sees
 * in review. That matters: `diff-composition.ts` classifies whole STATEMENTS by
 * structural hash, which mis-attributes emitter-generated churn — an accessor
 * line that merely gained or lost its `set:` clause has a different structural
 * hash, so it scores as REAL change when it is pure emitter noise.
 *
 * Categories, in the order they are tested:
 *
 *   accessor:setter   `Object.defineProperty(module.exports, "X", ...)` where X
 *                     is exported on BOTH sides and only the setter clause
 *                     differs — the binding's cross-file writability flipped.
 *   accessor:added    a newly exported binding (real, or a renamed one).
 *   accessor:removed  an export that went away.
 *   require:added     a new `const x = require("...")` header.
 *   require:removed   a header that went away.
 *   require:alias     same import PATH on both sides, different alias — pure
 *                     alias churn.
 *   body              everything else: actual code statements.
 *
 * Usage: npx tsx line-census.ts <priorSrcDir> <freshSrcDir> [label]
 */
import { execFileSync } from "node:child_process";

type Cat =
  | "accessor:setter"
  | "accessor:added"
  | "accessor:removed"
  | "require:added"
  | "require:removed"
  | "require:alias"
  | "body";

const ACCESSOR = /^Object\.defineProperty\(module\.exports,\s*"([^"]+)"/;
const REQUIRE = /^(?:const|var|let)\s+([$\w]+)\s*=\s*require\("([^"]+)"\)/;

interface FileDiff {
  file: string;
  added: string[];
  removed: string[];
}

/** Split `git diff --no-index -U0` into per-file added/removed line lists. */
function readDiff(prior: string, fresh: string): FileDiff[] {
  let out = "";
  try {
    out = execFileSync(
      "git",
      ["diff", "--no-index", "-U0", "--no-color", prior, fresh],
      { encoding: "utf8", maxBuffer: 1024 * 1024 * 1024 }
    );
  } catch (e) {
    // git exits 1 when there IS a diff; that is the normal path.
    const err = e as { stdout?: string };
    out = err.stdout ?? "";
  }
  const files: FileDiff[] = [];
  let cur: FileDiff | null = null;
  for (const line of out.split("\n")) {
    if (line.startsWith("diff --git ")) {
      cur = {
        file: line.slice(line.lastIndexOf(" ") + 1),
        added: [],
        removed: []
      };
      files.push(cur);
      continue;
    }
    if (!cur) continue;
    if (line.startsWith("+++") || line.startsWith("---")) continue;
    if (line.startsWith("+")) cur.added.push(line.slice(1).trim());
    else if (line.startsWith("-")) cur.removed.push(line.slice(1).trim());
  }
  return files;
}

/** Classify one file's added/removed lines, returning per-category line counts. */
function classifyFile(d: FileDiff): Map<Cat, number> {
  const out = new Map<Cat, number>();
  const bump = (c: Cat, n = 1) => out.set(c, (out.get(c) ?? 0) + n);

  const accName = (l: string) => ACCESSOR.exec(l)?.[1];
  const addedAcc = new Map<string, string>();
  const removedAcc = new Map<string, string>();
  for (const l of d.added) {
    const n = accName(l);
    if (n) addedAcc.set(n, l);
  }
  for (const l of d.removed) {
    const n = accName(l);
    if (n) removedAcc.set(n, l);
  }
  // Same export on both sides => the line differs only in its attributes
  // (in practice, the `set:` clause). Both lines are emitter noise.
  for (const [n] of addedAcc) {
    if (removedAcc.has(n)) bump("accessor:setter", 2);
  }
  for (const [n] of addedAcc) if (!removedAcc.has(n)) bump("accessor:added");
  for (const [n] of removedAcc) if (!addedAcc.has(n)) bump("accessor:removed");

  const addedReq = new Map<string, string>(); // path -> alias
  const removedReq = new Map<string, string>();
  for (const l of d.added) {
    const m = REQUIRE.exec(l);
    if (m) addedReq.set(m[2], m[1]);
  }
  for (const l of d.removed) {
    const m = REQUIRE.exec(l);
    if (m) removedReq.set(m[2], m[1]);
  }
  for (const [p, alias] of addedReq) {
    if (removedReq.has(p)) {
      // same path, different alias => pure alias churn (2 lines)
      if (removedReq.get(p) !== alias) bump("require:alias", 2);
    } else bump("require:added");
  }
  for (const [p] of removedReq) if (!addedReq.has(p)) bump("require:removed");

  const isHeader = (l: string) => ACCESSOR.test(l) || REQUIRE.test(l);
  bump("body", d.added.filter((l) => !isHeader(l)).length);
  bump("body", d.removed.filter((l) => !isHeader(l)).length);
  return out;
}

function main() {
  const [prior, fresh, label] = process.argv.slice(2);
  const files = readDiff(prior, fresh);
  const totals = new Map<Cat, number>();
  const worst = new Map<string, number>();
  for (const d of files) {
    const c = classifyFile(d);
    let headerNoise = 0;
    for (const [k, v] of c) {
      totals.set(k, (totals.get(k) ?? 0) + v);
      if (k !== "body") headerNoise += v;
    }
    if (headerNoise > 0) worst.set(d.file, headerNoise);
  }
  const total = [...totals.values()].reduce((a, b) => a + b, 0);
  const pct = (n: number) =>
    total ? `${((100 * n) / total).toFixed(1)}%` : "-";
  console.log(`=== DIFF LINE CENSUS${label ? ` — ${label}` : ""} ===`);
  console.log(`  changed files: ${files.length}   diff lines: ${total}`);
  for (const [k, v] of [...totals.entries()].sort((a, b) => b[1] - a[1])) {
    console.log(
      `  ${k.padEnd(18)} ${String(v).padStart(8)}  ${pct(v).padStart(6)}`
    );
  }
  console.log(
    "\n  worst files by EMITTER-generated churn (accessors + requires):"
  );
  for (const [f, n] of [...worst.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 12)) {
    console.log(
      `    ${String(n).padStart(5)}  ${f.replace(/^.*\/src\//, "src/")}`
    );
  }
}

main();
