# exp093 ADDENDUM — the vendor-inclusive M2 (2026-10-01)

## STATUS

- **State: MEASURED (2026-10-01) — the vendor-inclusive M2 the README's
  original STATUS block deferred "pending the coordinator". DONE, and it
  strengthens the LAND verdict: on the vendor surface the policy change is a
  small strict improvement (same 229 pairs, 3 of them corrected from
  blur-mispairs to true twins, ambiguity −18 with 0 new); on the runtime
  surface it is a LARGE strict improvement against the post-cutover prior
  (+461 pairs, −35 ambiguous rows, binding stillAmbiguous −56, binding
  unmatched −728) — same direction as the original M2, ~460× the amplitude,
  for a prior-shape reason explained below. No negative result to report:
  the feared "vendor's generic factories get MORE collision-prone under
  exact numbers" does not happen — vendor ambiguity is STRUCTURAL (giant
  same-shape, number-free families) and invariant under the number policy.**
- Everything here is LLM-FREE evidence: the match verb has no LLM path, and
  the prior was generated against the deterministic stub
  (`scripts/lib/stub-llm.ts`), never a model. No pipeline code was touched.
- Measurement artifacts: `/work/exp093-vendor-remeasure-2/`
  (`RUN2-MANIFEST.md` = inputs, binary sha256s, build provenance, wall
  times, output inventory). The PAUSED first attempt is kept at
  `/work/exp093-vendor-remeasure/` with its own manifest — its legs were
  terminated at the 2h mark with no dumps written. The old 12h dump under
  `/work/match-revalidation/` is NOT used anywhere: it was built from a
  dirty tree and unpacks different content (findings #68's lane).

## Method (replicates M2 verbatim, one coverage change)

Same pair (claude-code 2.1.215 → 2.1.216), same dump fields, same ambiguity
metric as the README's M2 — `compact.py`'s conventions applied to the
multi-file dump shape by the lane's `compact2.py` (per-surface stats, pairs
by tier, ambiguous rows + candidate-count histogram, runtime binding-cascade
block, matched-prior union), pair identity
`(prior_start, section, fresh_start)`. The ONE coverage change is the point
of the re-run: the fresh side is the whole unpack tree (runtime.js + 1,647
wrapped vendor factory files; 82,152 fresh rows = 64,493 runtime + 17,659
vendor — the corpus shape findings #68 recorded), so every number breaks out
runtime / vendor. ONE definitional note, forced by the dump shape: the
multi-file dump does not carry the per-call `unmatched` prior block, so the
original M2's `unmatchedPrior` is replaced by the matched-prior UNION and
its complement — computed identically on both legs.

**Both policies on identical input, one code delta.** NEW = main @
`3e9aab2d` (exact numbers, `FACTORY_HASH_VERSION` 3); OLD = `3e9aab2d` +
`git revert 43088432` = `ed01a247` (magnitude buckets, era 2), both built
`--release --locked` in clean detached worktrees; the revert touched only
exp093's own crates files. Binary polarity proven before the legs by a
forced-class smoke (`n*500` vs `n*900`: OLD → one 2-candidate ambiguous row;
NEW → unique `structuralHashUnique` pair with the true twin).

**The prior is the post-cutover humanified 2.1.215 tree** (generated with
the current release binary against the stub: 53,129 asks, ~16 min, 55.7 GiB
peak, sha256 `dee04eed…`; byte-identical to the paused lane's
already-generated tree, `diff -r` 0 files — which also proves the
`e12c94d2`→`3e9aab2d` delta does not move pipeline output).

- NEW leg: exit 0, wall **6h22m31s**. OLD leg: exit 0, wall **6h23m04s**
  (concurrent legs, combined RSS 14–15 GiB; this is findings #69's residual
  cost — #68's open close-tier question and the twins phase — recorded, not
  fixed here).

**One join-key correction, recorded as a lesson.** The lane's compact2
keyed a vendor section by sha256(freshText), which is NOT stable across
policy eras: the wrap re-injects each module's hash-derived runtime
identifier (`lib_22005f91()` → `lib_e12cfa3f()`), so 761/1,646 wrapped
files' texts differ between the legs (same-length; fresh spans agree). The
raw key manufactured churn — 114/229 vendor pairs "lost"+"gained", 125k
ambiguous rows "resolved"+"reappeared" with candidate-count-changed 0 —
which is the join moving, not the decisions. `compact3.py` blanks the
`lib_` ids before hashing (per-leg stats byte-equal to compact2's; only the
cross-leg join changes). All vendor numbers below are under the normalized
key; `compare2.txt` (raw key) is kept as the artifact of the lesson.

## Results (A/B old-buckets vs new-exact, identical inputs)

Prior rows 63,723 on both legs; fresh rows 82,152 (64,493 runtime + 17,659
vendor) on both legs; 63,723 = the same program's function count the TS-era
prior had — same program, same functions.

| metric                  | OLD (buckets) | NEW (exact) | delta                                                        |
| ----------------------- | ------------- | ----------- | ------------------------------------------------------------ |
| matched prior (union)   | 61,564        | 62,025      | **+461**                                                     |
| unmatched prior (union) | 2,159         | 1,698       | −461                                                         |
| runtime pairs           | 61,563        | 62,024      | **+461** (common 61,562; LOST 1, GAINED 462)                 |
| runtime ambiguous rows  | 900           | 865         | **−35** (36 resolved, 1 appeared)                            |
| vendor pairs            | 229           | 229         | **0** (226 common; 3 lost + 3 gained — see below)            |
| vendor ambiguous rows   | 171,942       | 171,924     | **−18** (18 resolved, 0 appeared, candidate-count-changed 0) |
| binding stillAmbiguous  | 389           | 333         | **−56**                                                      |
| binding unmatched       | 999           | 271         | **−728**                                                     |

Tier attribution (runtime): 59 COMMON pairs promoted into
`structuralHashUnique` (34 enclosingStatement, 19 propagation, 4
calleeShapes, 1 memberKey, 1 ordinal — the same promotion table as the
original M2's runtime), plus 11 lateral weak-tier shifts
(6 interchangeable→propagation, 3 interchangeable→ordinal, 2
enclosingStatement→memberKey) — same decision classes, different rung.
Vendor pairs by tier are byte-identical between legs (216
structuralHashUnique / 6 memberKey / 2 ordinal / 2 calleeShapes / 2
shingleSimilarity / 1 enclosingStatement).

The mega-classes shrank rather than moved: the runtime 226-candidate class
loses its number-differing members (26 rows → 18 rows of 218 candidates),
the 626-candidate class 160 → 148 rows of 617; in the resolvable 2–8
candidate band runtime ambiguity drops 274 → 261.

### What the 462 runtime gains are

The gained prior rows are the post-cutover mirror's dense
module-initializer closure families — `() => { A = w(lib_<hash>(), 1); B =
A.default; }` and friends — clustered in the mirror's re-link regions
(buckets 2.4–3.4M: 320 of them; 18.2M: 95). Under buckets these same-shape
families merged into the 226/626-candidate classes and matched nothing;
under exact numbers the prior rows whose numbers are actually equal resolve
uniquely. This is exactly the class the number-blur study predicted, at far
larger amplitude on the post-cutover prior — the TS-era mirror that the
original M2 used presented these functions differently (5-param wrapper
layout), which is why the original saw +1 pair and this run sees +461. The
one loss is the original M2's loss, the same interchangeable family
(`(7927216, 7663324)`), legal-equal under both policies.

### The 3 vendor swaps — all corrections, in the good direction

- LOST: an `[0-9A-Za-z]` predicate the OLD policy paired to an
  `[0-9A-Fa-f]` hex predicate (90/70 share a bucket, 122/102 share a
  bucket); `p => p.slice(1)` paired to `s => s.slice(2)`; an
  `{400,413,422}` status set paired to `{105,109,115}`. All three are
  DIFFERENT functions that the buckets merged — NEW refuses them.
- GAINED: true exact twins the blur had hidden in the giant families:
  `hol(2, e)` ↔ `nJh(2, e)`, `>=48 && <=57` ↔ `>=48 && <=57`,
  `RNh(e, 32)` ↔ `pAi(e, 32)`.

### Does vendor behave like runtime?

**It diverges — by being nearly INVARIANT.** Runtime: the number policy
moves 497 pairs and 35 ambiguous rows. Vendor: 6 pairs of 229 and 18
ambiguous rows of 171,942 (−0.01%), every change a correction, nothing new
appeared. Vendor ambiguity is dominated by number-free structurally-generic
families — 33,839 rows with 3 candidates, 7,441 rows each with 7, 8 and 17
candidates, 94,207 single-candidate rows — which no number policy can
split. That is the honest boundary: the vendor matcher's ambiguity floor is
structural, and exact numbers neither hurt it nor can fix it; they only
correct the handful of cases where numbers were doing real work.

### Vendor pairing against the post-cutover prior (informational)

**Still 229 of 17,659 vendor functions pair — the same count findings #68
recorded against the TS-era prior, under BOTH policies.** The
factory-wrapper-shape hypothesis for the 229 is therefore dead: with a
2-param post-cutover prior the function matcher still pairs only ~1.3% of
vendor fresh rows (and 228 of those 229 prior rows are also matched by the
runtime section — vendor's ownership stays with content keys, exp046/047).
This retires the "re-read the 229 against a post-cutover prior" follow-up
in findings #68's row.

### Row-#68-type skips (recorded, not chased)

Both legs loudly skipped the SAME file, `vendor/lib_74234e98.js`, with the
SAME error — `estree json program body missing` (not the `close-dump: row
JSON unavailable` error #68 recorded; same file, deterministic, symmetric
across legs, so it biases neither side of the A/B; the close-tier question
stays OPEN per findings #68/#69).

## Reads

- The LAND verdict is strengthened everywhere: zero real pairs lost (the
  single runtime loss is the same legal-equal interchangeable family as
  before; the 3 vendor losses are corrected MISpairs), and vendor — the
  unmeasured half the original M2 deferred — turns out to be the surface
  where exact numbers do the CLEANEST work: small, strictly corrective, no
  new ambiguity.
- The one prior-shape caveat cuts the other way: the +461 runtime gain is
  measured against the POST-CUTOVER prior (the right regime going forward),
  and it is much larger than the original M2's +1 — the original number
  UNDERSTATED the change's benefit because the TS-era prior could not
  present the initializer-closure families it now disambiguates.
- Cost: unchanged per the original M3 (~6.4h per match leg at
  walk-corpus scale is the INSTRUMENT's wall, findings #69, not a pipeline
  effect; no naming/ask surface is touched by a match-verb A/B).
- Follow-up for the ledger: none for exp093. The `lib_<hash>`-in-wrap
  join-churn artifact is recorded in `RUN2-MANIFEST.md` for whoever next
  A/Bs dumps across hash eras.
