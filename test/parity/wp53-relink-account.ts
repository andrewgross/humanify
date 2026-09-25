// WP5.3 accounting probe: which files of the oracle's shipped tree did a
// LATER pass rewrite? Takes the TS runnable tree captured right after emit
// (test/parity/wp53-emit-probe.ts), applies the Bun re-link's per-file
// rewrite (`relinkFactoryReferences`, the only split-file edit
// `relinkBunModules` makes) with the oracle run's own vendor manifest, then
// the `using` desugar (`desugarUsing`, the last content pass), and compares
// every file with the oracle run's output tree. The post-split reconcile
// reported no changes on the oracle cuts, so after those two passes every
// file must be byte-identical.
//
//   npx tsx test/parity/wp53-relink-account.ts <probe tree> <oracle out dir>
import fs from "node:fs";
import path from "node:path";
import {
  factoryLookup,
  relinkFactoryReferences
} from "../../src/split/bun-relink.js";
import { desugarUsing } from "../../src/split/using-desugar.js";
import type { BunModulesManifest } from "../../src/unpack/adapters/bun.js";

const [probeTree, oracleDir] = process.argv.slice(2);
if (!probeTree || !oracleDir) throw new Error("usage: see header");
const manifest: BunModulesManifest = JSON.parse(
  fs.readFileSync(path.join(oracleDir, "vendor", "_bun-modules.json"), "utf-8")
);
const lookup = factoryLookup(manifest);

function walk(dir: string, rel = ""): string[] {
  const out: string[] = [];
  for (const e of fs.readdirSync(path.join(dir, rel), {
    withFileTypes: true
  })) {
    const r = rel ? `${rel}/${e.name}` : e.name;
    if (e.isDirectory()) out.push(...walk(dir, r));
    else out.push(r);
  }
  return out;
}

let unchanged = 0;
let relinked = 0;
let desugared = 0;
const differ: string[] = [];
for (const rel of walk(probeTree).sort()) {
  const emitted = fs.readFileSync(path.join(probeTree, rel), "utf-8");
  const shipped = fs.readFileSync(path.join(oracleDir, rel), "utf-8");
  const linked = relinkFactoryReferences(emitted, rel, lookup);
  const after = desugarUsing(linked) ?? linked;
  if (after !== shipped) differ.push(rel);
  else if (after === emitted) unchanged++;
  else if (after === linked) relinked++;
  else desugared++;
}
console.log(
  `files: ${unchanged + relinked + desugared + differ.length}; identical as emitted: ${unchanged}; ` +
    `identical after the Bun re-link: ${relinked}; after the re-link + using desugar: ${desugared}; ` +
    `still differing: ${differ.length}`
);
for (const f of differ.slice(0, 20)) console.log(`  DIFFERS ${f}`);
process.exit(differ.length === 0 ? 0 : 1);
