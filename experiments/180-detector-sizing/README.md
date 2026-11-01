# exp180 — can a small "detector" index replace the package list and the 43 GB index for package discovery?

STATUS: MEASURED (2026-11-01), offline simulation only. Branch `exp180-detector-sizing` off main `5a48564bd`.
**Nothing under `crates/` changed**; no real index was built; no gate stage covers these files (Python scripts and
a stand-alone measurement crate under `experiments/`), so `npm run check` was not run. Every number is a
simulation over the full index `cb64c1f5` (format /4, 9,896 pinned packages = 12,023 owners in `versions.json`,
65,839 versions, Bun slices only) and three scored labels on it. NUMBERS-STATUS

## Totals first

**Yes, build it — but not the way the brief sketched it.** A detector that keeps, per package, about 20 prints
chosen to COVER every indexed version from as many source files as possible, and that may use prints shared by up
to three packages, is about **1 MB** and keeps **98-99%** of what today's 43 GB index names. A detector of each
package's K "most distinctive" prints (the brief's design: unique, in most versions, large) keeps only 77-85% at the
same size, and the version step needs its own second look: the newest versions are the wrong ones to fetch.

| set (truth)                                             | today (full index) | recommended detector `fcover3p`, K=20 (77k prints, 1.1 MB) | K=5 (38k, 0.6 MB) | "most distinctive" K=20 (`coverage`, 2.1 MB) | text discovery (banners + package.json) |
| ------------------------------------------------------- | -----------------: | ---------------------------------------------------------: | ----------------: | -------------------------------------------: | --------------------------------------: |
| bun-dev, 90 bundles: recall                             |             82.16% |            **80.56%** (keeps 98.1% of today's named bytes) |    77.85% (94.8%) |                               69.49% (84.6%) |                            5.56% (6.8%) |
| bun-held, 30 bundles: recall                            |             82.24% |                                         **81.52%** (99.1%) |    79.07% (96.1%) |                               72.32% (87.9%) |                            1.22% (1.5%) |
| cc 2.1.216 (sample): recall                             |             81.07% |                                         **80.01%** (98.7%) |    80.01% (98.7%) |                               44.75% (55.2%) |                              0.00% (0%) |
| packages flagged per bundle (bun-dev; truth holds 78.4) |         all 12,023 |                                  71.3, of which 4.6 absent |  66.5, 3.5 absent |                             51.7, 0.7 absent |                                     1.0 |
| wrong-family bytes kept (bun-dev; today 384,084)        |            384,084 |                                                    264,710 |           261,656 |                                      273,535 |                                  11,219 |

- **Detection.** The recommended detector flags packages holding 95.0% of bun-dev's library bytes (91.1% bun-held,
  98.9% cc), 98.2% of the bytes of packages the index holds at all; by count 83.3% of all packages and 93.1% of
  those over 1 KB. What it misses is mostly packages the index does not hold (the kryptokrona wallet stack,
  @liquify/specs, tslib: no detector built from this index can flag them) and zod 4.6, whose prints the index
  holds but whose picked prints sit in files those bundles do not contain.
- **False flags** (flagged, no unit of the family in the bundle) are 4.6 per bundle on bun-dev, 6.0 on bun-held,
  and read by eye they are almost all FORKS and near-copies of a package that IS present: `obug` (a fork of debug)
  in 40 bundles, `tunnel-agent` for tunnel, `lodash-compat` for lodash, `source-map-js` for source-map,
  `ajv-draft-04` for ajv, `@hot-loader/react-dom` for react-dom. Their cost is a fetch, not precision: today's full
  index already holds them, so restricting it to flagged packages never adds a competitor.
- **Text discovery** (`--library-index-from-bundle`'s seeds) names 1.0 package per bun-dev bundle, 0.7 per
  bun-held bundle and none usable on Claude Code (its two banners spell `azure/msal-…` without the `@`); it keeps
  6.8% / 1.5% / 0% of today's named bytes. It adds nothing on top of the detector (+0.00 to +0.02 points).
- **Versions.** Fetching each detected package's NEWEST versions loses a lot: today's print-supported claims kept
  by the latest 1 / 5 / 20 indexed versions are 82 / 91 / 97% (bun-dev), 70 / 80 / 91% (bun-held), 58 / 59 / 80%
  (cc). The TRUE bundled version alone keeps 99.5% / 99.8% / 100%. So the targeted fetch must find the version,
  not assume the date: an ideal second-stage vote (each claimed unit votes for the versions holding its print;
  keep the top 1) keeps 99.5% / 99.0% / 98.2%; the detector's own hits are a weaker vote (one version keeps 89.7%
  / 93.0% / 71.3% at K=20, 95.5% / 95.3% / 85.8% at K=100).

**Recommendation.** Build a detector with the `fcover3p` rule at K=20 (about 1 MB for 11,430 owners, roughly 100
bytes per package), m=1 hit. Pair it with a version step that votes from each detected package's full print list
(a per-package file fetched after detection), keeping 1-3 versions. Do not use the "most distinctive prints" rule,
a size rule, the newest-N versions, or text discovery as the discovery signal. K=5 (0.6 MB) is the floor where
the loss is still under 5% of today's named bytes on every set; K=100 saturates at 1.9 MB (99.5% / 99.5% /
98.8%).

## Design of the measurement

1. **Candidates** (`index_pass.py`, the one heavy read, under `/work/heavy.lock`): both slices'
   `units.<slice>.jsonl` merged by hash. Every print held by at most 3 owners becomes a candidate row per owner:
   nodes, content, the owner's indexed versions holding it (a bitmask over `versions.json` order), how many of its
   10 newest versions hold it, both-slices flag, source file, all owners. Prints below the LOOKUP's own floors
   (30 nodes, 3 content tokens: `mmlib.RULES`) are never picked — a print the matcher would not read is no
   evidence.
2. **Rules** (`simulate.py`), each a per-owner ordering cut at K = 1, 3, 5, 10, 20, 50, 100 (and "all"):
   - `coverage` — unique prints, most of the owner's versions first, then both slices, then size (the brief's rule);
   - `size` — unique prints in at least half the versions, largest first (the other brief rule);
   - `spread` — `coverage`, one print per source file before any second;
   - `recent` — unique prints in most of the 10 NEWEST versions first;
   - `cover` — greedy VERSION COVER over unique prints: each pick covers the most versions among those covered the
     fewest times so far, so the first picks cover every version once, the next ones twice;
   - `fcover` — `cover`, but a version's count is the number of DISTINCT FILES among the picks holding it;
   - `cover3` / `cover3p` / `fcover3p` — the cover rules over prints held by up to 3 owners (unique preferred on
     ties). `cover3` flags every holder of a hit print; the `p` rules flag only the owner the print was picked for.
     Why the rules evolved, in order, each from a miss read on bun-dev (rule 2: the mechanism was checked on the
     population after): `coverage` missed typescript (7.25 MB in two bundles) because typescript has 80 indexed
     versions from 0.8.3 and its widest prints are OLD code; `recent` (newest versions first) then lost the old bundles instead (gistpad's typescript 3.9.10);
     `cover` fixed version spread but still missed typescript 5.9, every one of whose prints is also held by
     `@ts-morph/common` and `monaco-editor` (carriers); `cover3` caught it but flagged the carriers too (29 false
     flags per bundle); `cover3p` flags only the picked owner (7.4); `fcover3p` stopped picking three prints of zod's
     `dist/esm/v3/types.js` (the v3 compatibility layer in 3.x and 4.x alike, absent from bundles using the v4 API).
3. **Detection**: a bundle flags an owner when m (1 or 2) of its detector prints are among the bundle's prints
   (every unit row's whole and function hashes from the label's `--unit-prints` report — exactly what the binary
   computes for its lookup).
4. **Judging** (scorer 14's truth, `extract.py` reusing `score.label_units` with every frozen layer; cc's labelled
   sample with span containment, as `score_sample`): a truly present package is DETECTED when any member of its
   family is flagged (byte- and count-weighted); FALSE when flagged with no unit of its family in the bundle (truth
   label, origin or via) and not the app's own package.
5. **End-to-end ceiling**: today's claims restricted to claims whose package is flagged (STRICT, the exact owner —
   the column every table reports; FAMILY, any member flagged, in the JSON), as a share of library bytes (the
   scorer's recall) and of today's named bytes. `everything` (flag every owner) reproduces today to the byte, and a
   planted false and true flag each count one (`--plant`).
6. **Versions** (`versions.py`): for every named unit, the claimed owner's rows holding any of its prints in the
   bundle's slice (the scorer's pickled unit table) give the versions supporting the claim; a version picker keeps
   the claim when it keeps one of them.

Labels: bun-dev `exp167-names-cb64` (names mode, the 82.16% exp167 measured; scorer recall reproduced here to the
byte, 176,073,145 named); bun-held `exp177-base-bun-held` and cc `exp177-base-cc` (report mode, main's binary
`5a48564bd`). The text discovery is `seeds/` — a stand-alone crate that replicates `bundle_revealed_seeds`
(`crates/humanify-cli/src/runtime_index.rs`, crate-private) line for line over `humanify-core`'s public scanners;
it reproduces the doc's Claude Code observation exactly (two `azure/msal-…` names, no `@`).

### How faithful the end-to-end simulation is

It keeps today's claims whose package survives and drops the rest. What it cannot see:

- **Claims a smaller index would ADD.** With fewer owners, a print today refused as multi-package (> 3 owners), a
  tie lost to an absent family, or a specificity refusal can turn into a claim — right or wrong. exp167 sized
  these guards at 2.4 MB (multi-package), 1.1 MB (tie fenced), 0.7 MB (tie lost) of bun-dev's missed bytes. The
  simulation counts none of it, on either side of precision.
- **Grown claims** (DOWN/MODULE/glue propagation, no print of their own: 4.5% of bun-dev's named bytes, 42% of
  bun-held's in report mode) are kept whenever their owner is flagged; in reality they follow their seeds.
- **The isolation veto** reads neighbours of the same family; dropping an unflagged family's seeds can only remove
  support from that family, which is dropped anyway, so this is faithful for the strict column.
- **Factory bodies the run kept as one unit** have no print: invisible to today's matcher and to the detector
  alike.
- **Version pick** assumes a claim survives whenever one supporting version is kept; it does not re-run the
  version-sensitive rules (none of today's lookup rules read the version, so this is close).

## Tables

<!-- tables -->

TABLES

<!-- /tables -->

## What could NOT be measured

- **The terser/esbuild re-minified sets.** Their corpora (`/work/exp126-data`, `/work/exp146-data`, remin2-4) are
  no longer on disk, and `cb64c1f5` holds Bun slices only. How a Bun-built detector does on esbuild/terser
  bundles, where exp142 measured prints mostly not surviving terser, is open. Expect it to track today's recall
  there (17-52%): the detector reads the same prints.
- **Claude Code's false flags.** Its truth is a 620-unit sample naming 44 packages; a flagged package outside the
  sample may well be present. The column reads "not judged".
- **Packages outside the index.** The detector's universe is the index's 12,023 owners. To REPLACE the
  hand-picked pin list as the source of discovery, the detector must be built over a wider package set, which
  still means bundling every candidate package at its versions once, offline (the build cost does not go away;
  the shipping and run-time cost does). At ~100 bytes per package (K=20), 100,000 packages is ~10 MB.
- **A real targeted fetch.** Building the flagged packages at 1-3 versions per bundle (71 packages) is the
  run-time `--library-index-from-bundle` path's job; its wall time on a user's machine was not measured here.
- **bun-held per-package detail** — private corpus, totals only; and the rules were developed on bun-dev's misses,
  so bun-dev reads optimistic (exp141's lesson). bun-held, never used to choose, agrees (99.1% kept).
- **Index-side collisions with app code** are judged only through the truth sets (app-package flags: 10 of 6,414
  bun-dev flags, all the app's own package). No bundle outside these 121 was read.

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python; E=experiments/180-detector-sizing; D=/work/exp180-data
flock /work/heavy.lock $PY -I $E/index_pass.py /work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28 $D/pass
$PY -I $E/extract.py bun-dev exp167-names-cb64 $D/x-bun-dev.json.gz      # also bun-held exp177-base-bun-held, cc exp177-base-cc
CARGO_TARGET_DIR=$D/seeds-target cargo build --release --manifest-path $E/seeds/Cargo.toml
xargs -a <(awk '{print $2}' $D/bundles-bun-dev.txt) $D/seeds-target/release/exp180-seeds > $D/seeds-bun-dev.jsonl
$PY -I $E/versions.py $D $D/versions.json
$PY -I $E/simulate.py $D/pass $D $D/sim.json --plant
python3 $E/report.py $D/sim.json $D/versions.json
$PY -I $E/eyeball.py $D/pass $D fcover3p 20 10                         # the false flags, read by eye
```

`bundles-<corpus>.txt` lists each entry's frozen bundle path (`lib096.corpus_path(entry, "bundle")`). Data
(extracts with unit prints, seeds, simulation JSON) stays under `/work/exp180-data`; nothing from the CC truth is
committed.

## Files

- `index_pass.py` — the heavy pass: candidate prints (≤ 3 owners) with version masks; `owners.json`.
- `extract.py` — per label, truth-labelled units, claims and every bundle print.
- `seeds/` — the text discovery's seeds, replicated (own cargo workspace, never a pipeline member).
- `simulate.py` — the rules, detectors, detection / false flags / end-to-end / version pick; `--plant`.
- `versions.py` — latest-N, nearest-N (oracle) and ideal vote-N version pickers; per-unit support masks.
- `report.py` — the tables above, from the JSON. `eyeball.py` — false flags with the code they hit.
