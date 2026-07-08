# Round 2 — cracking + closure votes + deferred retry (2026-07-08)

Fixes under test (branch `exp014-slot-keyed-transfer`, commits `b9ec241` +
`90afb64`, on top of round 1's slot-keyed transfer `c32e7ea`):

1. **Rename-invariant externalCalls** — a bound callee object named like a
   KNOWN_GLOBAL no longer leaks its current name into fingerprint
   features (fixed the 5 singleton-gate false rejections).
2. **Function↔binding cascade alternation** with reference-identity
   evidence: same-hash buckets crack by WHICH matched module binding or
   matched function each member references (export thunks reference
   without calling — no callee edge exists). Offline matcher effect:
   exact matches 35,450 → 35,835, fresh pool 6,339 → 5,977.
3. **Block-scope closure votes** — `classifyClosureCapture` resolves the
   owner via `getFunctionParent`, so votes for catch/if/for-block
   bindings apply instead of silently dropping; entries keyed by Binding.
4. **Deferred rename retry** — collision-rejected transfers (760/run)
   re-scan after all phases (chains unwind when a later phase frees the
   token) with validated temp-hop cycle breaking for pure swaps (G↔R).

Methodology identical to round 1 (seeded phase-6 `cc-119`, incremental
leg only, same LLM box). Diagnostic that drove the round:
`fresh-pool-overlap.ts` (offline matcher) + the reroll autopsy of round
1's debug log.

## Results

| metric               | phase-6 baseline | round 1 (slot) | round 2 | cumulative |
| -------------------- | ---------------- | -------------- | ------- | ---------- |
| diff lines           | 143,106          | 72,157         | 63,684  | −56%       |
| noise hunks          | 22,998           | 10,128         | 8,745   | −62%       |
| noise share of hunks | 92.0%            | 83.8%          | 82.1%   | −9.9pp     |
| genuine-change hunks | 2,001            | 1,953          | 1,903   | ~stable    |
| rename occurrences   | 47,761           | 18,848         | 16,423  | −66%       |
| distinct bindings    | 12,299           | 5,533          | 4,862   | −60%       |

Buckets (occurrences):

| bucket          | baseline       | round 1        | round 2        | r2 vs r1 |
| --------------- | -------------- | -------------- | -------------- | -------- |
| transfer-gap    | 25,839 (54.1%) | 11,885 (63.1%) | 10,761 (65.5%) | −9%      |
| asymmetric      | 14,572 (30.5%) | 1,586 (8.4%)   | 1,619 (9.9%)   | ~flat    |
| minifier-reroll | 7,350 (15.4%)  | 5,377 (28.5%)  | 4,043 (24.6%)  | −25%     |

The retry visibly cleared the swap families: `G→R`/`R→G` (215 occ in
round 1), `C→E` (94), `F→g` (89) are gone or heavily reduced (`g→F` 62
remains — partially in truncated megafunctions). Run health: parse OK,
invariant clean, genuine-change stable, 8m23s.

## What remains, mapped to tracked work

- `QH→dH` (474) + ~500 more: **factory vars** — declarations stripped at
  unpacking make them free identifiers (unrenameable, and they poison
  ~2,600 referencing functions' hashes). Fix at the Bun unpacker with
  stable hash-derived names.
- `y6→C6` (98), `HK→qK` (57) + ~1,100 more: **wrapper-scope classes** —
  invisible to the graph; votes exist and are discarded.
- `$_→w_` (111), `v6→X6` (114), `vH→T_` (104): **megafunction
  truncation** — locals beyond the 500-line LLM cap never get named in
  either leg.
- `serializeWithErrorHandling → stringifyWithTemplate` (422): the same
  structurally-changed function got a THIRD different LLM name this run
  (`convertObjectToJson` in round 1) — close-match naming instability;
  needs prior-name anchoring in the close-match prompt or naming
  determinism work.
- Long-tail transfer-gap (10.8K occ): remaining ambiguous buckets
  (identity arrows `H => H` with zero reference evidence, buckets whose
  referenced things are themselves unmatched — factory vars and classes
  again), plus LLM re-rolls in close-matched functions.
