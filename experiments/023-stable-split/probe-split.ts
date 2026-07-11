/**
 * Exp023 Step-0 probe: run the CURRENT (--split) app-code splitter offline
 * on a humanified output and report the plan stats, file-size distribution,
 * and per-file assignment manifest — the baseline the stability work must
 * beat. Usage: npx tsx probe-split.ts <humanified.js> <outDir>
 */
import fs from "node:fs";
import path from "node:path";
import { generateManifest, splitAndEmit } from "../../src/split/index.js";

const [input, outDir] = process.argv.slice(2);
if (!input || !outDir) {
  console.error("usage: probe-split.ts <humanified.js> <outDir>");
  process.exit(1);
}

let t0 = Date.now();
const plan = splitAndEmit([input], outDir, {});
console.log(`split+emit: ${((Date.now() - t0) / 1000).toFixed(1)}s`);
console.log("stats:", JSON.stringify(plan.stats, null, 2));

fs.writeFileSync(
  path.join(outDir, "_split-manifest.json"),
  JSON.stringify(generateManifest(plan, [input]), null, 2)
);

// File size distribution — the dumping-ground metric.
t0 = Date.now();
const sizes: Array<[string, number]> = [];
for (const f of fs.readdirSync(outDir)) {
  if (!f.endsWith(".js")) continue;
  const lines = fs
    .readFileSync(path.join(outDir, f), "utf-8")
    .split("\n").length;
  sizes.push([f, lines]);
}
sizes.sort((a, b) => b[1] - a[1]);
const total = sizes.reduce((s, [, n]) => s + n, 0);
console.log(`\nfiles: ${sizes.length}, total lines: ${total}`);
console.log("largest 15:");
for (const [f, n] of sizes.slice(0, 15)) {
  console.log(
    `  ${String(n).padStart(8)}  ${f}  (${((100 * n) / total).toFixed(1)}%)`
  );
}
const p = (q: number) => sizes[Math.floor(sizes.length * q)]?.[1] ?? 0;
console.log(`p50=${p(0.5)} p90=${p(0.1)} max=${sizes[0]?.[1]}`);
