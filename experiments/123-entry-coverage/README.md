# exp123: would building more entries per package version (exports conditions, fields, deep files) recover exp122's "not-emitted" misses? (census first)

## STATUS (2026-10-16)

DONE on branch `exp123-entry-coverage` (off main `7daf1f0b`). Census scripts, `census.json` and this README only.
**No builder change and no index built: the entry extension does not pay.** Neutrality is not needed: nothing in
`crates/` changed.

- **Entry coverage is not why the code is missing.** exp122 attributed most recipe-built class-d bytes to code "the
  index's own build never emitted". Its test, though, was a token-diff ratio under 0.5, and that also catches files
  the build DID emit. This census traces every one of those units to the package's own index build:
  - **What the brief's extension could recover is at most 0.01 points on every recipe-built set.** The brief's
    extension means one entry per (subpath x condition set), plus the `module`, `browser` and `main` fields, plus
    deep files. Held-out gets 0.005, pop-bun 0.009, apps-rollup 0.008 and task-master 0.010. Wrangler, a re-minified
    published bundle, gets 0.49.
  - **The bars were +1 on pop or apps, or +0.5 on held-out.** They are missed by a factor of more than 50.
- **It would also be expensive.** The extension was sized on a random sample of 300 k2-all roots: every declared
  entry point bundled, beyond the builder's require+import app. It adds **+47.5% source bytes**: 38.3 MB on top of
  80.6 MB, in 30% of the roots. On k2-all that is about **+3 GB on 6.4 GB**, plus one more bundle per root per slice,
  roughly **+6 h on the 12 h build at 30 cores**.
- **What the code really is: the package's OWN pre-bundled file, dropped whole as a "sponge".** Packages like this
  ship a rolldown/esbuild-built `dist/**/chunk.js`. The index builds that file, and the source maps show it in the
  output. Then `sponge_evidence` (build.rs, unit-rows/4) drops EVERY unit of it, because a single
  `//#region node_modules/<other>/...` marker in the file names another package. Of these units, the ones that are
  the package's own code (the truth gives them no copy origin):

  | set          | points |
  | ------------ | -----: |
  | apps-esbuild |   1.72 |
  | pop-bun      |   1.23 |
  | pop-esbuild  |   1.08 |
  | apps-rollup  |   0.47 |
  | held-out     |   0.40 |

  Units that are recognised copies of another package add pop 1.24, apps-esbuild 0.36 and apps-rollup 0.17.

  This class is narrow: at most 6 packages per set, and ONE package is 70-80% of it on every set. It is the next
  lever, but it is not this lane's to build (see §4).

## 1. Method (aggregates only; dev = held-out, task-master, wrangler, apps, pop; test and test2 not touched)

Input: exp122's private per-unit capture of main's shipped defaults on k2-all (scorer 9; labels `exp119-k2all-st`,
`-apps`, `pop-exp119-k2all-st`).

1. `scripts/select_ne.py` re-derives exp122's census cause for every class-d unit and keeps exp122's
   `not-emitted` units. A unit is `not-emitted` when it has no counterpart, or a token-diff ratio under 0.5, and no
   build or normalisation explains it. The bytes per set equal exp122's `census.json` (pop-bun 2.68, apps-esbuild
   2.10, and so on). That gives 1,301 (slice, package, version, truth file) keys. Before `/work/exp122-data` was
   pruned, the inputs were copied to `/work/exp123-data/exp122-inputs`.
2. `scripts/reach.py` takes each key against the package's OWN recipe/4 build for that slice. A key whose version
   has no own root is `dep-only`. The checks run in this order:
   - **the truth file is not in the npm package**: `file-not-in-package`, a chained source path;
   - **the file is among the files the build's source maps emitted**: `file-emitted`;
   - **otherwise, which entry would reach it.** Every entry point the package declares is a leaf: each leaf of the
     `exports` map with its condition path, and the `main`, `module`, `browser` (string and object map),
     `jsnext:main`, `es2015`, `unpkg` and `jsdelivr` fields. Each leaf's static import closure inside the package
     comes from an esbuild metafile, with the leaf's conditions active and other packages external. The file
     takes the class of the cheapest leaf kind whose closure holds it, with ties broken by overlap with the files
     the app bundled from that package. If no leaf reaches it, the class is `deep:*`.
3. `scripts/why_dropped.py` explains the emitted files. It emulates `sponge_evidence` line for line: a
   vendored-path directory, or a strict module-marker comment naming another package. The check holds: of the 93
   files it calls bundler-marker sponges, **0** have any k2-all row for that file@version. The rest are split by
   whether the index has rows for that file@version in the slice. The script writes `census.json`.
4. `scripts/cost_sample.py` measures what the extension would add. It takes 300 random k2-all roots (seed 123,
   bun-node-cjs-min) and compares the union of all leaf closures with what the current build emitted.

## 2. The census (points of the set's library bytes; `census.json` "grouped")

| set          | not-emitted (exp122) | entry coverage (exports cond. / fields / deep) | own pre-bundled file dropped as sponge: own code | ... : recognised copy | vendored-path sponge | emitted, no row of the file | emitted, rows, not this statement | dep-only, no sponge | truth file not in package |
| ------------ | -------------------: | ---------------------------------------------: | -----------------------------------------------: | --------------------: | -------------------: | --------------------------: | --------------------------------: | ------------------: | ------------------------: |
| held-out     |                 0.72 |                                      **0.005** |                                             0.40 |                  0.02 |                 0.06 |                        0.00 |                              0.23 |                0.00 |                      0.00 |
| task-master  |                 1.75 |                                      **0.010** |                                             0.00 |                  0.00 |                 0.00 |                        0.00 |                              0.00 |                0.00 |                      1.73 |
| wrangler     |                 5.69 |                                      **0.487** |                                             0.00 |                  0.00 |                 0.51 |                        0.00 |                              0.47 |                4.21 |                      0.02 |
| pop-bun      |                 2.68 |                                      **0.009** |                                             1.23 |                  1.24 |                 0.02 |                        0.00 |                              0.19 |                0.00 |                      0.00 |
| pop-esbuild  |                 2.42 |                                      **0.000** |                                             1.08 |                  1.24 |                 0.03 |                        0.00 |                              0.00 |                0.00 |                      0.06 |
| apps-esbuild |                 2.10 |                                      **0.000** |                                             1.72 |                  0.36 |                 0.03 |                        0.00 |                              0.00 |                0.00 |                      0.00 |
| apps-rollup  |                 2.14 |                                      **0.008** |                                             0.47 |                  0.17 |                 0.01 |                        1.22 |                              0.22 |                0.05 |                      0.00 |
| apps-webpack |                 0.17 |                                      **0.006** |                                             0.00 |                  0.03 |                 0.03 |                        0.03 |                              0.07 |                0.00 |                      0.00 |

Per (slice, package, version, file) key, 1,301 in all:

| class                                  |  keys |
| -------------------------------------- | ----: |
| emitted by the package's own build     | 1,097 |
| version only another root's dependency |   125 |
| truth file not in the package          |    31 |
| `deep:no-exports`                      |    30 |
| `field:module`                         |    11 |
| `exports:node-set`                     |     6 |
| `field:main`                           |     1 |

No key needed a `browser`, `module`, `development`/`production` or other export condition. The few `exports:node-set`
hits are files the closure tool reached and the bundler tree-shook.

Reading the other columns:

- **"emitted, no row of the file" on apps-rollup (1.22)** is almost all ONE 793 KB data-table statement: 0.84
  points in one unit. The index's rollup build emits its file but attributes no unit row to it. This is the
  data-table/payload territory of exp109 and exp113.
- **task-master's 1.73** is truth files that are the package's pre-build TypeScript sources, reached through a
  chained source map. They are not files of the npm package, so no entry choice can reach them.
- **wrangler's 4.21** is versions held only as another root's dependency. Wrangler is a re-minified published
  bundle, not a recipe build. This is exp122's outlier, unchanged.

## 3. Cost of the extension (300 random k2-all roots, bun-node-cjs-min)

| measure                                               | value                                                                                |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------ |
| source bytes the current build fingerprints (emitted) | 80.6 MB in 6,538 files                                                               |
| added by bundling every declared entry point          | 38.3 MB in 2,704 files (+47.5%)                                                      |
| roots that gain any file                              | 91 / 300 (30%); 120 have `exports`                                                   |
| mean entry points (leaves) per root                   | 15.3                                                                                 |
| projected on k2-all                                   | about +3 GB index, about +6 h build at 30 cores (one more bundle per root per slice) |

The ceiling is at most 0.01 points of the recipe-built sets, at a cost of about +47% index. **Not built.**

## 4. The lever this census found instead (not built here)

**Region-aware sponge rows.** The builder drops a whole file as a sponge when any module marker in it names another
package (`sponge_evidence`, bundler-marker). A package's own rolldown/esbuild-built chunk often inlines a few
dependencies, and the markers delimit them exactly: rolldown `//#region` ... `//#endregion`, and esbuild/Bun
`// <path>` up to the next marker. Keeping the rows OUTSIDE the foreign regions, under the shipping package, would
make the package's own code visible. Today that code is invisible: apps-esbuild 1.72, pop-bun 1.23, pop-esbuild
1.08, apps-rollup 0.47 and held-out 0.40 points, as ceilings.

The rule meets the +1 bar on pop and apps, but three things weigh against it:

- **It is narrow.** One package is 70-80% of the class on each set, and there are at most 6 packages per set.
- **It needs a new unit-rows version.** That version is in every module shard key, so it means a full k2-all
  re-fingerprint (the builds on disk can be reused).
- **It touches the same sponge machinery as lane exp121.** The foreign regions must stay dropped, or be credited
  to the package their marker names (exp115's marks). Otherwise they recreate the collisions that made exp097
  drop sponges in the first place.

## Reproduce

```bash
S=experiments/123-entry-coverage/scripts
python3 -I $S/select_ne.py            # -> /work/exp123-data/ne-units.jsonl.gz (private)
python3 -I $S/reach.py 24             # -> /work/exp123-data/reach.jsonl.gz (private), esbuild closures cached
python3 -I $S/why_dropped.py          # -> census.json + aggregates
python3 -I $S/cost_sample.py 300 123 16
```
