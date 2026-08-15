/**
 * exp074 measurement: WHERE does require-line churn come from?
 *
 *   node experiments/074-path-stability/require-churn.mjs <priorSrc> <freshSrc>
 *
 * A require line churns when it is absent from the SAME file in the prior
 * tree. That happens for three distinct reasons, which need different
 * fixes and are therefore counted separately:
 *   - the importing file is new (its whole content is new)
 *   - the target module moved (path instability — exp074's target)
 *   - the alias binding was renamed (naming, not placement)
 */
import fs from "node:fs";
import path from "node:path";

const [PRIOR, FRESH] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: require-churn.mjs <priorSrc> <freshSrc>");
  process.exit(1);
}

function walk(dir, base = dir, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

const aFiles = new Set(walk(PRIOR));
const bFiles = walk(FRESH);
const RE = /require\("([^"]+)"\)/g;

let total = 0;
let churned = 0;
let inNewFile = 0;
const byTarget = new Map();

for (const f of bFiles) {
  const txt = fs.readFileSync(path.join(FRESH, f), "utf8");
  const priorTxt = aFiles.has(f)
    ? fs.readFileSync(path.join(PRIOR, f), "utf8")
    : null;
  const priorLines = new Set(priorTxt ? priorTxt.split("\n") : []);
  for (const line of txt.split("\n")) {
    if (!line.includes("require(")) continue;
    total++;
    if (priorTxt && priorLines.has(line)) continue;
    churned++;
    if (!priorTxt) {
      inNewFile++;
      continue;
    }
    for (const m of line.matchAll(RE)) {
      const target = path.normalize(path.join(path.dirname(f), m[1]));
      byTarget.set(target, (byTarget.get(target) ?? 0) + 1);
    }
  }
}

console.log(`require lines (fresh tree):            ${total}`);
console.log(`churned (absent from same prior file): ${churned}`);
console.log(`  ...of which sit in a NEW file:       ${inNewFile}`);
const inExisting = churned - inNewFile;
console.log(`  ...in a file that existed prior:     ${inExisting}`);

const top = [...byTarget.entries()].sort((a, b) => b[1] - a[1]);
const targetExists = top.filter(([t]) => aFiles.has(t));
const targetGone = top.filter(([t]) => !aFiles.has(t));
const sum = (l) => l.reduce((s, [, n]) => s + n, 0);
console.log(`\nchurned lines in existing files, by target:`);
console.log(
  `  target EXISTS in prior tree (so the ALIAS or the path text changed): ${sum(targetExists)} lines over ${targetExists.length} targets`
);
console.log(
  `  target ABSENT from prior tree (real path instability):               ${sum(targetGone)} lines over ${targetGone.length} targets`
);
console.log(`\ntop targets by churned lines:`);
for (const [t, n] of top.slice(0, 10)) {
  console.log(
    `  ${String(n).padStart(5)}  ${t}  ${aFiles.has(t) ? "(exists prior)" : "(ABSENT prior)"}`
  );
}
