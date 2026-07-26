# 045 — Reorder: the untouched axis, and the best-shaped one left

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
_Idea → Evidence (table) → Conclusion_; **ceilings measured before builds**;
totals-first; every hop judged **on its own**.

Read [exp038 RESULTS.md](../038-dependency-aware-reorder/RESULTS.md) — it built
the load-order model this would extend — then
[exp044's findings](../044-naming-correspondence/RESULTS-correspondence.md) for
why naming is not the next lever.

## Why this axis, now

Five experiments have gone by without anyone looking at reorder. The ranking has
changed underneath it:

| source      |     lines | status                                                     |
| ----------- | --------: | ---------------------------------------------------------- |
| naming      |     7,616 | only ~2,600 tractable — rest is irreducible rotation       |
| **reorder** | **6,078** | **untouched since exp038**                                 |
| relocation  |     1,390 | solved (−91.1%), no evidence left to bring                 |
| alias       |       200 | metric understates it; the real fix FAILED (exp044 task B) |

Reorder is now the largest bucket with an unexplored mechanism.

## Ceiling — measured on exp043's trees, before any build

`probe.ts` reproduces exactly what `diff-composition.ts` charges as reorder: an
EXACT-matched statement (same hash AND same text) emitted outside the longest
common subsequence of its file's prior order.

| hop     | statements | git lines | files affected | top 10 files | p50 move | max move |
| ------- | ---------: | --------: | -------------: | -----------: | -------: | -------: |
| 85→86   |        251 |     1,918 |             94 |    **68.3%** |        6 |       51 |
| 197→198 |        115 |     1,950 |             56 |    **82.3%** |        6 |       65 |
| 215→216 |        180 |     1,952 |             77 |    **67.3%** |       13 |       89 |

**This is the best-shaped population any lever has had in this series.**
Relocation started at 15,699 lines spread over thousands of statements. Reorder
is ~1,950 lines per hop in **115–251 statements**, and two thirds of it sits in
**ten files**. A fix that works on ten files per hop pays for itself.

Two files recur across hops, which is the first thing to read:

    floor/server-status/otel-exports.js    274 ln (85→86), 502 ln (197→198)
    user-input/tool-usage/tool-prompt.js   144 ln (197→198), 112 ln (215→216)

And the distances are moderate — p50 of 6–13 slots, not a wholesale scramble. A
statement drifting six positions inside its file is an alignment failure, not a
dependency constraint forcing it across the file.

## The work, in order

### A. Read the recurring files — do NOT build before this

1. `otel-exports.js` on 197→198 (502 lines, 14 statements) and on 85→86 (274
   lines, 12). The same file failing twice is either one mechanism or one
   pathological file; which it is decides whether this experiment has a lever or
   a special case.
2. For each moved statement, establish whether it COULD have stayed:
   `src/split/load-order.ts` pins anything with a load-time effect, so a
   statement that cannot legally move is the constraint working, and it caps
   what any lever can recover. **Measure that cap before designing.** exp044
   died because a ceiling counted the benefit and not the cost; the mirror error
   here is counting movable lines that are actually pinned.
3. `table/skill-docs/files-api.js` on 85→86 is ONE statement worth 332 lines —
   read it, because a single statement moving that far may be its own bug.

### B. Only then design

exp038's model aligns emission order to the prior release per file. The
candidates, unranked until A is done: the LCS tie-break itself (when several
orders are equally legal, does it pick the prior-stable one?); statements whose
text changed dropping OUT of the exact-match population and taking their
neighbours' alignment with them (observed in exp044 task B, where 340 lines of
reorder appeared on 118→119 purely because a few statement texts changed); and
new statements inserted mid-file pushing the tail.

### C. Gate

`experiments/041-content-anchor/gate-verdict.sh exp043-nearident <label>`.
Control `exp043-nearident`. Non-negotiables unchanged, plus: **relocation must
not move** — it is at 1,390 after three experiments and emission order feeds the
same ledger placement consumes.

## Warnings carried forward

- **Never trust a match you have not eyeballed.** Two sizing predicates in
  exp044 confirmed hypotheses that reading the same data refuted.
- **A ceiling scoped to the directly-affected population under-predicts.**
  exp044's alias reservation destabilised nothing it measured and still cost
  +3,742 lines through second-order effects.
- 118→119 is the regression canary and is deliberately absent from the table
  above: its reorder is 258 lines, the smallest of the four, so it has almost
  nothing to win and everything to lose. Judge it on its own.
