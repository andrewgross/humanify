# exp191 — harness migration lane L0: launch goldens, frozen proof data, oracles run once

STATUS: DONE (2026-11-06), branch `exp191-harness-l0`. This lane adds tests,
recorded goldens, one owner row, and this README. The one harness change is
§1b: the e2e stage (`scripts/e2e.ts`) now holds its own launch golden
(Andrew's decision, 2026-11-06), so `unit` no longer builds the binary. Scope is `docs/dev-harness.md` (branch `exp185-dev-harness-design`)
§4 proofs a-c and the §5 L0 row, nothing more.

## Totals

- **Launch goldens: 11 recorded, 11 held**: 10 by one new test file, the e2e one by the e2e stage itself (§1b), covering
  every living launcher the doc names except the walk (it has no living
  launcher). Each one is a recorder log from a live run. None was typed.
  Planting a one-word change in `boot-gate.sh` and in `run.py` turned the
  matching test red. The whole set was recorded, then held twice: byte-stable.
- **Proof data frozen** under `/work/dev-harness-proof/` (CC-derived library
  data under `/work/cc-library-truth/dev-harness-proof/`): 6 eval tree sets,
  6 card dirs, 9 library labels (cards/summaries + raw reports). Every copied
  file's sha256 equals its source's (66 manifests, all equal).
- **Oracle (a), 034 KPI scorer:** reproduced. All 24 cards and all 12 report pages are byte-equal to the recorded ones, and all 24 partitions add up. The 3 names-mode summaries are byte-equal. The 3 off-mode (`ref-scratch`) summaries differ ONLY by `namingModes: ["off"]`, a provenance key exp171 added to `summarize.ts` after those summaries were written. Every `rescore.ts` line reads `OLD-FIELDS DIFFER`, which is an artifact: that check strips `churn.surfaces` from the new card and compares it with the old card, and these old cards already carry surfaces. The cards themselves are byte-equal.
- **Oracle (b), the exp096 library scorer:** reproduced, **9 / 9 labels**: every `summary.json` and every card (90 + 30 + 1 per index, 363 cards) byte-equal.
- **Gate:** `npm run check` 13/13 in the worktree and 13/13 from a clean checkout of `c1cf98597` (§4). `unit` held its wall time (111 s against 109 s before): the e2e golden runs alongside run-launch.test.ts, which takes 110 s. On the clean checkout `unit` took 228 s and `rust:build` 0.1 s, because the e2e golden test does the release build first (the same total work, moved earlier).

## 1. Launch goldens (proof c)

Method: the one `run-launch.test.ts` uses. Run the real launcher end to end,
with recorders in place of the binary and the tools it launches, normalise
run-specific paths (`<TMP>`, `<REPO>`, the stub's port, e2e's scratch dir),
and compare with a committed log. Most launchers find the binary relative to
their own file and build it with the user-level cargo, so they run from a
SANDBOX: a scratch git repo holding byte-identical copies of the launcher
and every module it imports (followed through the import graph), at the
same relative paths, with `node_modules` linked. A recorder `cargo`
"builds" by writing the binary recorder where cargo would have put the
binary. `test/launch-recorder.ts` holds the recorders.

Re-record only from a live run: `UPDATE_LAUNCH_GOLDENS=1 npx tsx --test
test/launch-goldens.test.ts`.

| Launcher                                              | Legs recorded                                                                                                                                                                                                            | Golden (`experiments/034-eval-harness/launch-goldens/`)              | Test                          |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------- | ----------------------------- |
| neutrality (eval.ts -> neutrality.sh -> build-bin.sh) | names mode (default): both builds, both `--help` probes, both legs, their `--config` files, the run config                                                                                                               | `neutrality-names.golden.txt`                                        | `test/launch-goldens.test.ts` |
|                                                       | off mode (`library.matching=off`)                                                                                                                                                                                        | `neutrality-off.golden.txt`                                          |                               |
|                                                       | baseline from before exp186 (`-p humanify-cli`, `target/release/humanify`)                                                                                                                                               | `neutrality-baseline-pre-rename.golden.txt`                          |                               |
|                                                       | legacy baseline (no `--config` in its `--help`) through eval.ts: REFUSED                                                                                                                                                 | `neutrality-legacy-refused.golden.txt`                               |                               |
|                                                       | the legacy leg's own argv (`--split ... --llm-cache ...`), neutrality.sh fed a run config with no setting                                                                                                                | `neutrality-legacy-leg.golden.txt`                                   |                               |
| library (096 `run.py`)                                | the build, `corpus.py verify`, `discover --unit-prints ... --config discover.toml` per bundle under `taskset`, the `discover.toml` it writes, `score.py`; default and agreement + `--sets` + `--cpus` + `--pipeline-set` | `library-structural.golden.txt`, `library-agreement-sets.golden.txt` |                               |
| e2e (`scripts/e2e.ts`)                                | every leg of all 19 fixture pairs: detect + fresh + prior x2 + sequential fresh x2 + sequential prior x2 (152 launches) with their config files                                                                          | `e2e.golden.txt`                                                     |                               |
| ask-trace (`scripts/ask-trace.ts`)                    | default; `--prior --sequential --collide --set ... -- --model m`; `--set split.enabled=true`                                                                                                                             | `ask-trace.golden.txt`                                               |                               |
| match-truth (`experiments/lib/match-truth/run.ts`)    | `npm run match-truth` defaults: per corpus pair `npm pack` x2, `format`, `match` (14 pairs)                                                                                                                              | `match-truth.golden.txt`                                             |                               |
| boot gate (`experiments/lib/boot-gate.sh`)            | `boot_version` and `boot_prompt`: the `timeout` deadline, the `env -u ...` scrub list, bun's argv and cwd, and which seeded variables reach the walked CLI                                                               | `boot-gate.golden.txt`                                               |                               |
| walk (`experiments/walk/walk.sh`)                     | NONE: no living launcher (not an `eval.ts` verb, nothing launches it, default binary still the pre-exp186 `target/release/humanify`). Port-or-retire stays Andrew's decision (dev-harness.md §1.8)                       | -                                                                    | -                             |

How faithful each one is, and what was patched:

- **neutrality, ask-trace, e2e:** the launcher files run unchanged from a
  sandbox. The only stand-ins are the recorders.
- **library:** `run.py`'s `main()` runs for real under the system `python3`
  (the real launcher uses the exp095 venv; `<PY>` in the golden). There is
  one patch: `lib096.CORPORA[<corpus>]` points at a scratch corpus, because
  its data roots are absolute `/work` paths. `corpus.py` and `score.py`,
  which run.py launches as subprocesses, are recorders. The `--jobs 1`
  setting keeps the log order fixed (with 4 jobs the order is completion
  order). The `eval library` -> `run.sh` -> venv layer is not in the golden.
  `eval-library-args.test.ts` already holds the argv `eval.ts` hands run.sh.
- **e2e:** first recorded from a sandbox with a PASS-THROUGH recorder to
  the real release binary, inside `unit`, which then had to build the
  binary itself (`unit` 111 s -> 228 s on a clean checkout). Superseded by
  §1b: the e2e stage now logs and holds its own launches.
- **boot gate:** `boot-gate.sh` is sourced in place, under `env -i` with a
  seeded session environment. `bun` (in `$HOME/.bun/bin`, which the gate
  puts first on PATH), `timeout` and `env` are recorders. The last two then
  run the real tool.
- **match-truth:** run from a scratch cwd (its defaults are cwd-relative),
  with the recorder at `<cwd>/target/release/humanify-ag` and an `npm`
  recorder that writes the tarball `npm pack` would download.

Findings while recording (observed only, nothing changed):

1. **The neutrality legacy leg is unreachable through eval.ts since exp171.**
   eval.ts always hands neutrality.sh a setting: `library.index` in names
   mode, `library.matching=off` in off mode. `leg_flags`' no-`--config`
   branch then refuses ("takes no --config file; --pipeline-set cannot
   reach it"). The refusal is the golden for the eval.ts path. The branch's
   argv is recorded separately, from neutrality.sh fed a run config with no
   setting (eval.ts cannot write one today). L3 can keep that branch or
   drop it. Either way it is a decision, not a port detail.
2. **Two session variables survive the boot gate's scrub:**
   `ANTHROPIC_SMALL_FAST_MODEL` (a model override, the same class the
   2026-10-02 incident stripped) and `CLAUDE_CONFIG_DIR`. Recorded as they
   are. Whether to strip them is a product decision for the boot gate.
3. `launch_parity_test.rs`'s transcribed neutrality entry is stale, as
   dev-harness.md §1 already says. The golden above is the live shape it
   should read in L4.

## 1b. The e2e golden is held by the e2e stage (2026-11-06)

Andrew's decision: hold the e2e golden inside the `e2e` stage, the one stage
that runs after `rust:build`, instead of building the binary in `unit`.

- `scripts/e2e.ts` logs every launch it makes (the 19 `detect` calls and
  the 133 runs: argv through a `printf %q` twin, the API key it hands over,
  the `--config` file's lines), normalises the scratch dir, the repo path
  and the stub's port, and after the last pair compares the log with
  `launch-goldens/e2e.golden.txt`. A difference fails the stage with the
  first differing line. `UPDATE_LAUNCH_GOLDENS=1 npx tsx scripts/e2e.ts`
  re-records it from that live run.
- The format is the shell recorder's, so the golden recorded from the
  sandbox is held UNCHANGED: the first native run matched all 152 launches,
  and a re-record with `UPDATE_LAUNCH_GOLDENS=1` wrote identical bytes
  (`cmp` equal against the copy taken before the change).
- `shellQuote` (the `printf %q` twin, `scripts/lib/launch-golden.ts`, with
  `goldenOf` moved there from `test/launch-recorder.ts`) is held to the
  real bash on 21 sample strings by `test/launch-golden.test.ts`, and
  refuses a control character rather than guessing.
- One launch change, no byte change: `detect --toolchain` now runs with
  `HUMANIFY_API_KEY` removed from its environment (it asks no model), so the
  golden line does not depend on the caller's shell. Before, it inherited
  the caller's key; the sandbox recording had it empty.
- **Drift proof:** planted `llm.retries` 0 -> 1 in e2e's launch config
  (one word). The e2e stage FAILED (exit 1): `line 4: golden: humanify-config|
retries = 0 / now: humanify-config| retries = 1`. Reverted; golden `cmp`
  equal.
- The e2e case and its `cargo build` are gone from
  `test/launch-goldens.test.ts`.

## 2. Frozen proof data

Copied, never moved. The trees are hardlinked (`cp -al`, same filesystem).
Everything else is a real copy. Every file was sha256-hashed both in the
copy and in its source, and the two manifests compared:
`/work/dev-harness-proof/manifests/*.sha256` (`<what>-<label>` = the copy,
`source-...` = the original). All 66 pairs are equal.

| What                                                              | Source                                                                                       | Dest                                                           |   Files |         Bytes | Manifest equal                       |
| ----------------------------------------------------------------- | -------------------------------------------------------------------------------------------- | -------------------------------------------------------------- | ------: | ------------: | ------------------------------------ |
| eval reference trees, 6 labels (+ 3 run configs)                  | `/work/eval-refs-frozen/<label>` (the doc said `/tmp/eval-work`; copied off /tmp 2026-11-05) | `/work/dev-harness-proof/trees/`                               | 348,995 | 8,716,372,232 | yes (also equal to `/tmp/eval-work`) |
| eval reference cards, 6 labels                                    | `/work/exp171-names-default/experiments/034-eval-harness/results/<label>`                    | `/work/dev-harness-proof/cards/`                               |     210 | 4,248,544,802 | yes                                  |
| library labels, cards + summaries (bun-dev, bun-held) x 3 indexes | main checkout `experiments/096-library-matching-score/results/exp184-*`                      | `/work/dev-harness-proof/library/results/`                     |     288 |       991,238 | yes                                  |
| library raw reports, bun-dev x 3                                  | `/work/exp150-data/bun-dev/frozen/results/exp184-*`                                          | `/work/dev-harness-proof/library/reports/bun-dev/`             |     549 |   713,139,026 | yes                                  |
| library raw reports + private cards, bun-held x 3                 | `/work/exp150-data/bun-held/frozen/results/exp184-*`                                         | `/work/dev-harness-proof/library/reports/bun-held/`            |     279 |   150,070,372 | yes                                  |
| library labels, cc x 3 (CC-derived)                               | main checkout `.../results/exp184-*-cc`                                                      | `/work/cc-library-truth/dev-harness-proof/library/results/`    |      12 |        51,233 | yes                                  |
| library raw reports, cc x 3 (CC-derived)                          | `/work/cc-library-truth/frozen/results/exp184-*-cc`                                          | `/work/cc-library-truth/dev-harness-proof/library/reports/cc/` |      15 |    77,947,343 | yes                                  |

The cc corpus's data stays inside `/work/cc-library-truth`, as the doc
requires. Nothing derived from it is committed. The indexes and the index
cache that proof (b) reads (`/work/library-index/cb64…`,
`/work/exp177-data/ix/…`, `/work/exp184-data/ix-default`,
`/work/exp096-data/cache`) were read in place and not copied.

## 3. Oracles, run once (proofs a and b)

Both ran at **`f8bbb20ae263b87b446d1dbd934a4fb12f9dbd08`** (main at the time; a detached worktree,
`/work/exp191-oracle-f8bbb20a`, clean, under `flock /work/heavy.lock`, one
after the other). Its harness code is byte-identical to this branch's head:
this branch changes no file under `scripts/`, `experiments/lib/`,
`experiments/034-eval-harness/*.ts|sh`, `experiments/096-*/*.py` or
`experiments/170-*`. Outputs: `/work/dev-harness-proof/oracles/` (logs
`oracle-a.log`, `oracle-b.log`; outputs `a/<label>/`, `b/<label>/`; cc under
`/work/cc-library-truth/dev-harness-proof/oracles/b/`).

| Proof                | Labels                                                                | What ran                                                                                                                                                                | Reproduced                                                                                                                                                                                                                                                                                                                                             |
| -------------------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| (a) 034 KPI scorer   | `ref-names-0357696e-r1..r3`, `ref-scratch-0f338ffa-r1..r3` (24 pairs) | `rescore.ts <label> --cards <frozen cards> --work <frozen trees>`, `summarize.ts`, `trail-report.ts` (names labels, which have report pages), `leaderboard.ts` per mode | **yes**: 24/24 cards, 12/12 report pages, 3/3 names summaries byte-equal. The 3 scratch summaries differ only by the later `namingModes` key (`oracles/a/scratch-summary-diffs.txt`; equal once it is removed). Today's output is the expected bytes for L2a, per the doc's rule. Leaderboard text kept in `oracles/a/leaderboard-{names,scratch}.txt` |
| (b) exp096 scorer 14 | `exp184-{cb64,iv,spec}-{bun-dev,bun-held,cc}`                         | `score.py <label> --corpus <c>` (exp095 venv)                                                                                                                           | **yes**: 9/9 summaries + every card byte-equal (bun-held's private cards, which score.py rewrites in place in the data dir, were byte-equal, so nothing needed restoring)                                                                                                                                                                              |
| (b) determinism.py   | -                                                                     | NOT RUN                                                                                                                                                                 | It compares two labels of ONE configuration. No such pair still has its raw reports on disk (the committed `bands.json` labels `ix-v2-agreement{,-repeat}` lost theirs), and the nine labels are nine configurations. Reproducing on-disk bytes is the check it stands for (dev-harness.md §4 b)                                                       |

Notes for L2a/L2b: `rescore.ts` takes each pair's prior from the card's `-run.json`
(`inputs.prior` = `/tmp/eval-work/<label>/<from>-rebased/...`), not from
`--work`. The copy there is sha-equal to the frozen one (manifests), so the
run read the same bytes. Once /tmp is cleared, a port must map that prefix to
`/work/dev-harness-proof/trees/` (trail-report was run that way here).

## 4. Gate

| Stage                    | Worktree | Clean checkout |
| ------------------------ | -------- | -------------- |
| typecheck                | PASS     | PASS           |
| lint                     | PASS     | PASS           |
| rust:fmt                 | PASS     | PASS           |
| rust:clippy              | PASS     | PASS           |
| knip                     | PASS     | PASS           |
| knip:prod                | PASS     | PASS           |
| census:clones (advisory) | PASS     | PASS           |
| census:concurrency       | PASS     | PASS           |
| unit (365 tests, 11 new) | PASS     | PASS           |
| rust:unit                | PASS     | PASS           |
| rust:build               | PASS     | PASS           |
| rust:format-golden       | PASS     | PASS           |
| e2e                      | PASS     | PASS2E         |

## 5. What this lane could not do

- **determinism.py**: not run (no same-configuration label pair with raw reports remains; see §3).
- **The walk**: no golden. It has no living launcher, and port-or-retire is Andrew's decision.
- **The `eval library` -> `run.sh` -> venv python layer** is not in the library golden: run.py is driven in-process under the system python with its corpus root patched (§1). The argv eval.ts hands run.sh is held by `eval-library-args.test.ts`.
- **Proof (b')** (re-freezing truth layers) and the statement-level hash check under (a) belong to L2a/L2b, not L0.
- **No tag created** (as briefed). The indexes proof (b) reads were not copied (46 GB; read in place).

## Proposed `harness-ts-final`

`harness-ts-final` = **`f8bbb20ae263b87b446d1dbd934a4fb12f9dbd08`**, the commit both oracles ran on (main when this lane started). This branch adds tests, goldens, one docs row and this README, and its only harness change is the e2e stage holding its own launch golden (§1b, no launch byte changed), so the merge of this branch would be an equivalent tag point that also carries the goldens. Either works. Andrew picks.
