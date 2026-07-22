# Family-rotation / head-flip repair ceiling — NO-GO (2026-07-22, post-pin)

Measured on the pin-rebased trees (fresh vs regenerated prior), all 4 pairs,
by `ceiling-family-rotation.ts`: for every noise statement, can substituting
the statement's OWN declared names (positionally paired with its unique
hash-twin, or a reciprocal-unique token-overlap partner inside a family
bucket) reproduce the prior text byte-for-byte — and is that substitution
collision-safe (prior name dead on the fresh side, fresh name novel)?

| pair    | noise ln | unique flip SAFE | family pairable flip | unique OTHER | family other |
|---------|----------|------------------|----------------------|--------------|--------------|
| 85→86   | 37,143   | **0**            | 146 ln               | 29,788 ln    | 7,203 ln     |
| 118→119 | 6,763    | **0**            | 65 ln                | 5,455 ln     | 1,241 ln     |
| 197→198 | 13,013   | **0**            | 87 ln                | 10,134 ln    | 2,787 ln     |
| 215→216 | 7,605    | **0**            | 19 ln                | 6,070 ln     | 1,484 ln     |

Verdict:

- The isolated head-flip class is EXTINCT post fn-head pin + statement-twin
  (risky remainder: 2-3 tiny statements/pair). The levers shipped 2026-07-22
  harvested it fully.
- Family-bucket pairable mass is ~300 ln total — below build threshold.
- The residual (~51k ln) is "unique-twin other": COUPLED RENAME WEBS.
  A noise statement's diff tokens mix its own declared names with
  references to OTHER flipped roots (changed-leaf chains, lazy-init
  rotation) — no single-statement substitution explains it, so any fix
  must solve the web jointly (fixpoint inheritance across statements).
  The reconcile pass does fixpoint rounds but is corpus-gated off on
  shuffle pairs (85→86: 24.7% aligned); an in-pipeline web solver is a
  major design, not a lever.
- Family "other" (~12.7k ln) is same-hash tiny statements without enough
  descriptive tokens to pair — LLM-floor tail.

Next candidate framing (needs its own ceiling): iterated echo-web
inheritance — pin provable roots, re-diff, repeat until fixpoint,
order-independent. Do not build without measuring convergence on 86.

## Addendum: echo-web fixpoint NO-GO → residual shape = twin-local transfer GO

`ceiling-echo-web.ts` (iterated cross-statement inheritance): 1 rename
total across 4 pairs — the residual has no cross-statement evidence
structure. `diagnose-residual-shape.ts` explains why: EVERY unique-twin
noise statement is pure rename shape (misaligned: 0 st on all pairs),
differing only at non-property identifier slots INSIDE the statement:

| pair    | aligned-locals    | aligned-props | family buckets |
|---------|-------------------|---------------|----------------|
| 85→86   | 490 st / 29,375 ln| 2 / 419       | 1,228 / 7,349  |
| 118→119 | 44 st / 5,454 ln  | 0             | 445 / 1,306    |
| 197→198 | 136 st / 10,052 ln| 5 / 87        | 773 / 2,874    |
| 215→216 | 52 st / 6,030 ln  | 1 / 72        | 361 / 1,503    |

= 722 st / 50,911 ln (79% of residual noiseLn). Slot kinds: cold-fn
internal locals (incl. J↔M swaps, 30 slots in one 1,183-ln statement),
below-floor minted bindings that missed inheritance (__m←languageCodeMap),
free-identifier mint drift (initEnvironmentVal's probe vars — every pair).

NEXT BUILD (twin-slot descent): extend statement-twin to pair ALL
differing identifier slots of a gated unique twin (descend into nested
fn locals), transfer through validated rename (swap machinery handles
J↔M), freeze owner fns from the LLM. Positional slots inside a
hash-equal statement are exact-grade testimony; no cross-statement
identity risk for internal locals.
