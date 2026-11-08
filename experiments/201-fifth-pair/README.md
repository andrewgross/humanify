# exp201 — the minifier-change hop as the eval's fifth pair

STATUS (2026-11-08): DONE. Code merged to main as `111401fe4` (branch
`exp201-fifth-pair`); three cold names-mode references on five pairs
(`ref-names-111401fe-r1/r2/r3`, host-only) and the names bands + names
self-hop range re-measured from them (branch `exp201-bands`, merged). The
names bands are no longer bimodal: noiseLn ±989 (was ±15,709).

Andrew, 2026-11-07: "make sure this minifier change hop is in our standard
testing setup, we can add it as a 5th".

## Why

The full walk (`/work/walk-9ce9dee2-full`) found that at 2.1.181 Claude Code
was minified differently: about 130 vendor modules changed hash while their
library source did not, and that one hop carried 56,222 vendor noise lines
(walk card `churn.vendor.noise`). None of the four eval pairs (85→86,
118→119, 197→198, 215→216) crosses it. Lane exp200 is fixing the restyle and
needs this pair in the eval to prove the fix.

## What changed

- `dev/eval/pairs.json`: `2.1.179 → 2.1.181` ("bundler restyle (~130 vendor
  modules re-minified, source unchanged)"), inserted in VERSION order — the
  third of five, not the last. The eval self-hops the LAST scored pair's
  `to`, and `self-hop-reference.json` has a range for 2.1.216 only, so
  appending it would have silently moved the self-hop to 2.1.181 and turned
  its verdict into "not judged". `libraryIndex` is unchanged. Inputs and the
  prior exist under `inputsBase` / `priorsBase`.
- **Mixed 4/5-pair comparisons go through the one guard.** New
  `identity::guard_labels` (label-level: the pair sets first, then `guard` on
  every common pair) with one per-purpose rule
  (`Purpose::compares_common_pairs`):
  - **leaderboard**: two labels with different pair sets are compared over
    the pairs BOTH scored. One loud line, `!! COMMON PAIRS: A and B scored
    different pairs (only B: 2.1.179->2.1.181); every total is over the 4
    common pairs (...)`, and EVERY label's totals (the baseline's too) are
    re-folded from its own cards over the common pairs by the summarizer's
    fold (`summarize_cards`). No `--force-mixed` needed; before, the
    leaderboard refused, and `--force-mixed` then printed totals over
    different pair sets side by side.
  - **bands**: repeats over different pair sets are refused, even forced.
    The guard now runs on every pair of every repeat (it compared only the
    first card before). Each measured slot records its pairs
    (`provenance.pairs`).
  - **leaderboard bands line**: when the band slot's pairs differ from the
    compared pairs it prints `bands: MEASURED OVER N PAIRS (...)`; a slot
    measured before exp201 records none and says so.
  - no pair in common: refused, always.
- **Off mode.** Both off-mode references (`ref-scratch-0f338ffa-*`) stay
  four-pair and are not re-measured. A new off-mode label scores five pairs;
  the leaderboard compares it with the off reference over the original four
  (with the `!! COMMON PAIRS` line), and the off bands — measured over those
  four — are the right floor for that comparison. The fifth pair's off-mode
  numbers have no off reference to be judged against.
- Launch goldens: `score_test` now builds its fake corpus from the committed
  `pairs.json` (a hard-coded four-pair list had silently SKIPPED the new pair
  with the goldens still passing). Both goldens re-recorded by the recorder:
  +57 lines each, the 2.1.179 rebase and the 2.1.181 scored leg, every old
  line unchanged.
- `humanify-ag-cli` `launch_parity_test` (half two: every golden launch
  resolves to a CAPTURED launch's settings) failed on the new pair: the
  captures are the TS era's four pairs, and the record holds the input path.
  It now compares records with every `2.1.N` spelled `<V>` — it proves the
  configuration; which version each path names stays pinned byte for byte
  by the dev crate's launch goldens.
- Docs: 034 README pair table and a section on mixed comparisons;
  `docs/responsibility.md` row; `docs/dev-harness.md` §3.2; the verb table's
  proves / cannot-prove text.

## Tests (red, then green)

- `identity_test`: four- vs five-pair labels compare over the common four out
  loud; same sets say nothing; a field mismatch on a common pair is still
  refused, one on a pair only one side scored is not compared; disjoint sets
  refused even forced; bands refuse different pair sets even forced.
- `leaderboard_test`: a five-pair label (frozen reference + a synthetic fifth
  card) against the four-pair reference, both orders: `novel` reads
  `4188 (=)`, so the fifth pair never reaches a compared total; two
  five-pair labels print no line. Band-slot pair-set lines.
- `bands_test`: repeats over different pairs refused, nothing written; a slot
  records its pairs.
- `committed_test`: the five pairs in version order, 215→216 last.
- `score_test`: goldens and launch counts from the committed pairs.

## References (step 4)

Three cold names-mode repeats of the merge commit `111401fe4`, scored from a
frozen worktree (`/work/ref-names-111401fe-frozen`), one after another, each
under `/work/heavy.lock` (one layer: `eval score` does not take the lock
itself, contrary to the brief's note), scorer v2, scratch bases, prebuilt
index `cb64c1f5…`. Results live on this host only, in
`/work/ref-names-111401fe-frozen/dev/eval/results/`.

| check | r1 | r2 | r3 |
| --- | --- | --- | --- |
| 5 scored legs exit 0, no errors, no provider errors | yes | yes | yes |
| 5 rebases (no "rebase FAILED", rebase provider errors 0) | yes | yes | yes |
| library matching `used` with index cb64c1f5 on every pair | yes | yes | yes |
| no LLM cache on any scored leg (cold, rule 10) | yes | yes | yes |
| run-status files present (5) | yes | yes | yes |
| boot gates, both halves | 5/5 | 4/5 → 5/5 re-recorded | 0/5 → 5/5 re-recorded |
| cold self-hop 2.1.216 (diff lines) | 22 | 2 | 8 |
| warm self-hop: identical, 0 files, 0 lines, 0 cache writes | yes | yes | yes |
| novel / realLn (5 pairs) | 5,336 / 556,875 | same | same |
| novel / realLn on the 4 shared pairs | 4,191 / 416,386 | same | same |

**Boot gates.** The Claude login expired during the runs: r2's 2.1.216 and
all five of r3's prompt halves failed with "Not logged in" / "OAuth access
token has been revoked" (every `--version` half passed). After the login was
restored, `cargo dev boot-gate <tree> <version>` re-ran on each of those six
trees as written: all six OK on both halves. Their `*-boot.json` were
re-recorded with a `rerecorded` note saying exactly this (the precedent is
main-2026-09-18), and the labels re-summarized.

**Hold columns.** novel / realLn are byte-equal across the three, per pair,
and on each of the four shared pairs equal to `ref-names-0357696e-r1-s2`
(85→86 787/78,791; 118→119 1,156/79,130; 197→198 1,262/136,399; 215→216
986/122,066). The new pair adds 1,145 / 140,489.

## New names bands vs old (scorer v2, names mode)

Written by `cargo dev eval bands ref-names-111401fe-r1 r2 r3` (generator
only). Totals are over FIVE pairs now (recorded as `provenance.pairs`).

| KPI | old (0357696e, 4 pairs) | new (111401fe, 5 pairs) |
| --- | ---: | ---: |
| noise | ±397 | ±9 |
| noiseLn | ±15,709 | ±989 |
| treeLn | ±2,116 | ±745 |
| cleanLn | ±2,120 | ±773 |
| cleanReal | ±1,042 | ±375 |
| reloc | ±17 | ±24 |
| relocSt | ±2 | ±1 |
| newName | ±34 | ±26 |
| mints | ±22 | ±14 |
| vendorLn | ±8 | ±64 |
| vendorReal | ±96 | ±148 |
| app:noiseLn | ±15,718 | ±791 |
| vendor:noiseLn | ±9 | ±256 |
| novel / realLn | 0 | 0 |

The old width was the bimodal 197→198 pair (fixed by exp178): every repeat
now lands in the low mode. The vendor bands widened, almost all from the new
pair (vendor noise 53,226–53,310 across the three on 179→181 alone). The
names self-hop range is now 2–22 (was 8–8), written into
`self-hop-reference.json` by the same command — `eval bands` gained that
half in this lane (it had no generator; the range used to be hand-copied).

## The fifth pair's own numbers (2.1.179 → 2.1.181, names mode)

| KPI | r1 | r2 | r3 |
| --- | ---: | ---: | ---: |
| vendor churn lines | 88,864 | 88,912 | 89,000 |
| **vendor noise lines (vendorLn)** | **53,310** | **53,226** | **53,298** |
| vendor real lines | 35,554 | 35,686 | 35,702 |
| app naming-noise lines (noiseLn) | 27,919 | 27,670 | 28,209 |
| noise statements | 638 | 629 | 640 |
| novel / realLn | 1,145 / 140,489 | same | same |
| tree churn lines (treeLn) | 33,847 | 33,816 | 33,770 |
| mints | 40 | 28 | 44 |

The walk measured 56,222 vendor noise lines (and 32,782 vendor real) on the
same hop; the eval reads ~53,300 noise / ~35,600 real. The difference is the
base: the walk's 2.1.179 inherited 90+ hops of names, the eval rebuilds
2.1.179 from scratch. Either way this one pair carries 97% of the five-pair
vendorLn total (54,887 of r1's), and ~35,600 lines of it are charged as REAL
vendor change while the library source did not change — that is exp200's
target, and the vendor columns of this pair are where its fix should show.
Comparing against the old reference over the common four pairs (with the
`!! COMMON PAIRS` line) gives noiseLn 49,510–49,960 against
ref-names-0357696e-r1-s2's 65,530 (its high mode).

## Gates

| gate | result |
| --- | --- |
| `cargo dev check`, branch worktree (48fca5ae4) | 11/11 PASS (clones REVIEW, 51 groups, none in this change) |
| `cargo dev check`, clean checkout of merge 111401fe4 | 11/11 PASS |
| `cargo dev check`, bands branch worktree (34807c697) | 11/11 PASS (clones REVIEW) |
| `cargo dev check`, clean checkout of bands merge 40654ebcd | 11/11 PASS |

## Proposed CLAUDE.md text (for Andrew to edit in; not edited here)

Replace the names-reference bullet's band sentence and add a bullet:

> - **`ref-names-111401fe-r1/r2/r3` — the current valid NAMES-mode cold
>   reference, FIVE pairs** (2026-11-08, exp201: the four plus 2.1.179→181,
>   the minifier-change hop; scratch bases, scorer v2, full prebuilt index
>   `cb64c1f5…`). In each, all 5 rebases, 5 scored legs and both self-hop legs exit 0, warm self-hops
>   byte-identical with 0 writes, cold self-hop 22/2/8, boots OK (six prompt
>   halves re-recorded after a login expiry, `rerecorded` note). novel 5,336 /
>   realLn 556,875 byte-equal across the three, and 4,191 / 416,386 on the
>   four shared pairs, equal to `ref-names-0357696e-*-s2`. They set
>   `noise-bands.json` `byScorer.2.byMode.names` and the names self-hop range
>   (2–22). **Names bands are no longer bimodal: noiseLn ±989, noise ±9,
>   treeLn ±745** (the old ±15,709 / ±397 / ±2,116 were the pre-exp178
>   197→198 pair). The fifth pair alone carries ~53,300 vendor noise lines
>   and ~35,600 vendor "real" lines with library source unchanged (exp200's
>   target). Results live on this host only:
>   `/work/ref-names-111401fe-frozen/dev/eval/results/`.
>
> In the `eval score` block: "score current tree on 5 pairs (~2.3 hr names
> mode)", and point `leaderboard` at `ref-names-111401fe-r1`. Add: "A label
> scored before exp201 has four pairs (both off-mode references stay that
> way). The leaderboard compares labels with different pair sets over the
> pairs both scored, with a `!! COMMON PAIRS` line and every total re-folded
> over those pairs; `eval bands` refuses different pair sets."
