/**
 * Distribution of the REAL (unbundled) source tree — the target shape.
 *   tsx srcdist.ts <version>
 * Walks claude-code-src-<version>/, reports the same size/folder metrics as
 * measure.ts so the two are directly comparable. No reference graph (we have
 * no cross-file binding graph for the real tree), so MQ/cycles are omitted.
 */

import fs from "node:fs";
import path from "node:path";
import { srcTreePath } from "./lib/io.js";
import {
  folderStats,
  histogram,
  lineCountsOf,
  sizeStats
} from "./lib/metrics.js";

const CODE_EXT = new Set([".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"]);
const SKIP_DIR = new Set(["node_modules", ".git", "dist", "build"]);

function walk(dir: string, root: string, out: string[]): void {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith(".") && entry.isDirectory()) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (!SKIP_DIR.has(entry.name)) walk(full, root, out);
    } else if (CODE_EXT.has(path.extname(entry.name))) {
      out.push(path.relative(root, full));
    }
  }
}

function main(): void {
  const version = process.argv[2] ?? "2.1.88";
  const root = srcTreePath(version);
  const rel: string[] = [];
  walk(root, root, rel);
  const fc = new Map<string, string>();
  for (const r of rel) fc.set(r, fs.readFileSync(path.join(root, r), "utf8"));

  const counts = lineCountsOf(fc);
  const s = sizeStats(counts);
  const fstats = folderStats(rel);
  const fpf = sizeStats(fstats.filesPerFolder);

  console.log(`\n===== REAL src ${version} (${root}) =====`);
  console.log(`files:        ${s.count}`);
  console.log(
    `folders:      ${fstats.folderCount}   maxDepth: ${fstats.maxDepth}`
  );
  console.log(
    `file lines:   median ${s.median.toFixed(0)}  mean ${s.mean.toFixed(0)}  ` +
      `min ${s.min}  max ${s.max}  stdev ${s.stdev.toFixed(0)}`
  );
  console.log(
    `files/folder: median ${fpf.median.toFixed(0)}  mean ${fpf.mean.toFixed(1)}  ` +
      `min ${fpf.min}  max ${fpf.max}`
  );
  console.log(`depth histogram (folders-above → files):`);
  for (const d of [...fstats.depthHistogram.keys()].sort((a, b) => a - b)) {
    console.log(`  depth ${d}: ${fstats.depthHistogram.get(d)}`);
  }
  console.log(`line histogram (bucket-lo → files):`);
  for (const b of histogram(counts, 20)) {
    console.log(`  ${b.lo.toFixed(0).padStart(6)}: ${b.n}`);
  }
}

main();
