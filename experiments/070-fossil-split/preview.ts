/**
 * 070 — dry-run distribution preview (Andrew's early deliverable):
 * what would the fossil-structured tree LOOK like, from a real saved
 * bundle, before any pipeline change lands?
 *
 *   npx tsx experiments/070-fossil-split/preview.ts \
 *     <freshBundle> <freshLedger> <priorBundle> <priorLedger> <currentTree> <label>
 *
 * Files come straight from the fossils (one module = one file). The
 * folder hierarchy is INFERRED from the import DAG — original folder
 * paths are not recorded in the bundle — by a deliberately simple,
 * fully deterministic rule set this preview exists to de-risk:
 *
 *   1. DOMINANT-IMPORTER NESTING: a module imported by exactly one
 *      other module is that importer's private helper — it nests in the
 *      importer's folder subtree.
 *   2. A module with 2+ importers is SHARED: it anchors at the top
 *      level (its own folder if it has nested helpers, else grouped).
 *   3. Folder names come from their anchor module's content stem;
 *      chains deeper than src/<a>/<b>/ collapse upward (matching the
 *      current layout's depth cap).
 *
 * Name sources are counted the way the real run would source them:
 * modules matched to the PRIOR bundle's fossils (src/split/fossil-match)
 * would carry their file identity; unmatched mint fresh content-derived
 * stems; ambiguous twins are reported as such.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parse } from "@babel/parser";
import type * as t from "@babel/types";
import {
  extractFossilModules,
  type FossilModule
} from "../../src/split/fossil-map.js";
import { matchFossilModules } from "../../src/split/fossil-match.js";
import { inferFossilPlacements } from "../../src/split/fossil-assign.js";

const [FRESH_BUNDLE, FRESH_LEDGER, PRIOR_BUNDLE, PRIOR_LEDGER, CURRENT, LABEL] =
  process.argv.slice(2);
if (!FRESH_BUNDLE || !FRESH_LEDGER || !CURRENT || !LABEL) {
  console.error(
    "usage: preview.ts <freshBundle> <freshLedger> <priorBundle> <priorLedger> <currentTree> <label>"
  );
  process.exit(1);
}

// ── extraction (same code path the pipeline will run) ─────────────────────
function largestBlock(ast: t.File): t.Statement[] {
  let body: t.Statement[] = [];
  (function walk(node: unknown): void {
    if (!node || typeof node !== "object") return;
    if (Array.isArray(node)) {
      for (const c of node) walk(c);
      return;
    }
    const n = node as { type?: string; body?: unknown };
    if (
      n.type === "BlockStatement" &&
      Array.isArray(n.body) &&
      (n.body as t.Statement[]).length > body.length
    ) {
      body = n.body as t.Statement[];
    }
    for (const k of Object.keys(n)) {
      if (k === "loc") continue;
      walk((n as Record<string, unknown>)[k]);
    }
  })(ast.program);
  return body;
}

function loadExtract(bundlePath: string, ledgerPath: string) {
  const code = fs.readFileSync(bundlePath, "utf8");
  const ast = parse(code, { sourceType: "unambiguous", errorRecovery: false });
  const body = largestBlock(ast);
  const ledger = JSON.parse(fs.readFileSync(ledgerPath, "utf8")) as {
    hashes?: string[];
  };
  const extract = extractFossilModules(body, ledger.hashes ?? []);
  return { extract, body };
}

// ── stats helpers ─────────────────────────────────────────────────────────
function linesOf(body: t.Statement[], statements: number[]): number {
  let lines = 0;
  for (const i of statements) {
    const loc = body[i].loc;
    if (loc) lines += loc.end.line - loc.start.line + 1;
  }
  return lines;
}

function quantiles(sorted: number[]): string {
  const q = (p: number) => sorted[Math.floor((sorted.length - 1) * p)] ?? 0;
  return `min ${sorted[0] ?? 0} · p25 ${q(0.25)} · median ${q(0.5)} · p75 ${q(0.75)} · p95 ${q(0.95)} · max ${sorted[sorted.length - 1] ?? 0}`;
}

function walkTree(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walkTree(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

// ── build the preview ─────────────────────────────────────────────────────
const { extract, body } = loadExtract(FRESH_BUNDLE, FRESH_LEDGER);
const placements = inferFossilPlacements(extract, body);

let matchedCount = 0;
let twinCount = 0;
if (PRIOR_BUNDLE !== "-" && PRIOR_LEDGER !== "-") {
  const prior = loadExtract(PRIOR_BUNDLE, PRIOR_LEDGER);
  const { matches } = matchFossilModules(
    prior.extract.modules.map((m) => ({
      hashes: m.hashes,
      imports: m.imports
    })),
    extract.modules.map((m) => ({ hashes: m.hashes, imports: m.imports }))
  );
  matchedCount = matches.size;
  const sig = (m: FossilModule) => m.hashes.join("|");
  const priorSigs = new Set(prior.extract.modules.map(sig));
  extract.modules.forEach((m, i) => {
    if (!matches.has(i) && priorSigs.has(sig(m))) twinCount++;
  });
}

// resolve duplicate file paths the way fossil-assign would (mint -N once)
const used = new Set<string>();
const finalPaths = placements.map((pl) => {
  let candidate = `${pl.folder}/${pl.file}`;
  if (used.has(candidate)) {
    const dot = candidate.lastIndexOf(".js");
    for (let k = 2; ; k++) {
      const c = `${candidate.slice(0, dot)}-${k}.js`;
      if (!used.has(c)) {
        candidate = c;
        break;
      }
    }
  }
  used.add(candidate);
  return candidate;
});

const stmtsPerFile = extract.modules
  .map((m) => m.statements.length)
  .sort((a, b) => a - b);
const linesPerFile = extract.modules
  .map((m) => linesOf(body, m.statements))
  .sort((a, b) => a - b);

const filesPerFolder = new Map<string, number>();
for (const p of finalPaths) {
  const folder = p.slice(0, p.lastIndexOf("/"));
  filesPerFolder.set(folder, (filesPerFolder.get(folder) ?? 0) + 1);
}
const folderSizes = [...filesPerFolder.values()].sort((a, b) => a - b);

// current-layout comparison
const currentFiles = walkTree(CURRENT);
const currentLines = currentFiles
  .map((f) => fs.readFileSync(path.join(CURRENT, f), "utf8").split("\n").length)
  .sort((a, b) => a - b);
const currentFolders = new Map<string, number>();
for (const f of currentFiles) {
  const folder = f.includes("/") ? f.slice(0, f.lastIndexOf("/")) : ".";
  currentFolders.set(folder, (currentFolders.get(folder) ?? 0) + 1);
}
const currentFolderSizes = [...currentFolders.values()].sort((a, b) => a - b);

const topFolders = [...filesPerFolder.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 20);

const out: string[] = [];
out.push(`## ${LABEL} — fossil-structured tree preview`);
out.push("");
out.push("| | fossil layout (preview) | current layout |");
out.push("|---|---|---|");
out.push(
  `| app files | **${extract.modules.length}** (+1 bootstrap for ${extract.eagerZone.length} eager stmts) | ${currentFiles.length} |`
);
out.push(`| folders | ${filesPerFolder.size} | ${currentFolders.size} |`);
out.push(
  `| statements/file | ${quantiles(stmtsPerFile)} | (ledger-equivalent: ${(stmtsPerFile.reduce((a, b) => a + b, 0) / Math.max(1, currentFiles.length)).toFixed(1)} avg if same statements) |`
);
out.push(
  `| lines/file | ${quantiles(linesPerFile)} | ${quantiles(currentLines)} |`
);
out.push(
  `| files/folder | ${quantiles(folderSizes)} | ${quantiles(currentFolderSizes)} |`
);
out.push(
  `| name sources | ${matchedCount} module-match carry · ${extract.modules.length - matchedCount - twinCount} fresh-named · ${twinCount} ambiguous twins (fresh-named, flagged) | n/a |`
);
out.push("");
out.push(`largest folders (files):`);
out.push("");
for (const [folder, count] of topFolders) {
  out.push(`- \`${folder}/\` — ${count}`);
}
out.push("");
const biggest = extract.modules
  .map((m, i) => ({ i, lines: linesOf(body, m.statements) }))
  .sort((a, b) => b.lines - a.lines)
  .slice(0, 8);
out.push(`largest files (lines):`);
out.push("");
for (const { i, lines } of biggest) {
  out.push(
    `- \`${finalPaths[i]}\` — ${lines} lines, ${extract.modules[i].statements.length} statements`
  );
}
out.push("");

fs.appendFileSync(
  path.join(path.dirname(new URL(import.meta.url).pathname), "PREVIEW.md"),
  `${out.join("\n")}\n`
);
console.log(out.join("\n"));
