/**
 * DAMAGE CEILING for "reserve prior import aliases when minting module-binding
 * names" — measured BEFORE building it, because the benefit is only 551 git
 * lines and the change reaches into the allocator whose output is the naming
 * metric itself.
 *
 * The proposed rule: a module-scope binding may not be named X when X was an
 * import alias in the prior release, so the incumbent alias survives and the
 * newcomer moves. The question this answers is what ELSE moves.
 *
 * The population splits in two, and only one half is damage:
 *
 *   DEFLECTED-FREE  the binding's name is new this release anyway. The diff
 *                   already prints a change at that name; sending it to a
 *                   different new name costs nothing extra.
 *   DESTABILISED    the binding carried the SAME name in the prior release and
 *                   the reservation would force it to a new one. That is pure
 *                   manufactured churn, and it is the number that decides
 *                   whether the lever is worth building.
 *
 * Usage: npx tsx reservation-damage.ts <priorOutDir> <freshOutDir> <label>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { loadSide, readLedger } from "../041-content-anchor/replay-lib.js";

interface Ledger {
  order: string[];
  aliases?: Record<string, string>;
}

function aliasesOf(dir: string): Set<string> {
  const led = JSON.parse(
    fs.readFileSync(path.join(dir, ".humanify", "split-ledger.json"), "utf8")
  ) as Ledger;
  return new Set(Object.values(led.aliases ?? {}));
}

function main(): void {
  const [priorDir, freshDir, label] = process.argv.slice(2);
  const priorAliases = aliasesOf(priorDir);
  const prior = loadSide(priorDir, readLedger(priorDir));
  const fresh = loadSide(freshDir, readLedger(freshDir));

  // Module-scope binding names on each side, with the statement that declares
  // them (for sizing what a forced rename would re-print).
  const priorNames = new Set<string>();
  for (const s of prior) for (const n of s.outerNames) priorNames.add(n);

  let deflectedFree = 0;
  let destabilised = 0;
  let destabilisedLines = 0;
  let destabilisedRefs = 0;
  const samples: string[] = [];
  const freeSamples: string[] = [];

  // Reference counts, so a destabilised name's cost is not just its declaration.
  const refCount = new Map<string, number>();
  for (const s of fresh) {
    for (const m of s.text.match(/[A-Za-z_$][\w$]*/g) ?? []) {
      refCount.set(m, (refCount.get(m) ?? 0) + 1);
    }
  }

  for (const s of fresh) {
    for (const n of s.outerNames) {
      if (!priorAliases.has(n)) continue;
      if (priorNames.has(n)) {
        // Same name existed at module scope in the prior release too: the
        // reservation would rename a STABLE binding.
        destabilised++;
        destabilisedLines += s.lines;
        destabilisedRefs += refCount.get(n) ?? 0;
        if (samples.length < 8) {
          samples.push(
            `${n}  (${refCount.get(n) ?? 0} refs, ${s.lines} ln stmt)`
          );
        }
      } else {
        deflectedFree++;
        freeSamples.push(`${n}  (${refCount.get(n) ?? 0} refs)`);
      }
    }
  }

  console.log(`=== RESERVATION DAMAGE — ${label ?? ""} ===`);
  console.log(
    `  prior import aliases (the reserved set): ${priorAliases.size}`
  );
  console.log(
    `  fresh module bindings whose name is a reserved alias: ${deflectedFree + destabilised}`
  );
  console.log(
    `    DEFLECTED-FREE (name is new this release anyway): ${deflectedFree}`
  );
  console.log(
    `    DESTABILISED  (same name in BOTH releases -- pure damage): ${destabilised}`
  );
  console.log(
    `      their declaring statements: ${destabilisedLines} lines; reference sites: ${destabilisedRefs}`
  );
  for (const s of samples) console.log(`      ${s}`);
  if (freeSamples.length) {
    console.log(
      "    the deflected-free names (the ones the lever exists for):"
    );
    for (const s of freeSamples) console.log(`      ${s}`);
  }
  console.log(
    `ROW|${label ?? ""}|${priorAliases.size}|${deflectedFree}|${destabilised}|${destabilisedRefs}`
  );
}

main();
