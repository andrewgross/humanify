/**
 * 073 ceiling — how many churned lines could an identity carry hold still?
 *
 *   npx tsx experiments/073-identity-carry/ceiling.ts <priorTree> <freshTree>
 *
 * FUNNEL-SIMULATED, per exp069's post-mortem: exp069 priced a forward
 * ceiling (224 ln) that a full funnel simulation put at ZERO, because the
 * mechanism's own gates selected statements whose names re-derive stably.
 * So this counts only lines the carry can ACTUALLY reach:
 *
 *   1. Start from the OBSERVED output of a real fossil run. Measuring the
 *      run's own trees means every tier that already fires (exact match,
 *      statement-twin, binding cascade, post-split reconcile) has already
 *      had its turn — what remains is genuinely unclaimed.
 *   2. Keep only modules the carry would act on: signature unique on BOTH
 *      sides. Twins are skipped, never picked (exp072: 12.5% here, 36% on
 *      date-fns).
 *   3. Split each emitted file into the module's OWN BODY and the
 *      emitter's plumbing (require headers, export blocks). They have
 *      different owners and different fixes, so they are priced apart —
 *      conflating them is what made the first pass of this script read
 *      7,036 phantom "other" lines (index-aligned comparison drifting
 *      after one inserted line).
 *
 * Reported separately, because each needs a different mechanism:
 *   BODY name-only      → a binding-name carry reaches these.
 *   ALIAS-only header   → an import-alias carry reaches these.
 *   PATH-differing hdr  → reachable only when the IMPORTED module's path
 *                         carries (already true for matched modules).
 *   BODY other          → genuine change; the carry must never touch it.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";

const [PRIOR, FRESH] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: ceiling.ts <priorTree> <freshTree>");
  process.exit(1);
}

interface LedgerModule {
  file: string;
  hashes: string[];
  imports: number[];
}

function ledger(root: string): LedgerModule[] {
  const p = path.join(root, ".humanify/split-ledger.json");
  const mods: LedgerModule[] = JSON.parse(fs.readFileSync(p, "utf8"))
    .fossilModules;
  if (!mods?.length) {
    console.error(`no fossilModules in ${p} — not a fossil run`);
    process.exit(1);
  }
  return mods;
}

const key = (m: LedgerModule) => m.hashes.join("|");
const isRequire = (l: string) => /=\s*require\(/.test(l);
const isExport = (l: string) =>
  /Object\.defineProperty\(module\.exports/.test(l) || /^\s*module\.exports/.test(l);
const requirePath = (l: string) => /require\("([^"]+)"\)/.exec(l)?.[1] ?? "";

function maskLine(l: string): string | null {
  const t = tokenizeLine(l);
  return t ? t.map((x) => (x.kind === "ident" ? "@" : x.text)).join("") : null;
}

/** Set-based billed pairs — the 055 ledger's rule, order-free so a single
 * inserted line cannot cascade into false differences. */
function billedPairs(a: string[], b: string[]): [string, string][] {
  const sa = new Set(a);
  const sb = new Set(b);
  const removed = a.filter((l) => !sb.has(l));
  const added = b.filter((l) => !sa.has(l));
  const n = Math.min(removed.length, added.length);
  const out: [string, string][] = [];
  for (let i = 0; i < n; i++) out.push([removed[i], added[i]]);
  return out;
}

function nameOnly(a: string, b: string): boolean {
  const ma = maskLine(a);
  const mb = maskLine(b);
  return ma !== null && mb !== null && ma === mb && a !== b;
}

const prior = ledger(PRIOR);
const fresh = ledger(FRESH);
const cp = new Map<string, number>();
for (const m of prior) cp.set(key(m), (cp.get(key(m)) ?? 0) + 1);
const cf = new Map<string, number>();
for (const m of fresh) cf.set(key(m), (cf.get(key(m)) ?? 0) + 1);
const priorByKey = new Map<string, LedgerModule>();
for (const m of prior) if (cp.get(key(m)) === 1) priorByKey.set(key(m), m);

let pairs = 0;
let bodyIdenticalModuloNames = 0;
let bodyGenuinelyDiffers = 0;
let bodyNameOnly = 0;
let bodyAliasDriven = 0;
let bodyOther = 0;
const aliasExamples: string[] = [];
let hdrAliasOnly = 0;
let hdrPathDiffer = 0;
let hdrOther = 0;
let filesFullyClean = 0;
const bodyExamples: string[] = [];
const hdrExamples: string[] = [];

for (const f of fresh) {
  if (cf.get(key(f)) !== 1) continue;
  const p = priorByKey.get(key(f));
  if (!p) continue;
  const pp = path.join(PRIOR, p.file);
  const fp = path.join(FRESH, f.file);
  if (!fs.existsSync(pp) || !fs.existsSync(fp)) continue;
  pairs++;
  const la = fs.readFileSync(pp, "utf8").split("\n");
  const lb = fs.readFileSync(fp, "utf8").split("\n");
  const split = (ls: string[]) => ({
    hdr: ls.filter((l) => isRequire(l) || isExport(l)),
    body: ls.filter((l) => !isRequire(l) && !isExport(l))
  });
  const A = split(la);
  const B = split(lb);

  // Does the module's own body match modulo names?
  const maskedA = A.body.map(maskLine);
  const maskedB = B.body.map(maskLine);
  const bodyMaskEq =
    maskedA.length === maskedB.length &&
    !maskedA.some((x) => x === null) &&
    !maskedB.some((x) => x === null) &&
    maskedA.join("\n") === maskedB.join("\n");
  if (bodyMaskEq) bodyIdenticalModuloNames++;
  else bodyGenuinelyDiffers++;

  // Aliases this file declares on each side — a body identifier that is an
  // alias is EMITTER-generated (derived from the imported module's path),
  // so a binding-name carry cannot reach it. Counting it as reachable is
  // exactly exp069's error.
  const aliasesOf = (ls: string[]) =>
    new Set(
      ls
        .filter(isRequire)
        .map((l) => /(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=/.exec(l)?.[1])
        .filter((x): x is string => !!x)
    );
  const aliasA = aliasesOf(A.hdr);
  const aliasB = aliasesOf(B.hdr);

  for (const [x, y] of billedPairs(A.body, B.body)) {
    if (nameOnly(x, y)) {
      const tx = tokenizeLine(x);
      const ty = tokenizeLine(y);
      let aliasDriven = false;
      if (tx && ty && tx.length === ty.length) {
        for (let i = 0; i < tx.length; i++) {
          if (tx[i].text === ty[i].text) continue;
          if (aliasA.has(tx[i].text) || aliasB.has(ty[i].text)) aliasDriven = true;
        }
      }
      if (aliasDriven) {
        bodyAliasDriven += 2;
        if (aliasExamples.length < 4)
          aliasExamples.push(`${f.file}\n        - ${x.trim().slice(0, 100)}\n        + ${y.trim().slice(0, 100)}`);
      } else {
        bodyNameOnly += 2;
        if (bodyExamples.length < 6)
          bodyExamples.push(`${f.file}\n        - ${x.trim().slice(0, 110)}\n        + ${y.trim().slice(0, 110)}`);
      }
    } else bodyOther += 2;
  }
  for (const [x, y] of billedPairs(A.hdr, B.hdr)) {
    const samePath = requirePath(x) === requirePath(y);
    if (nameOnly(x, y) && samePath) {
      hdrAliasOnly += 2;
      if (hdrExamples.length < 4)
        hdrExamples.push(`${f.file}\n        - ${x.trim().slice(0, 110)}\n        + ${y.trim().slice(0, 110)}`);
    } else if (samePath) hdrOther += 2;
    else hdrPathDiffer += 2;
  }
  if (la.join("\n") === lb.join("\n")) filesFullyClean++;
}

const pct = (n: number, d: number) => (d ? `${((100 * n) / d).toFixed(1)}%` : "n/a");
console.log("=== 073 CEILING — identity carry, funnel-simulated ===");
console.log(`  prior modules ${prior.length} | fresh modules ${fresh.length}`);
console.log(
  `  provably identical (unique both sides)   ${pairs}  ${pct(pairs, fresh.length)} of fresh`
);
console.log(`  ...emitted file already byte-identical    ${filesFullyClean}`);
console.log(
  `  ...body identical modulo names           ${bodyIdenticalModuloNames}  ${pct(bodyIdenticalModuloNames, pairs)}`
);
console.log(`  ...body genuinely differs                 ${bodyGenuinelyDiffers}`);
console.log("");
console.log("  REACHABLE by a binding-name carry:");
console.log(`    body name-only, TRUE binding churn     ${bodyNameOnly}`);
console.log("  NOT reachable by a name carry (emitter-generated aliases):");
console.log(`    body lines driven by import aliases    ${bodyAliasDriven}`);
console.log("  REACHABLE by an import-alias carry:");
console.log(`    header alias-only lines (path same)    ${hdrAliasOnly}`);
console.log("  Reachable only via the IMPORTED module's path carry:");
console.log(`    header lines whose require path moved  ${hdrPathDiffer}`);
console.log("  NOT reachable (genuine change / other):");
console.log(`    body other ${bodyOther} | header other ${hdrOther}`);
console.log("");
console.log("  body samples:");
for (const e of bodyExamples) console.log(`     ${e}`);
console.log("  header samples:");
for (const e of hdrExamples) console.log(`     ${e}`);
