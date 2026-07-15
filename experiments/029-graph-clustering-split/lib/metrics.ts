/**
 * Intrinsic split-quality metrics over the unwrapped Bun wrapper-body
 * statement sequence. Deliberately NOT ARI: we have no source map for the
 * decompiled bundle, and (per exp023) fidelity-to-original is unrecoverable
 * and the wrong target anyway. We measure what a human reviewer feels:
 *
 *   - size distribution (lines/file) vs the real src/ tree
 *   - MQ (Bunch Modularization Quality) — intra vs inter reference density
 *   - folder count + nesting depth
 *   - cyclic files — files in a >1 SCC of the directed file-import graph
 *     (a load-time-cycle proxy the runnable CJS emit cares about)
 *
 * The reference graph is `refs[i] = set of statement indices statement i
 * references` (same shape as stable-split's referenceIndices): edge i→j
 * means "i uses a binding declared by j", i.e. file(i) imports file(j).
 */

export interface SizeStats {
  count: number;
  mean: number;
  median: number;
  min: number;
  max: number;
  stdev: number;
}

/** `wc -l` semantics: count newlines (comparable to the user's histograms). */
export function lineCountsOf(
  fileContents: ReadonlyMap<string, string>
): number[] {
  const counts: number[] = [];
  for (const content of fileContents.values()) {
    let n = 0;
    for (let k = 0; k < content.length; k++) if (content[k] === "\n") n++;
    counts.push(n);
  }
  return counts;
}

export function sizeStats(counts: readonly number[]): SizeStats {
  if (counts.length === 0) {
    return { count: 0, mean: 0, median: 0, min: 0, max: 0, stdev: 0 };
  }
  const sorted = [...counts].sort((a, b) => a - b);
  const n = sorted.length;
  const sum = sorted.reduce((a, b) => a + b, 0);
  const mean = sum / n;
  const median =
    n % 2 ? sorted[(n - 1) / 2] : (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
  // Sample stdev (n-1) to match python statistics.stdev.
  const variance =
    sorted.reduce((a, b) => a + (b - mean) ** 2, 0) / (n > 1 ? n - 1 : 1);
  return {
    count: n,
    mean,
    median,
    min: sorted[0],
    max: sorted[n - 1],
    stdev: Math.sqrt(variance)
  };
}

export interface Bucket {
  lo: number;
  n: number;
}

/** Fixed-width histogram, matching the user's python munging (20 buckets). */
export function histogram(counts: readonly number[], buckets = 20): Bucket[] {
  if (counts.length === 0) return [];
  let min = counts[0];
  let max = counts[0];
  for (const c of counts) {
    if (c < min) min = c;
    if (c > max) max = c;
  }
  const size = (max - min) / buckets || 1;
  const bins: Bucket[] = Array.from({ length: buckets }, (_, i) => ({
    lo: min + i * size,
    n: 0
  }));
  for (const c of counts) {
    let idx = Math.floor((c - min) / size);
    if (idx >= buckets) idx = buckets - 1;
    if (idx < 0) idx = 0;
    bins[idx].n++;
  }
  return bins;
}

/**
 * Bunch Modularization Quality (mirrors src/split/quality.ts on the
 * statement graph). Per file: MF = intra / (intra + 0.5·inter); MQ =
 * mean(MF) over all files. A cross-file edge credits inter to BOTH
 * endpoint files. Range [0,1]; higher = more cohesive.
 */
export function modularizationQuality(
  refs: ReadonlyArray<ReadonlySet<number>>,
  fileOf: readonly string[]
): number {
  const intra = new Map<string, number>();
  const inter = new Map<string, number>();
  const files = new Set(fileOf);
  for (const f of files) {
    intra.set(f, 0);
    inter.set(f, 0);
  }
  for (let i = 0; i < refs.length; i++) {
    const fi = fileOf[i];
    for (const j of refs[i]) {
      const fj = fileOf[j];
      if (fi === fj) {
        intra.set(fi, (intra.get(fi) ?? 0) + 1);
      } else {
        inter.set(fi, (inter.get(fi) ?? 0) + 1);
        inter.set(fj, (inter.get(fj) ?? 0) + 1);
      }
    }
  }
  let sum = 0;
  for (const f of files) {
    const a = intra.get(f) ?? 0;
    const denom = a + 0.5 * (inter.get(f) ?? 0);
    if (denom > 0) sum += a / denom;
  }
  return files.size ? sum / files.size : 0;
}

/** Fraction of reference edges that cross a file boundary (lower = tighter). */
export function crossFileEdgeRatio(
  refs: ReadonlyArray<ReadonlySet<number>>,
  fileOf: readonly string[]
): number {
  let total = 0;
  let cross = 0;
  for (let i = 0; i < refs.length; i++) {
    for (const j of refs[i]) {
      total++;
      if (fileOf[i] !== fileOf[j]) cross++;
    }
  }
  return total ? cross / total : 0;
}

export interface FolderStats {
  fileCount: number;
  folderCount: number;
  maxDepth: number;
  /** depth (number of folder levels above the file) → count of files */
  depthHistogram: Map<number, number>;
  filesPerFolder: number[];
}

export function folderStats(files: readonly string[]): FolderStats {
  const folders = new Map<string, number>();
  const depthHist = new Map<number, number>();
  let maxDepth = 0;
  for (const file of files) {
    const parts = file.split("/");
    const depth = parts.length - 1;
    if (depth > maxDepth) maxDepth = depth;
    depthHist.set(depth, (depthHist.get(depth) ?? 0) + 1);
    const folder = parts.slice(0, -1).join("/") || ".";
    folders.set(folder, (folders.get(folder) ?? 0) + 1);
  }
  return {
    fileCount: files.length,
    folderCount: folders.size,
    maxDepth,
    depthHistogram: depthHist,
    filesPerFolder: [...folders.values()]
  };
}

/**
 * Count files that participate in a >1 strongly-connected component of the
 * directed file-import graph (iterative Tarjan — the graph can have ~2k
 * nodes once we split finely, so no recursion). These are the files that
 * would form import cycles; the runnable CJS emit must keep load-time
 * usage among them deferred or merge them.
 */
export function cyclicFileCount(
  refs: ReadonlyArray<ReadonlySet<number>>,
  fileOf: readonly string[]
): number {
  const idOf = new Map<string, number>();
  for (const f of fileOf) if (!idOf.has(f)) idOf.set(f, idOf.size);
  const nF = idOf.size;
  const adj: Array<Set<number>> = Array.from({ length: nF }, () => new Set());
  for (let i = 0; i < refs.length; i++) {
    const a = idOf.get(fileOf[i])!;
    for (const j of refs[i]) {
      const b = idOf.get(fileOf[j])!;
      if (a !== b) adj[a].add(b);
    }
  }
  const adjArr = adj.map((s) => [...s]);
  const index = new Array<number>(nF).fill(-1);
  const low = new Array<number>(nF).fill(0);
  const onStack = new Array<boolean>(nF).fill(false);
  const sccStack: number[] = [];
  let counter = 0;
  let cyclic = 0;
  for (let root = 0; root < nF; root++) {
    if (index[root] !== -1) continue;
    const work: Array<[number, number]> = [[root, 0]];
    while (work.length) {
      const frame = work[work.length - 1];
      const v = frame[0];
      if (frame[1] === 0) {
        index[v] = low[v] = counter++;
        sccStack.push(v);
        onStack[v] = true;
      }
      let recursed = false;
      while (frame[1] < adjArr[v].length) {
        const w = adjArr[v][frame[1]++];
        if (index[w] === -1) {
          work.push([w, 0]);
          recursed = true;
          break;
        }
        if (onStack[w] && index[w] < low[v]) low[v] = index[w];
      }
      if (recursed) continue;
      if (low[v] === index[v]) {
        let size = 0;
        let w = -1;
        do {
          w = sccStack.pop()!;
          onStack[w] = false;
          size++;
        } while (w !== v);
        if (size >= 2) cyclic += size;
      }
      work.pop();
      if (work.length) {
        const parent = work[work.length - 1][0];
        if (low[v] < low[parent]) low[parent] = low[v];
      }
    }
  }
  return cyclic;
}
