# exp180 — can a small "detector" index replace the package list and the 43 GB index for package discovery?

STATUS: MEASURED (2026-11-02), offline simulation only. Branch `exp180-detector-sizing` off main `5a48564bd`.
**Nothing under `crates/` changed**; no real index was built; no gate stage covers these files (Python scripts and a
stand-alone measurement crate under `experiments/`), so `npm run check` was not run. Every number is a simulation
over the full index `cb64c1f5` (format /4, 9,896 pinned packages = 12,023 owners in `versions.json`, 65,839
versions, Bun slices only) and three scored labels on it. The candidate prints come from the scorer's pickled copy
of that index's unit rows (`pickle_pass.py`); the planned direct read of the index (`index_pass.py`) waited seven hours
on `/work/heavy.lock` and was cancelled. The one thing it would add, the lookup's content floor on prints that never
occur in a judged bundle, moves the recommended rule by at most 0.13 points (below).

## Totals first

**Yes, build it, but not the way the brief sketched it.** Keep about 20 prints per package, chosen so that they
cover every indexed version from as many source files as possible, and allow prints that up to three packages share.
That detector is **1.1 MB** and keeps **98-99%** of what today's 43 GB index names. The brief's design, each
package's K "most distinctive" prints (unique, in most versions, large), keeps only 53-88% at twice the size. The
version step needs its own second stage: fetching the newest versions is wrong.

| set (truth)                                                | today (full index, 43 GB) | **recommended `fcover3p`, K=20** (76,979 prints, 1.1 MB) | `fcover3p`, K=5 (37,670, 0.6 MB) | "most distinctive", K=20 (`coverage`, 167,184, 2.1 MB) | text discovery (banners + package.json) |
| ---------------------------------------------------------- | ------------------------: | -------------------------------------------------------: | -------------------------------: | -----------------------------------------------------: | --------------------------------------: |
| bun-dev (90 bundles): recall                               |                    82.16% |          **80.56%** (keeps 98.0% of today's named bytes) |                   77.75% (94.6%) |                                         69.05% (84.0%) |                            5.56% (6.8%) |
| bun-held (30 bundles, private): recall                     |                    82.24% |                                       **81.51%** (99.1%) |                   79.18% (96.3%) |                                         72.31% (87.9%) |                            1.22% (1.5%) |
| cc 2.1.216 (labelled sample): recall                       |                    81.07% |                                       **80.10%** (98.8%) |                   80.01% (98.7%) |                                         43.04% (53.1%) |                              0.00% (0%) |
| bun-dev packages flagged per bundle (the truth holds 78.4) |                all 12,023 |                                70.3, of which 4.4 absent |        65.3, of which 3.3 absent |                              50.9, of which 0.7 absent |                                     1.0 |
| bun-dev wrong-family bytes kept                            |                   384,084 |                                                  264,710 |                          261,656 |                                                273,535 |                                  11,219 |

- **Detection.** The recommended detector flags the packages holding 94.9% of bun-dev's library bytes (91.1%
  bun-held, 99.0% cc). Of the bytes in packages the index holds at all, it flags 98.2% (97.2%, 100%). By count it
  flags 82.6% of all packages and 92.9% of those over 1 KB. Most of what it misses is packages the index does not
  hold, which no detector built from this index can flag: the kryptokrona wallet stack, @liquify/specs and tslib.
  The other big miss is zod 4.6 (2.16 MB in 4 bundles). The index holds its prints, but the picked prints sit in
  files those bundles do not include.
- **False flags.** These are packages flagged with no unit of their family in the bundle: 4.4 per bundle on bun-dev
  and 5.9 on bun-held. Read by eye (`eyeball.py`), almost all are forks or near-copies of a package that IS present:
  `obug` (a fork of debug) in 40 bundles, `source-map-js` for source-map, `pizzip` for jszip, `is-arguments`
  sharing object-keys' code. `proxy-from-env` and `unenv` were not traced further. One is a real generic hit:
  `@cucumber/gherkin-utils`, flagged on a TypeScript `__createBinding` helper. A false flag costs a fetch, not
  precision. Today's full index already holds every one of these packages, so a targeted index restricted to
  flagged packages never adds a competitor. Wrong-family bytes FALL, from 384,084 to 264,710, because some
  wrongly claimed families are never flagged. Another 10 flags (of 6,324) name the app's own package, an installed
  CLI shipping itself; they are counted apart.
- **Text discovery** (`--library-index-from-bundle`'s seeds) names 1.0 package per bun-dev bundle and 0.7 per bun-held
  bundle. On Claude Code it names none usable: its two banners spell `azure/msal-…` without the `@`. It keeps 6.8% /
  1.5% / 0% of today's named bytes. On top of the detector it adds +0.00 / +0.01 / +0.00 points.
- **Versions.** Fetching each detected package's NEWEST versions loses a lot. Of today's claims backed by a matching
  print, the latest 1 / 5 / 20 indexed versions keep 82 / 91 / 97% on bun-dev, 70 / 80 / 91% on bun-held and 58 / 59
  / 80% on cc. The TRUE bundled version alone keeps 99.5%, 99.8% and 100%. So the targeted fetch must find the
  version, not guess it from the date. An ideal second-stage vote keeps 99.5% / 99.0% / 98.2% with one version: each
  claimed unit votes for the versions that hold its print, and the top version is kept. The detector's own hits
  give a weaker vote. With one version kept they reach 89.7% / 93.0% / 71.4% of today's named bytes at K=20, and
  95.6% / 95.2% / 85.8% at K=100.
- **Fetch size.** The flagged packages hold 1,308 indexed versions per bun-dev bundle if every version is kept
  (2.0% of the index's 65,839). With 1-3 versions per package it is 70-210.

**Recommendation.** Build a detector with the `fcover3p` rule at K=20: about 1.1 MB for 11,423 owners, roughly 100
bytes per package, and one hit flags a package. Add a version step that votes from each detected package's full
print list (a per-package file fetched after detection) and keeps 1-3 versions. Do not use any of these as the
discovery signal: the "most distinctive prints" rule, a size rule, the newest-N versions, or text discovery. K=5
(0.6 MB) is the smallest setting that loses under 6% of today's named bytes on every set. The rule saturates at
K=100 (1.9 MB), keeping 99.5% / 99.5% / 98.8%.

## Gate and results

| check                                 | result                                                                                                                                                                                       |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `npm run check`                       | not run: no file under `test/`, `scripts/` or `crates/` changed (the commit hook ran prettier and the concurrency census on the staged files)                                                |
| counters fire (`simulate.py --plant`) | pass: a planted absent flag counts 1 false, a planted present flag 1 true; flagging every owner reproduces today's named bytes to the byte (176,073,145 on bun-dev, the scorer's own figure) |
| candidate table reproducible          | pass: `pickle_pass.py` re-run is byte-identical to the table the simulation read; the final simulation JSON is identical across two runs                                                     |
| content-floor sensitivity             | `fcover3p` K=5/20/100 kept share moves −0.13 / −0.01 / −0.01 (bun-dev), +0.12 / −0.01 / −0.01 (bun-held), 0 / +0.11 / 0 (cc) points when the floor is applied to every print that can hit    |

| set         | today recall | detector recall (`fcover3p` K=20) | band                                                                                                                                                              |
| ----------- | -----------: | --------------------------------: | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| bun-dev     |       82.16% |                            80.56% | none measured: one deterministic simulation over fixed reports (no LLM, no re-run variance); the open uncertainty is the simulation's fidelity (below), not noise |
| bun-held    |       82.24% |                            81.51% | same                                                                                                                                                              |
| cc (sample) |       81.07% |                            80.10% | the 620-unit sample's own stratified-estimate error; 28 library units carry the cc miss                                                                           |

## Design of the measurement

1. **Candidates** (`pickle_pass.py`; `index_pass.py` is the same in a direct read of the index). Both slices are
   merged by print. Every print held by at most 3 owners becomes one candidate row per owner, with:

   - nodes and content;
   - the owner's indexed versions that hold it, as a bitmask over `versions.json` order;
   - how many of the owner's 10 newest versions hold it;
   - whether both slices have it;
   - its source file and all its owners.

   No rule picks a print below the lookup's own floors (30 nodes, 3 content tokens: `mmlib.RULES`). A print the
   matcher would not read is no evidence. Content is filled from the bundle reports for the 248,275 candidate rows
   whose print occurs in any judged bundle.

2. **Rules** (`simulate.py`). Each rule orders an owner's prints and is cut at K = 1, 3, 5, 10, 20, 50, 100 (and
   "all"):

   - `coverage` (the brief's rule): unique prints, the most of the owner's versions first, then both slices, then
     size.
   - `size` (the brief's other rule): unique prints in at least half the versions, largest first.
   - `spread`: `coverage`, but one print per source file before any file gives a second.
   - `recent`: unique prints in the most of the owner's 10 NEWEST versions first.
   - `cover`: greedy VERSION COVER over unique prints. Each pick covers the most versions among those covered the
     fewest times so far, so the first picks cover every version once and the next ones twice.
   - `fcover`: `cover`, but a version's count is the number of DISTINCT FILES among the picks that hold it.
   - `cover3`, `cover3p` and `fcover3p`: the cover rules over prints held by up to 3 owners, unique prints preferred
     on ties. `cover3` flags every holder of a hit print. The `p` rules flag only the owner the print was picked
     for. A carrier's copy then flags the original, whose rows claim it as its origin.

   **Why the rules changed, and in what order.** Each change came from a miss read on bun-dev and was then checked
   on the whole population (rule 2).

   - `coverage` missed typescript (7.25 MB in two bundles). Typescript has 80 indexed versions from 0.8.3, and its
     widest prints are in its OLD code.
   - `recent` (newest versions first) then missed the old bundles instead: gistpad bundles typescript 3.9.10.
   - `cover` spread the picks across versions, but still missed typescript 5.9. Every typescript 5.9 print is also
     held by `@ts-morph/common` and `monaco-editor`, which carry copies of it.
   - `cover3` caught typescript 5.9 but flagged the carriers too: 29 false flags per bundle.
   - `cover3p` flags only the picked owner: 7.4 false flags per bundle.
   - `fcover3p` stopped picking three prints from zod's `dist/esm/v3/types.js`. That file is the v3 compatibility
     layer, present in 3.x and 4.x alike and absent from bundles that use the v4 API.

3. **Detection.** A bundle flags an owner when m (1 or 2) of that owner's detector prints are among the bundle's
   prints. The bundle's prints are every unit row's whole and function hashes from the label's `--unit-prints`
   report, exactly what the binary computes for its own lookup. m=2 collapses the file-diverse rules (72% kept at
   K=20), so m=1 is the setting.
4. **Judging.** bun-dev and bun-held use scorer 14's truth (`extract.py` reuses `score.label_units` with every
   frozen layer). cc uses its labelled sample with span containment, as `score_sample` does. A truly present package
   counts as DETECTED when any member of its family is flagged; this is weighted by bytes and by count. A flag is
   FALSE when no unit of its family is in the bundle (as truth label, origin or via) and it is not the app's own
   package.
5. **End-to-end ceiling.** Today's claims are restricted to those whose package is flagged. Every table uses STRICT
   (the exact owner must be flagged); FAMILY (any member flagged) is in the JSON. The result is a share of library
   bytes (the scorer's recall) and of today's named bytes.
6. **Versions** (`versions.py`). For every named unit, the claimed owner's rows that hold any of its prints in the
   bundle's slice (the scorer's pickled unit table) give the versions that support the claim. A version picker keeps
   the claim when it keeps one of those versions.

**Labels.** bun-dev uses `exp167-names-cb64`, the names-mode label that exp167 measured at 82.16%; the scorer's
named bytes reproduce here to the byte. bun-held uses `exp177-base-bun-held` and cc uses `exp177-base-cc`, both in
report mode with main's binary `5a48564bd`.

**Text discovery.** `seeds/` is a stand-alone crate that replicates `bundle_revealed_seeds` line for line over
`humanify-core`'s public scanners. The original, in `crates/humanify-cli/src/runtime_index.rs`, is private to its
crate. The replica reproduces the design doc's Claude Code observation exactly: two `azure/msal-…` names, without the
`@`.

### How faithful the end-to-end simulation is

It keeps today's claims whose package survives and drops the rest. What it cannot see:

- **Claims a smaller index would ADD.** With fewer owners, three kinds of refusal can turn into claims, right or
  wrong: a print refused today for having more than 3 owners, a tie lost to a family that is absent, and a
  specificity refusal. exp167 sized these guards at 2.4 MB (multi-package), 1.1 MB (tie fenced) and 0.7 MB (tie lost)
  of bun-dev's missed bytes. The simulation counts none of it, on either side of precision.
- **Grown claims.** These come from propagation (DOWN, MODULE, glue) and have no print of their own. They are 4.5%
  of bun-dev's named bytes and 42% of bun-held's (report mode). The simulation keeps them whenever their owner is
  flagged; in a real run they follow their seeds.
- **The isolation veto** reads neighbours of the same family. Dropping an unflagged family's seeds only removes
  support from that family, which is dropped anyway, so the strict column is faithful here.
- **Factory bodies the run kept as one unit** have no print, so today's matcher and the detector both miss them.
- **The version pick** assumes a claim survives whenever one supporting version is kept. None of today's lookup
  rules read the version.

## Tables

<!-- tables -->

Generated by `report.py` from `final/sim.json` and `versions.json`. Columns: detector entries and binary size (8-byte print + 4 bytes per owner + the owner-name table); detection by library bytes (family member flagged / the exact package flagged) and by count (all packages / packages over 1 KB); packages flagged per bundle and false flags per bundle; the share of today's named bytes the restricted claims keep; and the end-to-end recall (today's recall times that share, both over library bytes). m = 1 throughout except the m=2 table. `size` and the other rules are cut at the same K; `all` = every eligible print.

index owners 12,023; owners with an eligible print 11,423 (unique-print 11,183).

**bun-dev (90 bundles)** — today's claims name 82.16% of library bytes; m = 1 hit flags a package.

| rule                                    |   K |   entries |    size | detect bytes % (family / exact pkg) | detect count % (all / ≥1 KB) | flagged per bundle | false flags per bundle | kept of today's named % | end-to-end recall % |
| --------------------------------------- | --: | --------: | ------: | ----------------------------------: | ---------------------------: | -----------------: | ---------------------: | ----------------------: | ------------------: |
| coverage                                |   1 |    11,183 |  317 KB |                         66.9 / 63.1 |                  44.7 / 52.0 |               31.3 |                    0.2 |                    66.4 |               54.57 |
| coverage                                |   3 |    32,272 |  564 KB |                         72.9 / 70.0 |                  53.5 / 59.9 |               39.1 |                    0.4 |                    74.5 |               61.20 |
| coverage                                |   5 |    51,730 |  792 KB |                         74.6 / 71.5 |                  58.2 / 65.0 |               43.2 |                    0.4 |                    76.2 |               62.63 |
| coverage                                |  10 |    95,046 |  1.3 MB |                         78.9 / 76.2 |                  62.8 / 70.3 |               47.4 |                    0.6 |                    81.5 |               66.96 |
| coverage                                |  20 |   167,184 |  2.1 MB |                         80.9 / 78.9 |                  67.2 / 75.9 |               50.9 |                    0.7 |                    84.0 |               69.05 |
| coverage                                |  50 |   328,591 |  3.9 MB |                         83.3 / 81.2 |                  71.8 / 82.2 |               54.9 |                    0.9 |                    86.2 |               70.82 |
| coverage                                | 100 |   511,137 |  6.0 MB |                         87.8 / 85.3 |                  73.4 / 84.5 |               56.6 |                    1.0 |                    90.0 |               73.92 |
| coverage                                | all | 3,139,574 | 36.1 MB |                         95.4 / 94.5 |                  75.0 / 87.1 |               59.8 |                    2.4 |                    98.3 |               80.78 |
| size                                    |   1 |    11,183 |  317 KB |                         45.1 / 36.2 |                  32.8 / 35.3 |               20.7 |                    0.2 |                    38.6 |               31.75 |
| size                                    |   3 |    32,272 |  564 KB |                         57.4 / 49.1 |                  44.0 / 46.1 |               29.9 |                    0.3 |                    53.6 |               44.02 |
| size                                    |   5 |    51,730 |  792 KB |                         61.1 / 54.8 |                  48.0 / 50.2 |               33.8 |                    0.3 |                    58.9 |               48.40 |
| size                                    |  10 |    95,046 |  1.3 MB |                         65.1 / 60.2 |                  54.8 / 58.2 |               39.8 |                    0.4 |                    63.5 |               52.14 |
| size                                    |  20 |   167,184 |  2.1 MB |                         68.7 / 65.2 |                  60.0 / 65.1 |               44.6 |                    0.6 |                    68.5 |               56.25 |
| size                                    |  50 |   328,591 |  3.9 MB |                         78.8 / 75.4 |                  66.5 / 74.5 |               50.5 |                    0.8 |                    78.8 |               64.71 |
| size                                    | 100 |   511,137 |  6.0 MB |                         81.5 / 78.5 |                  70.7 / 80.3 |               54.2 |                    0.8 |                    82.1 |               67.45 |
| spread                                  |   1 |    11,183 |  317 KB |                         66.9 / 63.1 |                  44.7 / 52.0 |               31.3 |                    0.2 |                    66.4 |               54.57 |
| spread                                  |   3 |    32,272 |  564 KB |                         74.5 / 71.5 |                  56.1 / 63.9 |               41.3 |                    0.3 |                    75.8 |               62.29 |
| spread                                  |   5 |    51,730 |  792 KB |                         76.9 / 74.3 |                  60.3 / 68.4 |               44.8 |                    0.4 |                    78.9 |               64.82 |
| spread                                  |  10 |    95,046 |  1.3 MB |                         78.6 / 76.5 |                  64.4 / 72.7 |               48.6 |                    0.5 |                    81.4 |               66.85 |
| spread                                  |  20 |   167,184 |  2.1 MB |                         84.9 / 83.0 |                  69.5 / 79.0 |               52.8 |                    0.7 |                    88.4 |               72.65 |
| spread                                  |  50 |   328,591 |  3.9 MB |                         86.1 / 85.0 |                  72.5 / 83.2 |               56.0 |                    0.9 |                    90.6 |               74.42 |
| spread                                  | 100 |   511,137 |  6.0 MB |                         89.1 / 87.1 |                  74.0 / 85.5 |               57.0 |                    1.0 |                    92.0 |               75.58 |
| recent                                  |   1 |    11,183 |  317 KB |                         70.8 / 68.0 |                  47.1 / 55.8 |               33.6 |                    0.2 |                    71.8 |               59.00 |
| recent                                  |   3 |    32,272 |  564 KB |                         76.2 / 73.5 |                  55.2 / 62.6 |               40.6 |                    0.4 |                    77.6 |               63.78 |
| recent                                  |   5 |    51,730 |  792 KB |                         78.0 / 75.1 |                  59.9 / 67.8 |               44.3 |                    0.4 |                    79.3 |               65.18 |
| recent                                  |  10 |    95,046 |  1.3 MB |                         78.7 / 76.2 |                  63.7 / 71.6 |               47.9 |                    0.7 |                    80.3 |               66.01 |
| recent                                  |  20 |   167,184 |  2.1 MB |                         80.7 / 79.6 |                  67.4 / 76.6 |               51.6 |                    0.8 |                    83.7 |               68.79 |
| recent                                  |  50 |   328,591 |  3.9 MB |                         81.8 / 80.6 |                  70.9 / 80.9 |               54.6 |                    0.9 |                    84.7 |               69.58 |
| recent                                  | 100 |   511,137 |  6.0 MB |                         85.3 / 83.3 |                  73.0 / 84.0 |               56.2 |                    1.0 |                    87.6 |               71.94 |
| cover                                   |   1 |    11,183 |  317 KB |                         66.9 / 63.1 |                  44.8 / 51.9 |               31.4 |                    0.2 |                    66.5 |               54.61 |
| cover                                   |   3 |    32,272 |  564 KB |                         81.2 / 78.3 |                  62.7 / 71.1 |               46.4 |                    0.4 |                    82.5 |               67.76 |
| cover                                   |   5 |    51,730 |  792 KB |                         86.5 / 83.6 |                  68.0 / 77.9 |               51.0 |                    0.4 |                    86.5 |               71.03 |
| cover                                   |  10 |    95,046 |  1.3 MB |                         88.2 / 86.0 |                  72.2 / 83.0 |               54.9 |                    0.6 |                    88.7 |               72.86 |
| cover                                   |  20 |   167,184 |  2.1 MB |                         89.4 / 87.8 |                  73.4 / 84.6 |               56.5 |                    0.8 |                    90.4 |               74.29 |
| cover                                   |  50 |   328,591 |  3.9 MB |                         91.6 / 90.7 |                  74.6 / 86.5 |               57.7 |                    0.9 |                    93.7 |               77.01 |
| cover                                   | 100 |   511,137 |  6.0 MB |                         91.7 / 90.7 |                  74.7 / 86.6 |               58.0 |                    1.0 |                    93.8 |               77.04 |
| fcover                                  |   1 |    11,183 |  317 KB |                         66.9 / 63.1 |                  44.8 / 51.9 |               31.4 |                    0.2 |                    66.5 |               54.61 |
| fcover                                  |   3 |    26,897 |  501 KB |                         82.0 / 78.9 |                  63.3 / 72.1 |               46.7 |                    0.3 |                    82.9 |               68.12 |
| fcover                                  |   5 |    37,544 |  625 KB |                         86.6 / 84.4 |                  68.3 / 78.7 |               51.2 |                    0.4 |                    87.0 |               71.47 |
| fcover                                  |  10 |    54,961 |  830 KB |                         88.6 / 86.7 |                  71.5 / 82.6 |               54.2 |                    0.4 |                    89.4 |               73.44 |
| fcover                                  |  20 |    76,873 |  1.1 MB |                         90.9 / 89.1 |                  72.5 / 84.0 |               55.2 |                    0.5 |                    92.1 |               75.66 |
| fcover                                  |  50 |   113,946 |  1.5 MB |                         91.0 / 90.0 |                  72.5 / 84.1 |               55.4 |                    0.5 |                    93.2 |               76.54 |
| fcover                                  | 100 |   147,766 |  1.9 MB |                         91.3 / 90.3 |                  72.6 / 84.2 |               55.5 |                    0.5 |                    93.5 |               76.84 |
| cover3                                  |   1 |    11,375 |  331 KB |                         81.1 / 77.8 |                  57.7 / 67.0 |               54.8 |                   11.4 |                    82.7 |               67.91 |
| cover3                                  |   3 |    32,948 |  595 KB |                         88.7 / 86.9 |                  74.8 / 83.4 |               75.1 |                   15.4 |                    91.9 |               75.55 |
| cover3                                  |   5 |    52,973 |  841 KB |                         92.8 / 91.1 |                  79.6 / 88.7 |               83.2 |                   19.0 |                    94.8 |               77.88 |
| cover3                                  |  10 |    97,853 |  1.4 MB |                         94.7 / 93.6 |                  83.4 / 92.5 |               92.3 |                   23.7 |                    97.5 |               80.07 |
| cover3                                  |  20 |   173,115 |  2.3 MB |                         95.0 / 94.5 |                  84.7 / 94.1 |               98.2 |                   27.8 |                    98.1 |               80.57 |
| cover3                                  |  50 |   342,044 |  4.3 MB |                         95.3 / 94.7 |                  85.3 / 94.9 |              103.5 |                   31.9 |                    98.3 |               80.75 |
| cover3                                  | 100 |   535,717 |  6.6 MB |                         95.3 / 94.8 |                  85.4 / 95.1 |              107.8 |                   35.7 |                    98.3 |               80.77 |
| cover3                                  | all | 3,361,167 | 39.6 MB |                         96.4 / 96.0 |                  85.8 / 95.6 |              115.7 |                   42.5 |                    99.9 |               82.04 |
| cover3p                                 |   1 |    11,375 |  323 KB |                         80.5 / 76.9 |                  56.5 / 65.6 |               42.5 |                    1.3 |                    81.9 |               67.25 |
| cover3p                                 |   3 |    32,948 |  577 KB |                         88.4 / 86.1 |                  73.8 / 82.4 |               59.2 |                    2.3 |                    91.2 |               74.95 |
| cover3p                                 |   5 |    52,973 |  812 KB |                         92.7 / 90.8 |                  79.2 / 88.2 |               65.3 |                    3.5 |                    94.3 |               77.52 |
| cover3p                                 |  10 |    97,853 |  1.3 MB |                         94.4 / 92.5 |                  83.0 / 92.0 |               71.4 |                    5.1 |                    96.0 |               78.91 |
| cover3p                                 |  20 |   173,115 |  2.2 MB |                         94.7 / 93.5 |                  84.4 / 93.6 |               75.3 |                    6.6 |                    96.8 |               79.54 |
| cover3p                                 |  50 |   342,044 |  4.1 MB |                         95.0 / 94.5 |                  85.2 / 94.8 |               79.7 |                    9.5 |                    97.9 |               80.46 |
| cover3p                                 | 100 |   535,717 |  6.4 MB |                         95.0 / 94.5 |                  85.3 / 94.9 |               84.5 |                   13.6 |                    98.1 |               80.57 |
| cover3p                                 | all | 3,361,167 | 39.6 MB |                         96.4 / 96.0 |                  85.8 / 95.6 |              115.7 |                   42.5 |                    99.9 |               82.04 |
| fcover3p                                |   1 |    11,375 |  323 KB |                         80.5 / 76.9 |                  56.5 / 65.6 |               42.5 |                    1.3 |                    81.9 |               67.25 |
| fcover3p                                |   3 |    27,136 |  508 KB |                         89.3 / 86.7 |                  74.3 / 83.6 |               59.8 |                    2.4 |                    91.8 |               75.40 |
| fcover3p                                |   5 |    37,670 |  632 KB |                         93.2 / 91.4 |                  79.7 / 89.7 |               65.3 |                    3.3 |                    94.6 |               77.75 |
| fcover3p                                |  10 |    54,969 |  836 KB |                         94.4 / 92.9 |                  82.2 / 92.4 |               68.9 |                    4.0 |                    96.4 |               79.20 |
| fcover3p                                |  20 |    76,979 |  1.1 MB |                         94.9 / 94.4 |                  82.6 / 92.9 |               70.3 |                    4.4 |                    98.0 |               80.56 |
| fcover3p                                |  50 |   114,796 |  1.5 MB |                         96.0 / 95.5 |                  82.9 / 93.3 |               71.4 |                    5.0 |                    99.5 |               81.77 |
| fcover3p                                | 100 |   149,346 |  1.9 MB |                         96.0 / 95.5 |                  82.9 / 93.3 |               72.1 |                    5.5 |                    99.5 |               81.77 |
| text discovery (banners + package.json) |     |           |         |                           8.4 / 6.1 |                    1.9 / 3.0 |                1.0 |                    0.0 |                     6.8 |                5.56 |
| full index (today)                      |     |           |   43 GB |                                   — |                            — |                all |                      — |                   100.0 |               82.16 |

**bun-held (30, private: totals only)** — today's claims name 82.24% of library bytes; m = 1 hit flags a package.

| rule                                    |   K |   entries |    size | detect bytes % (family / exact pkg) | detect count % (all / ≥1 KB) | flagged per bundle | false flags per bundle | kept of today's named % | end-to-end recall % |
| --------------------------------------- | --: | --------: | ------: | ----------------------------------: | ---------------------------: | -----------------: | ---------------------: | ----------------------: | ------------------: |
| coverage                                |   1 |    11,183 |  317 KB |                         71.1 / 63.3 |                  41.8 / 49.0 |               28.8 |                    0.1 |                    75.5 |               62.13 |
| coverage                                |   3 |    32,272 |  564 KB |                         75.5 / 68.6 |                  50.0 / 56.3 |               35.9 |                    0.2 |                    81.5 |               67.03 |
| coverage                                |   5 |    51,730 |  792 KB |                         78.2 / 71.7 |                  54.9 / 62.1 |               39.8 |                    0.2 |                    85.0 |               69.90 |
| coverage                                |  10 |    95,046 |  1.3 MB |                         80.4 / 74.0 |                  59.5 / 66.9 |               43.7 |                    0.3 |                    86.2 |               70.90 |
| coverage                                |  20 |   167,184 |  2.1 MB |                         82.7 / 75.6 |                  63.9 / 72.2 |               47.2 |                    0.6 |                    87.9 |               72.31 |
| coverage                                |  50 |   328,591 |  3.9 MB |                         87.8 / 80.8 |                  69.7 / 80.1 |               51.8 |                    0.7 |                    92.6 |               76.14 |
| coverage                                | 100 |   511,137 |  6.0 MB |                         89.6 / 82.9 |                  70.8 / 81.7 |               53.4 |                    1.1 |                    94.9 |               78.07 |
| coverage                                | all | 3,139,574 | 36.1 MB |                         91.7 / 85.9 |                  71.8 / 83.4 |               56.1 |                    2.6 |                    98.2 |               80.78 |
| size                                    |   1 |    11,183 |  317 KB |                         55.7 / 47.0 |                  29.5 / 30.9 |               19.4 |                    0.0 |                    19.0 |               15.63 |
| size                                    |   3 |    32,272 |  564 KB |                         62.3 / 54.2 |                  40.2 / 40.5 |               27.3 |                    0.1 |                    27.0 |               22.23 |
| size                                    |   5 |    51,730 |  792 KB |                         63.2 / 55.5 |                  42.9 / 43.0 |               29.7 |                    0.2 |                    28.5 |               23.45 |
| size                                    |  10 |    95,046 |  1.3 MB |                         66.0 / 57.8 |                  48.9 / 50.9 |               34.6 |                    0.3 |                    31.0 |               25.50 |
| size                                    |  20 |   167,184 |  2.1 MB |                         70.4 / 62.2 |                  56.1 / 59.9 |               40.3 |                    0.3 |                    34.6 |               28.45 |
| size                                    |  50 |   328,591 |  3.9 MB |                         82.5 / 74.1 |                  62.5 / 69.3 |               46.1 |                    0.7 |                    85.4 |               70.24 |
| size                                    | 100 |   511,137 |  6.0 MB |                         85.1 / 77.0 |                  66.2 / 74.7 |               49.2 |                    0.8 |                    88.5 |               72.81 |
| cover                                   |   1 |    11,183 |  317 KB |                         71.1 / 63.3 |                  41.7 / 48.9 |               28.8 |                    0.1 |                    75.5 |               62.13 |
| cover                                   |   3 |    32,272 |  564 KB |                         81.7 / 74.7 |                  59.5 / 68.2 |               43.0 |                    0.3 |                    87.1 |               71.67 |
| cover                                   |   5 |    51,730 |  792 KB |                         88.2 / 81.8 |                  66.1 / 76.5 |               48.2 |                    0.4 |                    94.9 |               78.09 |
| cover                                   |  10 |    95,046 |  1.3 MB |                         89.4 / 83.5 |                  69.6 / 80.3 |               51.7 |                    0.5 |                    96.7 |               79.54 |
| cover                                   |  20 |   167,184 |  2.1 MB |                         91.5 / 85.1 |                  70.9 / 82.0 |               53.2 |                    0.7 |                    97.4 |               80.09 |
| cover                                   |  50 |   328,591 |  3.9 MB |                         91.6 / 85.8 |                  71.5 / 82.9 |               53.9 |                    0.8 |                    98.2 |               80.73 |
| cover                                   | 100 |   511,137 |  6.0 MB |                         91.7 / 85.8 |                  71.6 / 83.0 |               54.3 |                    1.1 |                    98.2 |               80.75 |
| fcover3p                                |   1 |    11,375 |  323 KB |                         76.2 / 68.8 |                  51.9 / 60.8 |               37.7 |                    1.6 |                    81.7 |               67.15 |
| fcover3p                                |   3 |    27,136 |  508 KB |                         85.8 / 79.1 |                  70.7 / 80.1 |               56.2 |                    3.3 |                    91.7 |               75.40 |
| fcover3p                                |   5 |    37,670 |  632 KB |                         89.2 / 83.3 |                  76.4 / 86.6 |               61.9 |                    4.3 |                    96.3 |               79.17 |
| fcover3p                                |  10 |    54,969 |  836 KB |                         90.9 / 85.0 |                  78.9 / 89.3 |               65.4 |                    5.4 |                    98.2 |               80.72 |
| fcover3p                                |  20 |    76,979 |  1.1 MB |                         91.1 / 85.9 |                  79.5 / 90.0 |               66.7 |                    5.9 |                    99.1 |               81.51 |
| fcover3p                                |  50 |   114,796 |  1.5 MB |                         92.2 / 87.0 |                  79.6 / 90.2 |               67.3 |                    6.3 |                    99.5 |               81.84 |
| fcover3p                                | 100 |   149,346 |  1.9 MB |                         92.2 / 87.0 |                  79.6 / 90.2 |               68.2 |                    6.9 |                    99.5 |               81.84 |
| text discovery (banners + package.json) |     |           |         |                           1.5 / 1.4 |                    1.1 / 1.7 |                0.7 |                    0.0 |                     1.5 |                1.22 |
| full index (today)                      |     |           |   43 GB |                                   — |                            — |                all |                      — |                   100.0 |               82.24 |

**cc 2.1.216 (labelled sample)** — today's claims name 81.07% of library bytes; m = 1 hit flags a package.

| rule                                    |   K |   entries |    size | detect bytes % (family / exact pkg) | detect count % (all / ≥1 KB) | flagged per bundle | false flags per bundle | kept of today's named % | end-to-end recall % |
| --------------------------------------- | --: | --------: | ------: | ----------------------------------: | ---------------------------: | -----------------: | ---------------------: | ----------------------: | ------------------: |
| coverage                                |   1 |    11,183 |  317 KB |                         69.1 / 34.2 |                  77.3 / 78.0 |              122.0 |             not judged |                    34.4 |               27.88 |
| coverage                                |   3 |    32,272 |  564 KB |                         70.2 / 40.5 |                  79.5 / 80.5 |              153.0 |             not judged |                    40.7 |               33.00 |
| coverage                                |   5 |    51,730 |  792 KB |                         70.3 / 41.5 |                  81.8 / 82.9 |              169.0 |             not judged |                    41.9 |               33.99 |
| coverage                                |  10 |    95,046 |  1.3 MB |                         70.3 / 41.6 |                  81.8 / 82.9 |              183.0 |             not judged |                    42.1 |               34.13 |
| coverage                                |  20 |   167,184 |  2.1 MB |                         79.2 / 50.6 |                  86.4 / 87.8 |              195.0 |             not judged |                    53.1 |               43.04 |
| coverage                                |  50 |   328,591 |  3.9 MB |                         79.2 / 59.8 |                  86.4 / 87.8 |              205.0 |             not judged |                    60.7 |               49.24 |
| coverage                                | 100 |   511,137 |  6.0 MB |                         79.2 / 64.2 |                  86.4 / 87.8 |              214.0 |             not judged |                    62.9 |               50.95 |
| coverage                                | all | 3,139,574 | 36.1 MB |                         98.1 / 89.0 |                  95.5 / 95.1 |              230.0 |             not judged |                    93.3 |               75.66 |
| size                                    |   1 |    11,183 |  317 KB |                         34.6 / 15.1 |                  50.0 / 51.2 |               76.0 |             not judged |                    15.1 |               12.24 |
| size                                    |   3 |    32,272 |  564 KB |                         43.4 / 20.3 |                  54.5 / 56.1 |              117.0 |             not judged |                    21.4 |               17.37 |
| size                                    |   5 |    51,730 |  792 KB |                         48.5 / 26.7 |                  59.1 / 61.0 |              132.0 |             not judged |                    27.1 |               21.95 |
| size                                    |  10 |    95,046 |  1.3 MB |                         62.8 / 34.7 |                  75.0 / 75.6 |              152.0 |             not judged |                    35.5 |               28.75 |
| size                                    |  20 |   167,184 |  2.1 MB |                         70.2 / 40.0 |                  79.5 / 80.5 |              165.0 |             not judged |                    42.0 |               34.04 |
| size                                    |  50 |   328,591 |  3.9 MB |                         70.2 / 47.3 |                  79.5 / 80.5 |              189.0 |             not judged |                    47.6 |               38.61 |
| size                                    | 100 |   511,137 |  6.0 MB |                         79.1 / 67.0 |                  84.1 / 85.4 |              206.0 |             not judged |                    66.3 |               53.72 |
| cover                                   |   1 |    11,183 |  317 KB |                         69.1 / 34.2 |                  77.3 / 78.0 |              124.0 |             not judged |                    34.4 |               27.88 |
| cover                                   |   3 |    32,272 |  564 KB |                         70.2 / 44.9 |                  79.5 / 80.5 |              172.0 |             not judged |                    42.8 |               34.71 |
| cover                                   |   5 |    51,730 |  792 KB |                         70.9 / 46.5 |                  81.8 / 82.9 |              192.0 |             not judged |                    44.7 |               36.28 |
| cover                                   |  10 |    95,046 |  1.3 MB |                         71.9 / 47.5 |                  86.4 / 85.4 |              203.0 |             not judged |                    46.0 |               37.32 |
| cover                                   |  20 |   167,184 |  2.1 MB |                         74.3 / 50.1 |                  90.9 / 90.2 |              211.0 |             not judged |                    49.2 |               39.86 |
| cover                                   |  50 |   328,591 |  3.9 MB |                         80.9 / 62.6 |                  93.2 / 92.7 |              217.0 |             not judged |                    64.6 |               52.34 |
| cover                                   | 100 |   511,137 |  6.0 MB |                         80.9 / 62.6 |                  93.2 / 92.7 |              219.0 |             not judged |                    64.6 |               52.34 |
| fcover3p                                |   1 |    11,375 |  323 KB |                         97.1 / 75.9 |                  90.9 / 90.2 |              167.0 |             not judged |                    80.9 |               65.60 |
| fcover3p                                |   3 |    27,136 |  508 KB |                         98.2 / 93.4 |                  93.2 / 92.7 |              233.0 |             not judged |                    97.7 |               79.21 |
| fcover3p                                |   5 |    37,670 |  632 KB |                         98.9 / 94.2 |                  95.5 / 95.1 |              251.0 |             not judged |                    98.7 |               80.01 |
| fcover3p                                |  10 |    54,969 |  836 KB |                         98.9 / 94.2 |                  95.5 / 95.1 |              264.0 |             not judged |                    98.7 |               80.01 |
| fcover3p                                |  20 |    76,979 |  1.1 MB |                         99.0 / 94.3 |                  97.7 / 97.6 |              272.0 |             not judged |                    98.8 |               80.10 |
| fcover3p                                |  50 |   114,796 |  1.5 MB |                         99.0 / 94.3 |                  97.7 / 97.6 |              280.0 |             not judged |                    98.8 |               80.10 |
| fcover3p                                | 100 |   149,346 |  1.9 MB |                         99.0 / 94.3 |                  97.7 / 97.6 |              282.0 |             not judged |                    98.8 |               80.10 |
| text discovery (banners + package.json) |     |           |         |                           0.0 / 0.0 |                    0.0 / 0.0 |                0.0 |             not judged |                     0.0 |                0.00 |
| full index (today)                      |     |           |   43 GB |                                   — |                            — |                all |                      — |                   100.0 |               81.07 |

| corpus   | rule/K        | m=1 kept % / false per bundle | m=2 kept % / false per bundle |
| -------- | ------------- | ----------------------------: | ----------------------------: |
| bun-dev  | cover/K20     |                    90.4 / 0.8 |                    86.5 / 0.4 |
| bun-dev  | fcover3p/K5   |                    94.6 / 3.3 |                    63.8 / 0.5 |
| bun-dev  | fcover3p/K20  |                    98.0 / 4.4 |                    72.3 / 0.9 |
| bun-dev  | fcover3p/K100 |                    99.5 / 5.5 |                    75.6 / 1.3 |
| bun-held | cover/K20     |                    97.4 / 0.7 |                    93.6 / 0.3 |
| bun-held | fcover3p/K5   |                    96.3 / 4.3 |                    75.5 / 0.7 |
| bun-held | fcover3p/K20  |                    99.1 / 5.9 |                    85.7 / 1.4 |
| bun-held | fcover3p/K100 |                    99.5 / 6.9 |                    86.9 / 2.0 |
| cc       | cover/K20     |             49.2 / not judged |             43.7 / not judged |
| cc       | fcover3p/K5   |             98.7 / not judged |             69.1 / not judged |
| cc       | fcover3p/K20  |             98.8 / not judged |             74.9 / not judged |
| cc       | fcover3p/K100 |             98.8 / not judged |             96.0 / not judged |

| corpus   | detector      | all versions | detector's 1 |    2 |    3 |    5 |
| -------- | ------------- | -----------: | -----------: | ---: | ---: | ---: |
| bun-dev  | cover/K20     |         90.4 |         83.3 | 83.8 | 84.0 | 87.1 |
| bun-dev  | cover3p/K20   |         96.8 |         88.1 | 89.2 | 89.7 | 92.9 |
| bun-dev  | fcover3p/K5   |         94.6 |         85.1 | 86.3 | 86.7 | 90.4 |
| bun-dev  | fcover3p/K20  |         98.0 |         89.7 | 90.8 | 91.5 | 94.6 |
| bun-dev  | fcover3p/K100 |         99.5 |         95.6 | 96.6 | 97.4 | 97.9 |
| bun-held | cover/K20     |         97.4 |         92.5 | 94.2 | 94.5 | 95.3 |
| bun-held | cover3p/K20   |         98.6 |         92.6 | 94.8 | 95.4 | 96.4 |
| bun-held | fcover3p/K5   |         96.3 |         87.7 | 90.0 | 90.5 | 92.2 |
| bun-held | fcover3p/K20  |         99.1 |         93.0 | 94.9 | 95.7 | 96.6 |
| bun-held | fcover3p/K100 |         99.5 |         95.2 | 97.2 | 97.5 | 98.2 |
| cc       | cover/K20     |         49.2 |         37.3 | 37.3 | 37.3 | 38.4 |
| cc       | cover3p/K20   |         98.7 |         65.9 | 65.9 | 65.9 | 67.0 |
| cc       | fcover3p/K5   |         98.7 |         62.9 | 62.9 | 62.9 | 64.0 |
| cc       | fcover3p/K20  |         98.8 |         71.4 | 71.4 | 73.0 | 73.0 |
| cc       | fcover3p/K100 |         98.8 |         85.8 | 87.6 | 87.6 | 87.6 |

| corpus   | claimed bytes with print support |       latest 1 / 3 / 5 / 10 / 20 | ideal vote 1 / 2 / 5 | true version ±0 (nearest 1) / nearest 3 |
| -------- | -------------------------------: | -------------------------------: | -------------------: | --------------------------------------: |
| bun-dev  |                   95.5% of named | 82.2 / 86.8 / 90.8 / 93.7 / 96.8 |   99.5 / 99.6 / 99.8 |                            99.5 / 100.0 |
| bun-held |                   57.7% of named | 70.5 / 76.9 / 79.7 / 85.0 / 91.5 |   99.0 / 99.3 / 99.4 |                             99.8 / 99.9 |
| cc       |                   90.8% of named | 57.6 / 57.6 / 58.9 / 65.2 / 79.8 |  98.2 / 98.2 / 100.0 |                           100.0 / 100.0 |

- `cover/K20`: typescript 7.25 MB/2, zod 3.24 MB/6, kryptokrona-crypto 2.80 MB/2, @liquify/specs 1.58 MB/2, @altimateai/dbt-integration 0.40 MB/2, jiti 0.38 MB/2, kryptokrona-utils 0.35 MB/2, encoding-sniffer 0.32 MB/10

- `cover3/K0`: kryptokrona-crypto 2.80 MB/2, @liquify/specs 1.58 MB/2, @altimateai/dbt-integration 0.40 MB/2, kryptokrona-utils 0.35 MB/2, kryptokrona-wallet-backend-js 0.29 MB/2, @openuidev/lang-core 0.20 MB/2, tslib 0.19 MB/16, @turtlecoin/mnemonics 0.19 MB/2

- `cover3p/K20`: kryptokrona-crypto 2.80 MB/2, zod 2.31 MB/6, @liquify/specs 1.58 MB/2, @altimateai/dbt-integration 0.40 MB/2, jiti 0.38 MB/2, kryptokrona-utils 0.35 MB/2, kryptokrona-wallet-backend-js 0.29 MB/2, vsce 0.21 MB/4

- `coverage/K0`: kryptokrona-crypto 2.80 MB/2, @liquify/specs 1.58 MB/2, @altimateai/dbt-integration 0.40 MB/2, kryptokrona-utils 0.35 MB/2, encoding-sniffer 0.32 MB/10, kryptokrona-wallet-backend-js 0.29 MB/2, source-map 0.22 MB/8, @openuidev/lang-core 0.20 MB/2

- `fcover/K20`: typescript 7.25 MB/2, kryptokrona-crypto 2.80 MB/2, @liquify/specs 1.58 MB/2, katex 0.56 MB/2, @altimateai/dbt-integration 0.40 MB/2, kryptokrona-utils 0.35 MB/2, encoding-sniffer 0.32 MB/10, kryptokrona-wallet-backend-js 0.29 MB/2

- `fcover3p/K100`: kryptokrona-crypto 2.80 MB/2, @liquify/specs 1.58 MB/2, @altimateai/dbt-integration 0.40 MB/2, kryptokrona-utils 0.35 MB/2, kryptokrona-wallet-backend-js 0.29 MB/2, @openuidev/lang-core 0.20 MB/2, tslib 0.19 MB/16, @turtlecoin/mnemonics 0.19 MB/2

- `fcover3p/K20`: kryptokrona-crypto 2.80 MB/2, zod 2.16 MB/4, @liquify/specs 1.58 MB/2, @altimateai/dbt-integration 0.40 MB/2, kryptokrona-utils 0.35 MB/2, kryptokrona-wallet-backend-js 0.29 MB/2, @openuidev/lang-core 0.20 MB/2, tslib 0.19 MB/16

- `fcover3p/K5`: kryptokrona-crypto 2.80 MB/2, zod 2.38 MB/7, @liquify/specs 1.58 MB/2, node-fetch-native 0.90 MB/2, @altimateai/dbt-integration 0.40 MB/2, kryptokrona-utils 0.35 MB/2, eslint-plugin-jsdoc 0.33 MB/2, cheerio 0.32 MB/10

The m=1 vs m=2 table, then the version picks: "detector's N" keeps per flagged package the N versions its own detector hits point at; the last table is the claims' own support (latest-N = newest indexed versions; ideal vote = each claimed unit votes for the versions holding its print; nearest = an oracle around the TRUE version). The list is bun-dev's largest packages each detector missed (MB / bundles).

<!-- /tables -->

## What could NOT be measured

- **The terser/esbuild re-minified sets.** Their corpora (`/work/exp126-data`, `/work/exp146-data`, remin2-4) are no
  longer on disk, and `cb64c1f5` holds Bun slices only. So it is open how a Bun-built detector does on esbuild or
  terser bundles, where exp142 found that prints mostly do not survive terser. The detector reads the same prints as
  today's matcher, so expect it to track today's recall there (17-52%).
- **The direct read of the index.** `index_pass.py` was queued on `/work/heavy.lock` from 2026-11-01 20:56 to 2026-11-02 03:55 UTC behind
  evals, gates and an index build, then cancelled. The pickled table it would replace holds the same rows except the
  content column. Content was filled for every print that occurs in a judged bundle. What remains unmeasured is a
  low-content print that never occurs in any judged bundle still taking a pick slot. Also missing: `owners.json`,
  so the size of the targeted index is reported in versions, not rows.
- **Claude Code's false flags.** Its truth is a 620-unit sample naming 44 packages, so a flagged package outside the
  sample may well be present. The column reads "not judged".
- **Packages outside the index.** The detector's universe is the index's 12,023 owners. To REPLACE the hand-picked
  pin list as the source of discovery, the detector must be built over a wider package set. That still means
  bundling every candidate package at its versions once, offline: the build cost does not go away, only the
  shipping and run-time cost does. At about 100 bytes per package (K=20), 100,000 packages come to about 10 MB.
- **A real targeted fetch.** Building about 70 flagged packages at 1-3 versions each per bundle is the job of the
  run-time `--library-index-from-bundle` path. Its wall time on a user's machine was not measured.
- **Rules tuned on bun-dev.** bun-held is private, so this README gives totals only for it. The rules were developed
  on bun-dev's misses, so bun-dev reads optimistic (exp141's lesson). bun-held was never used to choose a rule, and
  it agrees: 99.1% kept.
- **Collisions with app code** are judged only on these 121 bundles.

## Proposed CLAUDE.md text

None needed.

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python; E=experiments/180-detector-sizing; D=/work/exp180-data
R=/work/exp150-data; C=/work/cc-library-truth/frozen/results
$PY -I $E/pickle_pass.py $D/final/candidates.raw.tsv
$PY -I $E/pickle_pass.py --fill-content $D/final/candidates.raw.tsv $D/final/candidates.tsv \
    $R/bun-dev/frozen/results/exp167-names-cb64/reports $R/bun-held/frozen/results/exp177-base-bun-held/reports \
    $C/exp177-base-cc/reports
#   (or, under flock /work/heavy.lock: $PY -I $E/index_pass.py <index dir> $D/final — the direct read)
$PY -I $E/extract.py bun-dev exp167-names-cb64 $D/x-bun-dev.json.gz   # + bun-held exp177-base-bun-held, cc exp177-base-cc
CARGO_TARGET_DIR=$D/seeds-target cargo build --release --manifest-path $E/seeds/Cargo.toml
awk '{print $2}' $D/bundles-bun-dev.txt | xargs $D/seeds-target/release/exp180-seeds > $D/seeds-bun-dev.jsonl
$PY -I $E/versions.py $D $D/versions.json
$PY -I $E/simulate.py $D/final $D $D/final/sim.json --plant
python3 $E/report.py $D/final/sim.json $D/versions.json
$PY -I $E/eyeball.py $D/final $D fcover3p 20 8          # the false flags, with the code they hit
```

`bundles-<corpus>.txt` lists each entry's frozen bundle path (`lib096.corpus_path(entry, "bundle")`). The data stays
under `/work/exp180-data`: extracts with unit prints, seeds, candidate tables and simulation JSON. Nothing from the
CC truth is committed.

## Files

- `pickle_pass.py`: the candidate prints (≤ 3 owners) with version masks, from the scorer's pickled unit tables, plus
  `--fill-content`.
- `index_pass.py`: the same candidates from a direct read of `units.<slice>.jsonl` (not run; see above).
- `extract.py`: per label, the truth-labelled units, today's claims and every bundle print.
- `seeds/`: the text discovery's seeds, replicated. It is its own cargo workspace, never a pipeline member.
- `simulate.py`: the rules and detectors; detection, false flags, the end-to-end ceiling and the version pick;
  `--plant`.
- `versions.py`: the latest-N, nearest-N (an oracle) and ideal vote-N version pickers, plus per-unit support masks.
- `report.py`: the tables above, generated from the JSON.
- `eyeball.py`: the false flags, with the code they hit.
