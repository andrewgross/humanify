/**
 * 070 Task 0 — full-funnel derived-churn ceiling for fossil layout.
 *
 *   npx tsx experiments/070-fossil-split/ceiling.ts \
 *     <priorSrc> <freshSrc> <freshLedger> <matchDump.json> [label]
 *
 * Population: the 055 paired name-only churn (the hidden-churn ledger's
 * own predicate). Per exp069's stamped rule the funnel simulates
 * DELIVERY, gate by gate, and reports every loss:
 *
 *   line class      → what the churned tokens are
 *     alias-repoint    require alias whose TARGET PATH changed — the
 *                      import was re-wired because layout moved; fossil
 *                      layout holds the target file ⇒ candidate
 *     alias-drift      alias churned, path identical — naming-side, NOT
 *                      layout-fixable (loss)
 *     member           property-position churn — declaration naming, NOT
 *                      layout-fixable here (loss)
 *     plain            other identifier churn — naming (loss)
 *   module gate     → re-pointed target attributable to fossil module(s)
 *   match gate      → all covering modules matched across versions
 *   HELD            → a line is held iff EVERY churned token on it is held
 *
 * Rule 8: the one-sided/relocation mass (cross-file masked twins, 055's
 * other ledger) is NOT in this population — the ceiling is an UNDERCOUNT
 * of what fossil layout addresses; stated, not estimated.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";

const [PRIOR, FRESH, FLEDGER, MATCHDUMP, LABEL = ""] = process.argv.slice(2);
if (!PRIOR || !FRESH || !FLEDGER || !MATCHDUMP) {
  console.error(
    "usage: ceiling.ts <priorSrc> <freshSrc> <freshLedger> <matchDump.json> [label]"
  );
  process.exit(1);
}

const ledger = JSON.parse(fs.readFileSync(FLEDGER, "utf8")) as {
  order: string[];
};
const dump = JSON.parse(fs.readFileSync(MATCHDUMP, "utf8")) as {
  matches: [number, number][];
  freshStatementsOfModule: number[][];
};
const freshMatched = new Set(dump.matches.map(([, f]) => f));
const stmtToModule = new Map<number, number>();
dump.freshStatementsOfModule.forEach((stmts, mi) => {
  for (const s of stmts) stmtToModule.set(s, mi);
});
// fresh declared name -> declaring modules (ambiguity preserved)
const declModules = new Map<string, number[]>();
(dump as unknown as { freshDeclared: string[][] }).freshDeclared.forEach(
  (names, mi) => {
    for (const n of names) {
      (declModules.get(n) ?? declModules.set(n, []).get(n)!).push(mi);
    }
  }
);
// file (ledger-relative, e.g. "src/a/b.js") -> covering fossil modules
const fileModules = new Map<string, Set<number>>();
ledger.order.forEach((file, si) => {
  const mi = stmtToModule.get(si);
  if (mi === undefined) return;
  let s = fileModules.get(file);
  if (!s) fileModules.set(file, (s = new Set()));
  s.add(mi);
});

const REQ = /(?:const|var|let)\s+([A-Za-z_$][\w$]*)\s*=\s*require\("([^"]+)"\)/g;
const requireMapCache = new Map<string, Map<string, string>>();
function requireMap(root: string, rel: string): Map<string, string> {
  const key = `${root}::${rel}`;
  let m = requireMapCache.get(key);
  if (m) return m;
  m = new Map();
  try {
    const text = fs.readFileSync(path.join(root, rel), "utf8");
    for (const g of text.matchAll(REQ)) m.set(g[1], g[2]);
  } catch {
    /* file absent */
  }
  requireMapCache.set(key, m);
  return m;
}

function pairsIn(a: string, b: string): [string, string][] {
  const pa = a.split("\n");
  const pb = b.split("\n");
  const sa = new Set(pa);
  const sb = new Set(pb);
  const rem = pa.filter((l) => !sb.has(l));
  const add = pb.filter((l) => !sa.has(l));
  const k = Math.min(rem.length, add.length);
  const out: [string, string][] = [];
  for (let i = 0; i < k; i++) out.push([rem[i], add[i]]);
  return out;
}

const samples: NoiseSample[] = [];
composeDiff(PRIOR, FRESH, { samples, cap: 500_000 });

type TokenClass =
  | "alias-repoint-held"
  | "alias-repoint-unmatched"
  | "alias-repoint-unattributed"
  | "alias-drift"
  | "member-held-LN" // declaring module matched: held under layout+module-keyed naming
  | "member-ambiguous"
  | "member-unmatched"
  | "member-not-found"
  | "plain";
const tokenCounts = new Map<TokenClass, number>();
let pairsAll = 0;
let pairsHeldL = 0; // layout-only ceiling
let pairsHeldLN = 0; // layout + module-keyed init/export naming
const lossByClass = new Map<string, number>(); // L+N pairs lost, by first blocker

for (const s of samples.filter((x) => x.kind === "real")) {
  if (s.priorText === undefined || s.freshText === undefined) continue;
  const priorReq = requireMap(PRIOR, s.file);
  const freshReq = requireMap(FRESH, s.file);
  for (const [a, b] of pairsIn(s.priorText, s.freshText)) {
    const ta = tokenizeLine(a);
    const tb = tokenizeLine(b);
    if (!ta || !tb || ta.length !== tb.length) continue;
    let ok = true;
    const diffs: { i: number; prior: string; fresh: string }[] = [];
    for (let i = 0; i < ta.length; i++) {
      if (
        ta[i].kind !== tb[i].kind ||
        (ta[i].text !== tb[i].text && ta[i].kind !== "ident")
      ) {
        ok = false;
        break;
      }
      if (ta[i].text !== tb[i].text)
        diffs.push({ i, prior: ta[i].text, fresh: tb[i].text });
    }
    if (!ok || diffs.length === 0) continue;
    pairsAll++;
    let heldL = true;
    let heldLN = true;
    let blocker = "";
    for (const d of diffs) {
      const prev = tb[d.i - 1];
      const isProp = !!prev && prev.kind !== "ident" && /\.\s*$/.test(prev.text);
      let cls: TokenClass;
      if (isProp) {
        // member churn: held under L+N iff the fresh member's declaring
        // fossil module is unique and matched (name keys to module identity)
        const mods = declModules.get(d.fresh) ?? [];
        if (mods.length === 1 && freshMatched.has(mods[0]))
          cls = "member-held-LN";
        else if (mods.length > 1) cls = "member-ambiguous";
        else if (mods.length === 1) cls = "member-unmatched";
        else cls = "member-not-found";
      } else {
        const pf = priorReq.get(d.prior);
        const ff = freshReq.get(d.fresh);
        if (pf !== undefined && ff !== undefined) {
          if (pf === ff) cls = "alias-drift";
          else {
            // re-pointed import: >=1 matched fossil module covering the
            // fresh target file (approximation both directions: today's
            // files span ~2.2 modules; the fossil file is the declaring
            // module's — member-level lookup refines where available)
            const resolved = path
              .join("src", path.dirname(s.file), ff)
              .replace(/\\/g, "/");
            const mods = fileModules.get(resolved);
            if (!mods || mods.size === 0) cls = "alias-repoint-unattributed";
            else if ([...mods].some((m) => freshMatched.has(m)))
              cls = "alias-repoint-held";
            else cls = "alias-repoint-unmatched";
          }
        } else cls = "plain";
      }
      tokenCounts.set(cls, (tokenCounts.get(cls) ?? 0) + 1);
      const heldForL = cls === "alias-repoint-held";
      const heldForLN = heldForL || cls === "member-held-LN";
      if (!heldForL) heldL = false;
      if (!heldForLN) {
        heldLN = false;
        if (!blocker) blocker = cls;
      }
    }
    if (heldL) pairsHeldL++;
    if (heldLN) pairsHeldLN++;
    else lossByClass.set(blocker, (lossByClass.get(blocker) ?? 0) + 1);
  }
}

console.log(`=== 070 fossil ceiling (paired name-only population) — ${LABEL} ===`);
console.log(`  churned pairs                  ${pairsAll}  (${2 * pairsAll} ledger lines)`);
console.log(`  token classes:`);
for (const [k, v] of [...tokenCounts.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`    ${k.padEnd(28)} ${v}`);
console.log(`  L+N pairs lost, by first blocking class:`);
for (const [k, v] of [...lossByClass.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`    ${k.padEnd(28)} ${v}`);
console.log(
  `  CEILING (layout only)          ${pairsHeldL} pairs = ${2 * pairsHeldL} ledger lines`
);
console.log(
  `  CEILING (layout + module-keyed naming) ${pairsHeldLN} pairs = ${2 * pairsHeldLN} ledger lines`
);
console.log(
  `ROW|${LABEL}|${pairsAll}|${pairsHeldL}|${pairsHeldLN}|${2 * pairsHeldLN}`
);
