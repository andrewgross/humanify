# exp155 — the down-tree index (library-names plan step 2b: closing exp153's refutation gap)

STATUS: DONE (2026-10-26). Index recorded and re-merged, census re-run, every rule
simulated before building, one rule built (`tie-break-down`), measured on dev and shot
once at `bun-held`. **The default is unchanged (every new key ships OFF); every number
here is an off-default measurement.**

Andrew's directive (2026-10-26): library-matching evidence must look only **DOWN** the
graph — a unit's recursive callee tree, crossing package boundaries and below the index's
20-node row floor — never UP (callers are app integration code; INTO stays unread). This
is a **declared deviation from the cross-version matcher**, documented in
`docs/responsibility.md` (`libindex::callctx` row) and the module docs, and it lives in
the library profile only — `matching/cascade` is untouched.

## What was measured first: the index (`edges/2`)

`exp153`'s census could not refute safely: 9.1–9.2 MB of RIGHT claim bytes per bun-dev
set refuted because a missing callee was under the row floor or in another package's
rows — "unknown to the index" and "recorded by someone else" looked alike. The fix is a
flat callee-print table: **every distinct callee print any edge row records, keyed, with
its recorders** — INSIDE under the row's owner, OUT-OF under the callee's own package —
so a print with no unit row of its own (under the floor, stop-listed, row dropped) is
queryable by key.

- Format split: every module shard still keys `edges/1` (unchanged rows, unchanged shard
  keys — exp152's store of 197,529 shards re-merges with **zero re-fingerprints and zero
  installs**; the 680 `no-shard` refusals are byte-identical to exp152's), while the
  readable edge-data stamp moves to `edges/2` = rows + the derived `callee-prints.<slice>.bin`
  (magic `HXCLPR01`, u64 key → sorted recorder owner ids). A missing file at load is a
  refusal, never an empty table; a `/4` manifest stamped `edges/1` is refused loudly with
  both spellings (default runs read no index at all — `library.matching` defaults `off`).
- Re-merge cost: **3,411 s** wall, 116.7 GB peak RSS (under the 3-hour STOP threshold), all
  15 shared manifest tables byte-identical to exp152's; the only additions are the two bins.
- Size: **+51,589,781 bytes (+0.168%)** — 25.6 MB + 26.0 MB bins over exp152's 30.76 GB.
  Index id `a582df5b7f5d0ef7aff3ab2a4791146d3059370240956b11de86c532dc1d9fd1` at
  `/work/exp155-data/ix/full-bun-edges2-asof20261025`.
- Coverage: `bun-bun-cjs-min` 1,227,631 distinct callee prints (29.7% rowless);
  `bun-node-cjs-min` 1,246,399 (31.0% rowless = 386,048 prints answerable today only
  through the flat table). Under-floor callees were already recorded in edge LISTS; the
  gap was key-queryability, and that is what closed.

## The census, re-run on `edges/2` (bun-dev 90 bundles, cc 1; main defaults)

Instrument inertness re-verified like exp153: **90/90 bun-dev + 1/1 cc claims
byte-identical** to the same-index base labels (`bun-dev-exp155-edges2-base`,
`cc-exp155-edges2-base`, scored FIRST with main defaults; both byte-identical to exp153's
/4 bases — the index change and the binary change no claim under main defaults). The
SCC-collapsed DOWN call-tree hash re-measured on completed edges: **tree rows byte-identical
to exp153's census** (a rowless callee is a leaf on both sides, by design).

New per-pick readings: `knownFit` (Known scope — an unheld callee the index knows NOWHERE
is skipped), `foreignFit` (Family scope — a callee the accepting family holds ANYWHERE is
also skipped), each with a `silent` verdict (nothing counted) beside
confirm/refute/none, and every missing callee's `recordedBy` (the flat table's answer).

**The headline — innocent claim bytes that refute** (fit = exp153's plain reading):

| set          | fit:refute:right (before) | known:refute:right | foreign:refute:right | change |
| ------------ | ------------------------- | ------------------ | -------------------- | ------ |
| bun-dev-bun  | 9,103 KB (3,128 units)    | 1,695 KB (907)     | 1,168 KB (487)       | −87%   |
| bun-dev-node | 9,231 KB (2,435)          | 1,297 KB (451)     | 1,086 KB (405)       | −88%   |
| cc           | 174 KB (6)                | 99 KB (4)          | 75 KB (3)            | −57%   |

And **the wrong side falls alike**: fit:refute:wrong 36 KB → 7 KB (known) / 6 KB
(foreign) on bun-dev-bun, 47 → 9/7 KB on bun-dev-node. What the callees behind those
innocent refutations were (`claimRefuteScope`, refute picks' missing callees):

| class                                               | bun-dev-bun | bun-dev-node |
| --------------------------------------------------- | ----------- | ------------ |
| absent — the index never saw it (silence now)       | 12,548 KB   | 10,525 KB    |
| family — the family records it elsewhere (skip)     | 1,392 KB    | 247 KB       |
| other-family — only others record it (still counts) | 1,422 KB    | 1,229 KB     |

So: the refutation gap closed exactly as designed (87–88% of innocent refute bytes became
silence or family-held skips), **but the right:wrong refute ratio stays hopeless**
(bytes: 253:1 → 195:1; units: 52:1 → 37:1 on bun-dev-bun) — the residual other-family
refutations on right claims are version skew and shared helpers, not wrong families.

## The simulation, every rule priced before any build

`/work/exp155-data/sim-{bun-dev,cc}.json` (first-order, priced against the re-scored
edges/2 bases). Bar: **P ≥ base AND F0.5 ≥ base − 0.002 on every dev set, app claimed 0.**

| rule (scoped signal)             | bun-dev-bun dP / dF05  | bun-dev-node dP / dF05 | cc dP / dF05 | verdict  |
| -------------------------------- | ---------------------- | ---------------------- | ------------ | -------- |
| DOWN veto (`refute/foreign`)     | +43e-6 / **−0.00314**  | +52e-6 / **−0.00271**  | 0 / −0.00980 | FAIL     |
| DOWN veto (`refute/known`)       | +49e-6 / **−0.00458**  | +60e-6 / **−0.00325**  | 0 / −0.01305 | FAIL     |
| DOWN support (`support/foreign`) | +35e-6 / **−0.00205**  | **−65e-6** / −0.00189  | 0 / −0.00710 | FAIL     |
| DOWN support-or (`foreign`)      | **−7e-6** / +0.00106   | **−114e-6** / +0.00081 | 0 / +0.00253 | FAIL     |
| relax-floor (`foreign`)          | **−14e-6** / +0.00179  | +24e-6 / +0.00394      | 0 / 0        | FAIL     |
| relax (`foreign`)                | **−60e-6** / +0.00184  | **−10e-6** / +0.00401  | 0 / +0.00110 | FAIL     |
| **tie-break (`foreign`)**        | **+193e-6 / +0.00018** | **+351e-6 / +0.00034** | 0 / 0        | **PASS** |
| tie-break (`known`)              | +193e-6 / +0.00018     | +259e-6 / +0.00025     | 0 / 0        | PASS     |
| tie-break (`tree`)               | −107e-6 / −0.00010     | −206e-6 / −0.00020     | 0 / 0        | FAIL     |

Family scope (`foreign`) is the strongest tie-break (bun-dev-node: −31.2 KB wrong vs
−23.0 for `known`, −22.7 for plain fit) — so **only the tie-break with Family scope was
built**: `library.lookup.call_context = "tie-break-down"` (default OFF).

## What was built (default OFF)

- `CallContextRules::scope` (`libindex::lookup`): `FitScope::All` keeps exp153's
  package-level tie-break confirm byte-identically; `Family` routes the tie-break through
  `call_fit_scoped` at family level (`down_tie_family`): a tie resolves to the one tied
  family whose DOWN-scoped fit confirms — an unheld callee the index knows nowhere, or the
  family holds anywhere, is skipped instead of counted against. No unique family, no
  resolution (silence never resolves); an index without the flat table is silence too
  (the fallback pick stands). No other role reads the scope.
- Named setting `"tie-break-down"` under the existing key (config doc updated).
- Red/green: `lookup_test::the_down_scoped_tie_break_resolves_a_tie_the_plain_fit_cannot`
  (plain fit refutes both tied packages → alphabetical fallback; the DOWN fit skips the
  unknown callee and names the family that holds every recorded callee; a no-table index
  stays silent), plus the two index fixtures in `humanify-index`'s `edges_test` from the
  table work: a library calling into an under-20-node helper and its own cross-package
  dependency (both visible to the scoped fit today, invisible to rows), and a wrong-family
  claim whose callees genuinely diverge (`foreignFit` refutes it — the kill-side evidence
  exists; the veto still fails the bar above) beside an unknown callee that stays silent.

## The measured rule (edges/2 index; bases re-scored first)

| set          | variant        | P (Δpt)         | R (Δpt)        | F0.5 (Δpt)     | wrong KB | app |
| ------------ | -------------- | --------------- | -------------- | -------------- | -------- | --- |
| bun-dev-bun  | base           | 99.825          | 81.593         | 95.554         | 146.4    | 0   |
|              | tie-break-down | 99.827 (+.002)  | 81.633 (+.041) | 95.567 (+.013) | 145.0    | 0   |
| bun-dev-node | base           | 99.742          | 82.702         | 95.795         | 228.7    | 0   |
|              | tie-break-down | 99.746 (+.004)  | 82.757 (+.055) | 95.812 (+.018) | 225.3    | 0   |
| cc (sample)  | base           | 100.000         | 81.903         | 95.768         | 0.0      | 0   |
|              | tie-break-down | 100.000 (+.000) | 81.903 (+.000) | 95.768 (+.000) | 0.0      | 0   |

The bar holds measured, not just simulated: precision and F0.5 up on both bun dev sets,
wrong bytes down (−1.4 / −3.4 KB), app 0 everywhere, cc flips nothing. But note the
rule-11 shape: the first-order sim priced `tie-break/foreign` at +0.019/+0.035pt
precision; the measured rule moves +0.002/+0.004pt — propagation eats most of the
first-order win, and the measured rule is a touch WEAKER than exp153's plain `tie-break`
on the dev sets (plain measured +.003/+.005pt P, wrong −2.2/−3.9 KB on the /4 index).

### bun-held, one shot (summaries only, never diagnosed)

| set           | variant        | P (Δpt)        | R (Δpt)        | F0.5 (Δpt)     | wrong KB | app |
| ------------- | -------------- | -------------- | -------------- | -------------- | -------- | --- |
| bun-held-bun  | base           | 99.974         | 83.591         | 96.203         | 5.5      | 0   |
|               | tie-break-down | 99.944 (−.030) | 83.537 (−.055) | 96.167 (−.037) | 11.9     | 0   |
| bun-held-node | base           | 99.708         | 80.986         | 95.302         | 64.9     | 0   |
|               | tie-break-down | 99.705 (−.004) | 81.002 (+.015) | 95.304 (+.002) | 65.7     | 0   |

The held-out bun regression is the SAME one exp153's plain tie-break hit, to the digit
on bun-held-bun (−0.030pt P, wrong 5.5 → 11.9 KB): on this split the DOWN scope
resolves exactly the ties the plain fit already could, the same wrong way. The base is
also byte-identical to exp153's /4 bun-held base — the edges/2 index changes no claim
under main defaults there either. Absolute scale is tiny (11.9 KB wrong on ~30 MB
named) but the sign is real, and the measured band is 0.

## Verdict

**The default stays OFF; nothing flips without Andrew's call.** The index question is
closed: `edges/2` costs +0.17% and re-merges exp152's store with no re-fingerprint, and
it does exactly what it was built for — innocent refutation bytes fall 9.1–9.2 MB to
1.1 MB per bun-dev set, and the silence it buys is honest (absent stays absent). What it
does NOT buy is the veto: the wrong side shrinks with the right side (36–47 KB → 6–9 KB)
and every refute-side rule fails the dev bar by F0.5; DOWN support and the relaxers fail
it by precision. The one dev-bar-clean survivor is the tie-break with its evidence
scoped DOWN — `tie-break-down`, the first rule that reads the deviation — but it does
not clear the held split either (the same bun-held-bun regression as exp153's plain
tie-break), so it ships off-default with the rest. The census's residual refutation
mass (`other-family` on right claims, 1.1–1.4 MB per set) is version skew and shared
helpers, not wrong families — no index-side scope can tell those apart, and that is
what a future veto would have to solve first.

## Gate and neutrality

PLACEHOLDER-GATE-TABLE

## Where things live

- Index: `/work/exp155-data/ix/full-bun-edges2-asof20261025` (`a582df5b...`);
  `/work/library-index/current` NOT moved.
- Bases and variant labels:
  `experiments/096-library-matching-score/results/{bun-dev,cc,bun-held}-exp155-edges2-*`
  (bun-held: summaries only).
- Census aggregates and simulations: `/work/exp155-data/census-full-{bun-dev,cc}.json`,
  `/work/exp155-data/sim-{bun-dev,cc}.json`; per-bundle census reports under
  `/work/exp155-data/census/`; run logs `/work/exp155-data/*.log`.
- Merge/scoring scripts: `/work/exp155-data/{merge,post-merge,score-variants,run-held-neut}.sh`,
  coverage `experiments/155-down-tree/scripts/measure-index.py`.
- exp153's census/sim scripts were extended in place to read the scoped fields
  (`experiments/153-call-context/scripts/{census,simulate}.py`); they reproduce every
  exp153 category byte-identically (only the new scoped keys are added).
