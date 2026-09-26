# 5b-2 eval result — the formatter swap / end of the hash exemption (2026-09-26)

**Verdict: PASS.** The self-sufficient Rust binary (rust-port `c3b272f`: native
formatter, own hash bytes, no TS artifact read) scores the four eval pairs with
the real-change hold columns EXACTLY equal to a fresh cold TS control, and every
reducible KPI inside the band (or lower). Judged per 00-control §3 ("5b control",
"5b self-hop gate") and 17-formatter-swap.md §4.

## Runs

| label                   | pipeline                                          | commit    | notes                                                                     |
| ----------------------- | ------------------------------------------------- | --------- | ------------------------------------------------------------------------- |
| `ts-control-fec64e5-r2` | TS (`npx tsx src/index.ts`)                       | `fec64e5` | fresh COLD control (rule 10); all four bases rebased, no archive fallback |
| `rust-5b-c3b272f-a`     | Rust binary (`--bin`, built `--locked` by run.sh) | `c3b272f` | cold                                                                      |
| `rust-5b-c3b272f-b`     | same                                              | `c3b272f` | cold repeat                                                               |

All three: exit 0 on every pair, boot gate OK ×4 (both halves), `rebase FAILED`
count 0. Endpoint `http://192.168.1.234:8000/v1`, `openai/gpt-oss-20b`, low
reasoning, concurrency 32. Results under `experiments/034-eval-harness/results/`
(untracked, as for every label).

`ts-control-fec64e5` (r1) is INVALID and must not be cited: its 2.1.197 rebase
was OOM-killed by a parallel gate batch and it silently fell back to the ARCHIVE
prior (reloc 97.8%, churn 1.56M). Check eval logs for "rebase FAILED" before
trusting any label.

## Hold columns (band 0) — exact on every pair, both runs

| pair        | novel TS / A / B      | realLn TS / A / B           |
| ----------- | --------------------- | --------------------------- |
| 2.1.85→86   | 787 / 787 / 787       | 78,791 / 78,791 / 78,791    |
| 2.1.118→119 | 1,154 / 1,154 / 1,154 | 79,124 / 79,124 / 79,124    |
| 2.1.197→198 | 1,261 / 1,261 / 1,261 | 136,396 / 136,396 / 136,396 |
| 2.1.215→216 | 986 / 986 / 986       | 122,066 / 122,066 / 122,066 |

Totals 4,188 / 416,377 — byte-equal to every prior reference on record
(`main-2026-09-18`, `main-2026-08-20`, `main-2026-08-12`, `session-2026-08-05`).

## Reducible KPIs — TS vs A vs B (band = wider of noise-bands.json and |A−B|)

| pair    | kpi     | TS     | A      | B      | band | verdict        |
| ------- | ------- | ------ | ------ | ------ | ---- | -------------- |
| 85→86   | noise   | 1,500  | 1,498  | 1,501  | 3    | OK             |
| 85→86   | noiseLn | 32,887 | 32,960 | 32,693 | 514  | OK             |
| 85→86   | reloc   | 650    | 656    | 644    | 12   | OK             |
| 85→86   | newName | 308    | 306    | 300    | 15   | OK             |
| 85→86   | mints   | 16     | 21     | 15     | 11   | OK             |
| 118→119 | noise   | 429    | 432    | 425    | 7    | OK             |
| 118→119 | noiseLn | 4,423  | 4,594  | 4,244  | 514  | OK             |
| 118→119 | reloc   | 119    | 106    | 103    | 3    | lower (better) |
| 118→119 | newName | 1,530  | 1,534  | 1,545  | 15   | OK             |
| 118→119 | mints   | 17     | 22     | 21     | 11   | OK             |
| 197→198 | noise   | 519    | 504    | 509    | 5    | lower (better) |
| 197→198 | noiseLn | 7,410  | 7,320  | 7,325  | 514  | OK             |
| 197→198 | reloc   | 342    | 336    | 338    | 2    | lower (better) |
| 197→198 | newName | 1,012  | 1,006  | 1,010  | 15   | OK             |
| 197→198 | mints   | 19     | 24     | 21     | 11   | OK             |
| 215→216 | noise   | 290    | 288    | 294    | 6    | OK             |
| 215→216 | noiseLn | 5,090  | 4,432  | 5,234  | 802  | OK             |
| 215→216 | reloc   | 378    | 366    | 372    | 6    | lower (better) |
| 215→216 | newName | 739    | 742    | 737    | 15   | OK             |
| 215→216 | mints   | 24     | 29     | 27     | 11   | OK             |

`relocSt` equal on every pair (178 / 5 / 67 / 102). No KPI moved up out of band.
The "lower" rows are the good direction and are the size of the TS control's own
run-to-run spread (r1 vs r2 on the valid pairs: reloc 660/650, noise 1,502/1,500);
per rule 11 they are NOT claimed as an improvement.

On-disk tree churn (git lines): TS 147,764 · A 147,794 · B 147,621.

**Minted leftovers** read +5 on every pair in run A (96 vs 76), which looked
systematic. Investigated (finding #54): not a port bug — replaying each run's own
logged sweep answers through the TS and Rust deferred sweeps gives identical
decisions on 6/6 legs; the model's cold draw suggested names already bound in the
module wrapper (rejected as `target-in-scope` on both sides), and gpt-oss is not
deterministic at temperature 0. Run B reads −1/+4/+2/+3 (84 total) — mixed, as a
draw should be. All inside the band.

## Self-hop gate (two halves, 00-control §3)

| run        | cold self-hop 2.1.216 | warm self-hop                          |
| ---------- | --------------------- | -------------------------------------- |
| TS control | 54 ln                 | — (TS legs do not run the warm half)   |
| Rust A     | 90 ln                 | byte-identical, 0 cache writes, exit 0 |
| Rust B     | 38 ln                 | byte-identical, 0 cache writes, exit 0 |

The cold counts sit BELOW the recorded 92–180 ln reference range (fewer LLM
re-rolls — the TS control reads 54 too); below the range is not a failure, it is
a range recorded before this model settled. Worth re-recording the reference
from these three runs.

## What this closes, and what follows

- M4 (5b) is DONE: the Rust binary is the pipeline, judged by the eval, with the
  real-change columns exact.
- Next: cutover (17-formatter-swap.md §6) — retarget the harness/test imports
  from `src/`, retire the TS pipeline and the TS-dump comparison gates, rebuild
  `npm run check` around the Rust binary.
- Then the parked real bugs become ordinary levers, each eval-measured: #42
  (`??` rewrite changes program meaning), #16 (export-const split), #39 (split
  namer's over-long prompt), #48/#49 (rename ledger), #44/#45 (beautifier).
