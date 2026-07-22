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

## Addendum 2: the mint-poisoning mechanism (proven from the 216 log)

Trace for `__m` / `languageCodeMap` (results/pin-rebased/2.1.216.log):
1. `module-binding: matched vRm→__m` — the cascade matches fresh `vRm`
   to a prior binding whose name is a MINTED LEFTOVER (`__m`).
2. Fresh's minifier coincidentally named a DIFFERENT binding `__m`; its
   prior counterpart is `languageCodeMap`, witnessed by TWO exact votes
   (`exact-match: skipping __m→languageCodeMap` ×2).
3. Fresh-`__m` settles carrying its own minted name (same-name match) →
   not pending → the languageCodeMap votes never tally.
4. `vRm→__m` rejects (`target-in-scope`) at apply AND retry — the token
   is held by the binding from (3). Both bindings end wrong; the mint
   survives another hop (census stays ~flat at 475).

Lever candidate: BELOW-FLOOR PRIOR NAMES ARE NOT NAMES — a match whose
prior name fails the naming floor keeps its identity (context,
eligibility) but must not settle-and-keep or transfer the minted token;
the binding stays nameable (votes/LLM), and "never rename TO a minted
name" gets enforced at the transfer sites (vRm→__m should have been
refused as a downgrade, not attempted).

Text-proxy bounds (crude — string-literal words contaminate the carried
count; binding-level count needs an in-pipeline stat): noise statements
touching carried mints 15/16/5/32 st ≈ 3.1k/4.3k/1.6k/4.3k ln per pair;
pure mint-token diffs 5/5/6/2 st ≈ 3.8k ln total (includes unfixable
free-identifier drift).

## Standing next steps (ranked)

1. Instrument WHY twin-slot bridging left the 52-490 aligned-locals
   statements per pair: code reading says bridgeTwinSlots already pairs
   fn-internal locals of pending owners and the apply registers
   transferred names — so the leak is a gate (candidacy? owner settled?
   pairs applied then lost?). One -vv probe of the 216 leg + targeted
   debug lines answers it; fix follows the finding.
2. Below-floor prior-name lever (above): add the in-pipeline counter
   (matches carrying below-floor prior names) to size it, then build the
   floor guard at settle/transfer sites.
3. Family buckets (~13k ln, tiny statements) and free-identifier drift
   remain the tail after 1+2.
