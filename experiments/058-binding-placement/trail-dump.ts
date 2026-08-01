/**
 * The placement trail for one hop, with EVERY prior-carried tier live.
 *
 *   TRAIL_DUMP=<path> npx tsx experiments/058-binding-placement/trail-dump.ts \
 *      <freshBundle> <priorLedger> <priorBundle>
 *
 * This exists because `057/trail-check.ts` builds its carry as
 * `{...emptyPriorCarry(), statementTexts}` — which leaves `matchMap` EMPTY and
 * so runs the splitter with the `preempt` and `fill` tiers switched off. Those
 * are the two binding-identity tiers, i.e. exactly the ones exp058 is about. A
 * reconstruction with them off reports "identity never dissents" for the reason
 * measurement-pitfalls rule 10's corollary gives: the thing was not free to vary.
 *
 * The real map is on disk next to the bundle (`prior-match-map.json`, written by
 * the pipeline), so it is read rather than reconstructed, and the tier counts are
 * printed so they can be checked against the shipped run's own
 * `placementTrails.tiers` in the `--diagnostics` dump.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { topLevelStatements } from "../../src/prior-version/statement-twin.js";
import { emptyPriorCarry } from "../../src/split/prior-carry.js";
import { placementTrail } from "../../src/split/placement-trail.js";
import { stableSplitFromCode } from "../../src/split/stable-split.js";

const [BUNDLE, LEDGER, PRIOR_BUNDLE] = process.argv.slice(2);
if (!BUNDLE || !LEDGER || !PRIOR_BUNDLE) {
  console.error(
    "usage: trail-dump.ts <freshBundle> <priorLedger> <priorBundle>"
  );
  process.exit(1);
}

const code = fs.readFileSync(BUNDLE, "utf8");
const prior = JSON.parse(fs.readFileSync(LEDGER, "utf8"));

function priorStatementTexts(p: string): readonly string[] {
  const c = fs.readFileSync(p, "utf8");
  const ast = parseSync(c, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast) throw new Error(`could not parse ${p}`);
  return topLevelStatements(buildUnifiedGraph(ast, p)).map((x) =>
    x.node.start != null && x.node.end != null
      ? c.slice(x.node.start, x.node.end)
      : ""
  );
}

const mapPath = path.join(path.dirname(BUNDLE), "prior-match-map.json");
const matchMap = fs.existsSync(mapPath)
  ? new Map(
      Object.entries(
        JSON.parse(fs.readFileSync(mapPath, "utf8")) as Record<string, string>
      )
    )
  : new Map<string, string>();
console.log(
  `  matchMap entries : ${matchMap.size}${fs.existsSync(mapPath) ? "" : "  (no prior-match-map.json — identity tiers inert)"}`
);

placementTrail.reset(true);
const result = await stableSplitFromCode(code, {
  prior,
  priorCarry: {
    ...emptyPriorCarry(),
    matchMap,
    statementTexts: priorStatementTexts(PRIOR_BUNDLE)
  }
});
if (!result) throw new Error("split produced nothing");
const report = placementTrail.report();
console.log(`  statements       : ${report.trails.length}`);
console.log(`  tiers            : ${JSON.stringify(report.tiers)}`);

const dissent = report.trails.filter(
  (x) => x.placedBy === "hash" && x.alternatives
);
console.log(`  hash-placed with a dissenting tier: ${dissent.length}`);

const DUMP = process.env.TRAIL_DUMP;
if (DUMP) {
  fs.writeFileSync(DUMP, JSON.stringify(report));
  console.log(`  wrote ${DUMP}`);
}
