# exp196 — harness lane L2a: the 034 eval KPI scorer in Rust (scorer version 2)

STATUS: DONE (2026-11-07), branch `harness-l2a-eval-scorer`. Scope:
`docs/dev-harness.md` §2.4's `eval rescore|summarize|leaderboard|bands|diff`,
under Andrew's 2026-11-06 decision that the scorer is a NEW SCORER VERSION
re-scoring the frozen reference trees (no Babel-shape adapter; §4a / §8 risk
1 are superseded). The TS scorer stays in place, unchanged, until lane L5.

## Totals

- **Five verbs ported** to `cargo dev`: `eval rescore`, `eval summarize`,
  `eval leaderboard`, `eval bands`, `eval diff` (about 4,300 lines of Rust in
  `crates/humanify-ag-dev/src/eval/`, 48 unit tests).
- **Scorer v2 agrees with v1 on 99.97% of every statement-keyed count.** Over
  the six frozen references (24 pair cards), v2 differs from v1 in exactly
  **3 statements** per label (all three: a function that became `async`,
  which v1's hash could not see) and **one vendor file** in three off-mode
  cards (a correct relocation pair v1 missed by 0.004 of a threshold). Every
  other card field is byte-equal.
- **The hold columns agree across all six labels:** novel **4,191** /
  realLn **416,386** in every names and every off reference (v1: 4,188 /
  416,377). Band zero holds.
- **Every partition adds up** on all 24 cards (app + vendor + cross = total).
- **v2 bands equal v1 bands for every KPI** in both naming modes; written as a
  new section, `noise-bands.json` `byScorer.2`.
- **The wrapper-spelling rule matches the TS verdicts exactly** on the one
  real case on record (2.1.207->208): 17 pairs in the same 4 files,
  8,230 lines, every composition field equal.
- **The leaderboard refuses v1 vs v2** (and any other identity mismatch)
  through L1's one guard; legacy cards read as scorer 1.
- **Speed:** a label re-scores in about 33 s (4 pairs + report pages), against
  31-50 s per pair for the TS scorer.

## 1. What was ported (old path -> verb)

| Old (TS, deleted by L5)                                                                                                               | New                                                                                                |
| ------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `experiments/170-app-vendor-split/rescore.ts`                                                                                         | `cargo dev eval rescore`                                                                           |
| `experiments/034-eval-harness/analyze.ts`, `statements.ts`, `kpis.ts`, `surfaces.ts`, `build-constant-churn.ts`, `name-only-churn.ts` | `eval::analyze`, `stmts`, `kpis`, `surfaces`, `line_churn`                                         |
| `experiments/034-eval-harness/trail-report.ts`, `diff-ledger.ts`                                                                      | `eval::report` (written by `eval rescore`)                                                         |
| `experiments/034-eval-harness/summarize.ts` (+ `lib/invariants.ts` readers, `lib/run-manifest.ts` provenance readers)                 | `cargo dev eval summarize`                                                                         |
| `experiments/034-eval-harness/leaderboard.ts` (+ `run-manifest.ts` `baseModeComparison` / `namingModeComparison`)                     | `cargo dev eval leaderboard` (L1's guard)                                                          |
| `experiments/034-eval-harness/noise-bands.ts`, `scripts/eval.ts` `bands`                                                              | `cargo dev eval bands`                                                                             |
| `experiments/055-residual-recount/real-ledger.ts`, `experiments/037-noise-source-decomposition/diff-composition.ts`                   | `cargo dev eval diff`, `eval::compose`                                                             |
| `experiments/046-vendor-noise/vendor-churn.ts`, `relocation-pairing.ts`                                                               | `eval::vendor`                                                                                     |
| `experiments/lib/js/statement-hash.ts`, `trees.ts`, `js/babel.ts`, `js/wrapper.ts`, `js/tree-layout.ts`                               | `eval::stmts` over `humanify_ag_core` (`ingest`, `modules::wrapper`, `hash::forms::KPI_STATEMENT`) |
| `experiments/lib/js/structural-tokens.ts` (vendor use)                                                                                | `hash::serialize::canonical_tokens` (`IDENTITY_KEY`)                                               |
| `experiments/lib/js/wrapper-spelling.ts` (scorer use)                                                                                 | `hash::wrapper_spelling` (the one rule)                                                            |
| `experiments/lib/diff.ts`, `js/line-diff.ts`                                                                                          | `eval::difftool`, `eval::line_tokens`                                                              |

Still TS after this lane (other owners): `structural-tokens.ts` also feeds the
clone census (L4); `lib/read-tracking.ts` is a test helper for
`recorded-facts.test.ts` (L4's test port); `invariants.ts writeRunStatus` and
`run-manifest.ts` writers belong to `run.sh` (L3). The 034 one-off ceiling
scripts are RETIRE per the design.

## 2. The design

- **The KPI hash is a hash FORM.** `hash::forms::KPI_STATEMENT`
  (`kpi-statement/2`, stamped in the card's `scorerVersion`) is the split's
  statement walk with `wrapper_spelling: false`; `statement_hash_in(form, …)`
  takes the form, and `statement_hash` is `statement_hash_in(&STATEMENT, …)`.
  The pin test (`hash_forms_pin_test`) and every core test are unchanged and
  green: the product's bytes did not move.
- **The vendor scorer's tokens come from the canonical walk.**
  `hash::serialize::canonical_tokens` is the same walk as
  `canonical_serialize` with token boundaries recorded (the tokens
  concatenate to the stream; a red test first). The signature form is
  `IDENTITY_KEY` (bindings as slots, every literal / property / free name
  verbatim), intra-tree `require("./…")` masked in the ESTree first.
- **Statements are the pipeline's own parse** (`Ingest::parse_unambiguous`)
  and the pipeline's own wrapper detection (`find_wrapper_function`).
- **The wrapper-spelling advisory reads the one rule:** a leftover fresh and a
  leftover removed statement equal under `STATEMENT` and different under
  `KPI_STATEMENT`.
- **Cards carry `scorerVersion: 2`**; the identity backfill reads it, and a
  card without it is scorer 1. `label_cards`, `comparable` and `guard` are
  L1's, unchanged. Bands live per (scorer version, naming mode).
- Parallel work goes through `humanify_ag_core::par`; `diff` through L1's
  `proc`; CRLF copies through `humanify_ag_core::scratch`. The concurrency
  census is green with no new allowlist entry.

## 3. Proof

Data: `/work/dev-harness-proof/` (frozen trees and cards of
`ref-names-0357696e-r{1,2,3}` and `ref-scratch-0f338ffa-r{1,2,3}`); v1 =
the oracle cards `oracles/a/<label>/` (the TS scorer at `f8bbb20a`). v2 =
`cargo dev eval rescore <label>-s2` into the main checkout's (gitignored)
results directory, under `flock /work/heavy.lock`.

### 3.1 Invariants under v2

| Invariant                                              | Result                                                                    |
| ------------------------------------------------------ | ------------------------------------------------------------------------- |
| novel / realLn equal across r1/r2/r3, names mode       | 4,191 / 416,386 ×3                                                        |
| novel / realLn equal across r1/r2/r3, off mode         | 4,191 / 416,386 ×3                                                        |
| ... and equal between the names and the off references | yes (as under v1: 4,188 / 416,377)                                        |
| every surface partition adds up (24 cards)             | 24/24 "partitions: all add up"                                            |
| warm / cold self-hop facts                             | unchanged: the same banner lines (warm OK; cold 8 ln names, 184/32/2 off) |
| v2 bands vs v1 bands                                   | identical for every KPI, both modes                                       |

### 3.2 v1 vs v2, per pair (every card field compared; fields not listed are byte-equal)

| Pair         | Names r1/r2/r3                                                                        | Off r1/r2/r3                                      |
| ------------ | ------------------------------------------------------------------------------------- | ------------------------------------------------- |
| 2.1.85->86   | equal, except one vendor pair's printed score (0.918 -> 0.924; same pair, same lines) | byte-equal                                        |
| 2.1.118->119 | novel +2, noise -2, realLn +6, noiseLn -6; layout real +2, naming -2 (cleanReal +2)   | same                                              |
| 2.1.197->198 | novel +1, noise -1, realLn +3, noiseLn -3; layout real +2, naming -2                  | same, plus vendorReal -2/-2/-4, vendorLn +2/+2/+4 |
| 2.1.215->216 | byte-equal                                                                            | byte-equal                                        |

Every difference, explained by example:

- **The three statements are real changes v1 could not see.** The core's
  statement walk carries a function's `async` / `generator` head fields in the
  node content (exp094b); Babel's v1 hash did not. In 2.1.119,
  `.map(executeCommand => executeCommand())` became
  `.map(async runAsync => runAsync())` inside `runAllHandlers`, and
  `var createRemoteConfigCommand = async onDoneCallback => …` is a new async
  arrow whose only same-shape prior twin was not async; in 2.1.198,
  `async function handleAsyncOperation(input) { return; }` is new, and v1
  matched it to an unrelated non-async `function x(p) { return; }`. v1 called
  all three naming noise; v2 calls them novel. The same statements in the
  split files move 2 lines from `naming` to `real` on each pair.
- **The vendor pair.** v2's content shingles come from oxc's ESTree, so
  relocation scores move by a few thousandths (0.918 -> 0.924). On off-mode
  2.1.198 that lifts one pair over the 0.5 threshold (0.504, margin >= 0.2):
  the prior `prism-objc.js` (r1/r2; `prism-javascript/lib_a1b518c3.js` in r3)
  and the fresh `ruby/lib_792fafc4.js`. Both files are highlight.js's RUBY
  grammar (keywords `and then defined module … attr_reader`), so the pair is
  right: its draw lines move from real to noise.
- **Parse acceptance:** oxc and Babel cut every bundle into the same
  statements (23,442 / 31,839 statements, the same texts and line counts in
  the per-statement comparison of 2.1.119 and 2.1.198), and every split file
  parsed.

### 3.3 The wrapper-spelling rule against the TS verdicts

The reference pairs carry no wrapper re-spelling (0 lines under v1 and v2).
On the one real case on record — the walk trees 2.1.207 -> 2.1.208
(`/work/walk-rust-0926/trees`) — the TS composition and `eval::compose` give
the same tally (real 51,880, naming 9,446, alias 208, reorder 2, files
6,015, spelling 8,230) and the same 17 spelling pairs in the same 4 files.

`eval diff` against `real-ledger.ts` on ref-scratch r1 2.1.85->86 prints the
same two ROW lines (`real|22667|1712|7900|13055|8980`,
`accounting|34725|31647|5708|0.0|50.0`).

### 3.4 v2 bands (`noise-bands.json` `byScorer.2`)

Equal to the v1 bands for every KPI. Headline: off — noise 17, noiseLn
1,376, novel 0, realLn 0, treeLn 16, cleanReal 74, vendorLn 55, vendorReal
32, mints 33; names — noise 397, noiseLn 15,709, novel 0, realLn 0, treeLn
2,116, cleanReal 1,042, vendorLn 8, vendorReal 96, mints 22.

### 3.5 Leaderboard

`eval leaderboard ref-names-0357696e-r{1,2,3}-s2` prints the table with
`novel 4191 (=)`, `realLn 416386 (=)` and every other delta inside its band
(`~0±…`), bands "scorer v2, names mode". `ref-scratch-0f338ffa-r1` (v1)
against `ref-scratch-0f338ffa-r1-s2` is refused on `scorer.version: "1" vs
"2"`; names against off is refused on `namingMode` and `libraryIndex`.

## 4. Gate

| Check                            | Result                |
| -------------------------------- | --------------------- |
| `cargo test -p humanify-ag-dev`  | see the merge message |
| `npm run check` (worktree)       | see the merge message |
| `npm run check` (clean checkout) | see the merge message |

## 5. What changed from the design, and why

1. **No byte-identity proof (§4a):** superseded by Andrew's new-scorer-version
   decision; the proof is the invariants and the explained v1/v2 table above.
2. **`async` / `generator` are content in the KPI hash.** They come with the
   core's statement walk (one walk, no second copy); 3 statements per label.
3. **`spellingIdenticalLines` reads the one Rust rule** (any SAFE arrow in the
   statement, not only a regex-matched wrapper head). Same verdicts on the
   real case.
4. **The self-hop reference is not split per scorer version.** Its
   `coldDiffLines` are `diff` counts of the self-hop bundles, which no scorer
   computes; `eval rescore` copies the self-hop records unchanged.
5. **The leaderboard's base/naming comparisons are L1's guard**, compared pair
   by pair (a label with different pairs is refused unless forced). It no
   longer has a no-argument "browse every label" mode.
6. **Moves are sorted by `locale_compare`** (`humanify_ag_model::js`), the
   same order as the TS's `localeCompare` on these paths.
7. **v2 labels are not committed:** `experiments/*/results/*` is gitignored;
   they live in the main checkout's results directory as `<ref>-s2`.

## 6. What I could not prove

- That v2 agrees with v1 on trees outside the six references and the
  2.1.207->208 walk pair (the agreement is measured, not argued).
- The report page is a port of the TS page's HTML, not compared byte for byte
  (the TS one-line text console output was not ported; run.sh discarded it).
- `diff-ledger`'s `movedSt` stays 0, as under v1: it reads a `files` object
  the current ledger no longer writes.

## Proposed CLAUDE.md text (for L5)

> The eval KPI scorer is `cargo dev eval rescore|summarize|leaderboard|bands|diff`
> (scorer version 2, `crates/humanify-ag-dev/src/eval/`). Cards carry
> `scorerVersion`; a card without it is the TS scorer's (v1). The leaderboard
> refuses labels of different scorer versions unless `--force-mixed`. Bands
> are per scorer version: `noise-bands.json` `byScorer.2`. The v2 references
> are `ref-names-0357696e-r{1,2,3}-s2` and `ref-scratch-0f338ffa-r{1,2,3}-s2`
> (re-scored from the frozen trees in `/work/dev-harness-proof`): novel 4,191 /
> realLn 416,386.
