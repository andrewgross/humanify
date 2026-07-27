/**
 * Task 1, the number that decides whether task 3 is worth anything.
 *
 * The added/removed population on 197→198 is ~127 of the SAME libraries under
 * different humanify filenames (see grammar-identity.ts). The tempting
 * conclusion is "so those lines are noise." That does not follow, and this
 * measures the part that does.
 *
 * Two counterfactuals, per matched pair:
 *   charged now   = lines(prior file) + lines(fresh file)   [full delete + full add]
 *   stable name   = lines a per-file `diff` charges if the fresh content had
 *                   been written to the PRIOR path                [in-place edit]
 *
 * The saving from stabilising the filename is (charged now - stable name).
 * For a 2-line minified file that changed at all, both are 4, and the saving is
 * ZERO -- the rename is a reviewer-experience problem, not a line-count one.
 * This prints the real distribution rather than assuming that.
 */
import { execFileSync } from "node:child_process";
import {
  existsSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync
} from "node:fs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else if (p.endsWith(".js")) out.push(p);
  }
  return out;
}

function lines(text: string): number {
  if (text.length === 0) return 0;
  return text.split("\n").filter((_, i, a) => i < a.length - 1 || a[i] !== "")
    .length;
}

/** Lines GNU diff charges (`<` / `>`) between two texts. */
function diffLines(aText: string, bText: string, dir: string): number {
  const pa = join(dir, "a.js");
  const pb = join(dir, "b.js");
  writeFileSync(pa, aText);
  writeFileSync(pb, bText);
  try {
    execFileSync("diff", [pa, pb], { encoding: "utf8" });
    return 0;
  } catch (err) {
    const out = (err as { stdout?: string }).stdout ?? "";
    return out.split("\n").filter((l) => /^[<>]/.test(l)).length;
  }
}

const [aRoot, bRoot, pairsFile] = process.argv.slice(2);
if (!aRoot || !bRoot || !pairsFile) {
  console.error(
    "usage: rename-cost.ts <priorVendorDir> <freshVendorDir> <pairsTsv>"
  );
  console.error(
    "  pairsTsv: lines of `priorRel<TAB>freshRel` (matched same-module renames)"
  );
  process.exit(1);
}

const tmp = mkdtempSync(join(tmpdir(), "renamecost-"));
const pairs = readFileSync(pairsFile, "utf8")
  .split("\n")
  .map((l) => l.trim())
  .filter((l) => l.length > 0 && !l.startsWith("#"))
  .map((l) => l.split("\t"))
  .filter((p): p is [string, string] => p.length === 2);

let chargedNow = 0;
let stableName = 0;
let zeroSaving = 0;
let someSaving = 0;
const savings: { pair: string; now: number; stable: number }[] = [];

for (const [aRel, bRel] of pairs) {
  const aPath = join(aRoot, aRel);
  const bPath = join(bRoot, bRel);
  if (!existsSync(aPath) || !existsSync(bPath)) continue;
  const aText = readFileSync(aPath, "utf8");
  const bText = readFileSync(bPath, "utf8");
  const now = lines(aText) + lines(bText);
  const stable = diffLines(aText, bText, tmp);
  chargedNow += now;
  stableName += stable;
  if (now === stable) zeroSaving++;
  else someSaving++;
  savings.push({ pair: `${aRel} -> ${bRel}`, now, stable });
}

console.log(`# rename cost — ${pairs.length} matched same-module pairs`);
console.log("");
console.log("| accounting | lines |");
console.log("| ---------- | ----: |");
console.log(`| charged now (full delete + full add) | ${chargedNow} |`);
console.log(
  `| if the filename had been stable (in-place edit) | ${stableName} |`
);
console.log(
  `| **recoverable by stabilising the filename** | **${chargedNow - stableName}** |`
);
console.log("");
console.log(`pairs where stabilising saves NOTHING: ${zeroSaving}`);
console.log(`pairs where stabilising saves something: ${someSaving}`);
console.log("");
const top = savings
  .filter((s) => s.now !== s.stable)
  .sort((x, y) => y.now - y.stable - (x.now - x.stable));
if (top.length > 0) {
  console.log("## pairs with a nonzero saving");
  console.log("");
  console.log("| now | stable | saving | pair |");
  console.log("| --: | -----: | -----: | ---- |");
  for (const s of top.slice(0, 40)) {
    console.log(`| ${s.now} | ${s.stable} | ${s.now - s.stable} | ${s.pair} |`);
  }
}
