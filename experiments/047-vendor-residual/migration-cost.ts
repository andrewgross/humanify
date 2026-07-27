/**
 * The number `REBASE_PRIOR=1` structurally cannot show: the ONE-OFF cost of
 * migrating the existing history to the current manifest format.
 *
 * The eval's rebased mode regenerates the v-1 side with the current pipeline, so
 * both sides of every diff share a format and the KPI reports the STEADY-STATE
 * per-release churn. That is the right instrument for sizing a recurring noise
 * lever, but it hides a real cost: `vendor/_bun-modules.json` is a serialized
 * artifact committed to `claude-code-history.git`, and the first release emitted
 * after a format change diffs against a prior commit that predates it.
 *
 * This measures that first-commit cost directly: archive v-1 manifest against
 * the freshly emitted v manifest, no rebase.
 *
 * WHAT THE PER-FIELD COLUMNS ACTUALLY TEST (rule 3, in one sentence): each is
 * the number of `diff` lines that DISAPPEAR when that field is stripped from
 * both sides — which includes the re-alignment `diff` does once an entry block
 * changes height, NOT just the field's own lines. `factoryVar` happens to be
 * clean (6,332 = exactly one line on each of 1,592+1,493+1,623+1,624 entries),
 * but `hashOrdinal` reads 4,335 against a DIRECT cost of only 1,445
 * (302+296+423+424 annotated entries, one line each). Read the columns as an
 * upper bound on each field's contribution, never as the field's line count.
 *
 * IMPORTANT — the cost is CUMULATIVE, not exp047's alone. The archive manifests
 * still carry `factoryVar` on every entry (1,592 / 1,493 / 1,623 / 1,624) and no
 * `hashOrdinal`, so a diff against them charges exp046's field REMOVAL and
 * exp047's field ADDITION and reordering together. The breakdown below
 * attributes each so neither experiment is blamed for the other's lines.
 */
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

type Factory = Record<string, unknown> & { structuralHash: string };
type Manifest = { factories: Factory[] } & Record<string, unknown>;

const ARCHIVE =
  process.env.EVAL_PRIORS_BASE ??
  "/Users/andrewgross/Development/unpacked-claude-code/versions";

const HOPS: [string, string, string][] = [
  ["85→86", "2.1.85", "2.1.86"],
  ["118→119 🐤", "2.1.118", "2.1.119"],
  ["197→198", "2.1.197", "2.1.198"],
  ["215→216", "2.1.215", "2.1.216"]
];

const workRoot = process.argv[2];
if (!workRoot) {
  console.error(
    "usage: migration-cost.ts <freshTreeRoot e.g. /work/exp047-cold>"
  );
  process.exit(1);
}

const dir = mkdtempSync(join(tmpdir(), "migcost-"));
const load = (p: string): Manifest =>
  JSON.parse(readFileSync(p, "utf8")) as Manifest;

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

const strip = (m: Manifest, fields: string[]): Manifest => ({
  ...m,
  factories: m.factories.map((f) => {
    const c = { ...f };
    for (const k of fields) delete c[k];
    return c;
  })
});
const ser = (m: Manifest) => `${JSON.stringify(m, null, 2)}\n`;

console.log("# exp047 — one-off migration cost (archive base, no rebase)");
console.log("");
console.log(
  "What a reviewer sees on the FIRST commit emitted after the format change,"
);
console.log(
  "which the rebased gate cannot show. Cumulative over exp046 + exp047."
);
console.log("");
console.log(
  "| hop | archive v-1 vs fresh v | `factoryVar` removal, upper bound | `hashOrdinal`, upper bound | residual: order + names |"
);
console.log("| --- | ---: | ---: | ---: | ---: |");

let totals = [0, 0, 0, 0];
for (const [label, from, to] of HOPS) {
  const aPath = join(
    ARCHIVE,
    `claude-code-${from}`,
    "vendor/_bun-modules.json"
  );
  const bPath = join(workRoot, to, "vendor/_bun-modules.json");
  if (!existsSync(aPath) || !existsSync(bPath)) {
    console.log(`| ${label} | (missing tree) | | | |`);
    continue;
  }
  const A = load(aPath);
  const B = load(bPath);

  const full = diffLines(
    readFileSync(aPath, "utf8"),
    readFileSync(bPath, "utf8"),
    "full"
  );

  // Isolate each field's contribution: strip it from BOTH sides and re-diff.
  // The drop is what that field alone was charging.
  const noFactoryVar = diffLines(
    ser(strip(A, ["factoryVar"])),
    ser(strip(B, ["factoryVar"])),
    "nfv"
  );
  const noOrdinal = diffLines(
    ser(strip(A, ["hashOrdinal"])),
    ser(strip(B, ["hashOrdinal"])),
    "nho"
  );
  const neither = diffLines(
    ser(strip(A, ["factoryVar", "hashOrdinal"])),
    ser(strip(B, ["factoryVar", "hashOrdinal"])),
    "none"
  );

  const fvCost = full - noFactoryVar;
  const hoCost = full - noOrdinal;
  console.log(
    `| ${label} | **${full}** | ${fvCost} | ${hoCost} | ${neither} |`
  );
  totals = [
    totals[0]! + full,
    totals[1]! + fvCost,
    totals[2]! + hoCost,
    totals[3]! + neither
  ];
}
console.log(
  `| **TOTAL** | **${totals[0]}** | ${totals[1]} | ${totals[2]} | ${totals[3]} |`
);
console.log("");
console.log(
  "Per-field columns are UPPER BOUNDS -- they include diff re-alignment, not just"
);
console.log(
  "the field's own lines. hashOrdinal's DIRECT cost is 1,445 lines (one per"
);
console.log("annotated entry), against the 4,335 upper bound printed above.");
console.log("");
console.log(
  "The residual column is entry ORDER plus any vendor NAME differences between"
);
console.log(
  "the archive's pipeline and the current one — it is NOT a format cost, and it"
);
console.log("is paid once, on the first commit after regeneration.");
