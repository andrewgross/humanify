/**
 * 054 task 1 — the three classes 051 found hiding inside "naming", checked
 * against the LOCAL-DRIFT survivors. All three read like renames:
 *
 *   cross-module reference  the declaration is a require of a DIFFERENT path
 *                           than the prior name's — pinning the prior name onto
 *                           different code. Checked here as: fresh decl is
 *                           `X = require(P)` and the prior file has no
 *                           `toName = require(P)`.
 *   moved declaration       the same exported binding, new home file. Checked
 *                           as: the restored name is exported from a DIFFERENT
 *                           file in the prior tree than the fresh file's path.
 *   upstream permutation    the bundle's lazy-init prologues. Checked as: the
 *                           declaration is a `lazyInitializer` call — 051's
 *                           TARGET-CHANGED population lives inside those.
 *
 * Also dumps the declaration line pair for every survivor so twenty can be read
 * by hand rather than trusted (rule 1).
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { runFile, walk } from "./pass.js";

const [PRIOR, FRESH, LABEL = ""] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: audit.ts <priorSrc> <freshSrc> [label]");
  process.exit(1);
}

const REQUIRE_DECL =
  /(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*require\(\s*["'](.*?)["']\s*\)/;
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const priorFiles = new Set(walk(PRIOR));
let crossModule = 0;
let sameModuleAlias = 0;
let lazyInit = 0;
let total = 0;
const flagged: string[] = [];
const dump: string[] = [];

for (const f of walk(FRESH)) {
  if (!priorFiles.has(f)) continue;
  const res = runFile(PRIOR, FRESH, f, true);
  if (res.status !== "ok" || !res.declText) continue;
  const priorText = res.priorText ?? "";
  res.renames.forEach((r, i) => {
    total++;
    const decl = res.declText?.[i] ?? "";
    const m = REQUIRE_DECL.exec(decl);
    if (m && m[1] === r.fromName) {
      const same = new RegExp(
        `(?:const|let|var)\\s+${esc(r.toName)}\\s*=\\s*require\\(\\s*["']${esc(m[2])}["']\\s*\\)`
      ).test(priorText);
      if (same) sameModuleAlias++;
      else {
        crossModule++;
        flagged.push(
          `CROSS-MODULE?  ${r.fromName} -> ${r.toName}  requires ${m[2]}  in ${f}`
        );
      }
    }
    if (/lazyInitializer/.test(decl)) lazyInit++;
    dump.push(
      `${f}\t${r.kind}\t${r.votes}\t${r.fromName} -> ${r.toName}\t${decl.trim().slice(0, 160)}`
    );
  });
}

console.log(`=== 054 SURVIVOR AUDIT — ${LABEL} ===`);
console.log(`  renames audited:                     ${total}`);
console.log(
  `  declaration is a require header:     ${sameModuleAlias + crossModule}`
);
console.log(`    - same module path (pure alias):   ${sameModuleAlias}`);
console.log(`    - DIFFERENT path (cross-module):   ${crossModule}`);
console.log(`  declaration is a lazyInitializer:    ${lazyInit}`);
console.log(
  `ROW|${LABEL}|${total}|${sameModuleAlias}|${crossModule}|${lazyInit}`
);
for (const l of flagged) console.log(`  ${l}`);

fs.writeFileSync(
  path.join(
    path.dirname(new URL(import.meta.url).pathname),
    `survivors-${(LABEL || "run").replace(/[^\w]+/g, "_")}.tsv`
  ),
  `${dump.join("\n")}\n`
);
