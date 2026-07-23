/**
 * Identity-recovery ceiling: of the fns the cascade left AMBIGUOUS, how
 * many become uniquely pairable under IDENTITY-grade keys — the sets of
 * MATCHED callers / MATCHED callees translated through the match map —
 * where the blurred tiers (shapes, callee-hash SETS) tied?
 *
 * Gates mirror the production precision ladder: a recovery counts only
 * when the translated key selects exactly ONE candidate in the pool,
 * the key is non-empty, and no other ambiguous prior claims the same
 * candidate (injectivity both ways).
 *
 *   HUMANIFY_AMBIGUITY_PROBE=/tmp/probe.json <pipeline run>
 *   npx tsx ceiling-identity-recovery.ts /tmp/probe.json
 */
import * as fs from "node:fs";
import type { AmbiguityProbe } from "../../src/prior-version/ambiguity-probe.js";

const fmt = (n: number) => n.toLocaleString("en-US");

type KeyKind = "callee" | "caller" | "combined";

function main() {
  const probe = JSON.parse(
    fs.readFileSync(process.argv[2], "utf8")
  ) as AmbiguityProbe;
  const matches = new Map(Object.entries(probe.matches));
  const ambiguous = Object.entries(probe.ambiguous);

  const translate = (ids: string[]): string[] | null => {
    const out: string[] = [];
    for (const id of ids) {
      const mapped = matches.get(id);
      if (mapped) out.push(mapped);
    }
    return out.length > 0 ? out.sort() : null;
  };
  const freshKey = (ids: string[], all: Set<string>): string[] | null => {
    const out = ids.filter((id) => all.has(id)).sort();
    return out.length > 0 ? out : null;
  };
  // Fresh ids that are match TARGETS (identity-resolved on the fresh side).
  const matchedFresh = new Set(matches.values());

  const tryKey = (
    kind: KeyKind
  ): { recovered: Map<string, string>; withKey: number } => {
    const proposals = new Map<string, string>();
    const claimants = new Map<string, number>();
    let withKey = 0;
    for (const [priorId, pool] of ambiguous) {
      const pe = probe.prior[priorId];
      if (!pe) continue;
      const want =
        kind === "callee"
          ? translate(pe.callees)
          : kind === "caller"
            ? translate(pe.callers)
            : (() => {
                const ce = translate(pe.callees);
                const cr = translate(pe.callers);
                if (!ce && !cr) return null;
                return [...(ce ?? []), "|", ...(cr ?? [])];
              })();
      if (!want) continue;
      withKey++;
      const wantStr = want.join(",");
      let hit: string | null = null;
      let hits = 0;
      for (const candidate of pool) {
        const fe = probe.fresh[candidate];
        if (!fe) continue;
        const got =
          kind === "callee"
            ? freshKey(fe.callees, matchedFresh)
            : kind === "caller"
              ? freshKey(fe.callers, matchedFresh)
              : (() => {
                  const ce = freshKey(fe.callees, matchedFresh);
                  const cr = freshKey(fe.callers, matchedFresh);
                  if (!ce && !cr) return null;
                  return [...(ce ?? []), "|", ...(cr ?? [])];
                })();
        if (got && got.join(",") === wantStr) {
          hit = candidate;
          hits++;
        }
      }
      if (hit && hits === 1) {
        proposals.set(priorId, hit);
        claimants.set(hit, (claimants.get(hit) ?? 0) + 1);
      }
    }
    const recovered = new Map(
      [...proposals].filter(([, fresh]) => claimants.get(fresh) === 1)
    );
    return { recovered, withKey };
  };

  console.log(`TOTAL ambiguous prior fns: ${fmt(ambiguous.length)}`);
  const results = new Map<KeyKind, Map<string, string>>();
  for (const kind of ["callee", "caller", "combined"] as KeyKind[]) {
    const { recovered, withKey } = tryKey(kind);
    results.set(kind, recovered);
    console.log(
      `  ${kind.padEnd(9)} identity key: ${fmt(recovered.size).padStart(7)} uniquely recoverable  (${fmt(withKey)} had a non-empty key)`
    );
  }
  const union = new Set<string>();
  for (const r of results.values()) for (const k of r.keys()) union.add(k);
  console.log(`  union across keys:  ${fmt(union.size)}`);
  console.log(
    `REMAINING ambiguous after identity recovery: ${fmt(ambiguous.length - union.size)}`
  );

  console.log("\nsample recoveries (combined key):");
  for (const [priorId, freshId] of [...(results.get("combined") ?? [])].slice(
    0,
    8
  )) {
    console.log(
      `  ${probe.prior[priorId]?.head ?? priorId}  ->  ${probe.fresh[freshId]?.head ?? freshId}`
    );
  }
}

main();
