# exp151: INSIDE call edges in the library index (format `/4`)

## STATUS (2026-10-25)

DONE on branch `exp151-index-edges` (off main `2a24ae54e`). Step 1 of `docs/design/library-names-plan.md`.
**No default changed, no lookup rule reads edges** (step 2 does). Pipeline output byte-identical (neutrality below).

## What changed

- **Builder.** For every unit row, the builder records the WHOLE prints of the statement units its statement
  references in the pipeline's statement reference graph (`graph::references`, factory bodies cut), built over the
  same bundle it fingerprints (`libindex::units::bundle_unit_graph`). A callee the row's own package owns is INSIDE.
  A callee another package owns is OUT-OF and is stored with that owner. A callee with no print (a container: a
  wrapper or a bundled module's factory) or no owner is counted in the shard's notes (`unrecordedEdges`) and not
  recorded. Only a unit's own references are read; who calls into a package is never recorded. Stamp
  `fingerprints.edges = "edges/1"`, part of every module shard's key, so module shards rebuild (literal shards do
  not).
- **Format `humanify-library-index/4`.** `edges.<slice>.jsonl` is the row table for instruments (hash, owner, file,
  inside, outOf; a row with no callee has no edge row). `edges.<slice>.bin` is the lookup table: keyed by the unit
  row's position in `units.<slice>.bin`, sorted and fenced like it, and each distinct callee list is stored once. A
  row's list is the union over its versions.
- **Store.** `LibraryIndex` answers `ReferenceStore::edges` for INSIDE and OUT-OF from the table. A callee is
  `(owner id, print)` (`table::EdgeCallee`; the trait gained an associated `Callee` type, since an index callee need
  not be a row). A `/3` index (no edge tables, no `edges` stamp) still loads and answers `NotRecorded`. INTO and
  every-edge reads stay refused.

## Representation: the callee list, chosen by measurement

| per slice (Bun, 320 versions) | list table (built) | set hash only |      both |
| ----------------------------- | -----------------: | ------------: | --------: |
| bun-node-cjs-min              |          1,550,688 |       641,912 | 1,869,144 |
| bun-bun-cjs-min               |          1,424,068 |       589,704 | 1,716,620 |

The list costs 2.4x a set hash. That is +30% of the index's lookup bytes. The plan's step 2 rule is CONTAINMENT (an
INSIDE edge of the bundle that the index lacks counts against; one the bundle lacks counts for nothing), and a hash
of the sorted set answers only equality. So the list is required, and a hash on top adds nothing the list cannot
compute. Interning makes the list cheap: 36,569 keyed rows share 7,317 distinct lists on bun-bun (39,807 / 8,232 on
bun-node), because a statement's function rows share its list.

## Measurement

Index: 330 generic packages, npm-high-impact's top downloads (no `@types/`), each pinned to its latest version at
as-of 2026-10-15. 320 versions were indexed; 10 failed to build. Two Bun slices: `bun:target=node,format=cjs,minify,production`
and `bun:target=bun,format=cjs,minify,production`, `--sponge-regions origin`, Bun 1.4.0, 30 cores (34-63), `--jobs 4`,
under `flock /work/index-build.lock` and `/work/heavy.lock`. Scripts: `scripts/build3.sh`, `scripts/time4.sh`,
`scripts/measure.py`; output `measure.json`.

| measure                                      | without edges (`/3`, main) | with edges (`/4`) | delta            |
| -------------------------------------------- | -------------------------: | ----------------: | ---------------- |
| index bytes, total                           |                 29,087,508 |        53,184,854 | +24.1 MB (+83%)  |
| lookup tables (`.bin`, what a run reads)     |                  9,775,648 |        12,750,404 | +2.97 MB (+30%)  |
| edge row tables (`.jsonl`, provenance)       |                          - |        21,122,156 | +21.1 MB         |
| every other table                            |                          - |                 - | byte-identical   |
| build, warm work dir (fingerprint + merge)   |            41.3 s / 40.9 s |   41.3 s / 40.6 s | none measurable  |
| build, cold (install + bundle + fingerprint) |                          - |              67 s | (npm cache warm) |

Unit rows with at least one recorded callee:

| slice            | unit rows | any edge       | INSIDE         | OUT-OF     |
| ---------------- | --------: | -------------- | -------------- | ---------- |
| bun-node-cjs-min |    59,636 | 39,807 (66.8%) | 39,768 (66.7%) | 706 (1.2%) |
| bun-bun-cjs-min  |    58,982 | 36,569 (62.0%) | 36,537 (61.9%) | 647 (1.1%) |

References that record no callee: 13,754 / 11,775 to a container and 1,042 / 982 to an unowned unit (bun-node /
bun-bun), against 837,654 / 737,176 callees recorded on rows (function rows repeat their statement's list). A call
into another bundled CommonJS module goes through its factory (a container), so cross-module CommonJS edges are
the main gap.

Prints that collide today (more than one unit row), and whether the rows' recorded callees differ:

| slice    | kind                     | groups | rows differ   | every row distinct | differ among rows with edges | per owner differ |
| -------- | ------------------------ | -----: | ------------- | ------------------ | ---------------------------- | ---------------- |
| bun-node | several owners           |  2,086 | 1,841 (88.3%) | 1,304 (62.5%)      | 1,446 (69.3%)                | 1,834 (87.9%)    |
| bun-node | one owner, several files |  8,421 | 7,191 (85.4%) | 6,872 (81.6%)      | 1,726 (20.5%)                | -                |
| bun-bun  | several owners           |  2,106 | 1,868 (88.7%) | 1,336 (63.4%)      | 1,357 (64.4%)                | 1,860 (88.3%)    |
| bun-bun  | one owner, several files |  8,652 | 7,114 (82.2%) | 6,805 (78.7%)      | 1,069 (12.4%)                | -                |

How to read it. "Differ" includes a row with edges against one with none. For one owner's several files, that is
mostly the same code in a CommonJS file (whose callees sit behind containers) and an ESM file. Only 12-21% of those
groups differ when both rows record callees. Under containment, a bundle unit with INSIDE callees still counts
against an index row that records none, so those groups are separated too. That is evidence for step 2's census,
not a verdict: whether the separation is RIGHT is measured there, against truth. "Per owner differ" compares the union of each
owner's rows: in 88% of several-owner groups the owners' callees differ.

**Projection, not measured:** on the installed 7.1 GB `/3` index, the edge lookup table would be about the size of
the unit lookup table (1.0-1.1x here: about +0.65 GB over five slices), and the edge row table about 1.15x the unit
row table (about +5.7 GB). The row table is provenance only; slimming it is a follow-up if size matters.

## Tests (red, then green)

Each was watched failing with edge recording (builder) or edge reading (store) switched off, then passing.

- `humanify-index/src/edges_test.rs`: twin bodies with different callees record different edges; the merged
  index tells them apart through `ReferenceStore::edges`; a two-package build's edges stay with each caller (alpha's
  call into beta is OUT-OF alpha, never on beta; the app's calls are recorded nowhere).
- `matching/store/store_test.rs`: INSIDE and OUT-OF apart from an edge table; a `/4` index on disk answers its
  edges, a `/3` one still loads and answers `NotRecorded`, a `/4` one missing its edge table is refused.
- `libindex/table_test.rs`: the edge table answers each unit row and stores a shared list once.
- `libindex/format_test.rs`: `/3` is read without an edges stamp and only without one; every stamp is checked.

## Gate and neutrality

| gate                                                                              | result                                                                                                          |
| --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `npm run check` (13 stages)                                                       | PASS                                                                                                            |
| warm neutrality vs `2a24ae54e`, 2.1.215->2.1.216, default                         | NEUTRAL: 0 files / 0 lines, both legs +0 cache writes, exit 0 / 0                                               |
| warm neutrality vs `2a24ae54e`, report mode, `/work/library-index/current` (`/3`) | NEUTRAL: 0 files / 0 lines, +0 / +0, exit 0 / 0; `library-report.json` byte-identical too (index `c4d2bd6c...`) |

Both neutrality runs used a dead endpoint (`--endpoint http://127.0.0.1:9/v1`), so no prompt could reach a model:
every answer was replayed from `/work/neutrality-cache`.

The small index: `/work/exp151-data/ix-edges` (`/4`, id `d4fc8e09215777578c15025df08eba9c7c43c7952b288d6071e69258018ace74`). The same build without edges (main):
`/work/exp151-data/ix-main-cold`.
