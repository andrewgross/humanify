# exp038 — Dependency-aware emit order: RESULTS

Conventions: _Idea → Evidence (table) → Conclusion_; **ceilings measured before
builds**; every hop judged **on its own** — no cross-pair aggregates.

**Measurement basis.** Prior trees are the exp037 sweep's rebased priors
(`/tmp/eval-work/leverb-sweep/<v>-rebased`); align-OFF baselines are `<v>-off`
from the same sweep. The **current shipped behaviour is Lever B v2**, whose trees
are `/tmp/eval-work/leverb/<n>-on-v2` — note `leverb-sweep/<v>-on` holds the
superseded **v1** trees (v1 is the version that regressed 118→119), so comparing
against those sets the bar too low. Churn is in **git lines** (add+del), matching
`diff-composition.ts`. exp038 trees are under `/tmp/eval-work/exp038/`.

---

## Result

Every hop improves on its own, canary included. Same prior, same LLM cache, so
the bundles are byte-identical and only emit order differs.

| hop         | boots | pure reorder | naming vs OFF  | churn OFF | churn v2 | churn **v3** |      vs v2 |
| ----------- | ----- | ------------ | -------------- | --------: | -------: | -----------: | ---------: |
| **118→119** | ✅    | ✅ 0/1527    | byte-identical |    38,895 |   38,421 |   **37,845** |  **−1.5%** |
| 215→216     | ✅    | ✅ 0/1497    | byte-identical |    68,894 |   46,832 |   **32,086** | **−31.5%** |
| 85→86       | ✅    | ✅ 0/1528    | byte-identical |    80,012 |   60,814 |   **50,632** | **−16.7%** |
| 197→198     | ✅    | ✅ 0/1505    | byte-identical |    75,680 |   62,420 |   **55,614** | **−10.9%** |

118→119 is the regression canary — the feature-drop hop with almost nothing to
reorder, where Lever B v1 regressed +2.3%. It improves.

Reorder churn (`reorder-churn.ts` proxy, fresh-side lines) against no alignment:

| hop     |    OFF | v2 reduction | **v3** | **v3 reduction** |
| ------- | -----: | -----------: | -----: | ---------------: |
| 215→216 | 14,609 |         −63% |    997 |         **−93%** |
| 85→86   | 15,840 |         −38% |    908 |         **−94%** |
| 197→198 |  5,465 |          −9% |    984 |         **−82%** |
| 118→119 |    645 |         −19% |    129 |         **−80%** |

### What the diff a human reviews is now made of (`diff-composition.ts`)

| hop     | accounted churn |  REAL | NOISE | naming | alias | **reorder** |
| ------- | --------------: | ----: | ----: | -----: | ----: | ----------: |
| 85→86   |          41,516 | 81.8% | 18.2% |  13.8% |  0.0% |    **4.4%** |
| 215→216 |          31,948 | 90.3% |  9.7% |   3.0% |  0.5% |    **6.2%** |
| 197→198 |          61,620 | 94.4% |  5.6% |   1.9% |  0.4% |    **3.2%** |
| 118→119 |          38,715 | 98.0% |  2.0% |   1.2% |  0.2% |    **0.7%** |

Against exp037's v2 measurement, reorder falls **33.2% → 4.4%**, **26.6% → 6.2%**,
**14.3% → 3.2%**, **2.7% → 0.7%**. Real change did not move — 216 REAL is 28,850
lines under both v2 and v3, and naming/alias line counts are identical (they are
computed order-blind), so this is noise removal, not real change dropped. Naming's
_percentage_ rises only because the denominator shrank.

**Reorder is no longer the largest noise bucket on any hop.** On 85→86 naming
churn (13.8%) is now 3× reorder; the next lever is naming stability, not layout.

### Self-hop

| hop     | bundle diff lines | pre-existing (exp037) | src files differing | of which REORDER |
| ------- | ----------------: | --------------------: | ------------------: | ---------------: |
| 215→216 |             **0** |                     0 |                  82 |            **0** |
| 118→119 |                14 |                    14 |                  17 |            **0** |
| 197→198 |                 4 |                     4 |                  83 |                — |
| 85→86   |                44 |                    44 |                  45 |                — |

Bundle self-hop matches the documented pre-existing naming draw-flakes exactly.
The src-tree differences are **not** emit order: every differing file on 216 and
119 differs in CONTENT, none in ordering, and the shape is a statement changing
file (`var requestTimeoutMsResult = 2000;` leaving `api-response.js`) — split
assignment drift, which predates this work: Lever B v1's own self-hop differs by
**99** files on 216 versus v3's 82. Emit order is a fixed point.

---

## Task A — the ceiling, measured before building

`reorder-ceiling.ts` classifies every displaced statement with the real load-time
model, then **simulates** the best order that model permits. The simulation is the
ceiling; the class table says why.

Classes: `MOVABLE_FN` = function declaration (v2 already allows it to move);
`PURE_WRAPPER` = `lazyInitializer(...)`-shaped, verified structurally;
`FREE_DECL` = other effect-free declaration with no load-time read of a
load-time-written binding; `ORDER_BOUND` = effect-bearing or genuinely
dependency-constrained.

_(v2-tree numbers from `measure-v2-baseline.sh`; the shape held identically on the
v1 trees measured first: ORDER_BOUND 1.0% / 4.3% / 0.8% / 2.2%, ceiling −79% /
−77% / −86% / −82%.)_

**Genuinely order-bound is a few percent of the residual on every hop.** The
pinned set is almost entirely effect-free code that v2 pinned only because it was
not a function declaration. That is what made this worth building — and the real
runs beat the simulated ceiling (predicted 434 achievable lines on 118→119,
delivered 258).

### Why function declarations dominate the residual when v2 already moves them

`align-trace.ts` replays v2's gates against the on-disk diff, joining the ledger's
per-file hash sequence to the emitted body statements (they are 1:1). A large
share of displaced statements are functions the aligner **did** place at their
prior positions (`PLACED`). That is blame allocation, not failure: the LCS keeps
whichever backbone is longer, so when pinned `var` statements outnumber the
functions, the correctly-placed functions are charged as churn. The pinned set
therefore causes both its own churn and most of the churn charged to functions —
which is why freeing it recovers far more than the `NOT_MOVABLE` row suggests.

---

## Task B — the load-time dependency model

`src/split/load-order.ts`, pure and unit-tested (29 tests). Per top-level
statement: `writes` / `reads` / `effects` / `hoisted`, counting only what runs
**while the module loads** — function and arrow bodies are excluded, which is the
whole unlock (`var x = lazyInitializer(() => {...})` touches nothing at load
time).

Conservative by construction:

1. **Effect statements are barriers** — an unverified call can reach module state
   through a body the model does not read, so nothing crosses one.
2. **Hoisted function declarations are unconstrained** — initialized before any
   statement runs; the property Lever B already ships on.
3. **Every edge points forward in bundle order** — the constraint graph is a DAG
   whose topological orders include bundle order, so a legal answer always exists
   and the pass cannot fail.

Purity is never decided by name: `bundleLoadOrderFacts` identifies Bun's lazy-init
wrapper by its `x && (y = x(x = 0))` shape through the existing
`identifyBunLazyInit` detection, then admits calls to that binding. Because the
bundle is one scope, the helper's name identifies exactly one binding.

Accepted residual risk, documented at the code: a property read (`a.b`) is treated
as effect-free and charged only as a read of `a`. A getter with an observable side
effect would defeat that; the accessors this pipeline emits are `() => x`.

Property tests cover the brief's three — a reader never precedes its writer;
effect-bearing statements never reorder relative to each other; a file of pure
declarations is fully permutable — plus "always a permutation" over 50 seeded
shuffles with every model constraint re-checked on the output.

---

## Task C — wired into `alignFileStatements`

`orderRespectingLoadOrder` replaces the `isMovable` boolean: greedy topological
scheduling that takes, among statements whose predecessors are all emitted, the
one the prior-aligned order wants soonest. The unambiguous-hash precision gate is
unchanged (it is what killed v1's 118→119 regression), `HUMANIFY_NO_EMIT_ALIGN=1`
still turns everything off, and v2's align-then-restore special case is gone — it
was the degenerate "all non-functions form one chain" case of this model. v2's
"fewer than two movable+unambiguous statements" bail becomes "fewer than two
statements identifiable across versions", which is what brings 85→86's 25%
bailed-file bucket into play.

`npm run check`: 1,555 unit + 33 fingerprint green.
