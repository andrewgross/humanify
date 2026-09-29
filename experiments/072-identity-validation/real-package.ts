/**
 * 072 — the same validation on a REAL package's published versions.
 *
 *   npx tsx .../real-package.ts <dirA> <dirB> <outDir>
 *
 * A synthetic corpus can be too clean, so the identity verdict is re-scored
 * on real code that real maintainers really changed. Ground truth here is
 * the published sources themselves: a file is UNCHANGED iff its bytes are
 * identical between the two published tarballs.
 *
 * Each version gets a generated entry that DYNAMICALLY imports every module
 * — same reason as the corpus generator: a statically-imported ESM tree is
 * inlined and leaves no per-module initializer to read.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as crypto from "node:crypto";

const [dirA, dirB, outDir] = process.argv.slice(2);
if (!dirA || !dirB || !outDir) {
  console.error("usage: real-package.ts <dirA> <dirB> <outDir>");
  process.exit(1);
}

function modules(root: string): string[] {
  const out: string[] = [];
  (function walk(d: string): void {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith(".mjs") && !e.name.endsWith(".d.mjs")) {
        out.push(path.relative(root, p));
      }
    }
  })(root);
  return out.sort();
}

const filesA = modules(dirA);
const filesB = modules(dirB);
const hash = (root: string, f: string) =>
  crypto
    .createHash("sha1")
    .update(fs.readFileSync(path.join(root, f)))
    .digest("hex");

const hashesA = new Map(filesA.map((f) => [f, hash(dirA, f)]));
const labels: Record<string, string> = {};
let unchanged = 0;
let changed = 0;
let added = 0;
for (const f of filesB) {
  const before = hashesA.get(f);
  const now = hash(dirB, f);
  if (before === undefined) {
    labels[f] = "added-file";
    added++;
  } else if (before === now) {
    labels[f] = "unchanged";
    unchanged++;
  } else {
    labels[f] = "source-changed";
    changed++;
  }
}
console.log(
  `real package: ${filesA.length} → ${filesB.length} modules | unchanged ${unchanged} | changed ${changed} | added ${added}`
);

fs.mkdirSync(outDir, { recursive: true });
for (const [tag, root, list] of [
  ["a", dirA, filesA],
  ["b", dirB, filesB]
] as const) {
  const entry = path.join(root, "__entry.mjs");
  const lines = [
    "const mods = await Promise.all([",
    ...list.map((f) => `  import("./${f.replace(/\\/g, "/")}"),`),
    "]);",
    "let acc = 0;",
    "for (const m of mods) acc += Object.keys(m).length;",
    "console.log(acc);"
  ];
  fs.writeFileSync(entry, `${lines.join("\n")}\n`);
  console.log(`wrote entry for ${tag}: ${list.length} dynamic imports`);
}
fs.writeFileSync(
  path.join(outDir, "mutations.json"),
  JSON.stringify(
    { labels, movedFrom: {}, counts: { unchanged, changed, added } },
    null,
    2
  )
);
console.log(`truth written to ${outDir}/mutations.json`);
