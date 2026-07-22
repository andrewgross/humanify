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
