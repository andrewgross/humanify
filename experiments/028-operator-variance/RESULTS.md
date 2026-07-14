# exp028 — operator-variance normalization: measured, NOT a lever

**Verdict: operator normalization recovers ZERO cross-version function matches
on the primary use case (same-minifier consecutive versions). Do not build it.**

## Hypothesis (from a lost note, recorded in memory)

"Operator normalization is the biggest remaining naming-consistency win."
Idea: canonicalize semantics-preserving operator forms so the same source that
minifies to different shapes across versions stops producing diff/hash noise.

## What the structural hash actually distinguishes

Probed `computeStructuralHash` directly. Of the candidate forms, only these
change the hash (raw parse):

| form                              | hash         | note                                                        |
| --------------------------------- | ------------ | ----------------------------------------------------------- |
| `!0`/`true`, `!1`/`false`         | differ       | not normalized anywhere                                     |
| `void 0`/`undefined`              | differ (raw) | **production beautifier already normalizes** (babel plugin) |
| `a["lit"]`/`a.lit`                | differ       | but minifiers already emit dot form                         |
| `1e3`/`1000`, `0x10`/`16`, quotes | SAME         | hash keys on parsed `.value`, skips `.extra`                |
| `===`/`==`                        | differ       | correct — semantically different, must never normalize      |

So at most 4 forms are candidates; production already handles `void 0`.

## Ceiling measurement (`measure-ceiling.ts`)

For each v1 function with NO same-hash twin in v2, does it gain one under a hash
that normalizes all 4 forms? That count is the absolute ceiling.

```
CC 2.1.118 → 2.1.119   56,835 fns   base-match 99.11%   recovered = 0 (0.00%)
CC 2.1.119 → 2.1.120   59,320 fns   base-match 98.85%   recovered = 0 (0.00%)
positive control (!0 vs true)                            recovered = 1  ✓
```

- `memberDot` rewrites = **0** in the real bundles: Bun already emits `a.b` for
  every identifier-valid key. Nothing to normalize.
- `!0`/`!1`/`void 0` fire ~30k times per bundle — but _identically on both
  sides_ (the minifier is deterministic), so normalizing them changes which
  functions match by **0** (it even net-lost 1 match on 118→119).

## Why (the mechanism)

Same-minifier consecutive versions emit canonical operator forms
deterministically, and humanify never rewrites operators. So for any unchanged
function the operator shapes are byte-identical on both sides — there is no
variance for a normalizer to remove. Operator normalization would only help
when matching across DIFFERENT minifiers, which is not the cross-version
caching use case.

## Redirect

Matching is already ~99% at the pure-hash level (before the cascade). The
residual diff noise is NOT operator-shape and NOT (mostly) match failure — per
exp016 it is LLM naming instability among the small non-exact-match tail +
ambiguous-bucket reservoir. The next measurement should characterize the ~1%
non-exact-match set (genuine change vs recoverable instability) with
`inspect-hash-divergence.ts` before building anything — do not assume a lever.

Measurement is keepable: `measure-ceiling.ts` (positive control included).
