/**
 * Clustered fresh-grouping strategy for Bun wrapper bundles (exp029).
 *
 * The budget-grid assignFresh makes a few huge files; this cuts the wrapper
 * statement sequence at the real module SEAMS (valleys in IDF-weighted
 * cross-reference density) and groups those seams into a size-capped nested
 * folder tree under src/ — a shape that matches a real src/. Whole vendored
 * libraries (Bun CJS factories) are set aside in vendor/ untouched rather
 * than split. Order-respecting: files are contiguous runs, so the existing
 * assignWithPrior (name-vote + textual-locality residue) stays the correct
 * cross-version stability mechanism — this only replaces release-1 grouping.
 *
 * Returns a per-statement file-path assignment (same contract as
 * assignFresh); stableSplitFromCode does the byte-slice emit + ledger.
 * Deterministic. Paths are unique CASE-INSENSITIVELY so a case-insensitive
 * filesystem (macOS/Windows) can't collapse two files.
 *
 * exp029 RESULTS.md has the measurements; naming is mechanical (dominant
 * binding, same as the budget path) and LLM-polished by the caller's namer.
 */

import * as t from "@babel/types";
import { CODE_DIR, VENDOR_DIR } from "./layout.js";
import {
  type SplitNamer,
  acceptProposedName,
  referenceIndices,
  segmentBindings,
  segmentStem
} from "./stable-split.js";

export interface ClusterConfig {
  /** Only edges shorter than this count toward seam detection. */
  window: number;
  /** Min statements between two seam cuts. */
  minGap: number;
  /** Target app file count (deepest seams cut first). */
  targetFiles: number;
  /** Safety cap: budget-split any seam-sparse region above this many lines. */
  maxLines: number;
  maxSeg: number;
  /** Max files per top-level / sub folder (balanced foldering). */
  maxTop: number;
  maxSub: number;
}

export const DEFAULT_CLUSTER_CONFIG: ClusterConfig = {
  window: 40,
  minGap: 4,
  targetFiles: 1700,
  maxLines: 2500,
  maxSeg: 60,
  maxTop: 100,
  maxSub: 25
};

interface RefGraph {
  refs: Array<Set<number>>;
  idf: number[];
  lines: number[];
  n: number;
}

function buildRefGraph(body: t.Statement[]): RefGraph {
  const refs = referenceIndices(body);
  const n = body.length;
  const indeg = new Array<number>(n).fill(0);
  for (let i = 0; i < n; i++) for (const j of refs[i]) indeg[j]++;
  const idf = indeg.map((d) => Math.log(n / (1 + d)));
  const lines = body.map((s) =>
    s.loc ? s.loc.end.line - s.loc.start.line + 1 : 1
  );
  return { refs, idf, lines, n };
}

/** x[c] = IDF-weighted count of short edges spanning position c (difference
 * array, O(E)). A module seam is a valley. */
function crossingCurve(g: RefGraph, window: number): number[] {
  const diff = new Array<number>(g.n + 2).fill(0);
  for (let i = 0; i < g.n; i++) {
    for (const j of g.refs[i]) {
      const a = i < j ? i : j;
      const b = i < j ? j : i;
      if (b - a > window) continue;
      diff[a + 1] += g.idf[j];
      diff[b + 1] -= g.idf[j];
    }
  }
  const x = new Array<number>(g.n + 1).fill(0);
  let acc = 0;
  for (let c = 0; c <= g.n; c++) {
    acc += diff[c];
    x[c] = acc;
  }
  return x;
}

/** Global-deepest-seam cuts + maxLines safety splits. */
function deepSeamCuts(g: RefGraph, x: number[], cfg: ClusterConfig): number[] {
  const cand: number[] = [];
  for (let c = 1; c < g.n; c++) cand.push(c);
  cand.sort((a, b) => x[a] - x[b] || a - b);
  const taken = new Set<number>();
  const accepted: number[] = [];
  for (const c of cand) {
    if (accepted.length >= cfg.targetFiles - 1) break;
    let ok = true;
    for (let d = 1; d < cfg.minGap; d++) {
      if (taken.has(c - d) || taken.has(c + d)) {
        ok = false;
        break;
      }
    }
    if (ok) {
      accepted.push(c);
      taken.add(c);
    }
  }
  accepted.sort((a, b) => a - b);
  return enforceBudgets(g, x, accepted, cfg);
}

/** Position of the lowest x in [lo, hi] (leftmost on ties). */
function deepestIn(x: number[], lo: number, hi: number): number {
  let best = hi;
  let bestV = Number.POSITIVE_INFINITY;
  for (let c = lo; c <= hi; c++) {
    if (x[c] < bestV) {
      bestV = x[c];
      best = c;
    }
  }
  return best;
}

/** Furthest end from `start` (exclusive) under both size caps. */
function budgetReach(
  g: RefGraph,
  start: number,
  segEnd: number,
  cfg: ClusterConfig
): number {
  let end = start + 1;
  let acc = g.lines[start];
  while (
    end < segEnd &&
    end - start < cfg.maxSeg &&
    acc + g.lines[end] <= cfg.maxLines
  ) {
    acc += g.lines[end];
    end++;
  }
  return end;
}

/** Add budget cuts (at the deepest interior seam) so [segStart,segEnd) obeys the caps. */
function budgetSplit(
  g: RefGraph,
  x: number[],
  segStart: number,
  segEnd: number,
  cfg: ClusterConfig,
  out: Set<number>
): void {
  let start = segStart;
  while (start < segEnd) {
    const end = budgetReach(g, start, segEnd, cfg);
    if (end >= segEnd) break;
    const cut = deepestIn(x, Math.min(start + 1, end), end);
    out.add(cut);
    start = cut;
  }
}

function enforceBudgets(
  g: RefGraph,
  x: number[],
  accepted: number[],
  cfg: ClusterConfig
): number[] {
  const bounds = [0, ...accepted, g.n];
  const final = new Set<number>(accepted);
  for (let b = 0; b < bounds.length - 1; b++) {
    budgetSplit(g, x, bounds[b], bounds[b + 1], cfg, final);
  }
  return [...final].sort((a, b) => a - b);
}

/** Group sorted cut positions into runs of <= maxPerGroup, walling at the
 * deepest seam in each window (caps folder size, walls at real seams). */
function pickWalls(
  cuts: number[],
  x: number[],
  maxPerGroup: number
): Set<number> {
  const walls = new Set<number>();
  let start = 0;
  while (start < cuts.length) {
    const end = Math.min(start + maxPerGroup, cuts.length);
    if (end >= cuts.length) break;
    let best = end;
    let bestD = Number.POSITIVE_INFINITY;
    for (let k = start + 1; k <= end && k < cuts.length; k++) {
      if (x[cuts[k]] < bestD) {
        bestD = x[cuts[k]];
        best = k;
      }
    }
    walls.add(cuts[best]);
    start = best;
  }
  return walls;
}

// ── library extraction ──────────────────────────────────────────────

/** `var X = CALLEE(fn)` with a >=1-param inline callback (the CJS
 * `(exports, module)` shape; ESM inits take 0 params). Structural so it
 * survives beautification. */
export function factoryCallee(
  stmt: t.Statement
): { binding: string; callee: string } | null {
  if (!t.isVariableDeclaration(stmt) || stmt.declarations.length !== 1)
    return null;
  const decl = stmt.declarations[0];
  if (
    !t.isIdentifier(decl.id) ||
    !decl.init ||
    !t.isCallExpression(decl.init)
  ) {
    return null;
  }
  const callee = decl.init.callee;
  if (!t.isIdentifier(callee)) return null;
  const arg = decl.init.arguments[0];
  const isFn =
    arg && (t.isArrowFunctionExpression(arg) || t.isFunctionExpression(arg));
  if (!isFn || arg.params.length < 1) return null;
  return { binding: decl.id.name, callee: callee.name };
}

/** The CJS factory helper = the identifier wrapping the most modules
 * (>= 2), else null. */
export function detectCjsHelper(body: t.Statement[]): string | null {
  const tally = new Map<string, number>();
  for (const stmt of body) {
    const fc = factoryCallee(stmt);
    if (fc) tally.set(fc.callee, (tally.get(fc.callee) ?? 0) + 1);
  }
  let best: string | null = null;
  let bestN = 1;
  for (const [name, n] of tally) {
    if (n > bestN) {
      bestN = n;
      best = name;
    }
  }
  return best;
}

// ── case-insensitive unique names ───────────────────────────────────

/** A name unique among `usedLower` under case-folding; returns the
 * cased name, records the lowercase form. */
function caseSafeUnique(
  stem: string,
  ext: string,
  usedLower: Set<string>
): string {
  let name = `${stem}${ext}`;
  for (let k = 2; usedLower.has(name.toLowerCase()); k++) {
    name = `${stem}-${k}${ext}`;
  }
  usedLower.add(name.toLowerCase());
  return name;
}

// ── assignment ──────────────────────────────────────────────────────

interface Segment {
  s: number;
  e: number;
  top: number;
  sub: number;
}

/** Sub-folder walls: pick walls within each top group (between top walls). */
function subWallsWithin(
  cuts: number[],
  topWalls: Set<number>,
  x: number[],
  maxSub: number
): Set<number> {
  const subWalls = new Set<number>();
  let group: number[] = [];
  for (const c of cuts) {
    if (topWalls.has(c)) {
      for (const w of pickWalls(group, x, maxSub)) subWalls.add(w);
      group = [];
    } else {
      group.push(c);
    }
  }
  for (const w of pickWalls(group, x, maxSub)) subWalls.add(w);
  return subWalls;
}

/** Partition app segments into (top, sub) groups by balanced seam-depth walls. */
function groupSegments(
  cuts: number[],
  x: number[],
  appN: number,
  cfg: ClusterConfig
): Segment[] {
  const topWalls = pickWalls(cuts, x, cfg.maxTop);
  const subWalls = subWallsWithin(cuts, topWalls, x, cfg.maxSub);
  const bounds = [0, ...cuts, appN];
  const segments: Segment[] = [];
  let top = 0;
  let sub = 0;
  for (let i = 1; i < bounds.length; i++) {
    const s = bounds[i - 1];
    if (i > 1) {
      if (topWalls.has(s)) {
        top++;
        sub = 0;
      } else if (subWalls.has(s)) {
        sub++;
      }
    }
    segments.push({ s, e: bounds[i], top, sub });
  }
  return segments;
}

/** An item to be named: a span + the dedup scope it competes in. */
interface Named {
  key: string;
  s: number;
  e: number;
  /** Dedup/sibling scope (folder path for files, parent for sub-folders). */
  scope: string;
}

function widen(
  m: Map<string, [number, number]>,
  k: string,
  s: number,
  e: number
): void {
  const cur = m.get(k);
  m.set(k, cur ? [Math.min(cur[0], s), Math.max(cur[1], e)] : [s, e]);
}

/** One stem, LLM-polished when a namer is given, else mechanical. Validation
 * (identifier-safe, specific, camelCase) is acceptProposedName's job. */
async function polishStem(
  namer: SplitNamer | undefined,
  kind: "file" | "folder",
  mechanicalStem: string,
  siblings: string[],
  bindings: string[]
): Promise<string> {
  if (!namer) return mechanicalStem;
  const proposal = await namer({ kind, mechanicalStem, siblings, bindings });
  return (proposal ? acceptProposedName(proposal) : null) ?? mechanicalStem;
}

/**
 * Resolve final names for a level: mechanical stem per item, LLM-polished in
 * parallel (siblings = same-scope stems), then case-safe deduped serially
 * within each scope (so a case-insensitive FS can't collapse two).
 */
async function resolveNames(
  items: Named[],
  kind: "file" | "folder",
  appBody: t.Statement[],
  appRefs: Array<Set<number>>,
  namer?: SplitNamer
): Promise<Map<string, string>> {
  const mech = new Map<string, string>();
  const byScope = new Map<string, string[]>();
  for (const it of items) {
    mech.set(it.key, segmentStem(appBody, appRefs, it.s, it.e));
    byScope.set(it.scope, [...(byScope.get(it.scope) ?? []), it.key]);
  }
  const polished = new Map<string, string>();
  await Promise.all(
    items.map(async (it) => {
      const stem = mech.get(it.key) ?? "module";
      const siblings = (byScope.get(it.scope) ?? [])
        .filter((k) => k !== it.key)
        .map((k) => mech.get(k) ?? "");
      polished.set(
        it.key,
        await polishStem(
          namer,
          kind,
          stem,
          siblings,
          segmentBindings(appBody, appRefs, it.s, it.e, 10)
        )
      );
    })
  );
  const ext = kind === "file" ? ".js" : "";
  const usedByScope = new Map<string, Set<string>>();
  const final = new Map<string, string>();
  for (const it of items) {
    let used = usedByScope.get(it.scope);
    if (!used) {
      used = new Set<string>();
      usedByScope.set(it.scope, used);
    }
    final.set(
      it.key,
      caseSafeUnique(polished.get(it.key) ?? "module", ext, used)
    );
  }
  return final;
}

/** Name every app segment `<folder>/<subfolder>/<file>.js` from each level's
 * dominant binding (LLM-polished when a namer is present), case-safe. */
async function nameSegments(
  segments: Segment[],
  appBody: t.Statement[],
  appRefs: Array<Set<number>>,
  namer?: SplitNamer
): Promise<Map<number, string>> {
  const topSpan = new Map<string, [number, number]>();
  const subSpan = new Map<string, [number, number]>();
  for (const seg of segments) {
    widen(topSpan, `${seg.top}`, seg.s, seg.e);
    widen(subSpan, `${seg.top}/${seg.sub}`, seg.s, seg.e);
  }
  const byStart = (
    a: [string, [number, number]],
    b: [string, [number, number]]
  ) => a[1][0] - b[1][0];

  const topItems: Named[] = [...topSpan]
    .sort(byStart)
    .map(([k, [s, e]]) => ({ key: k, s, e, scope: "" }));
  const subItems: Named[] = [...subSpan]
    .sort(byStart)
    .map(([k, [s, e]]) => ({ key: k, s, e, scope: k.split("/")[0] }));

  const topNames = await resolveNames(
    topItems,
    "folder",
    appBody,
    appRefs,
    namer
  );
  const subNames = await resolveNames(
    subItems,
    "folder",
    appBody,
    appRefs,
    namer
  );

  // Each segment's final directory, collapsing a subfolder that merely
  // repeats its parent (`auth/auth` → `auth`): the dominant sub inherits
  // the top folder's dominant binding, so the middle level names the same
  // module and is pure noise.
  const dirs = segments.map((seg) => {
    const top = topNames.get(`${seg.top}`) ?? "module";
    const sub = subNames.get(`${seg.top}/${seg.sub}`) ?? "module";
    return sameFolderName(top, sub) ? top : `${top}/${sub}`;
  });

  // File names dedup within the FINAL (post-collapse) directory, so
  // merging repeated subfolders together can never produce a duplicate
  // basename — the collision-safe re-dedup the collapse requires.
  const fileItems: Named[] = segments.map((seg, idx) => ({
    key: `${idx}`,
    s: seg.s,
    e: seg.e,
    scope: dirs[idx]
  }));
  const fileNames = await resolveNames(
    fileItems,
    "file",
    appBody,
    appRefs,
    namer
  );

  const path = new Map<number, string>();
  for (let idx = 0; idx < segments.length; idx++) {
    path.set(idx, `${dirs[idx]}/${fileNames.get(`${idx}`) ?? "file.js"}`);
  }
  return path;
}

/** A case-safe dedup suffix (`-2`) stripped, so a subfolder disambiguated
 * from a twin still reads as the same stem as its parent. */
function baseStem(name: string): string {
  return name.replace(/-\d+$/, "");
}

/** True when a subfolder merely repeats its parent's name — same stem,
 * ignoring case and any dedup suffix — so the middle level adds nothing. */
function sameFolderName(top: string, sub: string): boolean {
  return baseStem(top).toLowerCase() === baseStem(sub).toLowerCase();
}

/**
 * Clustered per-statement file assignment — the fresh-grouping strategy.
 * Libraries set aside under vendor/ (case-safe), app statements seam-cut
 * into a nested, balanced, named tree under src/. Optionally LLM-polishes
 * new names via `namer`.
 */
export async function assignClustered(
  body: t.Statement[],
  options: { config?: Partial<ClusterConfig>; namer?: SplitNamer } = {}
): Promise<string[]> {
  const cfg = { ...DEFAULT_CLUSTER_CONFIG, ...options.config };
  const helper = detectCjsHelper(body);
  const usedLib = new Set<string>();
  const assignment = new Array<string>(body.length);
  const appIdx: number[] = [];
  for (let i = 0; i < body.length; i++) {
    const fc = helper ? factoryCallee(body[i]) : null;
    if (fc && fc.callee === helper) {
      assignment[i] =
        `${VENDOR_DIR}/${caseSafeUnique(fc.binding, ".js", usedLib)}`;
    } else {
      appIdx.push(i);
    }
  }
  if (appIdx.length > 0) {
    const appBody = appIdx.map((i) => body[i]);
    const g = buildRefGraph(appBody);
    const x = crossingCurve(g, cfg.window);
    const segments = groupSegments(deepSeamCuts(g, x, cfg), x, g.n, cfg);
    const segPath = await nameSegments(
      segments,
      appBody,
      g.refs,
      options.namer
    );
    for (let idx = 0; idx < segments.length; idx++) {
      const p = `${CODE_DIR}/${segPath.get(idx) ?? "module/module/file.js"}`;
      for (let a = segments[idx].s; a < segments[idx].e; a++) {
        assignment[appIdx[a]] = p;
      }
    }
  }
  return assignment;
}
