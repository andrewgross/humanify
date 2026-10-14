# exp111: the index builder's memory growth

## STATUS (2026-10-14)

- **State.** DONE. Branch `exp111-builder-memory`, off main `6182666d`. One change in `crates/humanify-index` only:
  the builder binary now uses mimalloc as its global allocator, as the pipeline binary has since 2026-09-24. No
  change in `humanify-core`, so the pipeline binary is untouched and no neutrality run was needed.
- **Totals first.** Same 431 package versions, same build (4 workers, 8 cores, 3 slices), peak RSS of the builder
  process:

  | versions started    | before (glibc) | after (mimalloc) |
  | ------------------- | -------------- | ---------------- |
  | 1-100               | 11.3 GB        | 4.2 GB           |
  | 101-200             | 16.4 GB        | 6.7 GB           |
  | 201-300             | 14.8 GB        | 1.6 GB           |
  | 301-400             | 14.7 GB        | 2.4 GB           |
  | 401-431             | 9.4 GB         | 2.8 GB           |
  | **whole run, peak** | **16.4 GB**    | **6.7 GB**       |
  | median RSS, 201-300 | 14.7 GB        | 0.8 GB           |
  | wall time           | 652 s          | 546 s            |

  **Outputs are byte-identical.** All 16 index table files have the same sha256 before and after. Of the 1,724
  shards, 5 or 6 differ between any two runs, and they differ just as much between two runs of the SAME binary. Each
  of those shards is a failed bundler build: it has 0 rows, and its `notes.builds` holds the bundler's first error
  line, which bun and esbuild report in a different order from run to run.

- **Cause.** Nothing in the build loop kept data from one version to the next. Shards are written and dropped, and
  there is no global cache, interner or retained subprocess output in `humanify-index` or `humanify-core`. The
  growth came from **glibc malloc**. A version with large bundles (tens of MB of code plus their sourcemaps) needs
  about 1-2 GB while it is fingerprinted: the oxc AST, unit prints and decoded segments. Each worker thread got that
  memory from its own glibc arena. When the version was done, the memory was freed but kept in the arena, not given
  back to the OS. So each arena held the largest version its worker had ever built, and the process RSS was roughly
  the sum of those high-water marks. That sum can only go up as a process builds more versions. A longer process
  meets more large packages, which explains exp108's 31-35 GB parts and the exit-137 kills in exp101/108.
- **Tests.** `crates/humanify-index/src/main_test.rs` checks that a 64 MB allocation made through the global
  allocator shows up in mimalloc's committed memory. Under glibc it moved that figure by 1.2 MB, so the test failed
  first. With the change it passes. The RSS curves above are the evidence that the change fixes the growth.
- **Consequence.** `experiments/108-app-corpus/scripts/build_chunks.py` (about 200 short processes) is no longer
  needed to stay under the memory limit. A long-lived builder now drops back to under 1-2 GB between large
  versions. Its peak is the working set of the versions running at that moment. The worst case here was four
  consecutive versions of `code-inspector-plugin`, each about 69 MB of bundles, all on the 4 workers at once.

## Evidence

Measured with `scripts/rss_run.py`. It runs one `humanify-index build` under `taskset -c 0-7` and samples
`/proc/<pid>/status` VmRSS every 2 s against the number of `building` lines. Logs and RSS tables are in
`/work/exp111-data/logs/`.

- Inputs: 431 pinned versions, taken from exp108's `chunks-k2-l12m4` pin chunks 1-4. These are the ones whose
  install already existed. The work directories are a `cp -al` copy of exp108's, so installs and bundles were
  reused and every run fingerprinted the same bundles. Registry: `/work/exp108-data/idx/registry`, offline. The
  run used a NEW `--store` and `--out` for each label under `/work/exp111-data/`. Recipes: bun target=bun,
  bun target=node and esbuild target=node, all cjs+minify. Bundlers: bun 1.4.0 and esbuild 0.28.2.
- Control `glibc-mmap128k`: the same old binary, run with `MALLOC_MMAP_THRESHOLD_=131072 MALLOC_TRIM_THRESHOLD_=131072`.
  It peaked at 16.1 GB, the same as before. The memory kept is many small blocks (AST nodes, strings), not large
  ones, so glibc's mmap threshold does not help.
- Single-worker probe: `@univerjs/sheets@0.23.0` (95 MB of bundles) followed by 60 small versions.
  - glibc: peak 2.2 GB, and it stayed at about 2.0 GB through the small versions.
  - mimalloc: peak 0.9 GB, then back to about 0.1 GB.
- A first mimalloc run on 4 cores (`mimalloc`, which shared the box with a single-worker glibc run) gave the same
  shape: peak 7.6 GB and a median of 0.8 GB over versions 201-300.

## Not done

- The peak of one large version (about 1-2 GB for about 20 MB bundles) was not reduced. It is real working set, not
  growth. If a 10x index meets larger bundles, the next lever is to schedule work by size, so the versions of one
  huge package do not run on all workers at once.
- The failed-build error text in shard notes changes between runs. It does not affect the index, but it means the
  store is not byte-stable on rebuild. A fix would be to record a normalised error class instead of the raw line.
