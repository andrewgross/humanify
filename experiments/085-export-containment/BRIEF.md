# 085 — BRIEF (hypothesis): containment rescue for the last displaced module

> Decision context (Andrew, 2026-08-20): "Lets test this, I worry it could
> cause incorrect code."

## Safety first: what a wrong module match CAN and CANNOT break

Module matching (`fossil-match.ts`) decides exactly one thing: which FILENAME
a module inherits. The emitted require graph is derived from the bundle's
actual import edges and the final file paths — never from the match — and the
concat-equivalence assertion plus the e2e stage hold regardless. **A wrong
match cannot produce incorrect code; the failure mode is a misleading
filename** (which is precisely the noise exp082 removed three cases of).
So the risk to manage is a wrong NAME, and the instrument for it is sampling
what a looser rule would newly match.

## The case (post-veto walk, the one remaining displacement)

`create-vqs-component`: the true heir gained exports (4 declared → 6, 3
shared). Jaccard = 3/(4+6−3) = 0.43, below the export-set floor 0.6, so the
heir mints `-2` (78 lines) while a graded-content match occupies the name.
Containment (|intersection| / |smaller set|) = 3/4 = 0.75 sees it clearly.

## The fix to test

A containment tier AFTER `tierExportSet` (so it can never outbid the stricter
rule): match when containment ≥ 0.75, mutual best on both sides, both sets
non-trivial (≥2 names each — a single common name like `initializeApp` must
not travel), and the export-heir-veto semantics apply unchanged.

## Known false-positive class (state it, then sample it)

A big module wholly containing a tiny module's export names scores 1.0. The
≥2-names guard blocks the single-name version; the sample must look for the
multi-name version (two modules sharing a 2-name subset).

## Gates and predictions (pre-registered)

- OFFLINE FIRST (deterministic, exact): replay the matcher on all three
  ledger pairs with the tier on. Predict: calm hops 0 differing pairs; busy
  hop exactly +1 (the vqs heir). ANY other new match is a false-positive
  candidate — eyeball every single one before proceeding; more than a
  handful, or one that pairs unrelated modules, kills the tier.
- red/green unit tests incl. the vqs shape and the shared-subset trap.
- `npm run check` 8/8, matcher preflight 4 fixtures unchanged.
- Cold walk: displaced 1 → 0; `novel`/`realLines` EXACT; calm inside spread.

Ceiling: ~94 git lines on this hop (78 displaced + moved remnants + the
`createVqsComponent → createVqsComponent2` alias flips).

## Offline replay RESULT (2026-08-20) + updated walk predictions (pre-registered)

The "exactly +1" prediction was WRONG in the good direction: 3 clusters, not

1. Every new match was eyeballed: each carries >= 3 inherited export names
   (or the prior's complete set) where the match it replaces carried ZERO, and
   each kills a `-2` mint. Calm hops: 0 differing pairs. The extra two are the
   same absorbed-module shape (upstream merged a small module into a neighbor;
   the merged module contains the prior's whole export set).

Walk predictions (shared walk with exp084, baseline = /work/exp082-walk):

- displaced modules 1 -> 0; suffixed `-2` files 7 -> ~4 (strip-ansi-2,
  create-session-key-2, create-vqs-component-2 mints killed).
- alias-form flips: the `X -> srcX` class ~0 (exp084's fix, ~91 line-pairs);
  killed mints remove their `-2` alias flips; the stripAnsi-class flips
  REMAIN in changed form (the import target genuinely moved — accepted
  regrouping cost).
- nameOnlyLines 5,080 -> ~4,850 (rough; the two fixes' classes summed).
- `novel` / `realLines` EXACT at 986 / 122,066 (alias choice and module
  matching cannot touch statement mass — any movement refutes).
- calm hop inside the 32-line spread; every hop passes the (now actually
  called) boot gate.
