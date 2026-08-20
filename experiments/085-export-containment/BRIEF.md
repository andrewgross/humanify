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
