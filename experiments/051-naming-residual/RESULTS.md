# 051 — RESULTS: the naming residual audited, and the arc closed

> Scored on the **committed post-050 cold 4-pair run** (`/work/exp050-cold`), the
> same trees the standing table came from. No pipeline run was needed and none
> was made: every number here is a re-reading of an existing gate's output, so
> rule 10 does not apply — there is no LLM call anywhere in this experiment.
>
> **Outcome: no pipeline code shipped. The noise arc is closed.**

## TOTAL, first

|                                                            | git lines | share of naming |
| ---------------------------------------------------------- | --------: | --------------: |
| **naming residual, 4 hops**                                | **7,440** |            100% |
| — irreducible without rewriting code                       |     3,374 |           45.3% |
| — the LLM naming floor                                     |     2,920 |           39.2% |
| — reducible, mechanism identified                          | **1,138** |       **15.3%** |
| **REMAINING after every lever this arc could still build** | **6,302** |           84.7% |

The 1,138 reducible lines are three separate mechanisms of 370 / 468 / 300 lines
across **four** hops, each concentrated on **one** hop. The harness's per-hop
`src/` resolution floor is **±2,800 git lines** (rule 11). Every one of them is
an order of magnitude below the instrument that would have to judge it.

## Task 0 — the brief's central hypothesis is REFUTED

The brief proposed that the naming bucket is inflated by `classifyFile` pairing
same-hash statements **FIFO within a file**, manufacturing renames out of
unrelated statements. `pairing-audit.ts` scores the same trees both ways.

The free, threshold-free half of the question first: **did the FIFO rule ever
have a choice?** An instance whose hash bucket held one prior candidate was not
"paired FIFO" in any meaningful sense — there was nothing else to pair with.

| hop       | naming (FIFO) | FORCED (1 candidate) |    CHOSEN (>1) |
| --------- | ------------: | -------------------: | -------------: |
| 85→86     |         5,698 |                5,564 |        **134** |
| 118→119   |           586 |                  552 |         **34** |
| 197→198   |           668 |                  616 |         **52** |
| 215→216   |           488 |                  476 |         **12** |
| **total** |     **7,440** |    **7,208 (96.9%)** | **232 (3.1%)** |

**96.9% of the naming mass comes from pairings the rule did not choose.** The
mispairing hypothesis cannot account for the bucket.

Corroborated pairing (best token overlap within the hash bucket, accepted above a
threshold; refused pairs fall to the real-change path) for completeness:

| hop       |      FIFO | corr @0.30 | corr @0.50 | corr @0.70 |
| --------- | --------: | ---------: | ---------: | ---------: |
| 85→86     |     5,698 |      5,558 |      5,366 |      5,148 |
| 118→119   |       586 |        580 |        580 |        514 |
| 197→198   |       668 |        660 |        586 |        488 |
| 215→216   |       488 |        484 |        466 |        412 |
| **total** | **7,440** |  **7,282** |  **6,998** |  **6,562** |

The delta (−158 / −442 / −878) **exceeds the entire CHOSEN mass**, which is the
tell: corroboration is not re-picking within buckets, it is REFUSING forced pairs
whose two sides share few tokens. That is a different claim from the brief's, it
is threshold-dependent across a 5.5× range, and reclassifying a line from naming
to real does not remove it from the diff — git prints it either way. **It changes
whether the line is reducible, not whether it is printed.**

`diff-composition` keeps FIFO as its default, byte-for-byte, pinned by a test
(`diff-composition.test.ts`). The corroborated rule is available behind
`{ pairing: "corroborated" }` so the correction stays visible and reversible.

## Task 1 — what the residual actually is, attributed PER LINE

The instance-level classifiers were the wrong unit and said so out loud:
`diff-composition` charges an instance the lines a line diff prints, and those
lines can be anywhere in a 492-line statement. Measured: **520 lines on 85→86 sit
in statements whose init-call lists are byte-identical** — the instance's biggest
identifier change was not on any billed line.

`line-ledger.ts` walks the line diff of every naming instance and attributes each
charged line to what changed **on that line**. It accounts **7,440 of 7,440** —
it reconciles exactly with the number it is decomposing.

| bucket               |     85→86 | 118→119 | 197→198 | 215→216 |    calm 3 | **all 4** |
| -------------------- | --------: | ------: | ------: | ------: | --------: | --------: |
| TARGET-CHANGED       |     3,170 |      36 |     110 |      58 |       204 | **3,374** |
| LOCAL-DRIFT          |     2,176 |     128 |     468 |     148 |       744 | **2,920** |
| PRIVATE FIELD (`#x`) |       316 |      84 |      38 |      30 |       152 |   **468** |
| ALIAS-ONLY           |        24 |     336 |       2 |       8 |       346 |   **370** |
| MOVED-DECL           |         8 |       0 |      48 |     244 |       292 |   **300** |
| UNALIGNED            |         4 |       2 |       2 |       0 |         4 |     **8** |
| **naming**           | **5,698** | **586** | **668** | **488** | **1,742** | **7,440** |

Ranking inside a line is conservative — TARGET-CHANGED beats MOVED-DECL beats
ALIAS-ONLY — so the two reducible buckets are LOWER bounds, the safe direction
for a decision to close.

### TARGET-CHANGED, 3,374 lines (45.3%) — upstream's, not ours

93.9% of it is on 85→86. Reading it (`init-lists.ts`, then the files): these are
the bundle's dependency-init prologues,
`lazyInitializer(() => { (0, m.init)(); ... })`, twenty-odd calls in a row. The
lists are **the same dependencies in a different order**:

| hop     | naming lines in call-list statements | same dependency multiset |
| ------- | -----------------------------------: | -----------------------: |
| 85→86   |                                4,606 |            3,000 (65.1%) |
| 118→119 |                                  282 |              254 (90.1%) |
| 197→198 |                                  244 |              106 (43.4%) |
| 215→216 |                                  278 |              206 (74.1%) |

**Disjoint dependency sets: 0 lines on every hop.** Nothing here is a mispairing.

Who chooses that order? Not this pipeline. These calls live inside a function
body, and nothing in `src/split/` or `src/rename/` mutates a body's statement
order — every `.sort()` in the split is over clusters, files, or require headers.
Both trees were produced by the same pipeline (`REBASE_PRIOR=1`), so a
pipeline-imposed order would have produced the same relative order on both sides;
neither list is sorted by anything. **The permutation is upstream's**, and
removing it would mean rewriting code, which this pipeline must never do.

This corroborates exp044's "87.4% permutation, exp036-irreducible rotation" by a
completely independent route.

### LOCAL-DRIFT, 2,920 lines (39.2%) — the LLM floor

Nothing on the line is require-bound: the pipeline chose a different name for the
same local thing. On the shuffle hop it is the vendor-counter class exp044 read
(`React123 → React93`, `React117 → React74`, `local_React → reactLib35`). On the
calm hops it is a long tail of genuine renaming instability, largest 12 lines:

```
retrieveAnthropicApiKey -> getAnthropicApiKeyOrNull      12 ln
statusElement           -> component                     10 ln
killTask                -> processTaskNotificationVal    10 ln
getFeatureData          -> getFeatureSettings            10 ln
activityState           -> skillCollections               8 ln
skillCollections        -> activityState                  8 ln   <- and back
```

The last two are the cycle shape exp039's correspondence hypothesis died on. This
is the floor exp014 / 016 / 021 / 022 / 035 / 044 have each worked; there is no
single lever in it, and the two biggest attempts (alias reservation, positional
tie-break) both made the diff worse.

### The three reducible mechanisms

**ALIAS-ONLY — 370 lines, 336 of them on 118→119.** Three substitutions carry
330: `commandEntry → interfaceCommandEntry` (178), `kairosCron →
logTaskEventKairosCron` (146), `statusIcon → planReviewStatusIcon` (12). The
require path is identical on both sides; only the alias widened a tier.

Mechanism, read out of `src/split/cjs-emit.ts` and confirmed in the trees. There
is already a stability tier (`claimPriorAliases`) and it did not fire, because
`nsNameIsFree` legitimately failed: aliases are **one per module tree-wide**, and
`importScope.isShadowed` is asked about every importing file. In 119
`table/config-sync/client-configs.js` — one of ~20 importers of `kairos-cron.js` —
gained a top-level binding named `kairosCron`. One shadow in one importer widened
the alias in **all** of them. Verified: no other file claims `kairosCron` in 119,
and no local shadowed it in 118.

**Git-capped ceiling: ≤256 lines**, not 336 — the real `diff -u` over every file
touching those three aliases prints 256 lines that mention them at all, which is
itself an upper bound on lines that changed _because of_ them. The ledger
over-charges this population by ~29%.

**PRIVATE FIELD — 468 lines.** `#x` renames; `family-permute` collects top-level
bindings only, so no pass reaches them. Unclaimed, exactly as the brief said, and
the brief's figure of 422 was close (468 on the post-050 trees).

**MOVED-DECL — 300 lines, 244 of them on 215→216.** A group of module-level
declarations (`commandLib`, `ReactRuntimeVar`, `randomBytesGeneratorRef`, …)
lived in `floor/cli-interaction/task-serializer.js` in 215 and in
`storage/error-messages/auth-manager.js` in 216; both files exist on both sides,
so the declarations moved, and every consumer's alias followed. 202 of the 244
are that one file-to-file move. This is relocation measured on the CONSUMER side,
which `relocatedStatements` never counted — but at 300 lines over four hops it
does not reopen the axis.

## Task 2 — the shuffle pair, separately, always

|                                            |             85→86 | calm 3 hops |
| ------------------------------------------ | ----------------: | ----------: |
| naming                                     |     5,698 (76.6%) |       1,742 |
| irreducible (target-changed + local-drift) | 5,346 (**93.8%**) | 948 (54.4%) |
| reducible, mechanism identified            |               348 |         790 |

Averaging these hides both: 85→86 is 94% irreducible by class, and the calm hops'
reducible share is more than twice as large in proportion while being small in
absolute terms.

## Task 3 — no lever. The arc is closed.

The decision rule set before the measurement was: under ~500 reducible lines
across four hops, close. Every mechanism qualifies **individually** (370 / 468 /
300), and the case against building any of them is stronger than the arithmetic:

1. **Below the instrument.** Rule 11: the `src/` per-hop draw band is ±2,800 git
   lines. The best of these is 336 on its best hop and single digits elsewhere.
   The gate would return a confident sign for pure draw variance, exactly as it
   did for exp048's inert pass.
2. **One hop each.** ALIAS-ONLY is 336/370 on one hop, MOVED-DECL 244/300 on
   another. Gate criterion 1 is "down on every hop, judged per hop"; three of
   four hops would be judging noise.
3. **The alias lever is the one shape rule 6 warns about.** Not identically —
   letting an unshadowed importer keep the short alias refuses nothing to anyone,
   and the shadowing local keeps its name — but it trades away the deliberate
   "one alias per module tree-wide" readability property for ≤256 lines on one
   hop of four. exp044 paid +3,742 lines for a change inside alias allocation
   that measured its own damage correctly (rule 5).

**Naming is not a lever any more; it is a floor.** 84.7% of what remains is
upstream's own permutation or the LLM's naming variance, and the three named
mechanisms together are 15.3% of a bucket that is itself 4.9% of the 154k-line
diff. Further work on this axis is a judgement call about diminishing returns,
and the judgement here is: stop.

## What this experiment leaves behind

| file                       | what it answers                                                                         |
| -------------------------- | --------------------------------------------------------------------------------------- |
| `pairing-audit.ts`         | naming scored under FIFO vs corroborated pairing, plus the forced/chosen split          |
| `line-ledger.ts`           | the naming residual attributed per GIT LINE; reconciles exactly with `diff-composition` |
| `init-lists.ts`            | do the two sides of a call-list statement call the same dependencies                    |
| `naming-drivers.ts`        | naming by driver, with the module-resolving predicate the original got wrong            |
| `diff-composition.test.ts` | pins FIFO as the default, byte-for-byte, and both corroborated rules                    |

`diff-composition.ts` gained a `pairing` option; the default is unchanged and the
4-pair decomposition reproduces 5,698 / 586 / 668 / 488 naming and 28 / 130 / 58 /
4 alias and 54 / 0 / 0 / 0 reorder exactly as before the change.

## Claims in this directory's own brief that did not survive

- **"The naming bucket is inflated by statement pairing."** It is not. 96.9% of
  it comes from pairings where one prior candidate existed.
- **"312 of 322 lines reference UNRELATED modules."** On the post-050 trees the
  cross-module mass is 320 of 488 on that hop, and half of it (244) is the SAME
  exported binding read out of the file it moved to. "Unrelated" was measured
  with a predicate that did not check whether the member survived.
- **"`setLimiter → shellProcessor` … those are not renames."** Correct that they
  are not renames; wrong that they are mispairings. They are references to a
  declaration that changed files, and to upstream's re-ordered init lists.
- **The `049` fidelity result was cited as a mechanism for this.** The per-file
  11.6% error is real, but it is not pairing: on the population it localises to,
  the ledger over-charges by ~29% and git caps it lower. Same direction, unrelated
  cause.
