/**
 * 057 task 1 — is MOVED-REN a real class, or a pairing artifact?
 *
 *   npx tsx experiments/057-alias-stability/moved-ren-identity.ts <priorSrc> <freshSrc> <nsJson> <label>
 *
 * MOVED-REN is the biggest NS-MEMBER bucket (3,746 lines over four hops), and it
 * is the one bucket whose membership test cannot be trusted. `name-drivers.ts`
 * pairs a removed line with an added line when their MASKED SHAPES match, FIFO
 * within the file. Two unrelated one-line calls — `(0, a.initA)();` and
 * `(0, b.initB)();` — have the same masked shape, so the pairing is guaranteed
 * whether or not they are the same statement. Every such pair then reads as
 * "alias renamed AND member renamed".
 *
 * The usage site cannot settle it: the pair matched precisely because the two
 * lines agree on every non-identifier token, so any counterfactual rewrite makes
 * them identical. Identity has to come from the DECLARATION.
 *
 * The test: take the declaration of `memberFrom` in prior-A and of `memberTo` in
 * fresh-B, mask their identifiers, and compare. Same shape = plausibly one
 * declaration that moved file and was renamed. Different shape = two different
 * declarations, and the substitution is not alias churn at all.
 *
 * Also reported: the share whose member is a SPLITTER-MINTED lazy-init
 * initializer (`initializeAppNN`, `initializeModuleNN`, `bootstrapNN`), which is
 * not a carried identity in the first place.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";

const [PRIOR, FRESH, NSJSON, LABEL = ""] = process.argv.slice(2);

const MINTED =
  /^(initializeApp|initializeModule|bootstrap|initialize|setupApp)\d*$/;
const DECL_START =
  /^(?:var|let|const|function|class|async function)\s+([A-Za-z_$][A-Za-z0-9_$]*)/;

const declCache = new Map<string, Map<string, string>>();
/** name -> masked shape of its top-level declaration, for one file. */
function declShapes(root: string, rel: string): Map<string, string> {
  const key = `${root} ${rel}`;
  const hit = declCache.get(key);
  if (hit) return hit;
  const out = new Map<string, string>();
  let text: string;
  try {
    text = fs.readFileSync(path.join(root, rel), "utf8");
  } catch {
    declCache.set(key, out);
    return out;
  }
  const lines = text.split("\n");
  let cur: string | null = null;
  let buf: string[] = [];
  const flush = () => {
    if (cur !== null) out.set(cur, mask(buf.join("\n")));
    cur = null;
    buf = [];
  };
  for (const line of lines) {
    const m = /^\S/.test(line) ? DECL_START.exec(line) : null;
    if (m) {
      flush();
      cur = m[1];
      buf = [line];
    } else if (cur !== null) {
      if (/^\S/.test(line)) flush();
      else buf.push(line);
    }
  }
  flush();
  declCache.set(key, out);
  return out;
}

function mask(text: string): string {
  return text
    .split("\n")
    .map((l) => {
      const t = tokenizeLine(l);
      return t ? t.map((x) => (x.kind === "ident" ? "" : x.text)).join("") : l;
    })
    .join("\n");
}

type Entry = {
  key: string;
  occurrences: number;
  from: string;
  to: string;
  file: string;
  memberFrom: string;
  memberTo: string;
  fromPath: string | null;
  toPath: string | null;
};

const data = JSON.parse(fs.readFileSync(NSJSON, "utf8"));
const entries: Entry[] = data.subs.movedRen ?? [];

let sameShape = 0;
let diffShape = 0;
let missing = 0;
let mintedMember = 0;
let occSame = 0;
let occDiff = 0;
let occMissing = 0;
let occMinted = 0;
const sameExamples: string[] = [];
const diffExamples: string[] = [];

for (const e of entries) {
  if (MINTED.test(e.memberFrom) || MINTED.test(e.memberTo)) {
    mintedMember++;
    occMinted += e.occurrences;
  }
  const a = declShapes(PRIOR, e.fromPath as string).get(e.memberFrom);
  const b = declShapes(FRESH, e.toPath as string).get(e.memberTo);
  if (a === undefined || b === undefined) {
    missing++;
    occMissing += e.occurrences;
    continue;
  }
  if (a === b) {
    sameShape++;
    occSame += e.occurrences;
    if (sameExamples.length < 6)
      sameExamples.push(
        `${e.file}: ${e.from}.${e.memberFrom} -> ${e.to}.${e.memberTo}  [${e.fromPath} => ${e.toPath}]`
      );
  } else {
    diffShape++;
    occDiff += e.occurrences;
    if (diffExamples.length < 6)
      diffExamples.push(
        `${e.file}: ${e.from}.${e.memberFrom} -> ${e.to}.${e.memberTo}  [${e.fromPath} => ${e.toPath}]`
      );
  }
}

const n = entries.length || 1;
const occ = entries.reduce((s, e) => s + e.occurrences, 0) || 1;
const pct = (x: number, d: number) => `${((100 * x) / d).toFixed(1)}%`;
console.log(`\n=== MOVED-REN IDENTITY — ${LABEL} ===`);
console.log(`  (file,sub) pairs: ${entries.length}   occurrences: ${occ}\n`);
console.log(
  `  declaration shape IDENTICAL   ${String(sameShape).padStart(5)} pairs (${pct(sameShape, n)})   ${occSame} occ (${pct(occSame, occ)})`
);
console.log(
  `  declaration shape DIFFERENT   ${String(diffShape).padStart(5)} pairs (${pct(diffShape, n)})   ${occDiff} occ (${pct(occDiff, occ)})`
);
console.log(
  `  declaration NOT FOUND         ${String(missing).padStart(5)} pairs (${pct(missing, n)})   ${occMissing} occ (${pct(occMissing, occ)})`
);
console.log(
  `  member is a MINTED initializer ${String(mintedMember).padStart(4)} pairs (${pct(mintedMember, n)})   ${occMinted} occ (${pct(occMinted, occ)})`
);
console.log(
  `REN|${LABEL}|${sameShape}|${diffShape}|${missing}|${mintedMember}|${occSame}|${occDiff}|${occMissing}|${occMinted}`
);
console.log(
  `\n  ── same-shape examples (candidate real moved+renamed declarations)`
);
for (const s of sameExamples) console.log(`     ${s}`);
console.log(`\n  ── different-shape examples (NOT the same declaration)`);
for (const s of diffExamples) console.log(`     ${s}`);
