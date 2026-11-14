# exp222 — count changed lines with the minimal diff

STATUS: DONE, harness only. No pipeline code changed, no pipeline run, no
eval, no walk. Every number below comes from re-scoring trees that already
exist. Branch `exp222-minimal-line-diff` from main `d3fed6a41`. Scripts and
outputs live on this host only, in `/work/exp222-runs/`.

## Totals

On the split-era walk (`walk-d3fed6a4-up`, 68 hops), the walk report's
changed-line column `diffLn` falls by **746,238 lines (-3.0%)**. Without the
two layout-transition hops it falls by 741,416 lines (-3.4%), which is
exp220's "instrument" figure (741,408) give or take 8 lines. On the
pre-split walk (`walk-73d01430-up`, 129 hops) it falls by 4,834 lines
(-0.2%). On the names references (five eval pairs) one scorecard column
moves: `vendorReal` goes down by 32 lines on every repeat. The off-mode
references do not move at all.

`novel`, `realLn`, `noiseLn`, `noise`, `treeLn`, `realExBuild`, `nameOnly`,
`naming`, `reloc` and `mints` read exactly the same, on both walks and on
every reference. No hop's count went up.

## How lines were counted before

Two modules shell out to GNU `diff` (normal format, a changed line is a `<`
or `>` line, so a modified line counts twice). Both used `diff`'s default
algorithm, which gives up searching for the shortest edit on long inputs
and reports more changed lines than there are.

| counter | what reads it |
|---|---|
| `eval::difftool::Differ` (the scorer's counter) | the `vendor/` decomposition: `vendorChurnLines`, `vendorReal`, `vendorLn`, `vendorNoise` (and through that `noiseLines`); `eval diff`'s "truth" line count (`real_ledger`) |
| `runners::tree_diff` (`diff -rN` on trees) | the walk report's `diffLn` / `src` / `vendor` columns; the self-hop's cold count; neutrality's line count |

What does **not** use GNU diff: the scorer's `src/` layout block (`treeLn`,
`realExBuild`, `naming`, `alias`, `reorderLn`, `nameOnly`) is built from
statement composition with its own exact LCS (`eval::compose`,
`eval::text::lcs_len`). The bundle KPIs (`noise`, `noiseLn`, `novel`,
`realLn`, `reloc`, `mints`) use statement hashes. That is why the giant
file's overcharge showed up in the walk's `diffLn` and not in `treeLn`.

## The change

- `eval::difftool::LINE_DIFF_FLAGS = ["--minimal"]`: one constant, used by
  both counters. GNU `diff --minimal` returns the shortest edit script, so
  the count no longer depends on how long a file is.
- `runners::tree_diff` counts with the same flags (`-rq`, which only lists
  differing files, is unchanged).
- Tests, written first and watched fail on main
  (`difftool_test::a_long_file_is_charged_its_minimal_edit_not_the_speed_heuristics`,
  `tree_diff_test::file_and_tree_counts_are_the_minimal_edit`): a
  generated 16,000-line pair whose minimum is known by construction
  (`eval::test_support::overcharged_pair`). The default diff reads 17,600
  lines and the minimal diff reads 14,000. (On this pair git's histogram
  diff reads 17,600 too: it is not minimal either, so `--minimal` is the
  exact choice.)
- Speed: on the two worst hops, the whole `src/` tree diff takes 4.3 s
  against 3.6 s (2.1.281 -> 2.1.282) and 3.1 s against 2.7 s
  (2.1.251 -> 2.1.252). The scorecard re-scores took the same time as
  before (36-57 s per label).

## Scorer version and bands

This is **scorer version 3**. `card::SCORER_VERSION` used to be tied to the
KPI statement form's version (`kpi-statement/2`). It is now its own number
in the dev crate, and its comment records what each version changed. A new
version is needed: the names references' `vendorReal` moves by 32 lines,
more than its band (±28). Comparing a v2 label with a v3 label would show a
false vendor improvement, and the leaderboard now refuses that mix unless
`--force-mixed`.

Re-derived from re-scored trees, with no run:

- `noise-bands.json` `byScorer.3.byMode.names`: written by `eval bands`
  from the four names references re-scored by scorer 3
  (`ref-names-40921b9f-r1/r2/r3-s3`, `serial-40921b9f-s1-s3`). Every band
  and matching band is **equal** to scorer 2's. The 32 lines came off every
  repeat equally, so the spread did not change.
- `noise-bands.json` `byScorer.3` (off mode): the three off references
  (`ref-scratch-0f338ffa-r1/r2/r3-s3`) re-scored by scorer 3 give totals
  equal to scorer 2's on every key, so the bands are scorer 2's, carried
  over with a provenance note. `eval bands` refuses these repeats because
  of an exact-matching check (`fnAsked` differs by 1). The check came in
  with exp208, after these runs were made, so it is not caused by this
  change.
- `self-hop-reference.json`: the cold self-hop count is a `tree_diff`
  count, so it was counted again on the same trees. Off-mode r1 goes from
  184 to 182, so the range is now 2-182. r2 and r3 and the names slot
  (2, 2, 2, 8) do not change.

The library scorer (`exp096-score/15`) does not count lines and is not
affected.

## Before / after

### Split-era walk `walk-d3fed6a4-up` (2.1.241..2.1.322, 68 hops)

| column | before | after | change |
|---|---:|---:|---:|
| diffLn | 24,934,648 | 24,188,410 | -746,238 (-3.0%) |
| src | 24,467,396 | 23,721,316 | -746,080 (-3.0%) |
| vendor | 408,368 | 408,210 | -158 |
| vendorReal | 224,969 | 224,769 | -200 |
| vendorLn | 176,776 | 176,818 | +42 |
| noiseLines | 2,688,722 | 2,688,764 | +42 |
| treeLn, realExBuild, nameOnly, naming, alias, reorderLn | — | — | 0 |
| noise, noiseLn, novel, realLn, relocSt, reloc, mints | — | — | 0 |

Without the two layout transitions (2.1.241 -> 2.1.242, 2.1.247 -> 2.1.248):
diffLn 21,786,269 -> 21,044,853 (-741,416, -3.4%).

The hops that moved most:

| hop | diffLn before | after |
|---|---:|---:|
| 2.1.281 -> 2.1.282 | 590,979 | 339,985 |
| 2.1.251 -> 2.1.252 | 373,607 | 205,003 |
| 2.1.250 -> 2.1.251 | 658,257 | 499,237 |
| 2.1.296 -> 2.1.299 | 656,339 | 583,757 |
| 2.1.270 -> 2.1.271 | 350,853 | 319,729 |

52 of 68 hops moved, and none went up.

### Pre-split walk `walk-73d01430-up` (2.1.89..2.1.241, 129 hops)

| column | before | after | change |
|---|---:|---:|---:|
| diffLn | 2,415,871 | 2,411,037 | -4,834 (-0.2%) |
| src | 2,214,228 | 2,209,470 | -4,758 |
| vendor | 167,915 | 167,839 | -76 |
| vendorReal | 125,749 | 125,671 | -78 |
| vendorLn | 42,285 | 42,287 | +2 |
| noiseLines | 163,457 | 163,459 | +2 |
| every other column | — | — | 0 |

87 of 129 hops moved, and none went up.

`vendorLn` (and through it `noiseLines`) can rise by a few lines. The
vendor decomposition counts a relocated file's own diff, and the minimal
edit script can split a file's changed lines differently between
"reducible" and "real". The total always goes down.

### Names references `ref-names-40921b9f-*` (5 pairs, totals)

| column | r1 | r2 | r3 | s1 |
|---|---|---|---|---|
| vendorChurnLines | 24,332 -> 24,300 | 24,278 -> 24,246 | 24,226 -> 24,194 | 24,298 -> 24,266 |
| vendorReal | 14,305 -> 14,273 | 14,277 -> 14,245 | 14,283 -> 14,251 | 14,279 -> 14,247 |
| novel / realLn | 5,336 / 556,875, equal | equal | equal | equal |

The 32 lines are 2.1.197 -> 2.1.198 (-30) and 2.1.179 -> 2.1.181 (-2), in
vendor files. Every other total key is equal.

### Off references `ref-scratch-0f338ffa-*` (4 pairs)

Every total key is equal before and after (novel 4,191, realLn 416,386).

## How the before/after was measured

- Walks: `eval walk-report` on shadow copies under `/work/exp222-runs/`
  (`trees`, `runs`, `history.git` symlinked read-only, a fresh `cards/`).
  "Before" is the walk's own cards rendered by the pre-fix harness binary
  (`bin/dev-before`); "after" re-scores every hop with the fixed binary.
  Every column outside the line counts is equal between the two, so the
  walk's cards were made by the same scorer apart from this change.
- References: `eval rescore` of each label with both binaries
  (`refs-before/`, `refs-after/`, `off-before/`, `off-after/`;
  `refs-s3/` and `off-s3/` are the v3 labels the bands name).
- Self-hop: `diff` and `diff --minimal` on each reference's 2.1.216
  bundle against its cold self-hop bundle.

## Left open

- The scorer's own `src/` LCS (`eval::text`) gives up above 25M cells.
  `lcs_len` then falls back to a multiset bound, and `on_lcs` counts every
  statement as in order. On the giant split-era file this may undercount
  reorders. This was not measured here.
