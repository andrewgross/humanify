# 056 — RESULTS: a clean 4-hop walk halves reviewer-facing churn

> ## STATUS: COMPLETE. No pipeline code changed — this is a measurement.
>
> Cold-built 2.1.212 from scratch (no prior), then walked 213→216, each hop
> taking the previous hop's own output as `--prior-version`. That is the
> production shape: one cold release, then a walk.

## TOTAL, first

| hop       |    old walk | clean walk |              delta |
| --------- | ----------: | ---------: | -----------------: |
| 212→213   |      45,381 | **30,886** |            −14,495 |
| 213→214   |       1,736 |  **1,391** |               −345 |
| 214→215   |       2,504 |  **1,567** |               −937 |
| 215→216   |      65,424 | **25,501** |            −39,923 |
| **total** | **115,045** | **59,345** | **−55,700 (−48%)** |

"old walk" is `unpacked-claude-code/versions`, built by the pre-054 pipeline on a
mature 124-version lineage. The clean walk beats it on **every** hop despite
starting from a fresh grouping with no lineage at all.

**No compounding.** Calm hops sit at 1,391 and 1,567; the two large releases
return to that level immediately after.

**Boot verified BOTH ways on all five builds** — `--version` and a live
`-p "say exactly: boot-ok"` round-trip, every one returning `boot-ok`. The walk
script originally checked `--version` only, which proves the module graph loads
but not that the runtime works; the tree can print a version and still die on
the first prompt. Fixed in `walk.sh`, which now reports `NOPROMPT` rather than
`ok` when only the version half passes.

## Per hop, as the walk reported it

| version        | treeLn | renames | carried | abstained | boot |
| -------------- | -----: | ------: | ------: | --------: | ---- |
| 2.1.212 (cold) |      — |       — |       — |         — | ok   |
| 2.1.213        | 30,886 |      30 |      12 |        18 | ok   |
| 2.1.214        |  1,391 |      18 |       0 |        18 | ok   |
| 2.1.215        |  1,567 |      18 |       0 |        18 | ok   |
| 2.1.216        | 25,501 |     250 |     177 |        73 | ok   |

## The carry's coverage is release-size dependent

| hop         | renames | carried |
| ----------- | ------: | ------: |
| 213 (large) |      30 |     40% |
| 214 (calm)  |      18 |      0% |
| 215 (calm)  |      18 |      0% |
| 216 (large) |     250 | **71%** |

Large releases are dominated by inner-local drift and carry well. Calm hops are
tiny and almost entirely top-level, so nothing carries — every abstention on 214
and 215 was `top-level-would-move-an-export-key`, the deliberate gate.

**A claim of mine that did not survive the full run:** after seeing 18/18
top-level on two calm hops I said the export-key limitation bites harder in a
real walk than the gate suggested, and that 057 should be promoted. 216 refutes
it — 71% carried, and the LINES live in the large releases. 057 keeps its rank.
Two hops of 18 renames each were not a population.

## Production shape

- **Cold start**: 1,903 files, 122 folders, 35,025 statements, 51 minted
  leftovers, ~40 min. Needs a **large heap** — it OOM'd at 14GB because with no
  prior all 63,102 functions reach the LLM; run it at ~96GB.
- **Each hop**: ~25 min, 14GB is enough.
- Fresh grouping picks 1,903 files where the 2.1.89-anchored lineage had 1,498.

## Method notes

- Non-destructive: `/work/exp056-clean`. The live `claude-code-history.git`
  (124 commits) and `versions/` were never touched.
- A first attempt seeded 2.1.212 from the OLD pipeline's output instead of
  rebuilding it. That made hop 1 a format transition, not a version bump —
  73,027 lines, and it starved the reconcile to 27 renames because the corpus
  gate abstains below 50% line alignment. `REBASE_PRIOR` in the eval harness
  exists for exactly this. Discarded and re-run cold.
