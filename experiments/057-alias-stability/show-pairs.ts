/**
 * 057 task 1 — print the raw line pairs behind one (file, substitution), so the
 * classification can be judged by reading rather than believed (rule 1).
 *
 *   npx tsx experiments/057-alias-stability/show-pairs.ts <priorSrc> <freshSrc> <file> <from> <to> [n]
 *
 * Uses exactly the pairing `name-drivers.ts` / `ns-classify.ts` use — masked
 * line shape, FIFO within the file — so what prints IS what was counted.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";

const [PRIOR, FRESH, FILE, FROM, TO, N = "10"] = process.argv.slice(2);

function shapeOf(line: string): string | null {
  const toks = tokenizeLine(line);
  return toks
    ? toks.map((t) => (t.kind === "ident" ? " " : t.text)).join("")
    : null;
}

const a = fs.readFileSync(path.join(PRIOR, FILE), "utf8");
const b = fs.readFileSync(path.join(FRESH, FILE), "utf8");
const al = a.split("\n");
const bl = b.split("\n");
const aSet = new Set(al);
const bSet = new Set(bl);
const pool = new Map<string, string[]>();
for (const l of al) {
  if (bSet.has(l)) continue;
  const s = shapeOf(l);
  if (s === null) continue;
  const list = pool.get(s) ?? [];
  list.push(l);
  pool.set(s, list);
}
let shown = 0;
let matched = 0;
for (const l of bl) {
  if (aSet.has(l)) continue;
  const s = shapeOf(l);
  if (s === null) continue;
  const list = pool.get(s);
  const from = list?.shift();
  if (!from) continue;
  const ta = tokenizeLine(from);
  const tb = tokenizeLine(l);
  if (!ta || !tb || ta.length !== tb.length) continue;
  for (let i = 0; i < ta.length; i++) {
    if (ta[i].kind !== "ident" || ta[i].text === tb[i].text) continue;
    if (!(tb[i + 1]?.text ?? "").startsWith(".")) break;
    if (ta[i].text !== FROM || tb[i].text !== TO) break;
    matched++;
    if (shown < Number(N)) {
      shown++;
      console.log(`--- pair ${matched}`);
      console.log(`  - ${from.trim().slice(0, 200)}`);
      console.log(`  + ${l.trim().slice(0, 200)}`);
    }
    break;
  }
}
console.log(`\n${matched} occurrences of ${FROM} -> ${TO} in ${FILE}`);
