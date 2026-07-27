/**
 * Task 2 ceiling, the design that cannot regress by construction.
 *
 * Every content-derived SORT key measured in `manifest-key-sweep.ts` takes
 * 85→86 to 0 and regresses 197→198 and the 118→119 canary. The mechanism: when
 * an entry's sort key changes, sorting relocates it, turning what bundle order
 * charged as an in-place modification into a delete at one position plus an add
 * at another. Bundle order keeps a content change local; sorting scatters it.
 *
 * So the answer is not a sort at all. The brief's own framing was the right one --
 * "position must be RECOVERABLE, not necessarily bundle order" -- and the
 * recoverable position that minimises the diff is the PRIOR RELEASE'S position.
 *
 * The order emitted here is: every entry that the prior manifest also had takes
 * the prior manifest's index; genuinely new entries are appended in bundle order.
 * This reduces to the identity when the prior order already equals bundle order,
 * so it cannot regress a hop -- which is the property the per-hop gate needs and
 * no sort key had.
 *
 * What this predicate tests, in one sentence: the `diff` line count between the
 * two manifests when the FRESH one is re-serialized with its entries in the
 * prior release's order.
 *
 * Three match keys are swept, because which entries count as "the prior manifest
 * also had this" decides how much is recoverable.
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

type Factory = {
  fileName: string;
  name: string;
  structuralHash: string;
  [k: string]: unknown;
};
type Manifest = { factories: Factory[]; [k: string]: unknown };

const load = (p: string): Manifest =>
  JSON.parse(readFileSync(p, "utf8")) as Manifest;

function diffLines(a: string, b: string, dir: string, tag: string): number {
  const pa = join(dir, `${tag}.a.json`);
  const pb = join(dir, `${tag}.b.json`);
  writeFileSync(pa, a);
  writeFileSync(pb, b);
  try {
    execFileSync("diff", [pa, pb], { encoding: "utf8" });
    return 0;
  } catch (err) {
    const out = (err as { stdout?: string }).stdout ?? "";
    return out.split("\n").filter((l) => /^[<>]/.test(l)).length;
  }
}

/**
 * Each entry is a match key with an optional fallback, tried only for entries
 * the primary key leaves unmatched. On 197→198 the 155 content-changed entries
 * have BOTH `fileName` and `structuralHash` rotated -- both are content-derived
 * -- so a fallback on the library `name` is the only link left that can find
 * them, and whether it does is the whole question for that hop.
 */
const MATCH_KEYS: {
  name: string;
  key: (f: Factory) => string;
  fallback?: (f: Factory) => string;
  /** Pair still-unmatched fresh entries with still-unmatched prior entries in bundle order. */
  pairLeftovers?: boolean;
}[] = [
  { name: "fileName", key: (f) => f.fileName },
  { name: "structuralHash", key: (f) => f.structuralHash },
  {
    name: "fileName, then name",
    key: (f) => f.fileName,
    fallback: (f) => f.name
  },
  {
    name: "structuralHash, then name",
    key: (f) => f.structuralHash,
    fallback: (f) => f.name
  },
  {
    name: "name, then fileName",
    key: (f) => f.name,
    fallback: (f) => f.fileName
  },
  {
    name: "fileName, then structuralHash",
    key: (f) => f.fileName,
    fallback: (f) => f.structuralHash
  },
  {
    name: "structuralHash, then name, then leftover pairing",
    key: (f) => f.structuralHash,
    fallback: (f) => f.name,
    pairLeftovers: true
  }
];

const HOPS: [string, string, string][] = [
  ["85→86", "2.1.85-rebased", "2.1.86"],
  ["118→119 🐤", "2.1.118-rebased", "2.1.119"],
  ["197→198", "2.1.197-rebased", "2.1.198"],
  ["215→216", "2.1.215-rebased", "2.1.216"]
];

const root = process.argv[2] ?? "/work/exp046-bodyinherit";
const dir = mkdtempSync(join(tmpdir(), "priororder-"));
const manifestPath = (v: string) => join(root, v, "vendor/_bun-modules.json");

/**
 * Reorder `fresh` to follow `prior`. Entries matched to a prior entry sort by
 * that entry's index. Unmatched entries -- genuinely new libraries, and entries
 * whose match key rotated -- are anchored to the LAST matched entry preceding
 * them in bundle order, so they stay local instead of collecting at the end.
 *
 * Anchoring locally is the whole point. A first version appended unmatched
 * entries to the end of the array, which regressed 197→198 by 494 lines and the
 * 118→119 canary by 8: on that canary only two entries changed, and sending them
 * to the end charged a delete at their old position plus an add at the tail
 * instead of a 20-line in-place edit. Relocating an entry is never cheaper than
 * editing it in place.
 *
 * Matching is one-to-one: a prior entry is consumed by the first fresh entry
 * that claims it, so duplicates cannot collapse onto one slot.
 */
function inheritOrder(
  prior: Factory[],
  fresh: Factory[],
  primary: (f: Factory) => string,
  secondary?: (f: Factory) => string,
  pairLeftovers = false
): Factory[] {
  const pools = new Map<string, number[]>();
  prior.forEach((f, i) => {
    const k = primary(f);
    const l = pools.get(k);
    if (l) l.push(i);
    else pools.set(k, [i]);
  });
  const pools2 = new Map<string, number[]>();
  if (secondary) {
    prior.forEach((f, i) => {
      const k = secondary(f);
      const l = pools2.get(k);
      if (l) l.push(i);
      else pools2.set(k, [i]);
    });
  }

  const claimed = new Set<number>();
  const matchOf = (f: Factory): number | undefined => {
    let idx: number | undefined;
    const p = pools.get(primary(f));
    while (p && p.length > 0) {
      const c = p.shift()!;
      if (!claimed.has(c)) {
        idx = c;
        break;
      }
    }
    if (idx === undefined && secondary) {
      const q = pools2.get(secondary(f));
      while (q && q.length > 0) {
        const c = q.shift()!;
        if (!claimed.has(c)) {
          idx = c;
          break;
        }
      }
    }
    if (idx !== undefined) claimed.add(idx);
    return idx;
  };

  // Optional third pass: whatever is still unmatched on each side is, by
  // construction, the set of entries whose content changed this release. Their
  // relative bundle order is largely preserved across a release, so pairing the
  // two leftover lists in order puts a changed entry back at the index its own
  // prior version held -- an in-place edit rather than a relocation.
  const leftoverPrior: number[] = [];
  if (pairLeftovers) {
    const firstPass = fresh.map((f) => matchOf(f));
    prior.forEach((_, i) => {
      if (!claimed.has(i)) leftoverPrior.push(i);
    });
    let li = 0;
    const placedPre = fresh.map((f, bundleIdx) => {
      const pre = firstPass[bundleIdx];
      if (pre !== undefined) return { f, anchor: pre, after: 0, bundleIdx };
      const take = leftoverPrior[li];
      if (take !== undefined) {
        li += 1;
        claimed.add(take);
        return { f, anchor: take, after: 0, bundleIdx };
      }
      return {
        f,
        anchor: Number.MAX_SAFE_INTEGER,
        after: bundleIdx,
        bundleIdx
      };
    });
    return placedPre
      .sort(
        (x, y) =>
          x.anchor - y.anchor || x.after - y.after || x.bundleIdx - y.bundleIdx
      )
      .map((r) => r.f);
  }

  // Position is a (anchor, tiebreak) pair: a matched entry sits exactly at its
  // prior index; an unmatched one sits just after the last matched entry that
  // preceded it in bundle order, keeping it local.
  let anchor = -1;
  let after = 0;
  const placed = fresh.map((f, bundleIdx) => {
    const priorIdx = matchOf(f);
    if (priorIdx !== undefined) {
      anchor = priorIdx;
      after = 0;
      return { f, anchor: priorIdx, after: 0, bundleIdx };
    }
    after += 1;
    return { f, anchor, after, bundleIdx };
  });

  return placed
    .sort(
      (x, y) =>
        x.anchor - y.anchor || x.after - y.after || x.bundleIdx - y.bundleIdx
    )
    .map((r) => r.f);
}

const ser = (m: Manifest, factories: Factory[]): string =>
  `${JSON.stringify({ ...m, factories }, null, 2)}\n`;

const baseline = new Map<string, number>();
for (const [label, a, b] of HOPS) {
  baseline.set(
    label,
    diffLines(
      readFileSync(manifestPath(a), "utf8"),
      readFileSync(manifestPath(b), "utf8"),
      dir,
      "d"
    )
  );
}

console.log("# Task 2 — prior-order inheritance ceiling");
console.log("");
console.log(
  "Manifest diff lines per hop. The gate is per-hop, so the `worst hop`"
);
console.log("column is what decides: any positive number is a failed gate.");
console.log("");
console.log(
  `| emitted order | ${HOPS.map(([l]) => l).join(" | ")} | TOTAL | worst hop |`
);
console.log(
  `| ------------- | ${HOPS.map(() => "---:").join(" | ")} | ----: | --------: |`
);
const baseTotal = [...baseline.values()].reduce((a, b) => a + b, 0);
console.log(
  `| _as shipped (bundle order)_ | ${HOPS.map(([l]) => baseline.get(l)).join(" | ")} | ${baseTotal} | — |`
);

for (const mk of MATCH_KEYS) {
  const vals: number[] = [];
  let worst = 0;
  for (const [label, a, b] of HOPS) {
    const A = load(manifestPath(a));
    const B = load(manifestPath(b));
    const ordered = inheritOrder(
      A.factories,
      B.factories,
      mk.key,
      mk.fallback,
      mk.pairLeftovers
    );
    const n = diffLines(
      readFileSync(manifestPath(a), "utf8"),
      ser(B, ordered),
      dir,
      "p"
    );
    vals.push(n);
    const delta = n - (baseline.get(label) ?? 0);
    if (delta > worst) worst = delta;
  }
  const total = vals.reduce((x, y) => x + y, 0);
  console.log(
    `| prior order, matched by ${mk.name} | ${vals.join(" | ")} | ${total} | ${worst > 0 ? `**+${worst} REGRESSES**` : "**all down or equal**"} |`
  );
}
