/**
 * 073 side-measurement — how much header churn does ONE moved module cause?
 *
 * The ceiling found 4,904 require-header lines churned inside modules that
 * are themselves provably identical: their own code did not change, but a
 * module they IMPORT landed at a different path. Each moved module churns
 * a line in every importer, so the amplification factor decides whether
 * match-rate work outranks name-carry work.
 */
import * as fs from "node:fs";
import * as path from "node:path";

const [PRIOR, FRESH] = process.argv.slice(2);
interface LM {
  file: string;
  hashes: string[];
  imports: number[];
}
const ledger = (r: string): LM[] =>
  JSON.parse(fs.readFileSync(path.join(r, ".humanify/split-ledger.json"), "utf8"))
    .fossilModules;

const prior = ledger(PRIOR);
const fresh = ledger(FRESH);
const key = (m: LM) => m.hashes.join("|");
const cp = new Map<string, number>();
for (const m of prior) cp.set(key(m), (cp.get(key(m)) ?? 0) + 1);
const cf = new Map<string, number>();
for (const m of fresh) cf.set(key(m), (cf.get(key(m)) ?? 0) + 1);
const priorByKey = new Map<string, LM>();
for (const m of prior) if (cp.get(key(m)) === 1) priorByKey.set(key(m), m);

// Which fresh modules kept their prior path, and which moved?
let kept = 0;
let moved = 0;
let unmatched = 0;
const movedPaths = new Set<string>();
for (const f of fresh) {
  const p = cf.get(key(f)) === 1 ? priorByKey.get(key(f)) : undefined;
  if (!p) {
    unmatched++;
    continue;
  }
  if (p.file === f.file) kept++;
  else {
    moved++;
    movedPaths.add(p.file);
  }
}

// How many require lines in the FRESH tree point at a path that does not
// exist in the PRIOR tree — i.e. an importer paying for a target that moved
// or is new?
const walk = (dir: string, base = dir, out: string[] = []): string[] => {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
};
const priorFiles = new Set(walk(path.join(PRIOR, "src")));
let requireLines = 0;
let requireToMissing = 0;
const missingTargets = new Map<string, number>();
for (const rel of walk(path.join(FRESH, "src"))) {
  const text = fs.readFileSync(path.join(FRESH, "src", rel), "utf8");
  for (const line of text.split("\n")) {
    const m = /require\("(\.[^"]+)"\)/.exec(line);
    if (!m) continue;
    requireLines++;
    const target = path.normalize(path.join(path.dirname(rel), m[1]));
    if (!priorFiles.has(target)) {
      requireToMissing++;
      missingTargets.set(target, (missingTargets.get(target) ?? 0) + 1);
    }
  }
}

console.log("=== 073 amplification: one moved module, many churned headers ===");
console.log(`  fresh modules ${fresh.length}`);
console.log(`  matched (unique sig) & path KEPT   ${kept}`);
console.log(`  matched & path MOVED               ${moved}`);
console.log(`  unmatched (mint fresh path)        ${unmatched}`);
console.log("");
console.log(`  require lines in fresh tree                    ${requireLines}`);
console.log(
  `  ...pointing at a path absent from prior tree ${requireToMissing}  (${((100 * requireToMissing) / (requireLines || 1)).toFixed(1)}%)`
);
console.log(`  distinct such targets                         ${missingTargets.size}`);
console.log(
  `  AMPLIFICATION: churned header lines per moved/new target ${(requireToMissing / (missingTargets.size || 1)).toFixed(1)}`
);
const top = [...missingTargets.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8);
console.log("\n  worst offenders (target → importers paying for it):");
for (const [t, n] of top) console.log(`    ${String(n).padStart(4)}  ${t}`);
