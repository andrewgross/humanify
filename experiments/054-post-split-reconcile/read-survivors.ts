/**
 * 054 task 1 — print the aligned line pairs behind a proposed rename so it can
 * be READ. Rule 1: seven hypotheses in this arc died on one look at the pairs.
 *
 *   npx tsx read-survivors.ts <priorSrc> <freshSrc> <renames.json> [file substring]
 *
 * For each rename it shows every clean rename-noise line pair in the file whose
 * differing positions include the renamed token — prior line above, fresh line
 * below — so the three classes 051 found hiding inside "naming" are visible:
 * a cross-module reference, an upstream permutation, and a moved declaration
 * all read like renames.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  computeNormalDiff,
  parseNormalDiff
} from "../../src/rename/diff-reconcile.js";

const [PRIOR, FRESH, JSON_PATH, FILTER = ""] = process.argv.slice(2);
if (!PRIOR || !FRESH || !JSON_PATH) {
  console.error(
    "usage: read-survivors.ts <priorSrc> <freshSrc> <renames.json> [file substring]"
  );
  process.exit(1);
}

interface Rec {
  file: string;
  from: string;
  to: string;
  kind: string;
  votes: number;
  declLine: number;
}
const data = JSON.parse(fs.readFileSync(JSON_PATH, "utf8")) as {
  label: string;
  renames: Rec[];
};

const byFile = new Map<string, Rec[]>();
for (const r of data.renames) {
  if (FILTER && !r.file.includes(FILTER)) continue;
  const l = byFile.get(r.file) ?? [];
  l.push(r);
  byFile.set(r.file, l);
}

const WORD = (n: string) =>
  new RegExp(`(?<![\\w$])${n.replace(/\$/g, "\\$")}(?![\\w$])`);

for (const [file, recs] of byFile) {
  const priorText = fs.readFileSync(path.join(PRIOR, file), "utf8");
  const freshText = fs.readFileSync(path.join(FRESH, file), "utf8");
  const hunks = parseNormalDiff(computeNormalDiff(priorText, freshText));
  console.log(`\n########## ${file}   [${data.label}]`);
  for (const r of recs) {
    console.log(
      `\n--- ${r.from}  ->  ${r.to}   (${r.kind}, ${r.votes} votes, decl line ${r.declLine})`
    );
    const re = WORD(r.from);
    let shown = 0;
    for (const h of hunks) {
      if (h.op !== "c" || h.priorLines.length !== h.newLines.length) continue;
      for (let k = 0; k < h.newLines.length; k++) {
        if (!re.test(h.newLines[k])) continue;
        const isDecl = h.newStart + k === r.declLine;
        console.log(
          `  ${isDecl ? "DECL" : "    "} - ${h.priorLines[k].trim()}`
        );
        console.log(`       + ${h.newLines[k].trim()}`);
        if (++shown >= 6) break;
      }
      if (shown >= 6) break;
    }
    if (shown === 0) console.log("  (no matching clean pair found)");
  }
}
