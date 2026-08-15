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

## Recorded for later, not done here

A third of the tree (1,040 of 3,261 files) sits flat under `src/` with no
folder. Andrew: "I don't want 1000 files at a single level." Sized in
[exp077](../077-flat-root-grouping/README.md) — 44% of that root is files a
signal DID place and `collapseSmallFolders` then evicted, so it is two
levers rather than one.

## Validation still owed

A cold scored run on the four pairs with the exp070/073/074/075 stack:
`novel`/`realLn` byte-exact, boot ×4, cache +0, self-hop ≤ 1 ask, and
the layout-independent moved-statement count below exp074's 567.
