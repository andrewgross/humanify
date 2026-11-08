# exp201 — the minifier-change hop as the eval's fifth pair

STATUS (2026-11-08): code half DONE (branch `exp201-fifth-pair`); references
and bands: see "References" below.

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
