# exp205 — no host-wide lock: the coordinator schedules, folders are claimed

STATUS (2026-11-08): DONE. Gated on the branch (worktree + clean checkout)
and merged to main --no-ff, with the merge commit gated from a clean checkout.

Andrew, 2026-11-08: "i dont know if we want to do explicit locks since we
have a master agent coordinating all of this work, i think we just need to
be mindful of what we can run together".

## What was removed

- `runners::heavy_lock`, `runners::lock_file`, `HEAVY_LOCK`
  (`/work/heavy.lock`) and every caller: `eval score`, `eval neutrality`,
  `library score` (verbs.rs), the two bun-dev batches (batch.rs),
  `index build`, the walk's per-hop lock, `library corpus build`.
- The `HELD_LOCK` environment read (`env.rs`, `HostEnv::held_lock`): nothing
  reads it now, so a stale `HELD_LOCK=1` in a caller's environment matters
  nowhere. A test (`main_test::the_host_wide_lock_is_gone`) fails if any
  harness source names `/work/heavy.lock`, `HELD_LOCK` or
  `/work/index-build.lock` again.
- The host-wide `/work/index-build.lock` that `index build` also took (it
  stopped two builds of ANY store running at once).
- The walk manifest's `heavyLock` field and each hop record's
  `lockWaitSeconds` (there is no lock to wait for).
- The `VERBS` table's `lock` field.

## What replaced it

1. **`footprint` on every verb** (`VERBS` in `crates/humanify-ag-dev/src/main.rs`):
   memory, LLM use, time, folders written — measured where a run recorded
   it (ref-names-111401fe run manifests: 9.7–17.4 GB, 189–500 s per scored
   leg, ~2.4 h per five-pair eval; walk 9ce9dee2: 17.3 GB, 89–490 s/hop;
   exp152/exp177: index merge 117 GB serial / 103.6 GB at merge-jobs 30,
   pinyin chunks 55–65 GB, 6.9 h full Bun build), "unmeasured" otherwise.
   `cargo dev` prints it. Light verbs print one `light` line. Test:
   `every_heavy_verb_declares_its_footprint` (16 verbs that run the
   pipeline, a model, a build or an index/corpus build).
2. **Folder claims** (`runners::claim`): a run writes `<folder>.live-run`
   beside the folder it writes (created exclusively; pid + process start
   time + what it is) and removes it when done. A second run on the same
   folder is REFUSED at once, naming the holder — nothing ever waits. A
   marker whose holder is dead (pid gone, or a reused pid with a different
   start time) is taken over, so a crash blocks nobody. Not a flock on
   purpose: a child forked in another thread holds a copy of the open file
   until it execs, which made a just-released claim read as held (seen in
   the walk resume test).

   Claimed folders: `eval score` `<workdir>/<label>`; `eval neutrality` its
   cache AND the workdir's `neutrality-*` files (two runs on one cache void
   each other's verdict: the baseline leg's zero-write count would hold the
   other run's writes); `eval walk` its `--walk-dir` (a `--resume` of a live
   walk is refused); `index build` its `--store` and `--index-work`; the two
   bun-dev batches their out dir.

## Locks kept

None. The one candidate, `index build`'s `/work/index-build.lock`, guarded
the whole host, not one folder; what it really protected (two writers of
one store — a shard's temp name is fixed per shard — or one work dir) is
now a claim on exactly those two folders. The registry snapshot stays
shared on purpose (a fetched packument is never fetched again).

## Default folders

- `eval score`: already per label (`/work/eval-work/<label>`); now claimed.
- `eval neutrality`: the workdir defaults to `/work` and the cache must be
  the shared warm one, so a per-run default would be a cold (void) run; a
  second concurrent run is refused instead.
- `eval walk`: `--walk-dir` is required; claimed.
- bun-dev batches: already per label; claimed.
- `match-truth` without `--out`: picked "the first free run-NNNN" without
  creating it, so two runs started together picked the same one. It now
  creates the directory atomically when it picks it (`runs_dir_test`).
- Not changed: `build-bin` (one worktree per commit: the same commit built
  twice at once would collide — rare), `library corpus build` (writes
  per-app folders under the corpus's data root), `library score` (per
  label).

## Proposed CLAUDE.md text

Replace "Every heavy verb (`eval score|neutrality|walk`, `library score`,
`index build`, the batch runners) takes `/work/heavy.lock` itself — `cargo
dev` prints when — so never wrap a `cargo dev` verb in `flock` (a nested
flock deadlocks); wrap only non-verb heavy jobs, such as a bare `cargo dev
check`." with:

> There is no host-wide lock (exp205): the coordinating agent decides what
> runs together, from each verb's `footprint` that `cargo dev` prints
> (memory, LLM use, time, folders written). Two runs never share a folder by
> accident: a verb claims the folder it writes (`<folder>.live-run`) and a
> second run on it is refused at once, naming the first.

## Gate

`cargo dev check`, bare, on ea90b9ca5 (with an eval and an index build running):

| stage | worktree | clean checkout |
|---|---|---|
| shims | PASS | PASS |
| no-private-addresses | PASS | PASS |
| rust:fmt | PASS | PASS |
| rust:clippy | PASS | PASS |
| census:clones | REVIEW (51 findings, the same count as main) | REVIEW |
| census:concurrency | PASS | PASS |
| census:slop | PASS | PASS |
| rust:unit | PASS (330 s) | PASS (341 s) |
| rust:build | PASS (166 s) | PASS (180 s) |
| format-golden | PASS | PASS |
| e2e | PASS (92 s) | PASS (179 s) |

The merge commit was gated again from a clean checkout before the push.
