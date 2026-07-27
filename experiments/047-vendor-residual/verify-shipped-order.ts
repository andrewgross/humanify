/**
 * Gate check before the eval: does the SHIPPED code reproduce the ceiling the
 * offline probes measured?
 *
 * `manifest-field-cost.ts` measured prior-order emission plus `hashOrdinal` at
 * 0 / 20 / 1418 / 189 using its own local copy of the algorithm. This runs the
 * real exported `annotateHashOrdinals` and `orderByPriorManifest` from `src/`
 * over the same four hops. If the two disagree, the probe measured something the
 * pipeline will not do -- which is the failure mode measurement-pitfalls rule 4
 * is about, and it is cheap to rule out here rather than after an hour of eval.
 *
 * Also asserts the two properties the eval cannot see:
 *   - every entry survives (no drop, no duplicate),
 *   - no field other than `hashOrdinal` changes on any entry, because a vendor
 *     NAME change rewrites `src/` require paths (the exp044 blast radius).
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { BunModulesManifestEntry } from "../../src/unpack/adapters/bun.js";
import {
  annotateHashOrdinals,
  orderByPriorManifest
} from "../../src/unpack/manifest-order.js";

type Manifest = { factories: BunModulesManifestEntry[]; [k: string]: unknown };

const HOPS: [string, string, string, number][] = [
  ["85→86", "2.1.85-rebased", "2.1.86", 0],
  ["118→119 🐤", "2.1.118-rebased", "2.1.119", 20],
  ["197→198", "2.1.197-rebased", "2.1.198", 1418],
  ["215→216", "2.1.215-rebased", "2.1.216", 189]
];

const root = process.argv[2] ?? "/work/exp046-bodyinherit";
const dir = mkdtempSync(join(tmpdir(), "verifyorder-"));
const manifestPath = (v: string) => join(root, v, "vendor/_bun-modules.json");
const load = (p: string): Manifest =>
  JSON.parse(readFileSync(p, "utf8")) as Manifest;
const ser = (m: Manifest, factories: BunModulesManifestEntry[]): string =>
  `${JSON.stringify({ ...m, factories }, null, 2)}\n`;

function diffLines(a: string, b: string, tag: string): number {
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

/** Everything except the ordinal — must be untouched by the reorder. */
const identityOf = (e: BunModulesManifestEntry): string =>
  JSON.stringify({ ...e, hashOrdinal: undefined });

console.log("# exp047 — shipped implementation vs measured ceiling");
console.log("");
console.log(
  "| hop | as shipped | prior order (real code) | probe predicted | agrees? |"
);
console.log(
  "| --- | ---------: | ----------------------: | --------------: | ------- |"
);

let allAgree = true;
let allSafe = true;
for (const [label, a, b, predicted] of HOPS) {
  const A = load(manifestPath(a));
  const B = load(manifestPath(b));
  const aAnn = annotateHashOrdinals(A.factories);
  const bAnn = annotateHashOrdinals(B.factories);
  const ordered = orderByPriorManifest(bAnn, aAnn);

  const before = diffLines(
    readFileSync(manifestPath(a), "utf8"),
    readFileSync(manifestPath(b), "utf8"),
    "base"
  );
  const after = diffLines(ser(A, aAnn), ser(B, ordered), "new");
  const agrees = after === predicted;
  if (!agrees) allAgree = false;

  // Safety: same multiset of entries, and no field but hashOrdinal moved.
  const sortedIds = (xs: BunModulesManifestEntry[]) =>
    xs.map(identityOf).sort();
  const preserved =
    ordered.length === B.factories.length &&
    JSON.stringify(sortedIds(ordered)) ===
      JSON.stringify(sortedIds(B.factories));
  if (!preserved) allSafe = false;

  console.log(
    `| ${label} | ${before} | **${after}** | ${predicted} | ${agrees ? "yes" : "**NO**"}${preserved ? "" : " / **ENTRY SET CHANGED**"} |`
  );
}

console.log("");
console.log(
  allAgree
    ? "Shipped code reproduces the measured ceiling exactly."
    : "MISMATCH — probe and shipped code disagree."
);
console.log(
  allSafe
    ? "Entry sets preserved on every hop; no field but `hashOrdinal` differs."
    : "UNSAFE — an entry was dropped, duplicated, or mutated."
);

// Idempotence: re-ordering an already-prior-ordered manifest is a fixed point,
// which is what the eval's self-hop invariant will check end to end.
let fixedPoint = true;
for (const [, , b] of HOPS) {
  const B = load(manifestPath(b));
  const ann = annotateHashOrdinals(B.factories);
  const once = orderByPriorManifest(ann, ann);
  if (JSON.stringify(once) !== JSON.stringify(ann)) fixedPoint = false;
}
console.log(
  fixedPoint
    ? "Self-hop: ordering a manifest against itself is a fixed point."
    : "SELF-HOP VIOLATED — ordering against itself changed the file."
);
process.exit(allAgree && allSafe && fixedPoint ? 0 : 1);
