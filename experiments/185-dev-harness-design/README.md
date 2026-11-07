# exp185 — the design for a pure-Rust measurement harness

STATUS (2026-11-05): DESIGN ONLY, not built. The deliverable is
[`docs/dev-harness.md`](../../docs/dev-harness.md). No code changed, no eval,
neutrality or index build ran, and nothing was merged. The branch is
`exp185-dev-harness-design`.

## What it is

Andrew decided (2026-11-05) to remove the TypeScript, bash and Python
measurement harness. In its place goes one Rust workspace crate,
`humanify-ag-dev` (`cargo dev ...`, `publish = false`). The doc covers:

- an inventory of every living harness piece (about 34,700 lines in 145
  files, including 17 files in old experiment folders that living
  instruments still import or launch);
- the `cargo dev` subcommand map;
- one settings registry for the harness;
- one run identity record per card, checked by ONE comparison guard;
- four byte-identity proofs, with where their data lives on this box;
- six lanes;
- the test-review findings, each verified;
- an advisory "slop" census for `crates/`;
- the risks.

## Gate

| Check                               | Result                                                                                                                                                    |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `npm run check` (full)              | NOT run: doc-only change (one doc, one README), no stage reads either file except the public-repo scan; the full gate is a heavy job (release build, e2e) |
| `test/no-private-addresses.test.ts` | PASS (3/3), run on the worktree with both files in place                                                                                                  |

## Results

| Question                                      | Answer                                                                                                              |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Living harness size                           | ~34,700 lines, 145 files (doc §0)                                                                                   |
| Old-experiment code a living instrument needs | 17 files: 037, 046, 055, 166, 170, 172, 095 misses + selection (doc §1.5); plus the truth-layer builders (§1.6)     |
| Test-review findings verified                 | 12 of 12 checked: 10 as stated, 1 corrected (seven stub listeners, not six), 1 not moot (wrapper spelling) (doc §6) |
| Proof data present on this box                | yes for all four proofs; trees on volatile `/tmp`, current cards only in the exp171 worktree (doc §4, §8 risk 6)    |

## Proposed CLAUDE.md text (for L5; do not apply before the port is proven)

Replace the "Checks" section's command block and stage list with:

> **One command. Run it before every commit:** `cargo dev check`. It runs
> every stage of the gate and prints which ran (`--only a,b` is labelled
> PARTIAL). The harness is the `humanify-ag-dev` crate
> (`crates/humanify-ag-dev`, `docs/dev-harness.md`). Its verb table is the
> only place to look for a supported instrument. The only JS left in the
> repo is under `tools/`: shims that run bundlers and tools, never harness
> logic.

Replace every `npm run eval -- <verb>` with `cargo dev eval <verb>` and
`cargo dev library <verb>`. Replace `experiments/034-eval-harness/...` data
paths with `dev/eval/...`. Replace `experiments/lib/neutrality.sh` with
`cargo dev eval neutrality`.
