# exp239: harness JSON is plain serde; the library scorer is version 16

STATUS: done on branch `exp239-harness-serde` (from origin/main `ab7f77dd7`),
not merged. Harness only (`crates/humanify-ag-dev`), plus a small product
edit in `humanify-ag-cli` that removes data nothing reads (the retired keys).
No pipeline output can change: the binary's only edit drops a table that
only the harness read.

## Totals

Against origin/main, raw lines (comments and blank lines included):
**code -662 net (+810 / -1,472), tests +133 net (+294 / -161).** The test
growth is the new identity pins, the segment-walk test and the walk test.

| Item | Code | Tests |
|---|---:|---:|
| 1. Python JSON spelling replaced by serde; library scorer 16; frozen identity writer | +615 / -1,135 | +249 / -128 |
| 4c. Truth-layer builders share one segment walk, package root, walk and layer-file writer | +148 / -224 | +41 / -3 |
| 3. Retired keys and `Registry.retired_*` deleted; thousands separator from `numfmt` | +50 / -116 | +4 / -30 |

The scores do not move. On the four labels this host can rescore, scorer 16
writes the same values as scorer 15 in every card, summary, timing and
breakdown file (207 files). Every truth layer of the five corpora this host
holds re-freezes byte-identically.

## What changed

**1. The library scorer writes plain JSON (scorer 16).** The harness no
longer copies Python's `json.dumps`. `PyValue` (a JSON value with Python's
int/float split and its own parser), `json/py.rs`, the npm cache's
`dumps_sorted` and the corpus build's `J` writer are gone. Library records
(cards, summaries, timing, `truth15.json`, `wrong-families.json`, label
`pipeline.json`, run records, `bands.json`, npm cache files, build records)
are written with `json::text` (serde pretty JSON and a newline) and read with
`serde_json`, which now has `float_roundtrip` in the harness crate (exact
float reads; the release binary does not get the feature, it is built with
`-p humanify-ag-cli`). `SCORER_VERSION` is `exp096-score/16`. Scorer 16
scores exactly as scorer 15; only the file spelling changed.

**The identity spelling is kept, frozen.** A corpus's `corpusSha256`, every
layer id, and the sha of every layer and truth file on the data roots are
hashes of one exact text. That text is now `json/frozen.rs`: a writer only
(about 150 lines), over `serde_json::Value`, used for identities and for the
layer and truth files the builders write. A test pins every recorded corpus
identity (all 16) and three layer ids
(`frozen_test::recorded_identities_are_unchanged`), and checks that every
committed layer names its corpus's identity and that `bands.json` names the
dev corpus's. The pinned values were computed independently before the
change.

**4c. The truth builders share their common code.** The four functions
that count map-segment bytes in a span (`truth::vote`, `misattr13::majority`,
`mapdefect14::label_source`, `defect15::label_sources`) and the vendor-precision
truth now read one walk (`truth::segment_bytes`, `source_bytes`). The
chained layer uses freeze's `package_root` (the two were identical);
mapdefect14's `package_of` is misattr13's `package_root` without dot
entries; `walk_package` is `pypath::walk_files` skipping `node_modules`;
the five layer builders share `layers::{mapped_entries, write_entry,
entries_block}`.

**3. Leftovers from exp236.** The harness's `--pipeline-set` check no longer
reads the pipeline registry's four retired keys: they are now plain unknown
keys, refused with a "did you mean". The four entries,
`Registry.retired_keys` / `retired_flags`, `retired_flag`, `retired_in` and
their test are deleted. The harness's two remaining thousands-separator
copies (`eval/report.rs`, `runners/walk/report.rs`) use
`humanify_ag_model::numfmt`; the third copy exp236 counted was already
deleted by exp237.

## Evidence

**Scorer 15 vs 16** (`cargo dev library rescore <label> --corpus C --out D`,
scorer 15 from origin/main, scorer 16 from this branch). Each file was compared
by value, ints and floats kept apart; the only allowed difference was the
`schema` string. Per-entry progress lines were byte-identical.

| label | corpus | files | different values | set: P / R / F1 / F0.5 (both versions) |
|---|---|---:|---:|---|
| exp213-after-bun-dev | bun-dev | 94 | 0 | bun: 0.999079 / 0.865216 / 0.927342 / 0.969092; node: 0.998574 / 0.859474 / 0.923817 / 0.967265 |
| exp213-after-bun-held | bun-held | 34 | 0 | bun: 0.99982 / 0.86963 / 0.930192 / 0.970754; node: 0.999688 / 0.832481 / 0.908455 / 0.961081 |
| exp181-esbuild | kind-esbuild | 41 | 0 | 0.998613 / 0.858473 / 0.923255 / 0.96704 |
| exp181-webpack | kind-webpack | 38 | 0 | 0.994681 / 0.897859 / 0.943793 / 0.973681 |

Checked after item 1 and again after 4c, since 4c changed the code that
counts votes.

**Layer re-freeze** (`cargo dev library layers <corpus> --out D`, final
code): kind-webpack 34, kind-esbuild 37, kind-rollup 34, bun-held 30, bun-dev
90 entries. All 10 layers (chained, origins, origins7, chained9, origins10,
chained11, origins12, copies2, misattr13, mapdefect14) reproduce the
committed sha for every entry, and every manifest is identical. The other
11 corpora have no data root on this host, so for them the identity pins
are the proof.

## Kept, and why

- `json/frozen.rs`, the identity spelling: changing it would change every
  recorded corpus and layer id (the plan's phase 2, not recommended).
- `library/value.rs`: the small value builders and the rounding a card's
  numbers are computed with (`round_n`, `share`, ties to even). That rounding
  is arithmetic, not spelling.
- `pypath.rs`, `pyre.rs`, `pyclass_table.rs`: the layer bytes depend on
  their path and regex results, so they stay as they are.
- `json::js_text`: the eval's files (cards, run manifests, launch goldens)
  are written in the product's own JS spelling (`humanify_ag_model::js`),
  and committed launch goldens depend on those bytes. It calls product code
  and is not a copy in the harness. Changing it belongs to lane 1d. The
  prettier emulation is already gone (exp237).
- The vendor-precision golden was re-spelled to plain JSON, values unchanged.

## For readers

- A scorer-15 label and a scorer-16 label are different scorer versions, so
  the leaderboard and `library proof` refuse to compare them. Rescore the
  old label once (`library rescore <label> --out DIR`). Library results are
  not committed (exp235), so nothing in the repo needed re-freezing.
- Old files in Python spelling (npm cache, `built.json`, label records) are
  still read: serde reads the same values.

## Gate

See the gate table in the final report of this lane (`cargo dev check`,
all 12 stages).
