/**
 * Fossil-guided statement assignment: emit the file layout the bundle
 * records instead of approximating one (exp070).
 *
 * The bundle for 2.1.86 records 3,273 original modules where the split
 * emitted 1,497 files — 2.2× coarser (exp068) — and 69% of the remaining
 * hidden churn is DERIVED from layout (importer lines, aliases, export
 * keys; exp069's reach funnel). This assignment makes module boundaries
 * a read, not a guess:
 *
 *  - each fossil module (see fossil-map.ts) becomes ONE file;
 *  - a module matched to the prior release (fossil-match.ts) inherits its
 *    prior file path VERBATIM — task 0 proved module-keyed naming is
 *    load-bearing (720-line ceiling with it, 4 without);
 *  - unmatched modules mint fresh identity — a deterministic
 *    content-derived name, never a guess and never a position;
 *  - the eager zone (entry tail, no fossil) goes to ONE bundle-ordered
 *    file, `src/bootstrap.js` — a counted residue, reported in stats.
 *    (Deviation from the brief's "existing placement tiers", recorded in
 *    the exp070 STATUS: those tiers need a prior tree shaped like their
 *    votes, which a first fossil run does not have.)
 *
 * Fails LOUDLY when the bundle records no fossils: the adapter declared
 * the capability, so an unbuildable map is a detection problem to fix
 * upfront, not something to paper over with a cruder grouping.
 */
import type * as t from "@babel/types";
import { extractFossilModules, type FossilModule } from "./fossil-map.js";
import { matchFossilModules } from "./fossil-match.js";
import type { FossilLedgerModule, StableSplitLedger } from "./stable-split.js";

export const FOSSIL_BOOTSTRAP_FILE = "src/bootstrap.js";

export interface FossilAssignment {
  /** file per wrapper statement, parallel to the body. */
  assignment: string[];
  /** every fresh module with its final file — the next hop's match targets. */
  fossilModules: FossilLedgerModule[];
  stats: {
    modules: number;
    inheritedFiles: number;
    freshNamedFiles: number;
    eagerStatements: number;
    matchTiers: Record<string, number>;
  };
}

/** kebab-case a declared identifier into a file stem. */
function stemOf(name: string): string {
  const kebab = name
    .replace(/^[_$]+/, "")
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replace(/[_$]+/g, "-")
    .toLowerCase()
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
  return kebab.length > 0 ? kebab : "module";
}

/**
 * A module's naming stem: its first hoisted declaration (function/class
 * lead the segment by construction), else its first declared var that is
 * not the init itself (the init def is always the LAST declaration).
 */
function moduleStem(module: FossilModule, body: t.Statement[]): string {
  for (const i of module.statements) {
    const stmt = body[i];
    if (
      (stmt.type === "FunctionDeclaration" ||
        stmt.type === "ClassDeclaration") &&
      stmt.id
    ) {
      return stemOf(stmt.id.name);
    }
  }
  const nonInit = module.declared.slice(0, -1);
  if (nonInit.length > 0) return stemOf(nonInit[0]);
  if (module.declared.length > 0) return stemOf(module.declared[0]);
  return `module-${module.hashes[0]?.slice(0, 8) ?? "empty"}`;
}

/** A module's proposed folder+file before collision resolution. */
export interface FossilPlacement {
  folder: string;
  file: string;
}

/**
 * Folder hierarchy INFERRED from the import DAG (design addendum,
 * 2026-08-14): original folder paths are not recorded in the bundle, so
 * structure comes from the graph —
 *
 *   1. DOMINANT-IMPORTER NESTING: a module imported by exactly one other
 *      module is that importer's private helper and nests in its subtree
 *      (cycles break toward the root; depth caps at src/<a>/<b>/).
 *   2. A module with 2+ importers is SHARED and anchors at the top level
 *      — its own folder when it has nested helpers, else flat under
 *      `src/` (the preview measured this flat bucket at ~1,300–2,300
 *      files; grouping THOSE is the open folder-naming question the
 *      exp070 STATUS carries forward).
 *   3. Folder names come from their anchor module's content stem.
 *
 * Exported for the dry-run preview (experiments/070-fossil-split/
 * preview.ts) so the artifact Andrew reviews and the pipeline's real
 * placements can never diverge.
 */
export function inferFossilPlacements(
  extract: { modules: FossilModule[] },
  body: t.Statement[]
): FossilPlacement[] {
  const n = extract.modules.length;
  const parent = dominantImporterParents(extract.modules);
  const children = new Map<number, number[]>();
  for (let i = 0; i < n; i++) {
    if (parent[i] !== -1) {
      const list = children.get(parent[i]) ?? [];
      list.push(i);
      children.set(parent[i], list);
    }
  }
  const stems = extract.modules.map((m) => moduleStem(m, body));
  const anchorOf = (i: number): number => {
    let cur = i;
    while (parent[cur] !== -1) cur = parent[cur];
    return cur;
  };
  const placements: FossilPlacement[] = new Array(n);
  for (let i = 0; i < n; i++) {
    const anchor = anchorOf(i);
    const anchorHasFamily =
      (children.get(anchor) ?? []).length > 0 || anchor !== i;
    if (!anchorHasFamily) {
      placements[i] = { folder: "src", file: `${stems[i]}.js` };
    } else if (anchor === i) {
      placements[i] = {
        folder: `src/${stems[anchor]}`,
        file: `${stems[i]}.js`
      };
    } else {
      const p = parent[i];
      const sub = p === anchor ? "" : `/${stems[p]}`;
      placements[i] = {
        folder: `src/${stems[anchor]}${sub}`,
        file: `${stems[i]}.js`
      };
    }
  }
  return placements;
}

/** parent[i] = the unique importer of i (else -1), cycles broken. */
function dominantImporterParents(modules: FossilModule[]): number[] {
  const n = modules.length;
  const importers = new Map<number, number[]>();
  modules.forEach((m, i) => {
    for (const imp of m.imports) {
      const list = importers.get(imp) ?? [];
      list.push(i);
      importers.set(imp, list);
    }
  });
  const parent = new Array<number>(n).fill(-1);
  for (let i = 0; i < n; i++) {
    const imps = [...new Set(importers.get(i) ?? [])].filter((x) => x !== i);
    if (imps.length === 1) parent[i] = imps[0];
  }
  for (let i = 0; i < n; i++) {
    const seen = new Set<number>([i]);
    let p = parent[i];
    while (p !== -1) {
      if (seen.has(p)) {
        parent[i] = -1;
        break;
      }
      seen.add(p);
      p = parent[p];
    }
  }
  return parent;
}

/** First free variant of a path: base, then -2, -3… (minted ONCE for a
 * new module; the ledger then carries it, so the ordinal never re-rolls). */
function claimPath(base: string, used: Set<string>): string {
  if (!used.has(base)) {
    used.add(base);
    return base;
  }
  const dot = base.lastIndexOf(".js");
  for (let n = 2; ; n++) {
    const candidate = `${base.slice(0, dot)}-${n}.js`;
    if (!used.has(candidate)) {
      used.add(candidate);
      return candidate;
    }
  }
}

export function assignFossil(
  body: t.Statement[],
  hashes: string[],
  prior: StableSplitLedger | undefined
): FossilAssignment {
  const extract = extractFossilModules(body, hashes);
  if (extract.modules.length === 0) {
    throw new Error(
      "fossil split: the bundle records no module fossils (no __esm init " +
        "definitions) — fix detection or run with --disable fossil-split"
    );
  }

  const priorModules = prior?.fossilModules ?? [];
  const { matches, tiers } = matchFossilModules(
    priorModules.map((m) => ({ hashes: m.hashes, imports: m.imports })),
    extract.modules.map((m) => ({ hashes: m.hashes, imports: m.imports }))
  );

  const used = new Set<string>([FOSSIL_BOOTSTRAP_FILE]);
  // Inherited paths claim their names FIRST — a new module must never
  // steal a matched module's carried identity.
  const fileOfModule = new Array<string>(extract.modules.length);
  for (const [freshIdx, priorIdx] of matches) {
    const file = priorModules[priorIdx].file;
    fileOfModule[freshIdx] = file;
    used.add(file);
  }
  const placements = inferFossilPlacements(extract, body);
  let freshNamed = 0;
  extract.modules.forEach((_module, i) => {
    if (fileOfModule[i] !== undefined) return;
    const base = `${placements[i].folder}/${placements[i].file}`;
    fileOfModule[i] = claimPath(base, used);
    freshNamed++;
  });

  const assignment = new Array<string>(body.length);
  extract.modules.forEach((module, i) => {
    for (const s of module.statements) assignment[s] = fileOfModule[i];
  });
  for (const s of extract.eagerZone) assignment[s] = FOSSIL_BOOTSTRAP_FILE;

  return {
    assignment,
    fossilModules: extract.modules.map((m, i) => ({
      file: fileOfModule[i],
      hashes: m.hashes,
      imports: m.imports
    })),
    stats: {
      modules: extract.modules.length,
      inheritedFiles: matches.size,
      freshNamedFiles: freshNamed,
      eagerStatements: extract.eagerZone.length,
      matchTiers: tiers
    }
  };
}
