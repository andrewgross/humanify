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
import {
  type FactoryCall,
  factoryCallOf,
  vendorStemFor
} from "../shared/cjs-factory.js";
import { uniqueCaseInsensitiveName } from "../shared/unique-name.js";
import { CODE_DIR, VENDOR_DIR } from "./layout.js";
import {
  type SplitNamer,
  acceptProposedName,
  referenceIndices,
  segmentBindings,
  segmentStem,
  toKebabCase
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
  /** Segments under this many lines merge into a neighbor (a run of
   * 3-line stubs is not a file a human writes). Budget caps win. */
  minLines: number;
  /** Min/max files per top-level / sub folder (balanced foldering). A wall
   * may only land within [min, max] cuts of the previous wall, so group
   * sizes are bounded on BOTH sides (the tail group may run short). */
  minTop: number;
  maxTop: number;
  minSub: number;
  maxSub: number;
  /** Top groups holding at most this many files emit FLAT (no sub level) —
   * humans don't nest a handful of files two folders deep. */
  flatTop: number;
}

export const DEFAULT_CLUSTER_CONFIG: ClusterConfig = {
  window: 40,
  minGap: 4,
  targetFiles: 1700,
  maxLines: 2500,
  maxSeg: 60,
  minLines: 25,
  minTop: 40,
  maxTop: 100,
  minSub: 6,
  maxSub: 25,
  flatTop: 8
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
  return dropTinySegments(
    g,
    [...final].sort((a, b) => a - b),
    cfg
  );
}

/** Merge segments under the minLines floor into a neighbor — a run of
 * 3-line stubs is not a file a human writes. The budget caps win: a cut
 * stays whenever dropping it would push the merged segment over
 * maxLines or maxSeg (the tiny-knob test configs rely on this). */
function dropTinySegments(
  g: RefGraph,
  cuts: number[],
  cfg: ClusterConfig
): number[] {
  const pre = new Array<number>(g.n + 1).fill(0);
  for (let i = 0; i < g.n; i++) pre[i + 1] = pre[i] + g.lines[i];
  const kept: number[] = [];
  let start = 0;
  for (let i = 0; i < cuts.length; i++) {
    const c = cuts[i];
    const next = i + 1 < cuts.length ? cuts[i + 1] : g.n;
    const mergedTooBig =
      pre[next] - pre[start] > cfg.maxLines || next - start > cfg.maxSeg;
    if (pre[c] - pre[start] >= cfg.minLines || mergedTooBig) {
      kept.push(c);
      start = c;
    }
  }
  // The tail segment may be tiny too: merge it back when the budget allows.
  if (kept.length > 0) {
    const last = kept[kept.length - 1];
    const prevStart = kept.length > 1 ? kept[kept.length - 2] : 0;
    const tailTiny = pre[g.n] - pre[last] < cfg.minLines;
    const fits =
      pre[g.n] - pre[prevStart] <= cfg.maxLines &&
      g.n - prevStart <= cfg.maxSeg;
    if (tailTiny && fits) kept.pop();
  }
  return kept;
}

/** Group sorted cut positions into runs of [min, max] cuts, walling at the
 * deepest seam within each allowed window (bounds folder size on both
 * sides, walls at real seams). Exported for unit tests. */
export function pickWalls(
  cuts: number[],
  x: number[],
  size: { min: number; max: number }
): Set<number> {
  const max = Math.max(1, size.max);
  const min = Math.max(1, Math.min(size.min, max));
  const walls = new Set<number>();
  let start = 0;
  while (cuts.length - start > max) {
    const lo = start + min;
    let hi = Math.min(start + max, cuts.length - 1);
    // Prefer walls that leave the tail a full group too, when possible.
    const hiKeepingTail = cuts.length - min;
    if (hiKeepingTail >= lo) hi = Math.min(hi, hiKeepingTail);
    let best = hi;
    let bestD = Number.POSITIVE_INFINITY;
    for (let k = lo; k <= hi; k++) {
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

/** Vendor stem for a factory statement: the binding when it passes the
 * filename floor, else lib_<hash of the statement's source slice>. The
 * floor needs the source text; without it the binding stands. */
function vendorStem(
  binding: string,
  stmt: t.Statement,
  code: string | undefined
): string {
  if (!code || stmt.start == null || stmt.end == null) return binding;
  return vendorStemFor(binding, code.slice(stmt.start, stmt.end));
}

/** A statement that is PURELY factory declarations — every declarator is
 * `X = CALLEE(fn)` with a >=1-param inline callback (the CJS
 * `(exports, module)` shape; ESM inits take 0 params) and one shared
 * callee. Comma-joined factories are common in real Bun output; at
 * statement granularity they vendor as ONE file, named after the first
 * binding. Structural (shared shape predicate) so it survives
 * beautification. */
export function factoryCallee(
  stmt: t.Statement
): { binding: string; callee: string; count: number } | null {
  if (!t.isVariableDeclaration(stmt) || stmt.declarations.length === 0) {
    return null;
  }
  const calls: FactoryCall[] = [];
  for (const decl of stmt.declarations) {
    const call = factoryCallOf(decl);
    if (!call || call.paramCount < 1) return null;
    calls.push(call);
  }
  const callee = calls[0].callee;
  if (calls.some((c) => c.callee !== callee)) return null;
  return { binding: calls[0].binding, callee, count: calls.length };
}

/** The CJS factory helper = the identifier wrapping the most modules
 * (>= 2), counting every declarator, else null. */
export function detectCjsHelper(body: t.Statement[]): string | null {
  const tally = new Map<string, number>();
  for (const stmt of body) {
    const fc = factoryCallee(stmt);
    if (fc) tally.set(fc.callee, (tally.get(fc.callee) ?? 0) + fc.count);
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
  size: { min: number; max: number }
): Set<number> {
  const subWalls = new Set<number>();
  let group: number[] = [];
  for (const c of cuts) {
    if (topWalls.has(c)) {
      for (const w of pickWalls(group, x, size)) subWalls.add(w);
      group = [];
    } else {
      group.push(c);
    }
  }
  for (const w of pickWalls(group, x, size)) subWalls.add(w);
  return subWalls;
}

/** Partition app segments into (top, sub) groups by balanced seam-depth walls. */
function groupSegments(
  cuts: number[],
  x: number[],
  appN: number,
  cfg: ClusterConfig
): Segment[] {
  const topWalls = pickWalls(cuts, x, { min: cfg.minTop, max: cfg.maxTop });
  const subWalls = subWallsWithin(cuts, topWalls, x, {
    min: cfg.minSub,
    max: cfg.maxSub
  });
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

/** Folders: same-name same-scope groups MERGE into one dir (first casing
 * wins, so a case-insensitive FS can't collide) — a human reads equal
 * names as one folder. Suffixing (-2) is what the old tree did, and
 * errorBuilders-2/-3/-4 was its signature failure. */
function mergedFolderNames(
  items: Named[],
  polished: Map<string, string>
): Map<string, string> {
  const canonical = new Map<string, string>();
  const final = new Map<string, string>();
  for (const it of items) {
    const name = polished.get(it.key) ?? "module";
    const key = `${it.scope}|${name.toLowerCase()}`;
    const first = canonical.get(key);
    if (first === undefined) canonical.set(key, name);
    final.set(it.key, first ?? name);
  }
  return final;
}

/** camelCase/kebab/snake stem → lowercase word tokens
 * ("abortErrorHandling" → ["abort","error","handling"]). */
function tokensOf(stem: string): string[] {
  return stem
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .toLowerCase()
    .split(/[\s_-]+/)
    .filter(Boolean);
}

/** Crude plural normalization, for token COMPARISON only (never emitted). */
function singular(token: string): string {
  return token.length > 3 ? token.replace(/s$/, "") : token;
}

/**
 * How a sub-folder name relates to its parent: null → the sub adds no
 * information (its tokens are a subset of the parent's — the
 * abortErrorHandling/abortError stutter) and collapses away; otherwise
 * the sub's final name — its residual tokens when it partially repeats
 * the parent (`auth`/`authToken` → `token`, as a human writes it), or
 * itself when disjoint. Exported for unit tests.
 */
export function mergeSubIntoTop(top: string, sub: string): string | null {
  const topTokens = new Set(tokensOf(top).map(singular));
  const subTokens = tokensOf(sub);
  const residual = subTokens.filter((tok) => !topTokens.has(singular(tok)));
  if (residual.length === 0) return null;
  if (residual.length === subTokens.length) return sub;
  const name = residual
    .map((tok, i) => (i === 0 ? tok : tok[0].toUpperCase() + tok.slice(1)))
    .join("");
  return acceptProposedName(name);
}

/** Trailing tokens that decorate rather than name a folder — a folder is a
 * domain, not "the thing that manages X". Stripped from folder names
 * (layoutEngineGroup → layout). */
const FOLDER_DECORATION = new Set([
  "group",
  "suite",
  "engine",
  "manager",
  "hub",
  "handler",
  "factory",
  "processor",
  "service",
  "module",
  "wrapper"
]);

/** Leading tokens that make a folder read as an action — a folder is a
 * noun, so a single leading verb is dropped (getDisplayName → displayName,
 * filterErrorsBySeverity → errorsBySeverity). */
const FOLDER_VERB = new Set([
  "get",
  "set",
  "build",
  "filter",
  "handle",
  "create",
  "make",
  "render",
  "process",
  "register",
  "add",
  "remove",
  "update",
  "fetch",
  "load",
  "parse",
  "init",
  "initialize",
  "run",
  "send",
  "apply",
  "resolve",
  "compute",
  "generate",
  "validate",
  "check",
  "format",
  "convert",
  "transform"
]);

/** A folder name reduced to its domain noun and kebab-cased: trailing
 * decoration tokens dropped, a single leading verb dropped (always leaving
 * at least one token). Kebab is the src/ tree convention. */
function cleanFolderSegment(name: string): string {
  let toks = tokensOf(name);
  while (toks.length > 1 && FOLDER_DECORATION.has(toks[toks.length - 1])) {
    toks.pop();
  }
  if (toks.length > 1 && FOLDER_VERB.has(toks[0])) toks = toks.slice(1);
  return toks.join("-") || toKebabCase(name);
}

/** Kebab-case a `top[/sub]` directory path segment-by-segment, cleaning
 * each folder segment (decoration/verb strip). */
function cleanDirPath(dir: string): string {
  return dir.split("/").filter(Boolean).map(cleanFolderSegment).join("/");
}

/** Group items by scope, preserving item order within each group. */
function groupByScope(items: Named[]): Map<string, Named[]> {
  const byScope = new Map<string, Named[]>();
  for (const it of items) {
    byScope.set(it.scope, [...(byScope.get(it.scope) ?? []), it]);
  }
  return byScope;
}

/** Validate one namer proposal: identifier-safe/specific via
 * acceptProposedName, and a folder may not merely echo one of its
 * members — a folder name describes the group, not its loudest file. */
function acceptForItem(
  proposal: string | null,
  members: string[] | undefined
): string | null {
  const accepted = proposal ? acceptProposedName(proposal) : null;
  if (!accepted) return null;
  const echo = members?.some(
    (member) => member.toLowerCase() === accepted.toLowerCase()
  );
  return echo ? null : accepted;
}

/**
 * Polish one level's mechanical stems via the namer, ONE call per sibling
 * scope (so the whole top level is a single joint batch and siblings are
 * named against each other). Returns key → validated polished stem; the
 * mechanical stem stands wherever the namer is absent, declines, or the
 * proposal fails validation.
 */
async function polishLevel(
  items: Named[],
  kind: "file" | "folder",
  mech: Map<string, string>,
  membersOf: ((key: string) => string[]) | undefined,
  appBody: t.Statement[],
  appRefs: Array<Set<number>>,
  namer?: SplitNamer,
  level?: "top" | "sub"
): Promise<Map<string, string>> {
  const polished = new Map<string, string>();
  for (const it of items) polished.set(it.key, mech.get(it.key) ?? "module");
  if (!namer) return polished;
  await Promise.all(
    [...groupByScope(items).values()].map(async (group) => {
      const requests = group.map((it) => ({
        kind,
        mechanicalStem: mech.get(it.key) ?? "module",
        siblings: group
          .filter((other) => other.key !== it.key)
          .map((other) => mech.get(other.key) ?? ""),
        bindings: segmentBindings(appBody, appRefs, it.s, it.e, 10),
        members: membersOf?.(it.key),
        level
      }));
      const proposals = await namer(requests);
      group.forEach((it, i) => {
        const accepted = acceptForItem(
          proposals?.[i] ?? null,
          requests[i].members
        );
        if (accepted) polished.set(it.key, accepted);
      });
    })
  );
  return polished;
}

/** Distinct polished file names (capped) of the segments `belongs` selects
 * — the members list a folder is named from. */
function collectMemberFiles(
  segments: Segment[],
  filePolished: Map<string, string>,
  belongs: (seg: Segment) => boolean
): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (let idx = 0; idx < segments.length && out.length < 12; idx++) {
    if (!belongs(segments[idx])) continue;
    const name = filePolished.get(`${idx}`) ?? "";
    if (!name || seen.has(name.toLowerCase())) continue;
    seen.add(name.toLowerCase());
    out.push(name);
  }
  return out;
}

/** Per-top-group structure: file count and distinct sub groups, for the
 * structural depth rules (small tops flatten; only-child subs collapse). */
function topGroupInfo(segments: Segment[]): {
  files: Map<string, number>;
  subs: Map<string, Set<string>>;
} {
  const files = new Map<string, number>();
  const subs = new Map<string, Set<string>>();
  for (const seg of segments) {
    const topKey = `${seg.top}`;
    files.set(topKey, (files.get(topKey) ?? 0) + 1);
    let set = subs.get(topKey);
    if (!set) {
      set = new Set<string>();
      subs.set(topKey, set);
    }
    set.add(`${seg.top}/${seg.sub}`);
  }
  return { files, subs };
}

/** A dir holding exactly one file is noise a human never writes — hoist
 * the file a level per round (a one-file top lands at the src/ root, "").
 * Two rounds cover the maximum depth; recounting each round lets hoisted
 * files merge into a parent that then keeps its (now >1) children. */
function hoistSingletonDirs(dirs: string[]): void {
  for (let round = 0; round < 2; round++) {
    const perDir = new Map<string, number>();
    for (const d of dirs) perDir.set(d, (perDir.get(d) ?? 0) + 1);
    for (let i = 0; i < dirs.length; i++) {
      if (dirs[i] !== "" && perDir.get(dirs[i]) === 1) {
        const cutAt = dirs[i].lastIndexOf("/");
        dirs[i] = cutAt === -1 ? "" : dirs[i].slice(0, cutAt);
      }
    }
  }
}

/** Name every app segment `[<folder>[/<subfolder>]/]<file>.js` from each
 * level's dominant binding (LLM-polished when a namer is present),
 * case-safe. Depth is structural, not fixed: small tops emit flat,
 * only-child subs collapse, name-repeating subs collapse, singleton dirs
 * hoist their file a level. */
async function nameSegments(
  segments: Segment[],
  appBody: t.Statement[],
  appRefs: Array<Set<number>>,
  cfg: ClusterConfig,
  namer?: SplitNamer
): Promise<Map<number, string>> {
  const topSpan = new Map<string, [number, number]>();
  const subSpan = new Map<string, [number, number]>();
  for (const seg of segments) {
    widen(topSpan, `${seg.top}`, seg.s, seg.e);
    widen(subSpan, `${seg.top}/${seg.sub}`, seg.s, seg.e);
  }
  const info = topGroupInfo(segments);
  // The sub level survives only where it adds structure: a top with more
  // files than the flat cap AND at least two distinct subs.
  const keepSub = (topKey: string): boolean =>
    (info.files.get(topKey) ?? 0) > cfg.flatTop &&
    (info.subs.get(topKey)?.size ?? 0) > 1;

  const byStart = (
    a: [string, [number, number]],
    b: [string, [number, number]]
  ) => a[1][0] - b[1][0];

  // Bottom-up naming: files FIRST, so every folder is then named from the
  // polished names of what it contains — the evidence a human names
  // folders from — instead of echoing its loudest member's binding.
  const fileItems: Named[] = segments.map((seg, idx) => ({
    key: `${idx}`,
    s: seg.s,
    e: seg.e,
    scope: keepSub(`${seg.top}`) ? `${seg.top}/${seg.sub}` : `${seg.top}`
  }));
  const mechOf = (its: Named[]) =>
    new Map(
      its.map((it) => [it.key, segmentStem(appBody, appRefs, it.s, it.e)])
    );
  const filePolished = await polishLevel(
    fileItems,
    "file",
    mechOf(fileItems),
    undefined,
    appBody,
    appRefs,
    namer
  );

  const subItems: Named[] = [...subSpan]
    .sort(byStart)
    .filter(([k]) => keepSub(k.split("/")[0]))
    .map(([k, [s, e]]) => ({ key: k, s, e, scope: k.split("/")[0] }));
  const subPolished = await polishLevel(
    subItems,
    "folder",
    mechOf(subItems),
    (key) =>
      collectMemberFiles(
        segments,
        filePolished,
        (seg) => `${seg.top}/${seg.sub}` === key
      ),
    appBody,
    appRefs,
    namer,
    "sub"
  );
  const subNames = mergedFolderNames(subItems, subPolished);

  const topItems: Named[] = [...topSpan]
    .sort(byStart)
    .map(([k, [s, e]]) => ({ key: k, s, e, scope: "" }));
  // A top with kept subs is named from its sub-folder names; a flat top
  // from its file names. scope "" puts ALL tops in one joint namer batch.
  const topMembers = (key: string): string[] => {
    if (!keepSub(key)) {
      return collectMemberFiles(
        segments,
        filePolished,
        (seg) => `${seg.top}` === key
      );
    }
    const subsUnder = new Set<string>();
    for (const [subKey, name] of subNames) {
      if (subKey.split("/")[0] === key) subsUnder.add(name);
    }
    return [...subsUnder].slice(0, 12);
  };
  const topPolished = await polishLevel(
    topItems,
    "folder",
    mechOf(topItems),
    topMembers,
    appBody,
    appRefs,
    namer,
    "top"
  );
  const topNames = mergedFolderNames(topItems, topPolished);

  // Each segment's final directory. A subfolder whose tokens add nothing
  // over its parent (`auth/auth`, `abortErrorHandling/abortError`)
  // collapses; one that partially repeats the parent renames to its
  // residual tokens (`auth/authToken` → `auth/token`).
  const dirs = segments.map((seg) => {
    const topKey = `${seg.top}`;
    const top = topNames.get(topKey) ?? "module";
    if (!keepSub(topKey)) return top;
    const sub = subNames.get(`${seg.top}/${seg.sub}`) ?? "module";
    const subFinal = mergeSubIntoTop(top, sub);
    return subFinal === null ? top : `${top}/${subFinal}`;
  });
  hoistSingletonDirs(dirs);

  // Kebab-case the src/ tree (folders cleaned of decoration/verb noise,
  // files simply kebab-cased). This is the LAST naming step — all the
  // camelCase-token merge/collapse/hoist logic above ran first.
  const kebabDirs = dirs.map(cleanDirPath);

  // File basenames dedup case-safely within the FINAL (post-collapse,
  // post-hoist, post-kebab) directory, so merged folders can never hold a
  // duplicate.
  const usedByDir = new Map<string, Set<string>>();
  const path = new Map<number, string>();
  for (let idx = 0; idx < segments.length; idx++) {
    let used = usedByDir.get(kebabDirs[idx]);
    if (!used) {
      used = new Set<string>();
      usedByDir.set(kebabDirs[idx], used);
    }
    const file = uniqueCaseInsensitiveName(
      toKebabCase(filePolished.get(`${idx}`) ?? "file"),
      used,
      ".js"
    );
    path.set(idx, kebabDirs[idx] === "" ? file : `${kebabDirs[idx]}/${file}`);
  }
  return path;
}

/**
 * Clustered per-statement file assignment — the fresh-grouping strategy.
 * Libraries set aside under vendor/ (case-safe), app statements seam-cut
 * into a nested, balanced, named tree under src/. Optionally LLM-polishes
 * new names via `namer`.
 */
export async function assignClustered(
  body: t.Statement[],
  options: {
    config?: Partial<ClusterConfig>;
    namer?: SplitNamer;
    /** The rendered source the statements were parsed from. When present,
     * vendor stems from minified-residue bindings floor to a content
     * hash (never vendor/H.js). */
    code?: string;
  } = {}
): Promise<string[]> {
  const cfg = { ...DEFAULT_CLUSTER_CONFIG, ...options.config };
  const helper = detectCjsHelper(body);
  const usedLib = new Set<string>();
  const assignment = new Array<string>(body.length);
  const appIdx: number[] = [];
  for (let i = 0; i < body.length; i++) {
    const fc = helper ? factoryCallee(body[i]) : null;
    if (fc && fc.callee === helper) {
      const stem = vendorStem(fc.binding, body[i], options.code);
      assignment[i] =
        `${VENDOR_DIR}/${uniqueCaseInsensitiveName(stem, usedLib, ".js")}`;
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
      cfg,
      options.namer
    );
    for (let idx = 0; idx < segments.length; idx++) {
      const p = `${CODE_DIR}/${segPath.get(idx) ?? "file.js"}`;
      for (let a = segments[idx].s; a < segments[idx].e; a++) {
        assignment[appIdx[a]] = p;
      }
    }
  }
  return assignment;
}
