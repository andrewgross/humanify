/**
 * Dependency-wave profile of the rename graph: how the pending nodes would
 * partition under barrier ("wave") scheduling, where wave N is every node
 * whose dependencies settled in waves < N.
 *
 * This is the go/no-go measurement for order-independent prompt context
 * (034 README, "the aggregate hides the churn"): making every wave's
 * prompts read one frozen AST snapshot and applying renames only at the
 * wave boundary removes completion-order nondeterminism — IF the wave
 * structure is wide and shallow. Many tiny waves would serialize the run;
 * few wide waves cost almost nothing. The profile answers which world the
 * real graph lives in, using exactly the dispatch loop's readiness rule
 * (all `graph.dependencies` edges settled).
 */

export interface WaveProfile {
  /** Number of waves needed to drain the pending set. */
  waves: number;
  /** Pending-node count per wave, in order. */
  sizes: number[];
  /** Total pending nodes profiled (excludes pre-settled seeds). */
  pending: number;
  /** Nodes on dependency cycles a barrier scheduler would need the
   * existing deadlock-break tiers for. */
  deadlocked: number;
}

interface WaveGraph {
  nodes: Map<string, unknown>;
  dependencies: Map<string, Set<string>>;
}

export function computeWaveProfile(
  graph: WaveGraph,
  doneIds: ReadonlySet<string>
): WaveProfile {
  const done = new Set(doneIds);
  const pendingIds = new Set<string>();
  for (const id of graph.nodes.keys()) {
    if (!done.has(id)) pendingIds.add(id);
  }
  const pending = pendingIds.size;
  const sizes: number[] = [];

  const isReady = (id: string): boolean => {
    const deps = graph.dependencies.get(id);
    if (!deps) return true;
    for (const dep of deps) {
      if (!done.has(dep)) return false;
    }
    return true;
  };

  while (pendingIds.size > 0) {
    const wave: string[] = [];
    for (const id of pendingIds) {
      if (isReady(id)) wave.push(id);
    }
    if (wave.length === 0) break; // cycle — deadlock-break territory
    sizes.push(wave.length);
    for (const id of wave) {
      done.add(id);
      pendingIds.delete(id);
    }
  }

  return {
    waves: sizes.length,
    sizes,
    pending,
    deadlocked: pendingIds.size
  };
}

/** Compact log line: wave count, head/tail sizes, deadlock residue. */
export function formatWaveProfile(profile: WaveProfile): string {
  const shown =
    profile.sizes.length <= 12
      ? profile.sizes.join(",")
      : `${profile.sizes.slice(0, 6).join(",")}…${profile.sizes.slice(-3).join(",")}`;
  return (
    `wave-profile: ${profile.pending} pending nodes drain in ` +
    `${profile.waves} waves [${shown}]` +
    (profile.deadlocked > 0 ? `; ${profile.deadlocked} on cycles` : "")
  );
}
