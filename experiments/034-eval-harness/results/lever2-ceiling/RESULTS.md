# Lever 2 ceiling — reloc recoverability (measured 2026-07-22)

**Question.** Of the `reloc` KPIs (same name, different home file — 783 on
the lever1-twin-v2 outputs), how many could a deterministic
positional/neighbor signal keep in place?

**Method.** `ceiling-lever2.ts` — for each reloc'd name, resolve its
declaring statement in the fresh body, test whether the prev/next
statement hashes map unanimously to the name's prior home file (the
candidate "neighbor tier"), and classify the reloc.

## Result (fresh = lever1-twin-v2 outputs, prior = archive)

| pair    | reloc | strict | either | ownUnanim | priorMultiFile | stmtNovel |
| ------- | ----- | ------ | ------ | --------- | -------------- | --------- |
| 85→86   | 284   | 0      | 1      | 0         | 198            | 25        |
| 118→119 | 55    | 0      | 1      | 0         | 27             | 19        |
| 197→198 | 225   | 0      | 15     | 0         | 111            | 78        |
| 215→216 | 219   | 0      | 7      | 0         | 132            | 62        |
| TOTAL   | 783   | 0      | 24     | 0         | 468            | 184       |

## Reading — NO-GO for the planned build

1. **The neighbor-hash signal is dead**: 0 strict / 24 loose of 783 (~3%).
   Relocated names do not sit next to statements that unanimously map to
   their prior home — the neighborhood moved with them or the context
   genuinely changed.
2. **60% (468) is a metric artifact, not pipeline churn**: the name is
   declared in SEVERAL prior files and `analyze.ts` compares only
   `nameToFiles[name][0]` — a reorder of instances flips `[0]` with no
   real file move and no importer-alias churn. The KPI overcounts.
3. **184 have novel statements** — content changed; they moved with real
   change (locality working as intended).
4. The true target population (single-file, unchanged-statement relocs) is
   ~131 names across 4 pairs — small, and the candidate signal recovers
   none of it.

**Action taken:** no reloc tier built (measure-first gate failed).
Follow-up candidate instead: refine the metric — count per-INSTANCE moves
(statement hash unanimous in prior file F, now placed in G) and set-level
changes for multi-file names, so `reloc` stops charging `[0]`-order flips
to the pipeline. Until then, read small reloc deltas on the leaderboard
with this artifact in mind.
