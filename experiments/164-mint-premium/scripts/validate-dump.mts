/**
 * exp164: validate the dump run against the real names-mode hop before
 * reading the census — (1) the stub run's minted names vs the real run's
 * mintedCensus.names (the stub names everything the pipeline asks about,
 * so its leftover junk should be the frozen/carried subset), (2) the
 * dump's bodyLen vs the shipped statement count, (3) the dump's regions
 * vs the real diag's placement groups, (4) the captured applications.
 * argv: <diagPath> (the dump run's diagnostics)
 */
import * as fs from "node:fs";
import * as parser from "@babel/parser";

const LABEL = "exp161-names-hop";
const dumpDiag = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const realDiag = JSON.parse(fs.readFileSync(`/tmp/eval-work/${LABEL}/2.1.216.diag.json`, "utf8"));

// 1. the stub run's minted names vs the real run's
const stubNames: string[] = dumpDiag.coverage.mintedCensus.names;
const realNames: string[] = realDiag.coverage.mintedCensus.names;
const stubSet = new Map<string, number>();
for (const n of stubNames) stubSet.set(n, (stubSet.get(n) ?? 0) + 1);
const realSet = new Map<string, number>();
for (const n of realNames) realSet.set(n, (realSet.get(n) ?? 0) + 1);
let subsetViolations = 0;
for (const [n, c] of stubSet) if ((realSet.get(n) ?? 0) < c) subsetViolations++;
console.log(
  `stub minted names ${stubNames.length} (distinct ${stubSet.size}), real ${realNames.length} (distinct ${realSet.size}), stub-not-in-real ${subsetViolations}`,
);

// 2. the dump's bodyLen vs the shipped text's wrapper-body statements
const er = dumpDiag.eraRestoration;
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body =
  p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression"
    ? p0.expression.body.body
    : ast.program.body;
console.log(`shipped body statements ${body.length} vs dump bodyLen ${er.bodyLen}`);

// 3. regions vs the real diag's placement groups
const groups = (realDiag.libraryDiscovery.placementGroups as any[]).length;
const stubGroups = (dumpDiag.libraryDiscovery.placementGroups as any[]).length;
console.log(
  `regions converted ${er.regions.length}, diag groups real ${groups} / stub ${stubGroups}, frozen ${er.frozen.length}, sites ${er.sites.length}, permuteMoves ${er.permuteMoves}`,
);

// 4. the captured answers
const realApps = (realDiag.libraryDiscovery.restorationApplications as any[]).length;
const stubApps = (dumpDiag.libraryDiscovery.restorationApplications as any[]).length;
console.log(`restoration applications real ${realApps} / stub ${stubApps}`);
const realRest = (realDiag.libraryDiscovery.restorations as any[]).length;
const stubRest = (dumpDiag.libraryDiscovery.restorations as any[]).length;
console.log(`captured restoration rows real ${realRest} / stub ${stubRest}`);

// 5. per-statement agreement of the two runs' claims is implied by the
// discovery's determinism; the sites themselves are the dump's payload —
// a spot check: the applied rows' name multiset vs the real run's Site-A
// restorations count is not directly comparable (Site A vs B split), so
// print the dump's own totals.
const applied = er.sites.reduce((a: number, s: any) => a + s.applied.length, 0);
const skipped: Record<string, number> = {};
for (const s of er.sites)
  for (const [, , cause] of s.skipped) skipped[cause] = (skipped[cause] ?? 0) + 1;
console.log(`Site A applied ${applied}, skipped by cause`, skipped);
const captured = er.sites.reduce(
  (a: number, s: any) => a + s.prints.filter((p: any) => p.captured).length,
  0,
);
const printed = er.sites.reduce((a: number, s: any) => a + s.prints.length, 0);
console.log(`Site A prints ${printed}, captured ${captured}`);
