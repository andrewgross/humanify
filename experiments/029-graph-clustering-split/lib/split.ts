/**
 * Assemble a clustered split from wrapper-body statements: build the graph,
 * segment into files (P2), later group into folders (P3), then emit byte
 * slices exactly like stable-split's emitFiles (exact bytes, no generator
 * drift). Returns the same {fileContents, order, body} the measure harness
 * scores for baseline, so the two are directly comparable.
 */

import * as t from "@babel/types";
import {
  type FileBudgets,
  type SeamOpts,
  DEFAULT_FILE_BUDGETS,
  crossingCurve,
  deepSeamCuts,
  segmentFiles,
  segmentsOf
} from "./cluster.js";
import {
  balancedTierOrder,
  tieredOrderFromCuts,
  tieredSplitOrder
} from "./folderize.js";
import { buildRefGraph } from "./graph.js";
import { type HierBudgets, DEFAULT_HIER_BUDGETS, hierSplit } from "./hier.js";

export interface Split {
  fileContents: Map<string, string>;
  order: string[];
  body: t.Statement[];
}

/** Emit exact-byte file contents from a per-statement file assignment. */
export function emitBySlice(
  body: t.Statement[],
  order: string[],
  code: string
): Map<string, string> {
  const parts = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    const { start, end } = body[i];
    if (start == null || end == null) {
      throw new Error(`statement ${i} is missing byte offsets`);
    }
    const arr = parts.get(order[i]) ?? [];
    arr.push(code.slice(start, end));
    parts.set(order[i], arr);
  }
  const fileContents = new Map<string, string>();
  for (const [f, arr] of parts) fileContents.set(f, `${arr.join("\n")}\n`);
  return fileContents;
}

/** P2-only clustered split: flat files named by segment index. */
export function clusteredSplit(
  code: string,
  body: t.Statement[],
  budgets: FileBudgets = DEFAULT_FILE_BUDGETS
): Split {
  const g = buildRefGraph(body);
  const cuts = segmentFiles(g, budgets);
  const segs = segmentsOf(g.n, cuts);
  const order = new Array<string>(g.n);
  const pad = String(Math.max(segs.length - 1, 0)).length;
  for (let k = 0; k < segs.length; k++) {
    const name = `file-${String(k).padStart(pad, "0")}.js`;
    for (let i = segs[k][0]; i < segs[k][1]; i++) order[i] = name;
  }
  return { fileContents: emitBySlice(body, order, code), order, body };
}

/** Tiered split (P2 files + P3 seam-depth foldering). */
export function tieredClusteredSplit(
  code: string,
  body: t.Statement[],
  fileBudgets: FileBudgets = DEFAULT_FILE_BUDGETS,
  tiers: number[] = [40, 250]
): Split {
  const g = buildRefGraph(body);
  const order = tieredSplitOrder(g, fileBudgets, tiers);
  return { fileContents: emitBySlice(body, order, code), order, body };
}

export const DEFAULT_SEAM_OPTS: SeamOpts = {
  window: 40,
  minGap: 4,
  targetFiles: 1700,
  // Cap only the truly-huge seam-sparse regions (real src maxes ~5.6k lines);
  // an unsplittable single megastatement can still exceed this.
  maxLines: 2500,
  maxSeg: 60
};

/** Deep-seam split (the mqsweep winner): global-deepest-seam files, tiered
 * into folders by seam depth. The production candidate. */
export function seamTieredSplit(
  code: string,
  body: t.Statement[],
  seam: SeamOpts = DEFAULT_SEAM_OPTS,
  tiers: number[] = [40, 250]
): Split {
  const g = buildRefGraph(body);
  const cuts = deepSeamCuts(g, seam);
  const x = crossingCurve(g, seam.window);
  const order = tieredOrderFromCuts(g.n, x, cuts, tiers);
  return { fileContents: emitBySlice(body, order, code), order, body };
}

/** If stmt is `var X = CALLEE(fn)` with an inline callback taking >= 1 param
 * (the CJS `(exports, module)` shape — ESM inits take 0), return the binding
 * and callee names. Structural so it survives beautification (the shipped
 * identifyBunCjsFactory keys on the minified `{exports:{}}` literal). */
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

/** The Bun CJS factory helper = the identifier that wraps the most modules.
 * null if no identifier wraps >= 2 (nothing library-shaped). */
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

export interface LibraryPartition {
  helper: string | null;
  isLibrary: boolean[];
  /** "libraries/<name>.js" for library statements, else null. */
  libraryFile: Array<string | null>;
}

/** Flag every Bun CJS factory statement and route it to libraries/<name>.js
 * (one file per library, never split). */
export function partitionLibraries(body: t.Statement[]): LibraryPartition {
  const helper = detectCjsHelper(body);
  const isLibrary = new Array<boolean>(body.length).fill(false);
  const libraryFile = new Array<string | null>(body.length).fill(null);
  if (!helper) return { helper, isLibrary, libraryFile };
  const used = new Set<string>();
  for (let i = 0; i < body.length; i++) {
    const fc = factoryCallee(body[i]);
    if (!fc || fc.callee !== helper) continue;
    isLibrary[i] = true;
    let file = `libraries/${fc.binding}.js`;
    for (let k = 2; used.has(file); k++)
      file = `libraries/${fc.binding}-${k}.js`;
    used.add(file);
    libraryFile[i] = file;
  }
  return { helper, isLibrary, libraryFile };
}

export interface LibraryAwareSplit {
  split: Split;
  libraryFiles: number;
  libraryLines: number;
  appFiles: number;
  /** App-only reference graph + order, for app-only metrics. */
  appRefs: Array<Set<number>>;
  appOrder: string[];
}

/**
 * Set libraries aside (libraries/<name>.js, untouched), cluster only the app
 * statements. This mirrors the production vendor-extraction pass, so the
 * measured distribution is app-only — no library megastatement inflating it.
 */
export function libraryAwareBalancedSplit(
  code: string,
  body: t.Statement[],
  seam: SeamOpts = DEFAULT_SEAM_OPTS,
  maxTop = 100,
  maxSub = 25
): LibraryAwareSplit {
  const { isLibrary, libraryFile } = partitionLibraries(body);
  const appIdx: number[] = [];
  for (let i = 0; i < body.length; i++) if (!isLibrary[i]) appIdx.push(i);
  const appBody = appIdx.map((i) => body[i]);

  const g = buildRefGraph(appBody);
  const cuts = deepSeamCuts(g, seam);
  const x = crossingCurve(g, seam.window);
  const appOrder = balancedTierOrder(g.n, x, cuts, maxTop, maxSub);

  const order = new Array<string>(body.length);
  let k = 0;
  for (let i = 0; i < body.length; i++) {
    order[i] = isLibrary[i] ? libraryFile[i]! : appOrder[k++];
  }
  const fileContents = emitBySlice(body, order, code);

  let libraryFiles = 0;
  let libraryLines = 0;
  for (const [rel, content] of fileContents) {
    if (rel.startsWith("libraries/")) {
      libraryFiles++;
      libraryLines += content.split("\n").length - 1;
    }
  }
  return {
    split: { fileContents, order, body },
    libraryFiles,
    libraryLines,
    appFiles: fileContents.size - libraryFiles,
    appRefs: g.refs,
    appOrder
  };
}

/** Deep-seam files + BALANCED foldering (folder size capped). */
export function seamBalancedSplit(
  code: string,
  body: t.Statement[],
  seam: SeamOpts = DEFAULT_SEAM_OPTS,
  maxTop = 100,
  maxSub = 25
): Split {
  const g = buildRefGraph(body);
  const cuts = deepSeamCuts(g, seam);
  const x = crossingCurve(g, seam.window);
  const order = balancedTierOrder(g.n, x, cuts, maxTop, maxSub);
  return { fileContents: emitBySlice(body, order, code), order, body };
}

/** Hierarchical seam split (P2+P3): nested folders + seam-cut files. */
export function hierClusteredSplit(
  code: string,
  body: t.Statement[],
  budgets: HierBudgets = DEFAULT_HIER_BUDGETS
): Split {
  const g = buildRefGraph(body);
  const order = hierSplit(g, budgets);
  return { fileContents: emitBySlice(body, order, code), order, body };
}
