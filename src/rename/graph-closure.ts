import type { UnifiedGraph } from "../analysis/types.js";

/**
 * Asserts the scheduler's input invariant: every id referenced by a
 * dependency or dependent edge resolves to a live graph node or an id
 * already marked done.
 *
 * A dangling edge blocks its dependents forever; they are released only by
 * the deadlock force-break, which dumps ALL blocked nodes unordered —
 * silently discarding leaf-first ordering in exactly the high-transfer runs
 * the pipeline is built for. Failing fast here turns that drift into a
 * signal: nodes must be marked done, never deleted from the graph.
 */
export function assertUnifiedGraphClosure(
  graph: Pick<UnifiedGraph, "nodes" | "dependencies" | "dependents">,
  doneIds: ReadonlySet<string>
): void {
  const missing = new Set<string>();
  const check = (id: string) => {
    if (!graph.nodes.has(id) && !doneIds.has(id)) missing.add(id);
  };
  for (const [id, deps] of graph.dependencies) {
    check(id);
    for (const dep of deps) check(dep);
  }
  for (const [id, deps] of graph.dependents) {
    check(id);
    for (const dep of deps) check(dep);
  }
  if (missing.size > 0) {
    const sample = [...missing].slice(0, 5).join(", ");
    throw new Error(
      `unified graph closure violated: ${missing.size} edge id(s) resolve to ` +
        `neither a live node nor a done id (e.g. ${sample}). Mark nodes done ` +
        `instead of deleting them from the graph.`
    );
  }
}
