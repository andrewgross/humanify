/**
 * Tree-shape metrics — how "spread out" is a file tree, quantified.
 *
 * Andrew, 2026-08-15: "I'm sure the academic literature has a score to
 * measure the spread-out-ness of a tree — curious if we can measure
 * something like that to quantify these approaches and benchmark against
 * existing codebases."
 *
 * There are two different questions here and they need different scores.
 * Conflating them is how a layout gets declared good because it is tidy.
 *
 * 1. SHAPE — how the files are distributed over folders, independent of what
 *    the files do. Standard descriptive statistics, all well-established:
 *      - Shannon entropy H of the folder-size distribution, and Pielou's
 *        evenness J = H / ln(k) — J=1 means every folder holds the same
 *        number of files, J→0 means one folder holds nearly everything.
 *        (Pielou 1966, ecology; the same normalisation is standard wherever
 *        a distribution's evenness is scored.)
 *      - Gini coefficient of folder sizes — 0 = perfectly even, →1 = one
 *        folder holds everything. The inequality view of the same fact.
 *      - depth distribution and mean branching factor.
 *    NOTE these are DESCRIPTIVE. A tree can be beautifully even and put
 *    unrelated files together.
 *
 * 2. QUALITY — does the grouping agree with how the code actually depends on
 *    itself? This is the question the software-clustering literature asks,
 *    and the metric to reach for is MODULARITY: intra-folder edges against
 *    what you would expect if the same edges were rewired at random.
 *      - Newman-Girvan modularity Q (Newman & Girvan 2004). Q ≈ 0 means the
 *        grouping is no better than chance; higher is better; real modular
 *        systems typically land in the 0.3-0.7 band.
 *      - The software-specific ancestor is MQ / Modularization Quality from
 *        Mancoridis et al.'s work on software module clustering (the Bunch
 *        tool), which trades intra-connectivity against inter-connectivity.
 *        Q is the better-behaved and more widely validated of the two, so
 *        that is what this computes.
 *    CAUTION on citations: these are named from memory of the literature,
 *    not from a source read today. The formulas implemented here are stated
 *    inline and are what the numbers actually mean — trust those over the
 *    attributions until someone checks the papers.
 *
 * Q is the one that can be BENCHMARKED: compute it on a real hand-organised
 * repo's own folders and its own import graph, and you have a target band
 * that is not a matter of taste.
 */

export interface TreeShape {
  files: number;
  folders: number;
  rootFiles: number;
  rootShare: number;
  maxFolderSize: number;
  medianFolderSize: number;
  meanDepth: number;
  maxDepth: number;
  /** Pielou evenness of folder sizes, 0..1. 1 = every folder equal. */
  evenness: number;
  /** Gini of folder sizes, 0..1. 0 = every folder equal. */
  gini: number;
}

/** Folder of a path, with the root as its own bucket. */
export function folderOf(file: string): string {
  const cut = file.lastIndexOf("/");
  return cut <= 0 ? "" : file.slice(0, cut);
}

export function treeShape(files: string[]): TreeShape {
  const byFolder = new Map<string, number>();
  for (const f of files)
    byFolder.set(folderOf(f), (byFolder.get(folderOf(f)) ?? 0) + 1);
  const sizes = [...byFolder.values()].sort((a, b) => a - b);
  const total = files.length;
  const root = [...byFolder.entries()]
    .filter(([f]) => f === "" || !f.includes("/"))
    .reduce((n, [f, c]) => (f === "" || f === "src" ? n + c : n), 0);

  // Shannon H over the folder-size distribution, normalised by ln(k).
  let h = 0;
  for (const n of sizes) {
    const p = n / total;
    if (p > 0) h -= p * Math.log(p);
  }
  const evenness = sizes.length > 1 ? h / Math.log(sizes.length) : 1;

  // Gini: mean absolute difference over twice the mean, computed on the
  // sorted sizes so it is O(k) rather than O(k^2).
  let cum = 0;
  let weighted = 0;
  sizes.forEach((n, i) => {
    cum += n;
    weighted += (i + 1) * n;
  });
  const gini =
    sizes.length > 1 && cum > 0
      ? (2 * weighted) / (sizes.length * cum) -
        (sizes.length + 1) / sizes.length
      : 0;

  const depths = files.map((f) => f.split("/").length - 1);
  return {
    files: total,
    folders: sizes.length,
    rootFiles: root,
    rootShare: total === 0 ? 0 : root / total,
    maxFolderSize: sizes[sizes.length - 1] ?? 0,
    medianFolderSize: sizes[Math.floor(sizes.length / 2)] ?? 0,
    meanDepth: depths.reduce((a, b) => a + b, 0) / Math.max(1, depths.length),
    maxDepth: Math.max(0, ...depths),
    evenness,
    gini
  };
}

/**
 * Newman-Girvan modularity Q of a folder partition over a dependency graph.
 *
 *   Q = sum over folders c of [ e_c / m  -  (d_c / 2m)^2 ]
 *
 * where m is the edge count, e_c the edges with BOTH ends inside folder c,
 * and d_c the total degree of c's members. The subtracted term is what the
 * same degree sequence would give by chance, which is why Q ≈ 0 means "this
 * grouping tells you nothing" rather than "this grouping is empty".
 *
 * Edges are treated as undirected: for layout, "these two files depend on
 * each other" is symmetric evidence that they belong together.
 */
export function modularity(
  fileOfNode: string[],
  edges: Array<[number, number]>
): number {
  const folder = fileOfNode.map(folderOf);
  const degree = new Array<number>(fileOfNode.length).fill(0);
  let m = 0;
  for (const [a, b] of edges) {
    if (a === b) continue;
    degree[a]++;
    degree[b]++;
    m++;
  }
  if (m === 0) return 0;
  const inside = new Map<string, number>();
  const totalDegree = new Map<string, number>();
  for (const [a, b] of edges) {
    if (a === b) continue;
    if (folder[a] === folder[b])
      inside.set(folder[a], (inside.get(folder[a]) ?? 0) + 1);
  }
  degree.forEach((d, i) => {
    totalDegree.set(folder[i], (totalDegree.get(folder[i]) ?? 0) + d);
  });
  let q = 0;
  for (const [c, d] of totalDegree) {
    q += (inside.get(c) ?? 0) / m - (d / (2 * m)) ** 2;
  }
  return q;
}

export function formatShape(label: string, s: TreeShape, q?: number): string {
  return (
    `${label.padEnd(22)} ${String(s.files).padStart(5)} files  ` +
    `${String(s.folders).padStart(4)} folders  ` +
    `root ${(100 * s.rootShare).toFixed(1).padStart(5)}%  ` +
    `median ${String(s.medianFolderSize).padStart(3)}  ` +
    `max ${String(s.maxFolderSize).padStart(4)}  ` +
    `depth ${s.meanDepth.toFixed(2)}/${s.maxDepth}  ` +
    `even ${s.evenness.toFixed(3)}  gini ${s.gini.toFixed(3)}` +
    (q === undefined ? "" : `  Q ${q.toFixed(3)}`)
  );
}
