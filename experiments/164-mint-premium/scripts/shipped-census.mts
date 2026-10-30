/**
 * exp164: the SHIPPED-TEXT minted census per binding, with locations.
 * Parses .humanify/humanified.js (the pre-split generated text the minted
 * census runs on), finds every binding whose name is in the diag's
 * mintedCensus.names, records name + decl span, then maps each to the
 * output file that holds it (via the split ledger's per-file byte ranges
 * when available) and to the claims/applications from the diag.
 * argv: <label> [tree]
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as parser from "@babel/parser";
import traverseMod from "@babel/traverse";

const LABEL = process.argv[2] ?? "exp161-names-hop";
const TREE = process.argv[3] ?? "2.1.216";
const ROOT = `/tmp/eval-work/${LABEL}/${TREE}`;
const diag = JSON.parse(fs.readFileSync(`/tmp/eval-work/${LABEL}/2.1.216.diag.json`, "utf8"));
const mintedNames = diag.coverage.mintedCensus.names;
const minted = new Set(mintedNames);
const shipped = fs.readFileSync(`${ROOT}/.humanify/humanified.js`, "utf8");
console.log(`shipped text ${shipped.length} bytes`);

const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const rows: { name: string; start: number; end: number }[] = [];
const seen = new Set<string>();
traverseMod.default(ast, {
  Scopable: {
    enter: (p: any) => {
      for (const [name, binding] of Object.entries(p.scope.bindings as Record<string, any>)) {
        if (minted.has(name)) {
          const key = `${name}@${binding.identifier.start}`;
          if (!seen.has(key)) {
            seen.add(key);
            rows.push({ name, start: binding.identifier.start, end: binding.identifier.end });
          }
        }
      }
    },
  },
});
console.log("junk bindings in shipped text:", rows.length, "(diag census:", mintedNames.length, ")");
const byName = new Map<string, number>();
for (const r of rows) byName.set(r.name, (byName.get(r.name) ?? 0) + 1);
const check = new Map<string, number>();
for (const n of mintedNames) check.set(n, (check.get(n) ?? 0) + 1);
let mismatch = 0;
for (const [n, c] of check) if ((byName.get(n) ?? 0) !== c) mismatch++;
console.log("per-name count mismatches:", mismatch);

fs.writeFileSync(
  `/work/exp164-mint-census/shipped-junk-${LABEL}.json`,
  JSON.stringify(rows),
);
