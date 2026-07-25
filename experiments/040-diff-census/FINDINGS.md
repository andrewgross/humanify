# 040 — Diff-noise census: what the reviewed lines actually are

Conventions: every hop judged **on its own**; ranked by measured lines, not by
how noisy something feels.

`line-census.ts` classifies every line of the real `git diff` between two split
trees by what produced it. It reads git's own output rather than re-deriving a
diff, so the categories describe exactly the lines a human sees.

## Per-hop census

| hop     | diff lines |  body | require:rm | require:add | accessor:rm | accessor:add | setter | alias |
| ------- | ---------: | ----: | ---------: | ----------: | ----------: | -----------: | -----: | ----: |
| 85→86   |     50,662 | 89.7% |  **1,933** |   **1,900** |         719 |          551 |    108 |    28 |
| 118→119 |     37,821 | 93.9% |        102 |       1,137 |         107 |          897 |      6 |    54 |
| 197→198 |     52,380 | 93.7% |        788 |       1,240 |         357 |          859 |     34 |    44 |
| 215→216 |     31,709 | 94.2% |        286 |         687 |         189 |          582 |     12 |    74 |

## Finding 1 — `diff-composition` counts import churn as REAL change

`composeDiff` matches statements by structural hash. A `const x = require("…")`
header present on only one side has no counterpart, so it is charged as **real
change**. It only lands in the `alias` bucket when the same PATH exists on both
sides with a different alias.

That is a real blind spot: on 85→86 the census finds **1,933 require lines
removed and 1,900 added**. Near-balanced add/remove is the signature of churn,
not growth — compare 118→119, a large feature drop, where requires are 1,137
added against only 102 removed. So some part of that 3,833 lines is sitting in
the "real change" column and the 89.7%-real headline for that hop is optimistic.

## Finding 2 — `relocatedStatements: 0` does not mean nothing moved

The 034 tree metric only compares statements whose hash is **unique on both
sides**. On these trees that is 11,452 of 19,966 statements (85→86) and 21,797 of
35,903 (215→216) — so **39–43% of statements are invisible to it**. Reporting
"0 relocated" from it, as I did earlier, overstates the result.

Measured order-independently from `nameToFiles`, restricted to names with exactly
one home file on both sides (a name like `key` is declared in 40+ files, and
counting any change to that set inflates the number ~2.5×):

| hop     | single-home names | **moved home file** |
| ------- | ----------------: | ------------------: |
| 85→86   |            28,521 |      **75** (0.26%) |
| 215→216 |            52,050 |      **77** (0.15%) |

Small in percentage, but each move rewrites the `require` header in every
importer plus the accessor line in both files.

**It is NOT, however, what drives Finding 1's import churn.** Three mechanisms
were proposed and all three were tested and refuted:

| hypothesis                                                                                                                       | test                                                                                                             | verdict                   |
| -------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------- |
| relocation of single-home bindings                                                                                               | match removed→added path pairs against known moves                                                               | **16 of 1,929 — refuted** |
| same-hash vendor shims swapping their `-N` suffix                                                                                | `vendor/noop/lib_eb5345cb{,-2}.js` proxy targets in 85 vs 86                                                     | **stable — refuted**      |
| first-declaration file changed (the emitter picks a binding's home from `binding.identifier.start`, i.e. `nameToFiles[name][0]`) | 276 names did change it, and 3,833 ÷ 276 ≈ 14 lines each looked like a perfect fit — so match the pairs directly | **20 of 1,947 — refuted** |

The third is a caution worth recording: the per-name arithmetic fit was a
**coincidence**, and it read as compelling. Fit is not evidence; only matching the
actual removed→added pairs settled it.

The vendor check is worth keeping: those two files are NOT content-duplicates,
they are distinct re-export shims that share a structural hash and are told apart
only by a `-2` suffix (the documented "same hash, different library" hazard). They
happened to stay stable here, but the churn pattern that led there is real — the
SAME path is both added and removed across different files in near-equal numbers
(`+34/-31` for one shim, `+28/-32` for its sibling), i.e. the set of files
importing a given module is churning.

### The mechanism, found by reading the diffs (Finding 2b)

Sampling 10 files with churned imports and classifying all **417** import changes
against the body evidence in the same file:

| verdict   | count |   share |
| --------- | ----: | ------: |
| **NOISE** |   269 | **65%** |
| REAL      |    68 |     16% |
| unclear   |    80 |     19% |

So the import churn **is** predominantly noise — my three mechanisms were wrong,
not the classification.

**Proof, not inference.** 85's `session/plan-review/status-message.js` and 86's
`completion/decision/decision-reason.js` share **263 byte-identical lines** — the
whole `exitPlanMode` approval-tool object, down to its prose
(`"Wait for the team lead to review your plan"`, present in 85's status-message,
present in 86's decision-reason, **absent from 86's status-message**). A ~260-line
business-logic object moved between output files. 152 of the 269 noise entries
have a byte-identical twin removed-in-one / added-in-another **within the 10-file
sample alone**.

**Why every name-keyed measure missed it.** The relocated code lives inside

    var initializeApp256 = (0, resourceLifecycle.lazyInitializer)(() => { … });

a minted-name lazy-init block. It moved file **and** got a different minted name,
so it has no counterpart in `nameToFiles` on the other side — it counts as a "new
name", never as a move. That is why Finding 2's strict measure found 75 moves
while the reality is far larger.

**The population.** 2.1.86 has **3,273** `lazyInitializer` blocks, **1,868** of
them minted-named. On that hop the split reports
`inherited 19044/19966 (10954 via hashes, 466 via ordinals, 922 residue by locality)`
— **922 statements placed by "follow your preceding neighbour"**, with no identity
evidence at all. When upstream reshuffles the bundle, their neighbours change and
they relocate wholesale, dragging their imports, their exports, and their entire
body through the diff as a delete+add pair.

**Why the tiers abstain, and the fix direction.** These statements change slightly
between releases (263 of ~280 lines identical here), so the structural hash flips
and the hash tier cannot match them; their name is a minted counter, so the
name-vote tier has nothing to vote on. Both abstain → locality. But their CONTENT
is highly distinctive — unique prose strings. A **content-anchor tier** (a fresh
statement sharing a rare literal with exactly one prior statement inherits that
statement's file) would pin exactly this population, and it is precision-gated by
construction: rare literal, unique match, or abstain.

Two shapes stand out:

- **Swaps between sibling files.** On 85→86, `initModuleVar` moved
  `custom-map.js → map-entry-manager.js` while `initializeZT8` moved the other
  way. Both are same-shaped lazy-init statements, so the assignment tiers cannot
  tell them apart — the interchangeable-bucket problem, on the file axis.
- **Group migrations.** On 215→216, `analyzeEventStats`, `processMessageStats`
  and `toolIdToNameMap` all moved `message-stats.js → branch-fetcher.js`
  together, and `hasLocalTool` left `api-query.js` — which is exactly one of the
  accessor removals visible at the top of that file's diff.

## Finding 3 — header churn is over-weighted in PERCEIVED noise

`api-query.js` on 215→216 (the file that prompted this): 1,164 diff lines, of
which the tools say ~97% is genuine new upstream code — verified by reading it
(`areModelsSame`, `isModelInTriedModels`, refusal-chain routing). Only 56 lines
are emitter-generated.

But those 56 sit in the **sorted accessor and require block at the top of the
file**, so they are the first thing a reviewer sees, and they read as a wall of
near-identical churned lines. Low line count, high review cost. Worth fixing
above its line-count rank.

(One naming-quality item in the new code: `function doNothing3()` — a minted
name on genuinely new code.)

## Finding 4 — relocation is the largest noise source on EVERY hop

`relocation-churn.ts` sizes it. Statements are keyed by exact text (a floor), and
separately by a RARE string literal (12+ chars, ≤1 occurrence per side) with the
diff-ledger's ≥50% token-overlap gate — which catches the statements that moved
**and** were edited, the class every name-keyed and exact-text measure misses.

| hop     | exact-text moves | moved **and edited** |     total | share of hop |
| ------- | ---------------: | -------------------: | --------: | -----------: |
| 85→86   |      67 st / 258 |    33 st / **7,325** | **7,583** |    **15.0%** |
| 197→198 |      39 st / 106 |    24 st / **6,099** | **6,205** |    **11.8%** |
| 215→216 |      43 st / 150 |    36 st / **1,692** | **1,842** |     **5.8%** |
| 118→119 |       10 st / 36 |       3 st / **755** |   **791** |     **2.1%** |

Against naming (5,740 / 780 / 754 / 456) relocation is **larger on all four**.
The largest pairs are unmistakable — 390→390, 289→289, 278→279, 271→271 lines —
same-size statements landing in a different file.

**The similarity gate is load-bearing.** Without it the measure paired a
5,073-line prior statement with a 7-line fresh one because they shared one rare
string, and charged 5,080 lines for it; 215→216 read 8,956 instead of 1,692. A
shared rare literal is necessary but not sufficient.

**Ceiling for the fix:** these statements were _found_ by rare-literal +
similarity matching, so by construction that is exactly the signal a
content-anchor inheritance tier would use — the recoverable share is essentially
all of it. The prior statement text is already available in the pipeline (the
reconcile pass reads the prior bundle), so the tier is implementable where the
assignment decision is made.

## Ranked, for the worst hop (85→86, 50,662 lines)

| rank | mechanism                    | lines | note                                                                  |
| ---- | ---------------------------- | ----: | --------------------------------------------------------------------- |
| 1    | **cross-file relocation**    | 7,583 | Finding 4; largest on ALL four hops; content-anchor tier is the fix   |
| 2    | naming churn (body)          | 5,740 | exp039; 32% of it is 6+-rename genuine drift                          |
| ?    | require add/remove churn     | 3,833 | UNCLASSIFIED — 3 mechanisms refuted; may be real upstream refactoring |
| 3    | reorder (body)               | 1,816 | exp038 residue                                                        |
| 4    | accessor add/remove + setter | 1,378 | relocation + rename driven                                            |
| 5    | require alias                |    28 | exp038 Task D already cut this                                        |

## Next

1. Teach `composeDiff` to score require add/remove as its own category instead of
   folding it into real change — the metric currently flatters itself.
2. **Classify the import churn before acting on it** — read ~20 sampled files and
   decide whether the new dependency follows from that file's own body change. It
   is the second-largest line count on the worst hop but currently has no
   demonstrated mechanism, and three plausible ones are already dead.
3. File-assignment stability is still worth attention on its own terms (75 real
   single-home moves on 85→86, 77 on 215→216), just not on the strength of the
   import-churn number.
