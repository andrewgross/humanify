/**
 * Materialize the PRODUCTION split to a real directory so the folder/file
 * LAYOUT can be browsed: stableSplitFromCode (clustered fresh grouping,
 * mechanical names — no LLM) + the runnable CJS emit that `--split` now
 * performs by default, falling back to the byte-exact pure tree on a
 * load-time cycle exactly like production.
 *
 *   NODE_OPTIONS=--max-old-space-size=8192 tsx layout.ts 2.1.89 [outDir]
 *
 * Expect: app code under src/, vendored libraries under vendor/, index.js
 * at the root requiring ./src/…, _bundle.js under .humanify/. Names are
 * MECHANICAL placeholders — judge the STRUCTURE, not the names.
 */

import fs from "node:fs";
import path from "node:path";
import { tryEmitRunnableCjs } from "../../src/split/cjs-emit.js";
import { SPLIT_LEDGER_PATH } from "../../src/split/layout.js";
import { stableSplitFromCode } from "../../src/split/stable-split.js";
import { loadBeautified } from "./lib/io.js";

async function main(): Promise<void> {
  const version = process.argv[2] ?? "2.1.89";
  const outDir =
    process.argv[3] ?? "/Users/andrewgross/Development/exp029-sample-tree";
  const code = await loadBeautified(version);

  const stable = await stableSplitFromCode(code);
  if (!stable) throw new Error("not a wrapper");
  const runnable = tryEmitRunnableCjs(
    code,
    stable.ledger,
    (reason) => console.log(`runnable emit declined: ${reason} — pure tree`),
    stable.wrapper
  );
  const tree = runnable ?? stable.fileContents;
  console.log(
    `${runnable ? "RUNNABLE" : "PURE"} tree: ${tree.size} files ` +
      `(${stable.stats.files} split files, ${stable.stats.folders} folders)`
  );

  fs.rmSync(outDir, { recursive: true, force: true });
  for (const [rel, content] of tree) {
    const abs = path.join(outDir, rel);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, content);
  }
  const ledgerPath = path.join(outDir, SPLIT_LEDGER_PATH);
  fs.mkdirSync(path.dirname(ledgerPath), { recursive: true });
  fs.writeFileSync(ledgerPath, JSON.stringify(stable.ledger));
  console.log(`\n=== wrote ${tree.size} files + ledger to ${outDir} ===`);

  // Root overview: the front door should be harness + src/ + vendor/ +
  // .humanify/ only.
  console.log("\n=== root entries ===");
  for (const entry of fs.readdirSync(outDir).sort()) {
    const stat = fs.statSync(path.join(outDir, entry));
    console.log(`  ${stat.isDirectory() ? "d" : "f"}  ${entry}`);
  }

  // Top-level folder overview by size.
  const top = new Map<string, { files: number; lines: number }>();
  for (const [rel, content] of tree) {
    const t0 = rel.includes("/") ? rel.split("/")[0] : "(root)";
    const e = top.get(t0) ?? { files: 0, lines: 0 };
    e.files++;
    e.lines += content.split("\n").length - 1;
    top.set(t0, e);
  }
  console.log(`\n=== top-level folders (file count, total lines) ===`);
  for (const [name, e] of [...top.entries()].sort(
    (a, b) => b[1].lines - a[1].lines
  )) {
    console.log(
      `  ${name.padEnd(12)}  ${String(e.files).padStart(5)} files  ${String(e.lines).padStart(8)} lines`
    );
  }

  // Sample nested app paths.
  const appFiles = [...tree.keys()].filter((f) => f.startsWith("src/"));
  console.log(`\n=== sample src/ paths (${appFiles.length} total) ===`);
  for (const f of appFiles.slice(0, 8)) console.log(`  ${f}`);
  console.log(
    `\nBrowse it:  open ${outDir}   (or:  find ${outDir} | head -40)`
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
