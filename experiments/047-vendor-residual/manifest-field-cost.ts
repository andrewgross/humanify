/**
 * Task 2 — what the ORDER-RECOVERY FIELD itself costs.
 *
 * Emitting the manifest in the prior release's order requires bundle order to
 * stay recoverable, because `priorNameFor` disambiguates same-`structuralHash`
 * groups by a factory's position within its group IN BUNDLE ORDER
 * (`bun-module-classification.ts:197-208`), and 129-145 groups per hop have
 * members that disagree about `name` — so losing that tie-break would misname
 * them and rewrite `src/` require paths. The brief proposed storing the bundle
 * index as a field.
 *
 * But a bundle index is exactly the shape of the field exp046 DELETED:
 * `factoryVar` churned on every entry of every release because Bun rerolls it,
 * and a bundle index churns for the same reason — 1,132 of 1,592 entries land at
 * a different index on 85→86. A field that records the churn cannot be free.
 * This measures three variants rather than assuming any of them:
 *
 *   A. `bundleIndex` on every entry.
 *   B. `bundleIndex` only on entries in a >= 2-member same-hash group — the only
 *      entries whose tie-break is ever consulted.
 *   C. `hashOrdinal`: the entry's position WITHIN its structuralHash group, on
 *      those same entries. This is what `priorNameFor` actually indexes with; it
 *      is 0 for the 1,200-odd singleton groups and stays 0,1 for a stable pair,
 *      so it should be near-inert across releases.
 *
 * Reported as the manifest diff for prior-order emission WITH the field, so the
 * number is the net effect and not the field in isolation.
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

/** Ordinal of each factory within its structuralHash group, in bundle order. */
function hashOrdinals(fs: Factory[]): Map<Factory, number> {
  const seen = new Map<string, number>();
  const out = new Map<Factory, number>();
  for (const f of fs) {
    const n = seen.get(f.structuralHash) ?? 0;
    out.set(f, n);
    seen.set(f.structuralHash, n + 1);
  }
  return out;
}

function groupSizes(fs: Factory[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const f of fs)
    m.set(f.structuralHash, (m.get(f.structuralHash) ?? 0) + 1);
  return m;
}

type Variant =
  | "none"
  | "bundleIndex-all"
  | "bundleIndex-ambiguous"
  | "hashOrdinal-ambiguous";

/** Annotate factories, in BUNDLE order, with the chosen recovery field. */
function annotate(fs: Factory[], variant: Variant): Factory[] {
  const ord = hashOrdinals(fs);
  const sizes = groupSizes(fs);
  return fs.map((f, i) => {
    const ambiguous = (sizes.get(f.structuralHash) ?? 1) >= 2;
    switch (variant) {
      case "none":
        return f;
      case "bundleIndex-all":
        return { ...f, bundleIndex: i };
      case "bundleIndex-ambiguous":
        return ambiguous ? { ...f, bundleIndex: i } : f;
      case "hashOrdinal-ambiguous":
        return ambiguous ? { ...f, hashOrdinal: ord.get(f) ?? 0 } : f;
    }
  });
}

/** Prior-order emission: the winning scheme from manifest-priororder-ceiling.ts. */
function inheritOrder(prior: Factory[], fresh: Factory[]): Factory[] {
  const pool = (key: (f: Factory) => string) => {
    const m = new Map<string, number[]>();
    prior.forEach((f, i) => {
      const k = key(f);
      const l = m.get(k);
      if (l) l.push(i);
      else m.set(k, [i]);
    });
    return m;
  };
  const byHash = pool((f) => f.structuralHash);
  const byName = pool((f) => f.name);
  const claimed = new Set<number>();
  const take = (m: Map<string, number[]>, k: string): number | undefined => {
    const l = m.get(k);
    while (l && l.length > 0) {
      const c = l.shift()!;
      if (!claimed.has(c)) return c;
    }
    return undefined;
  };

  const first = fresh.map((f) => {
    let idx = take(byHash, f.structuralHash);
    if (idx === undefined) idx = take(byName, f.name);
    if (idx !== undefined) claimed.add(idx);
    return idx;
  });
  const leftover: number[] = [];
  prior.forEach((_, i) => {
    if (!claimed.has(i)) leftover.push(i);
  });
  let li = 0;
  return fresh
    .map((f, bundleIdx) => {
      const pre = first[bundleIdx];
      if (pre !== undefined) return { f, anchor: pre, bundleIdx };
      const t = leftover[li];
      if (t !== undefined) {
        li += 1;
        return { f, anchor: t, bundleIdx };
      }
      return { f, anchor: Number.MAX_SAFE_INTEGER, bundleIdx };
    })
    .sort((x, y) => x.anchor - y.anchor || x.bundleIdx - y.bundleIdx)
    .map((r) => r.f);
}

const HOPS: [string, string, string][] = [
  ["85→86", "2.1.85-rebased", "2.1.86"],
  ["118→119 🐤", "2.1.118-rebased", "2.1.119"],
  ["197→198", "2.1.197-rebased", "2.1.198"],
  ["215→216", "2.1.215-rebased", "2.1.216"]
];
const root = process.argv[2] ?? "/work/exp046-bodyinherit";
const dir = mkdtempSync(join(tmpdir(), "fieldcost-"));
const manifestPath = (v: string) => join(root, v, "vendor/_bun-modules.json");
const ser = (m: Manifest, factories: Factory[]): string =>
  `${JSON.stringify({ ...m, factories }, null, 2)}\n`;

const baseline: number[] = HOPS.map(([, a, b]) =>
  diffLines(
    readFileSync(manifestPath(a), "utf8"),
    readFileSync(manifestPath(b), "utf8"),
    dir,
    "d"
  )
);

console.log("# Task 2 — cost of the order-recovery field");
console.log("");
console.log(
  "Manifest diff lines, prior-order emission, with each candidate field."
);
console.log(
  "Both sides carry the field, so this is the steady state, not the one-off"
);
console.log("release that introduces it.");
console.log("");
console.log(
  `| variant | ${HOPS.map(([l]) => l).join(" | ")} | TOTAL | worst hop |`
);
console.log(
  `| ------- | ${HOPS.map(() => "---:").join(" | ")} | ----: | --------: |`
);
console.log(
  `| _as shipped: bundle order, no field_ | ${baseline.join(" | ")} | ${baseline.reduce((a, b) => a + b, 0)} | — |`
);

for (const variant of [
  "none",
  "hashOrdinal-ambiguous",
  "bundleIndex-ambiguous",
  "bundleIndex-all"
] as Variant[]) {
  const vals: number[] = [];
  let worst = 0;
  HOPS.forEach(([, a, b], i) => {
    const A = load(manifestPath(a));
    const B = load(manifestPath(b));
    const aAnn = annotate(A.factories, variant);
    const bAnn = annotate(B.factories, variant);
    // Prior side keeps ITS emitted order (already prior-ordered in steady state);
    // fresh side is reordered to follow it.
    const ordered = inheritOrder(aAnn, bAnn);
    const n = diffLines(ser(A, aAnn), ser(B, ordered), dir, "v");
    vals.push(n);
    const delta = n - (baseline[i] ?? 0);
    if (delta > worst) worst = delta;
  });
  const label =
    variant === "none"
      ? "prior order, NO field (breaks the tie-break)"
      : `prior order + \`${variant}\``;
  console.log(
    `| ${label} | ${vals.join(" | ")} | ${vals.reduce((a, b) => a + b, 0)} | ${worst > 0 ? `**+${worst} REGRESSES**` : "**all down or equal**"} |`
  );
}

// How many entries would carry the field at all?
console.log("");
console.log("| hop | entries | in >= 2-member hash groups | share |");
console.log("| --- | ------: | -------------------------: | ----: |");
for (const [label, , b] of HOPS) {
  const B = load(manifestPath(b));
  const sizes = groupSizes(B.factories);
  const amb = B.factories.filter(
    (f) => (sizes.get(f.structuralHash) ?? 1) >= 2
  ).length;
  console.log(
    `| ${label} | ${B.factories.length} | ${amb} | ${((amb / B.factories.length) * 100).toFixed(1)}% |`
  );
}
