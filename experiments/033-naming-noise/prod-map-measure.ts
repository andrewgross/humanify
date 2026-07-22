/**
 * Production-map measurement for Levers B (fill) + A (preempt) — DETERMINISTIC.
 *
 * Unlike b-ceiling.ts (which builds an ORACLE map by matching final↔final), this
 * uses the REAL production map captured by a wired `-vv` run
 * (`.humanify/prior-match-map.json`, written by writePriorMatchMapDebug). It
 * re-splits the SAME produced humanified output with the map OFF vs ON, so the
 * only variable is the identity tier — zero LLM confound.
 *
 * OFF = pre-B behavior. ON = fill (Lever B, votes.size===0) + preempt (Lever A,
 * overrides a disagreeing name-vote). The stat breakdown separates them; the
 * away-from-prior count is the precision guard (a wrong match / locality ripple
 * that moves a binding OFF its prior home = a regression, never a win).
 *
 * Run:
 *   NODE_OPTIONS=--max-old-space-size=14336 npx tsx \
 *     experiments/033-naming-noise/prod-map-measure.ts <runDir> [priorVer]
 *   # runDir holds .humanify/humanified.js + .humanify/prior-match-map.json
 *   # add WRITE_TREES=/some/dir to dump both trees for a direct line diff
 */
import * as fs from "node:fs";
import {
  type StableSplitLedger,
  stableSplitFromCode
} from "../../src/split/stable-split.js";

const VERSIONS = "/Users/andrewgross/Development/unpacked-claude-code/versions";
const runDir = process.argv[2];
const priorVer = process.argv[3] ?? "2.1.215";
if (!runDir) throw new Error("usage: prod-map-measure.ts <runDir> [priorVer]");

function readJson<T>(path: string): T {
  return JSON.parse(fs.readFileSync(path, "utf8")) as T;
}
function fileOf(l: StableSplitLedger, name: string): string | undefined {
  return l.nameToFiles[name]?.[0];
}

async function main() {
  const newCode = fs.readFileSync(`${runDir}/.humanify/humanified.js`, "utf8");
  const priorLedger = readJson<StableSplitLedger>(
    `${VERSIONS}/claude-code-${priorVer}/.humanify/split-ledger.json`
  );
  const mapObj = readJson<Record<string, string>>(
    `${runDir}/.humanify/prior-match-map.json`
  );
  const priorMatchMap = new Map<string, string>(Object.entries(mapObj));
  console.log(
    `production map entries (final -> prior): ${priorMatchMap.size}\n`
  );

  console.log("splitting OFF (no identity tier)…");
  const off = await stableSplitFromCode(newCode, { prior: priorLedger });
  console.log("splitting ON (fill + preempt)…");
  const on = await stableSplitFromCode(newCode, {
    prior: priorLedger,
    priorMatchMap
  });
  if (!off || !on) throw new Error("split returned null");

  console.log("\n=== stats (deterministic) ===");
  console.log(
    `  OFF: residueLocality=${off.stats.residueLocality} ` +
      `identity=${off.stats.inheritedViaIdentity} ` +
      `preempt=${off.stats.inheritedViaIdentityPreempt}`
  );
  console.log(
    `  ON : residueLocality=${on.stats.residueLocality} ` +
      `identity=${on.stats.inheritedViaIdentity} ` +
      `preempt=${on.stats.inheritedViaIdentityPreempt}`
  );
  console.log(
    `  → fill (B) rescued ${on.stats.inheritedViaIdentity}, ` +
      `preempt (A) overrode ${on.stats.inheritedViaIdentityPreempt} name-votes`
  );

  // Relocation vs the 215 prior + regression count, over the map's bindings.
  let relocOff = 0;
  let relocOn = 0;
  let fixed = 0;
  let regressed = 0;
  const regressions: string[] = [];
  for (const [finalName, priorName] of priorMatchMap) {
    const home = fileOf(priorLedger, priorName);
    if (!home) continue;
    const fOff = fileOf(off.ledger, finalName);
    const fOn = fileOf(on.ledger, finalName);
    const movedOff = fOff !== undefined && fOff !== home;
    const movedOn = fOn !== undefined && fOn !== home;
    if (movedOff) relocOff++;
    if (movedOn) relocOn++;
    if (movedOff && !movedOn) fixed++;
    if (!movedOff && movedOn) {
      regressed++;
      if (regressions.length < 15) {
        regressions.push(
          `  ${finalName} (was ${priorName}): ${home} -> ${fOn}`
        );
      }
    }
  }
  console.log("\n=== relocation of matched bindings vs prior (home-file) ===");
  console.log(`  relocated OFF: ${relocOff}`);
  console.log(`  relocated ON:  ${relocOn}`);
  console.log(
    `  → fixed ${fixed}, regressed ${regressed} (net ${fixed - regressed})`
  );
  if (regressions.length) {
    console.log("\n  away-from-prior regressions (first 15):");
    console.log(regressions.join("\n"));
  }

  if (process.env.WRITE_TREES) {
    for (const [name, res] of [
      ["off", off],
      ["on", on]
    ] as const) {
      const dir = `${process.env.WRITE_TREES}/${name}`;
      for (const [rel, content] of res.fileContents) {
        const full = `${dir}/src/${rel}`;
        fs.mkdirSync(full.slice(0, full.lastIndexOf("/")), { recursive: true });
        fs.writeFileSync(full, content);
      }
    }
    console.log(`\nwrote trees under ${process.env.WRITE_TREES}/{off,on}`);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
