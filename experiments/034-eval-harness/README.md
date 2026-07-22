# 034 — cross-version eval harness

A repeatable scorecard for the deobfuscation pipeline on a fixed set of version
transitions. Run it before merging anything you think moves the needle; compare
the result against the committed baselines to see the **real** impact.

The goal it serves: drive cross-version diff **noise** toward zero while leaving
**real** code change untouched — and know, per change, which way the numbers went.

## What it measures (per version pair `v-1 → v`)

Runs the current pipeline on `input(v)` with `--prior-version humanified(v-1)`,
then scores two deterministic signals plus the churn:

### 1. Determinism (from the pipeline's `--stats-json`)

How each identifier got its name — the answer to "what _should_ be deterministic":

- **deterministic** = cached / exact-transfer + already-named + nothing-to-rename.
  These reproduce byte-for-byte across runs.
- **closeMatchLLM** = matched a prior counterpart but the match wasn't exact, so
  the LLM re-named it _with the prior as context_. Has a prior → the
  "shouldn't-really-be-nondeterministic" bucket (Levers A1/A2 target it).
- **coldLLM** = genuinely new code, no prior → legitimately nondeterministic.
- `%det` / `%llm` summarize the split. On CC 2.1.216: 98% deterministic, only
  3.3% reach the LLM (876 close + 1,259 cold of 64,493 functions).

### 2. Churn (rename-invariant, `analyze.ts`)

Diff `humanified(v)` against `humanified(v-1)` at the statement level using the
split's own identifier-blind `statementHash`:

- **clean** (`unchangedClean`) — hash in both _and_ text identical → names
  reproduced. The stable majority; the pipeline got these right.
- **noise** (`unchangedChurned`) — hash in both but text differs → pure **naming
  noise** (the hash ignores names, so a function-local flip lands here too).
- **novel** — new/changed structure → **real** change.
- `noiseLn` / `realLn` are the line magnitudes of the noise / novel statements.

Every statement is exactly one of those three: **`stmts = clean + noise + novel`**.
Separately, on a **binding→file axis** (from the split ledger, a different
denominator — one entry per declared name, not per statement):

- **reloc** (`sameNameMovedFile`) — a binding that kept its name but changed home
  file, dragging every importer's `require`-alias. Deterministic, reducible.
- **newName** (`novelNames`) — names in `v` absent from `v-1` (new/flipped).

**Which numbers are stable?** Everything except `noiseLn`/`noise` is deterministic
run-to-run. The naming-noise magnitude carries the ~20k-line LLM floor (temp is
already 0; it's concurrent batch-serving), so read `noiseLn` against `%llm`, not
in isolation. The KPIs to drive to **0** are the _reducible_ ones: `reloc`,
`mints`, and any `noise` beyond the LLM-named population.

## Pairs (`pairs.json`)

| pair        | character                               |
| ----------- | --------------------------------------- |
| 2.1.85→86   | quiet 80s hop, small base — the control |
| 2.1.118→119 | biggest clean feature drop (+475 KB)    |
| 2.1.197→198 | feature on a large base (+330 KB)       |
| 2.1.215→216 | ongoing work, largest base (+284 KB)    |

Edit `pairs.json` to change the set or the LLM config (endpoint/model/effort).

## Usage

```bash
# Score the current tree under a MODEL label (a branch, commit, or idea name):
experiments/034-eval-harness/run.sh main-4117212

# ...make a change, then score it:
experiments/034-eval-harness/run.sh fix-close-match

# Compare every model side by side (deltas vs the first-listed baseline):
npx tsx experiments/034-eval-harness/leaderboard.ts main-4117212 fix-close-match
```

Each run is one pipeline invocation per pair (~10–15 min each; ~1 hr for four).
A failed pair is logged and skipped, never aborts the sweep. Results land in
`results/<model>/`:

- `<v>.json` — per-pair scorecard (determinism + churn)
- `<v>.stats.json` — the raw `--stats-json` breakdown
- `<v>.log` / `<v>.stdout` — the pipeline run
- `summary.json` — aggregated totals (what the leaderboard reads)
- `commit.txt` — the pipeline commit that produced this model

Commit the baseline models you want to keep as references. `leaderboard.ts` with
no args lists every model that has a `summary.json`.

### If a change alters formatting (not just names)

The eval diffs a freshly-humanified `v` against the archive `v-1`. That archive
prior was produced by an earlier pipeline, so it is a valid base **as long as
formatting is unchanged** — the rename-invariant `statementHash` cancels naming
differences, and real formatting is identical, so only names/real-change show up.
If a change alters **formatting** (whitespace, statement shape, generator output),
the archive `v-1` is no longer like-for-like and formatting diffs swamp the
signal. Regenerate the prior first:

```bash
REBASE_PRIOR=1 experiments/034-eval-harness/run.sh <label>
```

This re-humanifies each base version with the current pipeline (inheriting its own
archive names) before scoring, so the pair's diff again reflects only naming/real
change. It doubles the runs per pair; it is expected and fine when formatting moved.

## As a pre-merge gate

Before merging a change that claims to reduce noise: run the eval under a label,
`leaderboard` it against the baseline, and confirm the reducible KPIs (`reloc`,
`mints`, `noise` above the floor) went **down** and `novel`/`realLn` (real change)
did **not** move — a change that "reduces noise" by dropping real code is a
regression.
