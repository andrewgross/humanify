# 076 — statement placement stability under fossil layout

> **STATUS (2026-08-15): BUILT AND GATED (`npm run check`, 8/8). Sized
> offline; NOT yet validated by a cold scored run.** Sits on top of the
> exp070/073/074/075 stack, so it cannot merge to main independently.

## The finding this answers

exp074 shipped the fossil layout's module-path fix (require-line churn
−65%) and then measured what remained. The layout-independent signal:

| pair      | total churn |       statements that CHANGED FILE |
| --------- | ----------: | ---------------------------------: |
| 85→86     |      50,000 |                                244 |
| 118→119   |      45,273 |                                 29 |
| 197→198   |      63,772 |                                133 |
| 215→216   |      33,471 |                                161 |
| **total** | **192,516** | **567** (pre-fossil layout: **1**) |

Every moved statement drags its lines through the diff twice. exp074's
attribution (unique-line method) put the top cases in two classes —
FOLDER churn and FILE RENAME — on modules that did **not** match.

## Mechanism, read off the code

`assignFossil` does two different things to two populations:

- a **matched** module inherits its prior path VERBATIM. 3,054 of 3,273
  modules on 85→86. It cannot move, by construction.
- an **unmatched** module takes its folder from `inferFossilPlacements`,
  which re-derives the ENTIRE tree from the current bundle's import graph
  (barrel → dominant-importer → co-importer → consensus → collapse).

So a fresh module's folder is computed from a graph, while the tree it
lands in is made of inherited paths. Two consequences, both bad:

1. it can land in a folder the emitted tree does not contain;
2. the derivation is global — one added module shifts consensus votes and
   collapse decisions elsewhere — so it lands somewhere ELSE next release
   and every statement it holds moves.

`redact-url.js` → `redact-url/redact-url.js` (492 lines, exp074) is this
exactly: content and name stable throughout, folder re-inferred.

## Task 0 — sizing, offline (`stability.ts`)

Runs the real extractor/matcher/placer over the RAW MINIFIED bundles for
2.1.85 and 2.1.86. No pipeline run, no LLM, ~2 minutes.

|                                               |                                                  |
| --------------------------------------------- | -----------------------------------------------: |
| fresh modules                                 |                                            3,273 |
| matched (tiers A+B; production also has C)    |                                    3,054 (93.3%) |
| **unmatched — the population that can churn** | **219 modules, 1,402 statements (7.4% of mass)** |
| …with ≥1 MATCHED importer                     |                                  **179 (81.7%)** |
| …importers all unmatched                      |                                               28 |
| …no importers at all                          |                                               12 |

That 81.7% is the lever's reach, measured before writing it.

A second reading, worth recording because it is easy to over-claim: if
the inference were TRUSTED for matched modules too, 24 of 3,054 would
change placement signal and 93 would cross the root/folder boundary, but
folder MEMBERSHIP is identical for only 926 and scattered (<0.5 Jaccard)
for 207. The inference is stable in kind and unstable in detail. It is
not a churn number — matched modules inherit — it is how much the
inference should be trusted where nothing is settled.

## The change

**Fresh modules anchor to the tree that already exists.** Where an
unmatched module's importers have settled paths and ≥50% agree on a
folder, it joins them; the inferred signals decide only the remainder.
Iterated three passes, so a module anchored in one pass is evidence for
the modules it imports. Ties break by folder name; the denominator is ALL
importers, not just settled ones, so one settled importer out of ten does
not get to name the folder.

This deliberately OUTRANKS every inferred signal — they are a guess from
one bundle, the settled tree is evidence from the last release. It is the
same move that fixed module identity one level up: inherit, do not
re-derive. Modules with no settled importer keep their inferred placement
and are counted (`stats.signals.settledAnchor`), never silently re-homed.

## Task 1 — measured effect (`churn.ts`, same offline method)

A/B on 85→86, identical inputs, only the anchor pass differs:

|                                      | baseline |    with anchor |
| ------------------------------------ | -------: | -------------: |
| modules that changed FOLDER          |       50 |         **25** |
| statements in them                   |      680 |        **453** |
| modules anchored to a settled folder |        — | **175 of 219** |

**Folder instability halved.** 175 of the 179 reachable modules anchor,
so the mechanism is at its ceiling — the residual 25 are modules with no
settled neighbour to anchor to, and 44 such modules exist. There is
nothing left to squeeze from this lever.

### Two things this instrument CANNOT show, stated

1. **File-name churn is invisible here and is the other half.** On
   minified input a fresh module's stem is a kebab of a minified
   identifier, which the minifier re-rolls every release — so the name
   half of every fresh path churns unconditionally, and total "moved
   file" reads 872 statements in BOTH legs. That is the input's noise,
   not the pipeline's (rule 11: the gate cannot resolve an effect under
   its own noise floor). Folders derive from the graph, not from names,
   so the folder-only column IS comparable. In production the stem comes
   from a humanified name; exp074 measured real file renames at 465 of
   its top-4 lines, so this class is real and roughly the same size as
   the folder class.
2. **Magnitude does not transfer.** Statement counts stand in for line
   counts, and tier C is off (minified stems), so the unmatched
   population here OVERCOUNTS production's. Direction and relative size
   only.

### A retracted first measurement

The first version of `churn.ts` set the ground-truth identity floor at
overlap ≥ 0.7 and reported **1 moved module in both legs** — a confident
zero. The floor was the bug: a pair the production matcher ACCEPTS
inherits its path and cannot move, so measuring at the matcher's own
floor reports ~0 by construction. The churning population is exactly the
one the matcher DECLINES, and exp074's two worst cases sit at overlap
0.42 and 0.33. Recorded because the wrong number was plausible and
self-consistent.

## What is NOT fixed, and the open question

File renames on unmatched modules. `is-task-active.js` →
`usage-stats-schema.js` (312 lines) and
`render-server-auth-component.js` → `format-reconnect-result-2.js` (153).
The stem comes from the module's first hoisted declaration, so when the
naming stage picks a different word the file renames and every statement
in it moves.

This is the question exp074 left for Andrew and it is unchanged by this
work: `redact-url` and `manage-marketplaces` have a UNIQUE stem on both
sides with exactly one candidate each, but rewrote 33–42% of their
content, so the stem-corroborated tier (floor 0.7) declines them. Is
"this stem appears exactly once among unmatched modules on BOTH sides"
sufficient on its own — process of elimination, nothing else can claim
it — or should a heavily-rewritten file earn a fresh identity? Note that
moving the floor the OTHER way (0.8) destroys exp074's entire 3,204-line
win, so this is a question about the RULE, not about the threshold.

## Folder-collapse threshold: 3 → 2 (Andrew, 2026-08-15)

"What if we change the folder rule to be 1 instead of 3, I think I'm fine
with 2 file folders, let's see." Swept on the real 2.1.85 bundle
(`collapse-sweep.ts`), release 1 where the rules are fully in charge:

|  minFolderFiles | folders | flat root | biggest folder |  evenness |         Q |
| --------------: | ------: | --------: | -------------: | --------: | --------: |
|               1 |     504 |     20.4% |            665 |     0.811 |     0.185 |
| **2 (shipped)** | **497** | **20.8%** |        **679** | **0.808** | **0.184** |
|         3 (was) |     281 |     31.9% |          1,040 |     0.739 |     0.197 |
|               4 |     189 |     39.9% |          1,300 |     0.684 |     0.205 |
|               5 |     135 |     46.7% |          1,522 |     0.636 |     0.193 |

**The flat root falls from 32% to 21% and the biggest folder from 1,040 to
679 files.** 1 and 2 are within a rounding error of each other (504 vs 497
folders), so 2 is the honest choice: it matches "fine with 2-file folders",
and single-file folders are already handled by the hoist above rather than
by the threshold.

**Modularity Q does not adjudicate this.** It spans 0.184–0.205 across the
whole sweep, peaking at min=4 — the opposite direction from the shape
metrics — and the whole range is the same size as the noise Q shows between
two versions of the SAME layout scheme (see exp077). The threshold is a
taste lever, and it is honest to call it that rather than dress it in a
number that cannot separate the options.

## Recorded for later, not done here

A third of the tree (1,040 of 3,261 files) sits flat under `src/` with no
folder. Andrew: "I don't want 1000 files at a single level." Sized in
[exp077](../077-flat-root-grouping/README.md) — 44% of that root is files a
signal DID place and `collapseSmallFolders` then evicted, so it is two
levers rather than one.

## Validation run exp076-r1 (cold, 4 pairs) — RESULT: NOT THE PREDICTED WIN

Frozen at `380a064`, so this leg carries the settled-anchor pass ONLY — not
the singleton hoist and not the `minFolderFiles` 3→2 change, both of which
landed afterwards. Fully valid: exit 0 ×4, boot gate OK ×4, **cache +0 on
every pair** (every prompt live, rule 10), self-hop 86 lines (inside the
72–182 band).

**Gates pass.** `novel` 4,188 / `realLn` 416,377, byte-identical to
exp074-r1 and to the standing references. Nothing real moved.

**Bundle KPIs improved slightly:** noise 2,772 → 2,751, noiseLn 50,361 →
49,526, mints 83 → 79.

**But the metric this experiment exists to move went the WRONG WAY:**

| on-disk (git lines)                        | exp074-r1 | exp076-r1 |       delta |
| ------------------------------------------ | --------: | --------: | ----------: |
| total churn                                |   192,516 |   218,172 | **+25,656** |
| real                                       |    81,015 |    77,337 |      −3,678 |
| noise                                      |    15,288 |    15,328 |         +40 |
| **relocSt (statements that changed FILE)** |   **567** |   **613** |     **+46** |

Per pair, relocSt: 244→268, 29→47, 133→136, 161→162. Worse or flat on all
four.

### The offline instrument and the gate DISAGREE, and neither can currently win

`churn.ts` measured folder churn halving (50 → 25 modules, 680 → 453
statements) on the same version pair. The gate says relocSt rose. Both were
run correctly. Candidate reasons for the divergence, none yet tested:

- the offline sim runs on MINIFIED bundles, so the matcher's stem tier is
  inert and the matched/unmatched split differs from production's;
- it scores only the 2,652 modules it could pair by content, where the gate
  scores every line on disk including vendor and the eager zone;
- it counts statements in fossil modules; `relocSt` counts statements in the
  emitted tree, which is not the same population.

**The deeper problem, and why no conclusion is drawn here: THERE IS NO
MEASURED NOISE FLOOR FOR THIS REGIME.** `noise-bands.json` was measured at
`76c012b` — pre-fossil, a ~1,500-file tree where `relocSt` was ~1 and its
band came out 0. Under a 3,274-file fossil tree neither that band nor
`treeLn`'s 129 can be assumed to survive; exp074 already recorded `treeLn`
+48,194 against the pre-fossil reference. So +46 relocSt and +25,656 churn
are UNADJUDICATED, not refuted and not confirmed. Rule 11 says measure what
the gate reads for two runs that should agree before letting it decide
anything, and that has never been done under fossil layout.

**Action taken:** two cold repeats of the SAME commit at the current head
(`exp076-head-a`, `exp076-head-b`, `/work/exp076-pair.sh`). That scores the
head — which includes the hoist and the 3→2 threshold — and produces the
fossil-regime bands in the same pass. Until those land, this experiment's
effect on placement is UNKNOWN, and the honest reading of exp076-r1 is:
safe, bundle KPIs marginally better, target metric not improved.

**Merge status: NOT MERGEABLE on this evidence.** The stated criterion was
"if the run moves the churn number, merge; if not, stay on the branch and
fix the file-rename half first." It did not move it.

## VERDICT (2026-08-16): REFUTED. The stack regresses the tree diff.

Two cold repeats of the SAME commit (`db1bbb6`) produced the first noise
bands ever measured under fossil layout — and the regime turns out to be
FAR more deterministic than the pre-fossil one:

| KPI         |      band, fossil (db1bbb6) |                        band, pre-fossil (76c012b) |
| ----------- | --------------------------: | ------------------------------------------------: |
| **relocSt** | **0** — 658 vs 658, exactly | 0 (but relocSt was ~1 there, so it meant nothing) |
| treeLn      |                      **14** |                                               129 |
| noise       |                           2 |                                                40 |
| reloc       |                           1 |                                               166 |
| noiseLn     |                         514 |                                             1,085 |

That relocSt band of 0 is now load-bearing: it was measured where relocSt
is in the hundreds, so it says two identical runs agree EXACTLY. Any
movement in that column is real.

**With a floor that applies, the stack's effect is unambiguous:**

|                  |       exp074-r1 |    exp076-r1 (anchor) | exp076-head-a (+hoist, +threshold) |
| ---------------- | --------------: | --------------------: | ---------------------------------: |
| treeLn (band 14) |         192,516 | 218,172 **(+25,656)** |              221,261 **(+28,745)** |
| relocSt (band 0) |             567 |         613 **(+46)** |                      658 **(+91)** |
| reloc (band 1)   |           2,782 |      3,137 **(+355)** |                   3,424 **(+642)** |
| noise (band 2)   |           2,772 |                 2,751 |                2,773 (inside band) |
| novel / realLn   | 4,188 / 416,377 |             identical |                          identical |

Nothing real broke — the hold columns are byte-identical across all three
runs — and **nothing improved outside its band.** The tree a reviewer reads
got 15% MORE churn, and 91 more statements changed file. The settled-anchor
pass alone accounts for ~89% of it.

**So the change is refuted by its own success criterion**, which was fixed
before the run: "success = the layout-independent moved-statement count
falls below exp074's 567." It rose to 658.

### Why the offline instrument said the opposite, and what that costs

`churn.ts` measured folder churn halving. The gate measured tree churn
rising. The likely mechanism, NOT yet tested: the eval's base tree is
re-humanified against the ARCHIVE prior, whose ledger predates fossils and
carries no `fossilModules` — so the base has no matches and places every
module by inference, while the fresh tree anchors to inherited paths. The
anchor then moves modules AWAY from where inference had put them in the
base. Both trees agreeing on inference is worth more than one of them being
"right".

If that is the mechanism, the pass may behave differently on a real version
walk, where every release after the first carries a fossil ledger and both
sides anchor. **That is a hypothesis, and it is exactly the kind that has
been wrong here before** (exp044's correspondence hypothesis, exp071's
detector). It does not license merging: what the gate measures is what a
reviewer sees, and it got worse.

**Next step is a decision, not more code:** either test the asymmetry
hypothesis directly (score a pair whose base was itself built with a fossil
ledger) or revert the settled-anchor pass and keep the hoist and threshold,
which cost +3,089 treeLn between them and are the parts Andrew asked for.

## FINAL (2026-08-16): the walk confirms it. Anchor REVERTED, hoist + threshold KEPT.

Andrew: "we need to do a real version walk to measure this with a fresh run
... when testing big changes like this we always need to do a fresh run from
scratch." Two legs of `walk.sh`, four consecutive versions, one cold start
then every hop inheriting the last, differing in ONE switch:

| pair    | metric                | anchor ON | anchor OFF |
| ------- | --------------------- | --------: | ---------: |
| 214→215 | statements moved file |         2 |      **2** |
| 214→215 | churn lines           |     2,116 |  **2,084** |
| 215→216 | statements moved file |       166 |    **161** |
| 215→216 | churn lines           |    36,031 | **33,297** |

**The anchor costs ~2,700 lines on a busy hop and ~30 on a calm one, and
moves NOTHING in the right direction.** Reverted — code deleted rather than
left behind a switch, because a permanently-off switch is a dead knob.

**The calm pair is its own noise control, and it is a tight one.** Two
INDEPENDENT cold legs, different code, agree to within 32 lines out of 2,084
on 214→215. So the 2,734-line gap on 215→216 is real, not draw variance.

### Two findings worth more than the change was

**1. My asymmetry hypothesis is REFUTED — do not revive it.** Yesterday's
record proposed that the eval's rebased base (built against a pre-fossil
prior, so no module matches) was distorting the verdict. It is not. On
215→216 the real walk gives relocSt 166 / churn 36,031; the eval gave
166 / 36,027 — identical inside the measured band of 14. The eval's base
construction was NOT the problem, and the explanation I found comfortable
was wrong. Recorded because it was plausible, self-consistent, and would
have justified merging a regression.

**2. A CALM HOP COSTS 2 MOVED STATEMENTS AND 2,116 LINES.** This is the
first time the fossil layout has been scored on a pair where little real
change happened, and it is the number the whole campaign is actually about.
All four eval pairs are big-change pairs — 215→216 alone carries 19,247
lines of real change — so every placement number ever argued over here came
from the noisiest available sample. `relocSt` 2 on a calm hop sits beside
the pre-fossil layout's 1, on a tree with 3.2x more files.

The walk is ~2h per leg (72 min for the cold start, ~15 min per warm hop)
and it answers questions the four-pair eval structurally cannot. It should
be part of the standing instrument set, not a one-off.

## LEG 3 — the shipped layout on the same walk. THE STACK IS NOT READY.

The comparison that had never been run: legs 1 and 2 measured a change
WITHIN the fossil layout against itself. This runs the same four versions,
same frozen tree, same order, with `--disable fossil-split` — the layout
main ships today.

| pair           | metric                |    SHIPPED | FOSSIL |
| -------------- | --------------------- | ---------: | -----: |
| 214→215 (calm) | churn lines           |  **1,673** |  2,084 |
| 214→215        | statements moved file |      **0** |      2 |
| 215→216 (busy) | churn lines           | **23,323** | 33,297 |
| 215→216        | statements moved file |      **1** |    161 |
| 215→216        | name-only noise       |    **136** |  1,202 |
| tree           | files                 |      1,918 |  4,851 |

**On a real release chain the shipped layout produces a markedly cleaner
diff.** Not marginally: 10,000 more lines on a busy hop, 160 more moved
statements, 9x the name-only noise.

### Where the extra 9,974 lines come from — decomposed, not guessed

| component         | shipped | fossil |   delta |
| ----------------- | ------: | -----: | ------: |
| file add/remove   |   **0** | 12,902 | +12,902 |
| real (classified) |  23,187 | 19,193 |  −3,994 |
| name-only noise   |     136 |  1,202 |  +1,066 |

The shipped layout has **zero** file add/remove churn because its file set is
fixed at 1,918 on both sides — new code is inserted into existing files. The
fossil layout emits one file per real module, so a new module is a NEW FILE
and its whole body counts as added lines.

Some of that is honest: a genuinely new module IS a new file, and showing it
as one is more truthful than smearing its statements into a neighbour. But
the hop also has **23 RENAMED files**, and a rename reads to git as a whole
file deleted plus a whole file added — the same code counted twice, for zero
real change. At the tree's ~190 lines/file that is roughly 4,400 lines of
pure double-counting.

### So the blocker is FILE NAMING, and it is the half that was never built

The renames come from `moduleStem`: a file is named after its first hoisted
declaration, so when the naming stage picks a different word for that one
identifier, the file renames and every line in it lands in the diff twice.
This experiment fixed the FOLDER half (hoist, threshold) and reverted its
own folder change; the naming half is exactly what Andrew specified on
2026-08-15 — name a file from a sample of everything it declares, and reuse
that name across versions via module identity — and it is untouched.

**The name-only noise column says the same thing independently:** 1,162
against 94. Under the fossil layout the pipeline is churning names an order
of magnitude harder, on a tree where each name change can rename a file.

### Verdict

**Do not merge the stack.** Its steady-state diff is worse than what ships,
and the cause is identified and unbuilt rather than mysterious. The file
naming and reuse work is now a PREREQUISITE for the layout, not a follow-on:
until a file's name survives a version hop, one file per module costs more
than it returns.

What survives from this experiment: the singleton-folder hoist, the
`minFolderFiles` 3→2 threshold, `walk.sh` itself, and the finding that a
calm hop under the fossil layout costs 2 moved statements — the layout's
placement really is nearly stable. Placement was never the problem. Naming
is.

## Validation still owed

A cold scored run on the four pairs with the exp070/073/074/075 stack:
`novel`/`realLn` byte-exact, boot ×4, cache +0, self-hop ≤ 1 ask, and
the layout-independent moved-statement count below exp074's 567.
