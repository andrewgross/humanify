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

Residual reorder churn of the **v2** trees, classified (git lines), plus the
simulated best order the model permits:

| hop     | residual v2 | MOVABLE_FN | PURE_WRAPPER | FREE_DECL | **ORDER_BOUND** | achievable |    ceiling |
| ------- | ----------: | ---------: | -----------: | --------: | --------------: | ---------: | ---------: |
| 85→86   |      19,788 |      9,056 |        2,862 |     7,756 |  **114 (0.6%)** |      4,132 | **−79.1%** |
| 215→216 |      10,864 |      2,790 |        6,152 |     1,484 |  **438 (4.0%)** |      2,832 | **−73.9%** |
| 197→198 |       9,922 |      4,882 |        2,804 |     2,154 |   **82 (0.8%)** |      1,976 | **−80.1%** |
| 118→119 |       1,050 |        790 |          204 |        40 |   **16 (1.5%)** |        288 | **−72.6%** |

Classes: `MOVABLE_FN` = function declaration (v2 already allows it to move);
`PURE_WRAPPER` = `lazyInitializer(...)`-shaped, verified structurally;
`FREE_DECL` = other effect-free declaration with no load-time read of a
load-time-written binding; `ORDER_BOUND` = effect-bearing or genuinely
dependency-constrained.

**Genuinely order-bound is 0.6–4.0% of the residual on every hop.** The pinned
set is almost entirely effect-free code that v2 pinned only because it was not a
function declaration. That is what made this worth building.

The shipped pass **met or beat the simulated ceiling on every hop** — 19,788 →
1,816 (−90.8% vs −79.1% predicted), 10,864 → 1,994 (−81.6% vs −73.9%), 9,922 →
1,968 (−80.2% vs −80.1%), 1,050 → 258 (−75.4% vs −72.6%). The simulation runs on
the emitted tree, where a cross-file read looks like a read of the require alias;
the real pass runs in bundle space, where that binding is not in the file's slot
set at all, so it has strictly more freedom.

### Why function declarations dominate the residual when v2 already moves them

`align-trace.ts` replays v2's gates against the on-disk diff, joining the ledger's
per-file hash sequence to the emitted body statements (they are 1:1). On 215→216:

| gate                                         | share |
| -------------------------------------------- | ----: |
| NOT_MOVABLE (pinned: not a function decl)    | 67.4% |
| PLACED (function, already at prior position) | 21.9% |
| AMBIG (precision guard refused the claim)    | 10.2% |
| BAIL (too few identifiable statements)       |  0.4% |

`PLACED` means the aligner **did** put the statement where the prior had it and it
still reads as displaced. That is blame allocation, not failure: the LCS keeps
whichever backbone is longer, so when pinned `var` statements outnumber the
functions, the correctly-placed functions are the ones charged as churn. The
pinned set therefore causes both its own churn and most of the churn charged to
functions — which is why freeing it recovers far more than the `NOT_MOVABLE` row
alone suggests. `AMBIG` stays by design: it is the guard that killed v1's 118→119
regression.

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
statements identifiable across versions", which is what brings 85→86's bailed-file
bucket into play.

`npm run check`: 1,555 unit + 33 fingerprint green.

---

## Task D — require-alias drift (exp037 Finding 4)

Two complementary fixes in `cjs-emit.ts`, independent of A–C.

**1. Freeness scoped to the files that declare the alias.** `nsNameIsFree` used to
reject a candidate if the identifier appeared **anywhere in the bundle** — including
a nested local in a file that does not even import the module. `const <alias> =
require(...)` is emitted only in a module's IMPORTERS, so those are the only files
where the name can shadow or be shadowed. `buildImportScope` computes identifiers
per file plus the declFile→importers relation, and `isShadowed(declFile, name)`
asks only about the importers. This subsumes the old `scope.hasBinding` check too:
a top-level binding lives in exactly one file, whose identifier set already
contains it. Wrapper parameters (`exports`, `require`, …) stay globally excluded —
they are in scope in every emitted file.

**2. A still-legal prior alias is kept.** The ledger gains `aliases` (file →
import name), written by the runnable emit and consulted on the next release ahead
of the candidate ladder. Without it, an alias that widened last release because of
a collision snaps back the moment the collision disappears, rewriting the import
line and every reference in every importer. Contested prior aliases are taken by
neither file — the same rule the ladder already uses.

Kept deliberately: one alias per module tree-wide (`claimed` is global), so a
reader sees the same import name for the same path everywhere.

**3. A property name is not a shadow.** The per-file identifier set counted
_every_ identifier, including property positions (`a.b`, `{b: 1}`,
`class { b() {} }`). Those name a property, never a variable, so they can neither
bind nor resolve — yet one `apiQuery.memoryExtractor` member read in an importer
was still enough to widen that module's alias tree-wide (it was one of exp037
Finding 4's three original cases). The set now counts binding and reference
positions only.

Tests (`cjs-emit.test.ts`): an unrelated file's local no longer blocks the bare
alias; a property of that name in an importer does not either; a still-legal prior
alias is kept; a prior alias a NEW local would shadow is dropped (stability never
beats correctness); the chosen aliases land on the ledger. The two pre-existing
shadowing tests pass unchanged.

`npm run check`: 1,560 unit + 33 fingerprint green.

### Validation

The alias rule changed, so the prior tree is regenerated with the same rule
(`validate-alias-fix.sh`) — otherwise the hop measures a one-time migration
instead of the steady state, the same reason 034 has `REBASE_PRIOR`.

| hop     | boots | alias churn before | after fixes 1+2 | after fix 3 |
| ------- | ----- | -----------------: | --------------: | ----------: |
| 215→216 | ✅    |                146 |              98 |      **74** |
| 197→198 | ✅    |                250 |          **44** |           — |

215→216 ends with **3 aliases changed across 1,497 files** (4 before fix 3), and
those three are genuine: `memoryExtractor` is a real top-level binding declared in
`api-query.js`, so an importer of `memory-extractor.js` that also references it
holds that identifier in reference position before the rewrite. Blocking there is
conservative — the reference does get rewritten to `apiQuery.memoryExtractor`, so
the alias would not actually collide — but distinguishing needs the rewrite plan,
which is computed after the aliases are chosen. That is the honest floor for this
rule, and it is ~70 lines on the worst hop.

Reorder and REAL churn are unchanged on both hops (1,994→1,950 and 1,968→1,956;
REAL 28,850→28,800 and 58,158→57,984), confirming this touches aliases only.

---

## Pre-merge validation found a real regression: Lever B broke split idempotence

Running the 034 gate properly surfaced something the exp037 sweep could not see:
its self-hop check compares only the **bundle**, while 034 compares the bundle
**and the split ledger**. On the ledger, the self-hop failed.

| 2.1.216 re-split against its own output | bundle    | ledger `order` | src files differing |
| --------------------------------------- | --------- | -------------: | ------------------: |
| align OFF (main's behaviour)            | identical |          **0** |                   0 |
| align ON (Lever B v1 / v2 / exp038)     | identical |         **33** |              **82** |

**Mechanism.** The ledger mixes two kinds of data: IDENTITY (`nameToFiles`, what
the next release inherits from) and LAYOUT (`hashes`, the emitted order).
`buildLedger` was fed the EMITTED body, so `nameToFiles[name]` listed a
redeclared name's files in emission order — and `voteFor` picks
`files[ordinal]`, so that list's order decides where the k-th redeclaration
lands next release. Emit alignment reorders statements within a file, which
flips the cross-file interleaving, which hands the ordinal a different file. 33
of 35,903 statements changed file; 82 files churned.

Note this is a **Lever B** defect (exp037, also unmerged), not exp038's — but
every version of it is on this branch, so it had to be fixed before merging.

**Fix.** Build `nameToFiles` from the BUNDLE-ordered body; `hashes` keeps the
emission order. Identity data in a stable order, layout data separate. Guarded by
a test that runs the same split with alignment on and off and requires the file
contents to differ (proving the aligner fired) while `nameToFiles` is identical.

| after the fix (2.1.216)                | result                                          |
| -------------------------------------- | ----------------------------------------------- |
| self-hop bundle / ledger `order` / src | identical / **0** / **0 files**                 |
| ledger fully converged                 | hop2 → hop3 **byte-identical**                  |
| steady-state churn vs rebased prior    | **31,714** (pre-fix 32,086) — free              |
| composition                            | REAL 91.2%, naming 754, alias 74, reorder 1,950 |
| boot                                   | ✅ 2.1.216                                      |

A residue remains: hop1 → hop2 leaves **44 of 35,903** `hashes` entries different
(ambiguous-hash statements re-anchoring once against a new target sequence) even
though the bundle and the whole src tree are byte-identical. It settles on the
next hop and never moves again. Shipped artifacts are an immediate fixed point;
only the ledger's layout metadata takes one extra hop.

**Migration note.** This changes what a ledger means, so the archive priors are no
longer like-for-like: measured against an old-style prior, 216 churns 73,950 lines
(pure one-time migration). Every measurement above rebases the prior with the same
code — and the 034 eval must run with `REBASE_PRIOR=1` for the same reason.

---

## Follow-ups

- **The eval still cannot see emit order.** 034's `noise`/`novel` classification
  is position-blind, so none of this experiment's win registers there. A
  within-file-order KPI in the harness would keep it from regressing silently.
- **Naming is now the leading noise bucket on the shuffle hop** (13.8% of 85→86
  vs reorder's 4.4%). The ranked ideas in `docs/roadmap-noise-reduction.md` are
  the front line again.
- **Split assignment drift** — a statement changing file between two runs of the
  same version, visible as the self-hop src-tree differences (82 files on 216,
  independent of this work: v1 shows 99). Unmeasured as a churn source.
