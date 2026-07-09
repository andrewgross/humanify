# Experiment 016 — cross-version diff noise → human-reviewable

**Goal (set 2026-07-09):** drive the noise in the v119→v120 humanified
diff down — ideally to 0, at minimum to where a human can review the
version diff like a real git diff. Continues exp014 (22,998 → 6,206
noise hunks) and exp015 (→ 5,788, reroll families eliminated).

Branch `exp016-diff-noise-convergence` (exp015 merged to main first).
Baseline for this experiment = exp015 run 2: **noise 5,788 hunks
(75.0% of hunks), occurrences 10,206, buckets transfer-gap 7,536 /
asymmetric 1,259 / reroll 1,411, genuine ~1,934.**

## Round-1 autopsy (offline, before any fix)

`classify-decl-kinds.py` on the run-2 diff splits the 10,206
occurrences by the old name's declaration kind:

| family                  | occurrences | pairs | mechanism                                                                                                                                                          |
| ----------------------- | ----------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| function-decl names     | 1,470       | 402   | drifted fns: no match path pins the NAME (`serializeWithHelper` 423 occ, five names in five runs — its v120 fn is no-counterpart per the matcher dump)             |
| class-decl names        | 1,326       | 313   | classes aren't graph nodes; skipped from the module pool with a comment claiming otherwise (`y6→C6` 98, `HK→qK` 57, `m3→a3` 41 — `class m3` is the zod base class) |
| other (locals/params/…) | 7,410       | 2,651 | LLM naming instability + the uniquified-sibling population from exp015 that name-keyed transfer could not converge                                                 |

Also measured: suffix/decoration instability (`valueVal→value`,
`identity4→identity2`) 465 occ / 141 pairs — expected to converge via
transfer, not directly fixed. The deep reservoir behind "other" is the
ambiguous-bucket fresh pool (4,111 v120 functions with exact hash twins
stuck in m:n buckets — exp014's cracking problem, next round's lever).

## Round-1 fixes (all red/green TDD)

1. **Binding-keyed statement-align transfers** (`c1fb8b9`):
   close-match body-local evidence was keyed by the new-side minified
   NAME — same-named siblings collapsed onto one key, their different
   prior names failed unanimity, and ALL were dropped; application then
   resolved first-name-wins. Evidence is now per slot-resolved Binding
   and pairs carry it end-to-end (`TransferPair.binding`). Also fixes a
   silent recall gap: bindings declared inside an aligned container
   statement resolved to undefined from the statement's outer scope.
   This is the convergence unblock for exp015's uniquified population
   (`toolExecutionResult→o_10` and the 1,309 inside-oversized
   transfer-gap occurrences).
2. **Class declarations join the module-binding pool** (`25b8ae5`):
   one choke point (`shouldSkipBinding`) feeds the LLM pool, the graph's
   `ModuleBindingNode`s, the binding cascade, AND vote application — so
   classes get fresh-leg LLM naming and receive the external-ref votes
   exact-matched referencing functions already emit (exp014 measured 94
   discarded votes per class). Named class expressions assigned to
   declarators (also nodeless) stay in the pool too.
3. **Vote-based name transfer for drifted function declarations**
   (`ebb565c`): external refs whose binding is a FunctionDeclaration now
   collect per-binding votes and apply at the module-binding two-vote
   agreement floor; the applied name registers in
   `priorVersionTransferred` so the LLM pass leaves it alone.
   Single-vote case stays inert (precision test).
4. Riding along from the exp015 tail: `MF5` single-line-blob char cap
   (kills the last 400-failing module batch).

## A/B protocol

Same as exp014/015: fresh v119 + incremental v120 via
`run-phase2.sh`, `PHASE2_OUT=/tmp/exp016-rN`, measured with
`classify-diff.py`, `attribute-noise.py`, `classify-decl-kinds.py`,
`attribute-to-megafunctions.py`.

Round-1 results: see `RESULTS.md` (pending run completion).
