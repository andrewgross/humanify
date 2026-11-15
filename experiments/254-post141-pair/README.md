# exp254 — a sixth eval pair from the Bun 1.4.1 era

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), the new pair
scored ALONE end to end (`exp254-pair261-s1`). No full six-pair run yet:
a long walk was running on this host and an eval must run alone. Branch
`exp254-post141-pair` from origin/main `7b546062e`. Not merged.

## Totals

- **New pair: 2.1.260 -> 2.1.261.** It is the eval's sixth pair and the
  first one built by Bun 1.4.1, a code-split bundle (1,639 chunks). The
  five existing pairs are unchanged, and so are their launches, byte for
  byte, in `run-launch.golden.txt`.
- **The eval can now see the post-1.4.1 noise.** The new pair alone reads
  **noiseLn 68,392**. The five old pairs together read 67,829 on
  `ref-names-40921b9f-r1`, and the biggest old base (2.1.215 -> 2.1.216)
  reads 4,913. Its noise statements are 713 (1.2%); 215 -> 216 has 286.
- **The single-pair run is clean.** Rebase, scored leg and both self-hop
  legs exit 0. There are no provider errors, the boot gate passes both
  halves (`claude-opus-4-1`), and library matching was `used` with index
  `4736dcfa…` (kind `bun`). The cold self-hop reads 34 lines. The warm
  self-hop is byte-identical with 0 cache writes.
- **Index: keep `4736dcfa…` (pairs.json `libraryIndex`).** Both indexes
  handle 1.4.1 bundles. A pair cannot carry its own index (§2).
- **Comparing 5-pair and 6-pair labels works as it is.** The leaderboard
  folds both labels to their common pairs and prints `!! COMMON PAIRS`.
  It was checked on real reference labels, and the numbers came out
  identical (§3).
- **Harness change: the self-hop pair is marked, not inferred.** Before,
  the self-hop ran on the last pair in pairs.json. Adding 2.1.261 would
  have moved it off 2.1.216, the only version the self-hop reference
  range describes. Now pairs.json marks it (`"selfHop": true` on
  2.1.215 -> 2.1.216), and the loader refuses a file that marks none or
  two.
- **Lines:** code +31 / -11 (4 files), tests +133 / -12 (3 files), golden
  +58, pairs.json +8 / -2.

## 1. Which pair, and why

The per-hop numbers come from walk #5 (`/work/noise-141/phases.txt`;
binary 3cb20421, index 77b12699). The verbatim in-file moved lines come
from `moved.py` on the same walk history:

| hop | real | naming noise | noise / 100 real | in-file moved lines (of src diff) |
| --- | ---: | ---: | ---: | ---: |
| 2.1.251 -> 252 | 5,707 | 32,884 | 576.6 | 71,760 of 88,960 |
| 2.1.258 -> 259 | 56,212 | 4,328 | 7.9 | 11,626 of 79,094 |
| **2.1.260 -> 261** | **50,952** | **5,190** | **10.3** | **30,644 of 84,690** |
| 2.1.272 -> 273 | 59,180 | 3,118 | 5.4 | 11,566 of 85,042 |
| 2.1.274 -> 275 | 80,354 | 6,482 | 8.3 | 55,152 of 160,280 |

The median post-1.4.1 hop reads 8.7 noise per 100 real.

2.1.260 -> 2.1.261 meets both requirements in one pair:

- It is **typical**: moderate real change (51k lines) and a noise ratio
  near the median.
- It **exercises in-chunk reordering**: 36% of its src diff is code moved
  verbatim inside a file. Most of it sits in the same file that dominates
  251 -> 252.

2.1.251 -> 2.1.252 is the purer reorder probe, with almost no real
change. It is also the least typical hop (576 noise per 100 real). It is
the documented candidate for a seventh pair (§5).

**Why one pair and not two: cost.** A post-1.4.1 scratch rebase is a full
cold run of a 43 MB bundle. Here it took **2,609 s** at 17.9 GB. The eval
runs one rebase at a time, so each such pair adds about 45 minutes to the
critical path. That is about half of today's ~95-minute eval.

## 2. The library index

- `4736dcfa…` (pairs.json) and `77b12699…` (exp213, used by every
  post-1.4.1 walk) have the **same plan**: format `humanify-library-index/4`,
  as-of 2026-10-25, the same 9,896 packages and 65,839 versions, and the
  same two Bun 1.4.0 slices (`bun-bun-cjs-min`, `bun-node-cjs-min`).
- `77b12699` is a `--replay` of `4736dcfa` with exp213's source-confirm
  rule. It drops 0.06% of unit rows, and module rows are identical. It
  was never adopted.
- A 1.4.1 bundle is detected as kind `bun`, which both indexes fit. The
  single-pair run proves `4736dcfa` on a 1.4.1 input: `outcome: used`,
  `kind: bun`.
- **A pair cannot carry its own index.** The index is one setting per run
  (`library.index` in every launch). The label records one `libraryIndex`
  in `pipeline.json`, and `guard_label` and the identity guard refuse a
  label or comparison that mixes indexes.
- So the new pair uses `4736dcfa` like the other five. Adopting `77b12699`
  is a separate decision: it would change every pair and need a new
  reference.
- One consequence: walk numbers for this hop (index `77b12699`, binary
  `3cb20421`, a walk prior instead of a scratch base) are not
  directly comparable with the eval card.

## 3. Comparing 5-pair and 6-pair labels

The leaderboard was run on copies of the scorer-v2 references
`ref-names-40921b9f-r1` and `-r2`. In one copy of r2, a sixth card was
filed as 2.1.260 -> 2.1.261 (the 2.1.216 card, renamed) and the summary
re-folded:

- **r1 vs six-pair r2, in either order:** one line,
  `!! COMMON PAIRS: … (only six-r2: 2.1.260->2.1.261); every total is over
  the 5 common pairs`. Every number in every block was identical to
  r1 vs the real r2. The diff shows only column padding (the label's
  width).
- **Two six-pair labels:** no COMMON PAIRS line. The leaderboard prints
  `bands: MEASURED OVER 5 PAIRS …, but these totals cover 6 … these ±
  figures are not this comparison's floor`.
- The unit test
  `a_five_pair_label_against_a_four_pair_baseline_compares_the_common_four_out_loud`
  already guards the fold. It keys on pair names, not on how many pairs
  there are.

Decision: no harness change for comparison.

- A new 6-pair label compared against the current 5-pair references is
  compared over the five shared pairs, and that is said out loud.
- The new pair's own KPIs have no band until a 6-pair reference
  exists (§5).
- A label holding only the new pair has no pair in common with a
  reference and is refused.

## 4. Harness changes

- `dev/eval/pairs.json`: the new pair, and `"selfHop": true` on
  2.1.215 -> 2.1.216.
- `committed/eval.rs`: `Pair.self_hop`. `pairs()` refuses a file where
  the number of `selfHop` pairs is not exactly 1.
- `runners/score.rs`: `self_hop_index` picks the `selfHop` pair when it
  is selected. Otherwise it picks the last selected pair, as before.
  `--pairs 260->261` therefore self-hops 2.1.261, and that is how the
  self-hop was proven on a code-split tree.
- `runners/score_test.rs`: the fake corpus mirrors the host's two layouts
  (from 2.1.242 on, the entry is at the root of `binary-decompiled/` with
  chunks beside it). The golden therefore records the new pair launching
  `binary-decompiled/index.js`. New tests:
  - a full run self-hops the marked pair, not the last one;
  - a code-split pair launches its root entry.

  The rebase-order and parallel-rebase tests no longer assume the last
  pair, or five pairs.
- `committed_test.rs`:
  - the marked pair's `to` equals `self-hop-reference.json`'s version (both
    modes);
  - a file with no marked pair, or with two, is refused.
- `humanify-ag-cli` `launch_parity_test.rs`: every golden launch must
  match a captured launch's settings. The captures all read a single-file
  release, so the root-entry input path of a code-split release is
  compared as the single-file spelling. Every other key is compared as it
  is.
- `cli.rs` footprint: the new pair's measured cost. `keys.rs` doc: the
  self-hop runs in its pair's lane, not the last pair's.

Nothing else was needed:

- The input path shape was already handled (`input_of` falls back to the
  root `index.js`).
- The boot gate is the walk's, and it boots 2.1.261 with the same pinned
  model.

Red was seen first: the loader test, the self-hop test and the golden
all failed before the code change.

## 5. Single-pair result (`exp254-pair261-s1`, names mode, scorer v3)

Run with `cargo dev eval score exp254-pair261-s1 --pairs 260->261`, cold,
no cache. It ran beside walk `7b546062-post141`, which shared the LLM
server, so the times are pessimistic. Free memory at the start was 217 GB.

| step | exit | wall | peak RSS |
| --- | ---: | ---: | ---: |
| rebase 2.1.260 (scratch) | 0 | 2,609 s | 17.9 GB |
| scored leg 2.1.261 | 0 | 799 s | 29.8 GB |
| cold self-hop | 0 | 416 s | 31.3 GB |
| warm self-hop | 0 | 345 s | 31.0 GB |

| KPI | 2.1.260 -> 261 | 2.1.215 -> 216 (r1) |
| --- | ---: | ---: |
| stmts | 57,260 | 35,903 |
| noise (stmts) | 713 | 286 |
| novel | 1,236 | 986 |
| noiseLn | 68,392 | 4,913 |
| realLn | 276,965 | 122,066 |
| tree churn / real / noise (git lines) | 46,344 / 39,896 / 3,632 | — |
| reorder (git lines) | 0 | — |
| mints | 70 | — |

- **Matching:** 101,748 functions; 92,247 carried, 844 close-matched,
  1,361 asked the LLM, 1,092 unmatched, 697 ambiguous.
- **LLM:** 2,330 calls sent; p50 5.0 s, p90 8.8 s.
- **Cold self-hop:** 34 lines, not judged; the reference range is
  2.1.216's only.
- **Warm self-hop:** byte-identical, 0 writes. The cold leg filled 26
  entries.

The tree's `reorder` column reads 0, although the walk's history counted
30,644 in-file moved lines for this hop. Those moves are not reaching the
tree as byte-identical reordered statements. Either they land in `real`,
or the scratch base orders the chunk differently from the walk's prior.
This is unexplained (open item 3).

## 6. What a six-pair reference needs

- **Labels:** three same-commit cold repeats of main, for example
  `ref-names-<sha>-r1/r2/r3`, plus optionally a serial `-s1`, all
  names mode on index `4736dcfa`. `eval bands` then writes a
  six-pair `byScorer.3.byMode.names` slot (and `matchingBands`). The
  current bands were measured over five pairs, and the leaderboard says
  so on every six-pair comparison.
- **Self-hop range:** unchanged. The self-hop stays on 2.1.216. The new
  repeats re-record the names range the usual way (`eval bands`).
- **Time:** about 95 min plus about 45-60 min for the extra serial
  rebase, so **about 2.5 h per repeat**, and about 7.5 h for three.
  Running them alone frees the LLM server, so the rebase should be
  faster than 2,609 s.
- **Memory:** with 5 lanes the old sum was 27-31 GB. A 29.8 GB scored
  leg overlapping the others could put the peak around 50-60 GB, which
  fits the host's 247 GB.
- **Off mode:** `ref-scratch-0f338ffa-*` stays four pairs. An off-mode
  sixth pair would need its own repeats.

## Open items

1. Run the three six-pair repeats when the host is free, then `eval bands`.
2. Seventh pair 2.1.251 -> 2.1.252 (pure reorder, 5.7k real): add it if
   the reorder lever needs a probe without real change, at about 45 min
   more per run.
3. Find why `reorder` reads 0 on this pair while the walk saw 30k
   in-file moved lines.
4. The identity's input sha covers only the 19 KB entry stub of a
   code-split input. The stub names its chunks by content hash, so it
   is a proxy for them, not a hash of every chunk.
5. Adopting index `77b12699` (exp213) remains Andrew's call. It would
   apply to all six pairs.
