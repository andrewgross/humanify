# 054 — RESULTS: post-split reconcile, measured, read, built and gated

> ## STATUS: SHIPPED (pending merge). Draw-pinned four-pair gate PASSED on every hop.
>
> **−5,026 git lines across four hops, 0 created**, every hop landing on the
> number predicted from the mechanism before the run. Both legs replayed every
> prompt and wrote 0 cache entries, so all four bundle pairs are byte-identical
> and the deltas are exact rather than draw-contaminated.
>
> **Two of this experiment's own decisions were wrong and the measurement caught
> both.** The pass was first placed post-emit/pre-write; `finishSplitOutput`
> rewrites the tree afterwards, which starved it of **922 lines**. And the
> ledger patch missed every multi-declarator slot, because `statementAlignName`
> joins ALL of a statement's declared names. Both are fixed and re-gated.
>
> Read this file, not the brief. Claims of the brief's that did not survive are
> listed at the bottom (rule 9).

## TOTAL, first

**Gated draw-pinned, four pairs, judged per hop: PASS. Every hop landed on the
number predicted from the mechanism before the run.**

|                                               |     85→86 |   118→119 |   197→198 |   215→216 | **all 4** |
| --------------------------------------------- | --------: | --------: | --------: | --------: | --------: |
| **git lines removed**                         | **1,128** |   **104** | **2,024** | **1,770** | **5,026** |
| predicted, before the run                     |     1,128 |       104 |     2,024 |     1,770 | **5,026** |
| git lines CREATED                             |         0 |         0 |         0 |         0 |     **0** |
| renames shipped (control: 0)                  |       206 |        20 |       331 |       268 |   **825** |
| `realLn` / `novel` / `vendor.*` / `reloc(st)` |         0 |         0 |         0 |         0 |     **0** |
| bundles ON vs OFF                             | identical | identical | identical | identical |   **4/4** |
| boot gate, both legs                          |        OK |        OK |        OK |        OK |   **8/8** |
| ledger incoherence                            |         0 |         0 |         0 |         0 |     **0** |

The decision rule fixed before the measurement was: under ~500 git-capped lines
across four hops, close the arc. The pre-code ceiling was **4,944** — ten times
that — and unlike every lever since 051 it is not concentrated on one hop.

Both legs replayed every prompt from a warm cache and each wrote **0** cache
entries, so the two `humanified.js` bundles are **byte-identical on all four
hops**. The deltas are therefore exact, not draw-contaminated: this pass never
touches the bundle, so an identical bundle means both legs entered the pass from
the same state and the only difference is the pass.

## Task 0 — the ceiling, before any code

No pipeline run, no LLM call. `ceiling.ts` runs the **real** `reconcileDiffNoise`
— production options copied from `reconcile-step.ts`, rule 4: no proxy is
written for a gate that is one import away — over every file present at the same
path in both trees of `/work/exp050-cold`, applies it, checks the pure-rename
invariant, rewrites the fresh TEXT at the renamed identifier locs, and re-diffs
with the same instrument.

The number is **git-capped by construction**: both sides are counted from a real
`diff` of two real texts, so it cannot over-charge the way a decomposition can
(051 measured its own ledger over-charging this population by 29%).

Two guards in the harness earn their place:

- Every rewritten file is **re-parsed** and must still carry the original file's
  structural signature. On 85→86 one file failed this and was dropped; without
  the check it would have contributed a fake 6-line "saving".
- Lines CREATED are reported separately and never netted against lines removed.
  The figure is 0 on all four hops, which is the load-bearing claim: the pass
  does not trade noise for noise.

### What the number is NOT

It is **not** a subset of 051's LOCAL-DRIFT bucket (2,920 lines over four hops).
The classified LOCAL-DRIFT portion alone is 4,514. The excess is real and
explainable: `diff-composition` charges a statement to _naming_ only when its
`statementHash` matches a prior statement's, so a name-only line inside a
statement that was genuinely edited is charged to _real change_ and never
appears in the naming bucket. Those lines are still name churn a reviewer reads,
and the pass removes them. Measuring on git lines directly is what makes them
visible.

## Task 1 — reading the survivors, and the one wrong class

Roughly forty pairs were read. Priced by ablation (each class re-run alone
against the same prior text; class prices sum to the joint within 4 lines):

| class            |    4 hops | what it is                                                              |
| ---------------- | --------: | ----------------------------------------------------------------------- |
| **LOCAL-DRIFT**  | **4,514** | the stated target — the LLM chose a different word for the same binding |
| COUNTER-RESTORE  |       358 | `bootstrapApp17 → initApp26`; counter-decorated on both sides           |
| SUFFIX-COLLAPSE  |        64 | `isStreamingFallbackActive_3_2 → isStreamingFallbackActive`             |
| ~~ALIAS-HEADER~~ |   ~~524~~ | **excluded, see below**                                                 |

Genuine, verified by hand: the `completion/decision/api-query.js` mass (18 read
individually — `streamCompletedToolResults ↔ yieldCompletedToolResults`,
`dialogDecision ↔ reactiveCompactDecision`), catch parameters,
`activeAgentId_2 → currentAgentId` at 26 votes, and the three `lazyInitializer`
module vars in `floor/server-status/otel-exports.js` — whose bodies are
**byte-identical between the two releases**, so they pair correctly and are not
the upstream permutation 051 warned about.

### The wrong class: five cross-module renames, all one shape

`cwdManager → bunDetection`, `motionIcon → valueSerializer`,
`eventPropagatorAnsiParser → actionRegistry`. Every one is a **consumer-tier**
rename on a `require` alias whose exported member moved split files. In 197,
`configFilePath` was exported by `type-inspector/newline-detection/bun-detection.js`;
in 198 it is exported by `lsp/command-processing/cwd-manager.js`. The alias
correctly followed the member to its new home, and the pass proposed naming the
`cwd-manager` namespace after a module it is not.

This is exactly 051's cross-module / moved-declaration class, and it reads like a
rename, which is why rule 1 exists. The mechanism is not incidental: **the
consumer tier fires precisely when the DECLARATION genuinely changed**, and for
an import binding a changed declaration means a different module — unchanged
consumers cannot tell `aliasA.member()` from `aliasB.member()`.

**Fix shipped: `skipImportDeclarations`, refusing every tier on a binding
declared by `require(<string>)`.** It costs 46 lines and removes all five.

### The ALIAS-HEADER class, and why it is excluded

Restoring `logTaskEventKairosCron → kairosCron`,
`interfaceCommandEntry → commandEntry`, `planReviewStatusIcon → statusIcon` is
worth 524 lines, 476 of them on 118→119. It is exp051's declined alias-widening
population arriving through a different door, and it has a cost 051 already
weighed: aliases are **one per module tree-wide**, and the short alias can only
be restored in the ~19 importers where no local shadows it — not in the one
importer whose shadowing local caused the widening. The tree would end up
calling one module two different things.

The same `skipImportDeclarations` gate excludes it. 118→119 drops from 556 to
**80** as a result; that hop's win was almost entirely this class.

## Task 2 — the pipeline change

`src/split/post-split-reconcile.ts`, behind `HUMANIFY_NO_POST_SPLIT_RECONCILE=1`,
wired in `unified.ts` as phase 3.6. Written red-first: the
`skipImportDeclarations` test reproduced the `cwdManager → bunDetection` misfire
before the flag existed.

### The ordering problem — post-emit, pre-write, textual rewrite

Reconcile is phase 3.3 and the split is 3.5, so a post-split pass runs after
file assignment. Of the three hazards the brief names:

- **Re-emitting.** Not done. The rename is applied to the file's AST and then to
  its TEXT, at the identifier positions whose name changed. A validated rename
  only rewrites identifier tokens, so a textual rewrite at those locs is what a
  re-emit would contain — without re-running the assembler, whose require
  headers, export blocks and statement order must not move. Verified on
  4 × ~1,500 real files: the non-identifier skeleton is broken in **0** files and
  the line count changes in **0** files.
- **The split ledger.** `emitHashes` is a `statementHash` array and
  `statementHash` **masks identifier names**, so a pure rename provably cannot
  move it — which is what makes patching `emitNames` alone safe here. The
  lockstep bug 050 nearly shipped only bites when both arrays really change.
  Confirmed on the real 35,903-slot ledgers: `emitHashes` unchanged on every
  hop, `emitNames` patched in 59 / 2 / 21 / 9 slots. `nameToFiles` is moved in
  the same step. **Only TOP-LEVEL renames touch the ledger** — both fields
  describe declared statements, and a unit test pins that an inner local sharing
  a spelling with another file's top-level declaration does not rewrite its slot.
- **The boot gate.** `index.js` is untouched: renames are intra-file, and a CJS
  split file exports through `defineProperty(module.exports, "name", …)` whose
  string key a rename cannot reach, so importers are unaffected.

### The shipped code reproduces the ceiling exactly

`verify-pass.ts` runs `postSplitReconcile` — the production function, with the
production ledgers off disk — over the same two trees:

|                                    |     85→86 | 118→119 |   197→198 |   215→216 |
| ---------------------------------- | --------: | ------: | --------: | --------: |
| ceiling (harness, pre-code)        |     1,162 |      80 |     2,028 |     1,674 |
| shipped pass                       | **1,162** |  **80** | **2,028** | **1,674** |
| skeleton broken / line count moved |     0 / 0 |   0 / 0 |     0 / 0 |     0 / 0 |
| `emitHashes` moved                 |        no |      no |        no |        no |

## The gate — draw-pinned, four pairs, judged per hop: **PASS**

| hop       | GIT CHURN OFF |     ON |  **delta** |  predicted | renames |
| --------- | ------------: | -----: | ---------: | ---------: | ------: |
| 85→86     |        41,324 | 40,196 | **−1,128** |     −1,128 |     206 |
| 118→119   |        37,319 | 37,215 |   **−104** |       −104 |      20 |
| 197→198   |        44,332 | 42,308 | **−2,024** |     −2,024 |     331 |
| 215→216   |        27,778 | 26,008 | **−1,770** |     −1,770 |     268 |
| **total** |               |        | **−5,026** | **−5,026** | **825** |

Every "must not move" KPI is **exactly 0 on every hop**: `realLn`, `novel`,
`vendor.churnLines`, `vendor.noise`, `reloc(st)`, `noise(st)`. Split placement is
identical between legs (same tier counts). Zero pure-rename violations. Zero
ledger-incoherence warnings. Boot gate green on all eight trees. Every control
leg shipped **0** renames, so no hop's KPIs can have been moved by a pass that
did nothing there (rule 11).

**Isolation, proven and not assumed.** Both legs replayed every prompt from a
warm cache and each wrote **0** cache entries, so the two `humanified.js`
bundles are **byte-identical on all four hops**. This pass never touches the
bundle, so an identical bundle means both legs entered it from the same state:
the delta IS the pass. (The first gate run, from a cold cache, was not this
clean — see below.)

### Self-hop, draw-pinned, both legs

| leg                 | bundle diff | move hunks | `src/` tree diff | cache written |
| ------------------- | ----------: | ---------: | ---------------: | ------------: |
| control (pass off)  |           4 |          0 |                4 |             0 |
| candidate (pass on) |       **4** |      **0** |            **4** |             0 |

Identical, and the prediction was written into the script before it ran: on a
self-hop the prior tree IS the fresh tree, so every per-file diff is empty and
the pass is inert. The 4 residual lines are the control's. Note this compares
the TREE as well as the bundle — 049's self-hop compared the bundle alone, which
would be structurally blind to a pass that only edits split files.

### The classified noise KPI understates this pass up to 30×

`layout.noise` moves −174 / −20 / −194 / −60 while git prints
−1,128 / −104 / −2,024 / −1,770. Most of what the pass removes sits in
statements `composeDiff` charges to real change (see "what the number is not").
`noiseLn` is 0 delta on every hop **and always will be** — it scores the BUNDLE,
which the pass never touches. That is a fact about the instrument, not a null
result, and it is why the binding criterion here is a direct `diff -r`.

`layout.real` falls with it (−953 / −87 / −1,801 / −1,641) and that is NOT real
change disappearing: step 3 of `composeDiff` charges an edited statement only
the lines a line diff prints once its token overlap with the removed prior
statement clears 50%, and a restored name raises that overlap. The guards for
"did real change shrink" are `realLn` and `novel`, from a different classifier,
and both are exactly 0.

## Two wrong decisions of this experiment's own, caught by measurement

### 1. The pass was in the wrong place — 922 lines

`writeSplitTree` is not the last thing to touch the tree. `finishSplitOutput`
then runs `relinkBunModules` and `desugarUsingInTree`, both of which rewrite
`src/` on disk. Placed before them, the pass diffed text that was not what
ships, against a prior that had already been through both.

The first gate run measured −4,104. Re-applying the pass to each ON leg's FINAL
on-disk tree recovered exactly what was missing, and the loss tracked vendor
churn — which is the mechanism, since the re-link rewrites vendor require paths
inside `src/` files:

| hop       |    starved by | vendor churn on that hop |
| --------- | ------------: | -----------------------: |
| 85→86     |            36 |                       82 |
| 118→119   |             0 |                      341 |
| 197→198   |           452 |                    2,853 |
| 215→216   |           570 |                    1,889 |
| **total** | **922–1,058** |                          |

Moved to run after `finishSplitOutput`, on disk, re-writing the ledger. The
re-gate recovered all of it: 331 renames on 197→198 where the old placement
managed 270, and 268 on 215→216 where it managed 189.

### 2. The ledger patch missed every multi-declarator slot

`statementAlignName` records **all** of a statement's declared names, sorted and
comma-joined (`"isHostPermitted,urlString"`), because `var a,b,c` and `var d,e,f`
share a statement hash and have to key apart. Comparing a slot against a bare
name never matches one of those, so any rename inside a multi-declarator
statement left the slot naming a binding no longer in the tree — and the next
release's emission aligner would key on it. Not marginal: `emitNames` now
patches **90** slots on 85→86 where it patched 59, and **65** on 197→198 where
it patched 21.

**The coherence counter added to catch that then fired 3 times on its own first
live run, and it was wrong too.** The first hypothesis — a swap corrupting
`nameToFiles` — was built as a fixture and produced **zero** renames: a
same-scope swap is impossible, because `runReconcileRounds` stops when no rename
applies. The real shape is a CHAIN: `oversize-report.js` ships
`readSessionTemplate → loadSessionTemplate` and then
`fetchSessionNotesPrompt → readSessionTemplate`, so `readSessionTemplate` comes
BACK as a target. The data was right; the check was flagging the target. Fixed
to exclude names that return; the live runs now report **0**.

`nameToFiles` was also split into all-removals-then-all-additions. That ordering
hazard is currently unreachable — the rounds happen to emit blockers first — but
depending silently on another module's ordering is how the multi-declarator gap
survived in the first place.

## The bundle carry — the lineage a forward walk inherits through

`.humanify/humanified.js` is what the NEXT release points `--prior-version` at.
The pass renames bindings in split FILES, so without a carry the tree and the
bundle disagree by exactly those renames and every hop re-earns the restoration
from the prior tree instead of inheriting it.

Self-hopping off the run's own output, 85→86:

| prior bundle         | renames re-earned next hop | self-hop tree diff |
| -------------------- | -------------------------: | -----------------: |
| no carry             |                        207 |                 36 |
| carry EVERYTHING     |                         20 |            **322** |
| **carry inner only** |                     **96** |             **36** |

**Gated, four pairs, draw-pinned: PASS.** Tree churn is identical to the
pre-carry gate (−1,128 / −104 / −2,024 / −1,770 = **−5,026**) because the carry
changes the NEXT hop, not this one. `realLn` / `novel` / `vendor.*` /
`reloc(st)` exactly 0 on every hop; boot green 8/8; both legs wrote 0 cache
entries; the ON↔OFF bundle delta is a pure rename (identical line counts).

**`noiseLn` moved for the first time in this experiment: −4 / 0 / −1,258 / −183
= −1,445.** It scores the BUNDLE, so it was structurally pinned at 0 while the
pass could not reach it. Down on three hops of four is the lineage improvement
appearing as a KPI rather than only in the self-hop.

### Two findings the carry cost, both caught by measurement

**Top-level renames must never be carried — 238 of 238, zero exceptions.** A
split file exports through
`defineProperty(module.exports, "name", { get: () => local })`, whose key is a
STRING the tree's rename cannot reach. In the tree the declaration moves and the
key does not, so consumers keep the old name at no cost. Carry it and the next
release derives the export key from the bundle's new name: the key moves and
every consumer of that module churns. Carrying everything measured **322**
self-hop lines against a baseline of 36, and every one of the 238 drifted naming
lines carried a name that is an export key. Inner locals are never export keys.

**Occurrence positions must be captured BEFORE the rename.** The first
implementation inferred them by re-traversing the mutated AST and matching
names, which cannot tell a renamed occurrence from an unrelated identifier that
already had the new name — one bundle holds **4,223** bindings spelled `error`.
Substitutions went missing, the rewritten text was not a pure rename, and all
192 renames were discarded. Two other hypotheses were killed first and are worth
recording as refuted: shorthand-property expansion (0 occurrences) and duplicate
substitution positions (0 occurrences). The diagnostic that finally pointed at
the cause was itself wrong in the same way — it counted nodes by FINAL name.

### Why hash or name matching could not find the bundle binding

Hash: the runnable emit rewrites cross-file references (`f(x)` → `ns.f(x)`), so
an emitted statement is not structurally the bundle statement it came from.
Name: 27% of renames share a `fromName` with another binding in the same file,
and one trail holds `retryAttemptCount -> reactiveCompactResponse` AND
`retryAttemptCount -> reactiveCompactResponseSecondary`. So the emitter now
records `emitIndexes` — the bundle statement index behind each emitted slot,
which it already had as `q[at]` and was discarding.

## Why the gate is pinned and not cold

The per-hop effect is 1,162 / 80 / 2,028 / 1,674. The `src/` per-hop draw band is
**±2,800** (exp048, rule 11). A cold A/B cannot resolve a single one of these
hops and would still print a confident sign for each. Rule 10 permits the cache
for a deterministic surface; this pass is deterministic and sits **after every
prompt**, so with the prompts replayed both legs render the same pre-pass tree
and the delta IS the pass. `pinned-ab.sh` runs the ON leg first to populate and
reports the OFF leg's write count as the key diagnostic.

### What the pinned run cannot see, stated with the number

1. **Multi-hop feedback, and one specific asymmetry.** The renames are computed
   per split file and cannot be applied to a bundle by name, so
   `.humanify/humanified.js` — which is what `--prior-version` points the NEXT
   release at — does not carry them. The prior TREE and the prior BUNDLE
   therefore disagree by exactly this pass's renames. The tree is self-correcting
   (the pass re-earns the restoration each hop from the prior tree, which has it),
   but the restoration has to be re-earned rather than carried by the ordinary
   matcher. Carrying the renames into the bundle is the obvious follow-up and is
   not built here.
2. **Draw-dependent interactions.** With prompts pinned, a rename that would have
   changed what the LLM proposes elsewhere cannot show up.

## Claims in this directory's own brief that did not survive

- **"Target = LOCAL-DRIFT, 2,920 git lines over 4 hops."** The reachable
  population is larger than that bucket, not a subset of it: 4,514 lines
  classify as local drift and the bucket only counts the ones inside
  hash-matched statements.
- **"Expect [the diff-covered-line gate] to remove most of the population."** It
  is the top skip reason on 85→86 (283) and near-invisible on the calm hops
  (8 / 10 / 10). `consumer-single-hunk` and `decl-not-clean` are what actually
  bound the calm hops.
- **The brief listed the consumer tier among the safety gates without
  qualification.** It is the one tier that produced wrong renames here, and only
  post-split — the failure needs an import binding, which a bundle barely has.
