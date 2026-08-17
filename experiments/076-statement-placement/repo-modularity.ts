/**
 * Benchmark a file tree's LAYOUT QUALITY against its own dependency graph.
 *
 *   npx tsx --max-old-space-size=32768 \
 *     experiments/076-statement-placement/repo-modularity.ts \
 *       "label=/path/to/src" "label2=/path/to/other/src" ...
 *
 * Andrew, 2026-08-15: quantify the approaches and benchmark against existing
 * codebases.
 *
 * SHAPE METRICS ALONE ARE A TRAP, and this repo has the proof in hand. Our
 * PRE-fossil output scores better on every shape metric than the fossil
 * layout that replaced it — evenness 0.977 vs 0.80, flat root 0% vs 21%,
 * biggest folder 26 files vs 715 — and it is the layout we abandoned for
 * being 2.2x coarser than the truth. A metric that rewards it would send us
 * backwards. Evenness and Gini describe how a tree LOOKS; they cannot tell
 * an invented grouping from a real one.
 *
 * Modularity Q can, because it scores the folders against how the code
 * actually imports itself:
 *
 *   Q = sum over folders c of [ e_c / m - (d_c / 2m)^2 ]
 *
 * with m edges, e_c edges inside folder c, d_c the degree of its members.
 * The second term is what the same degree sequence would give at random, so
 * Q ~ 0 means "these folders tell you nothing about the dependencies".
 *
 * Bounds worth stating before quoting any number:
 *   - Q has a known resolution limit and is bounded by graph structure. A
 *     dependency graph with one utility imported by thousands of files
 *     cannot reach a high Q under ANY partition, so compare Q BETWEEN
 *     layouts of the same graph, never against an absolute band.
 *   - the edge extraction is textual (relative require/import specifiers).
 *     It cannot see dynamic requires, and it ignores bare package imports on
 *     purpose — a shared external dependency is not evidence that two files
 *     belong together.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { formatShape, modularity, treeShape } from "./tree-shape.js";

const args = process.argv.slice(2);
if (args.length === 0) {
  console.error('usage: repo-modularity.ts "label=/path/to/src" ...');
  process.exit(1);
}

const SKIP = new Set(["node_modules", ".git", "dist", "build", "coverage"]);

function walk(root: string): string[] {
  const out: string[] = [];
  const rec = (dir: string, rel: string) => {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (e.name.startsWith(".") || SKIP.has(e.name)) continue;
      const r = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory()) rec(path.join(dir, e.name), r);
      else if (
        /\.(ts|tsx|js|jsx|mjs|cjs)$/.test(e.name) &&
        !/\.d\.ts$/.test(e.name)
      )
        out.push(r);
    }
  };
  rec(root, "");
  return out.sort();
}

/** Relative specifiers only: a shared npm package is not evidence that two
 * files belong in one folder. */
const SPEC = /(?:require\(\s*|from\s+|import\(\s*)["'](\.[^"']+)["']/g;

function buildEdges(root: string, files: string[]): Array<[number, number]> {
  const index = new Map<string, number>();
  files.forEach((f, i) => {
    index.set(f, i);
    index.set(f.replace(/\.[^./]+$/, ""), i);
  });
  const resolve = (fromFile: string, spec: string): number | undefined => {
    const base = path.posix.join(path.posix.dirname(fromFile), spec);
    const normalized = base.replace(/^\.\//, "");
    for (const cand of [
      normalized,
      normalized.replace(/\.js$/, ".ts"),
      `${normalized}.js`,
      `${normalized}.ts`,
      `${normalized}/index.js`,
      `${normalized}/index.ts`,
      normalized.replace(/\.[^./]+$/, "")
    ]) {
      const hit = index.get(cand);
      if (hit !== undefined) return hit;
    }
    return undefined;
  };
  const edges: Array<[number, number]> = [];
  let unresolved = 0;
  files.forEach((f, i) => {
    let text: string;
    try {
      text = fs.readFileSync(path.join(root, f), "utf8");
    } catch {
      return;
    }
    for (const m of text.matchAll(SPEC)) {
      const target = resolve(f, m[1]);
      if (target === undefined) unresolved++;
      else if (target !== i) edges.push([i, target]);
    }
  });
  if (unresolved > 0) {
    console.error(
      `  (${unresolved} relative specifiers did not resolve to a file in the tree)`
    );
  }
  return edges;
}

console.log(
  "tree                        files folders     root   median   max     depth   evenness  modularity"
);
for (const arg of args) {
  const eq = arg.indexOf("=");
  const label = eq === -1 ? arg : arg.slice(0, eq);
  const root = eq === -1 ? arg : arg.slice(eq + 1);
  if (!fs.existsSync(root)) {
    console.log(`  ${label.padEnd(26)} (absent: ${root})`);
    continue;
  }
  const files = walk(root);
  if (files.length < 10) {
    console.log(`  ${label.padEnd(26)} (only ${files.length} files — skipped)`);
    continue;
  }
  const edges = buildEdges(root, files);
  console.log(
    `${formatShape(`  ${label}`, treeShape(files), modularity(files, edges))}` +
      `  (${edges.length} edges)`
  );
}
