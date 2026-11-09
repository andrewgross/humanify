# exp204 — the library index under the identity transforms (exp200 phase 2)

STATUS: BUILT AND VALIDATED, NOT ADOPTED, NOT MERGED (2026-11-09), branch
`exp204-index-identity-transforms` on main `8851e154b` (exp200 merged).
The new index is `/work/library-index-exp204/4736dcfadff79ee2dd921d1d778fd7d92f604a708f3ad5b652c146ed89d67c84`
(`units/4`, `vendor-content-key/5`). `/work/library-index` and
`pairs.json`'s `libraryIndex` are untouched: adoption is Andrew's call after
the eval (`--library-index` the path above, against `ref-names-111401fe-r1`).

## Totals

- **Recall gain, as exp200 §6 predicted:** on 2.1.179, 113 Bun modules gain a
  real source-path answer from the index (82 that were package-only, 31 the
  model used to name); 2.1.149 the same 113; 2.1.121 107. Predicted 125-136
  per version, 37 of them model-named. 9 modules (opentelemetry, semver
  classes) LOSE a path to a package-only name; 8 move to an
  `index-disagrees` leave. Neither is a wrong path. 2.1.181, which the old
  index already matched, is flat (+12 / -10).
- **The index:** the same 65,839 package@versions as cb64c1f5, the same
  slices, literals byte-identical; unit rows -3,875 (bun) / -2,520 (node),
  module rows -5,022 / -7,238 — restyled copies now fold into one row with
  more version ranges. 43 GB, as before.
- **Cost:** shard phase 4.67 h (16,806 s, 120 chunks × 4 jobs, 6 at a time,
  0 failures), merge 2.47 h (8,901 s, 24 merge jobs, peak 93.8 GB). Peak
  build-tree RSS 89 GB (one chunk alone 82 GB); the memory guard never
  paused. New module shards 276 GB (exp200 estimated 167), index 43 GB;
  `/work` is at 98% (202 GB free) — the cb64 shards are kept as asked.
- **Library score:** see §3 (side by side with cb64).
- **Replay correctness:** with the OLD forms, the replay rebuilt 40 random
  versions' 80 module shards byte-identical to cb64's (§2).

## What changed in the code

- **The index forms.** `LIBRARY_UNITS` `units/3` -> `units/4` and
  `VENDOR_CONTENT` `vendor-content-key/4` -> `vendor-content-key/5`
  (`crates/humanify-ag-core/src/hash/forms.rs`). Both now derive from
  `MATCH_KEY_V5_SETTINGS`, a FROZEN copy of the v5 match key's settings
  (identity transforms concise-arrow-body + parentheses + wrapper
  spelling; private names slotted per class — blinded for the units, as
  before). Not `..MATCH_KEY`: exp200's decoupling stays, so a later
  match-key era cannot silently re-key an index built offline.
  `forms_test::the_index_forms_carry_the_match_key_v5_settings` holds the
  index forms equal to the match key's (units = the twin gate's masked
  stream, module print = the carry's content key) and fails the day the
  match key moves past v5 — the moment to decide whether the index
  follows.
- **The edge and names tables** re-key with the unit prints automatically
  (their rows hold unit prints); their stamps `edges/2` and `names/1`
  describe the row FORMAT, which did not change, so they are not bumped —
  a units/4 index is refused by an older binary on the `units` stamp
  alone (deviation from a literal reading of §6, explained).
- **`literals/1`** untouched.
- **Refusal both ways** (`format_test::an_index_fingerprinted_before_the_identity_transforms_is_refused`):
  cb64's exact stamps are refused by the new binary naming
  `vendor-content-key/4` and `units/3`; a manifest stamped AHEAD of the
  binary is refused the same way; the new stamps are accepted.
- **`humanify-ag index build --replay <index-dir> --sponge-regions M`**
  (`crates/humanify-ag-index/src/replay.rs`, NEW — a deviation from the
  brief, which assumed the builder could already do this). Since exp184
  the builder takes only `--spec`, and a spec cannot express cb64's plan:
  cb64 was built from 66,523 hand pins (exp152), and the spec is by design
  rules, never a package list. The replay reads an EXISTING index's
  manifest — its indexed package@versions, as-of, slice recipes and
  bundler pins, names iff it has a names stamp — and feeds them to the
  builder's pinned-plan path (the one the run-time build already uses).
  It chooses no packages; it refuses an index built under another
  `recipe/N`. Sponge regions are a flag because the manifest does not
  record them.
- **The plan's shard reuse check reads the key at the file's head**
  (`shard::has_current`): the plan used to fully parse every stored shard
  to compare its key — ~285 GB of JSON on the full Bun store, re-read by
  every chunk (the first dry run took over 30 minutes, one read held 25 GB
  RSS on a giant shard). Now 23 s. The merge still reads through
  `read_current`.
- **Pins re-recorded:** `hash_forms_pin_test` modules moved in its
  index-key lines ONLY (K 34 of 496, V 2,640 of 4,604; every factory
  hash R and carry key C line unchanged); units moved 1,798 of 5,166
  print lines; `UNIT_MODEL_PINS` / `BUNDLER_UNIT_MODEL_PINS` got their
  version-4 rows (the unit cut did not change, only the prints).
  `unpack_test`'s synthetic in-package name moved
  (`module-a7712902` -> `module-ede81996`: the synthetic name is the
  content key's prefix, and for a factory with no free names the v5
  content key now equals its structural hash).
- docs: `docs/design/library-index.md` §2 (`--replay`, `units/4`),
  `docs/responsibility.md` (the replay plan's owner row).

## 1. What was built, and how

`humanify-ag index build --replay /work/library-index/cb64c1f5… --sponge-regions origin`
over a hard-linked copy of exp161's store (`/work/exp204-data/store`,
`cp -al`; shard writes are write-then-rename, so cb64's shards are never
touched) and a work dir of SHELLS of exp152's install trees
(`/work/exp204-data/work`: `build/`, `entries/` and the small files
hard-linked, `node_modules` symlinked — the builder deletes a tree after its
shards land since exp184, and a rebuild removes its out dir before writing,
so no original inode is ever written). Registry: a hard-linked copy of
exp152's snapshot, `--offline`. Ran OUTSIDE the heavy lock by Andrew's
approval (parallel with exp200), with a memory guard (pause under 20 GB
free); the lock was removed host-wide during the run (exp205).

The plan: 65,839 tasks, every literal shard reused, 131,678 module shards
rebuilt; no install, no bundle (every `.recipe` stamp matched). Builds that
had failed for cb64 (~2%) were retried: on 6,000 sampled shards every
build outcome is the same as cb64's (138 carry a failed build, failed
the same way). After the build, a dry run over the store reports all
131,678 module shards current.

## 2. Validation

**Replay = cb64's plan.** A control replay with the OLD forms (this branch
with `forms.rs` reverted) over a scratch store with 40 random versions'
module shards removed rebuilt all 80 shards byte-identical to cb64's.

**Counts vs cb64c1f5** (`/work/exp204-data/logs/exp204-counts.txt`):

| | cb64c1f5 | exp204 4736dcfa |
| --- | --- | --- |
| fingerprints | vendor-content-key/4, units/3 | vendor-content-key/5, units/4 (literals/1, edges/2, names/1 same) |
| packages | 65,839 | 65,839 (identical set) |
| slices, data files, literals tables | | identical |
| refused | 6,330 | 5,643 (the 687 pins cb64 never indexed are not replay pins) |
| registry packuments | 9,898 | 9,897 (one package with no indexed version) |
| units rows bun / node | 6,891,788 / 6,511,763 | 6,887,913 / 6,509,243 |
| module rows bun / node | 408,562 / 440,918 | 403,540 / 433,680 |
| edges rows bun / node | 4,919,454 / 4,948,289 | 4,916,691 / 4,946,648 |
| names rows bun / node | 4,729,312 / 4,555,275 | 4,727,983 / 4,554,099 |
| size | 43 GB | 43 GB |

Fewer rows everywhere: rows are stored once per (package, file, print) with
version ranges, and copies of one file that differed only in minifier style
now share a print.

**Recall gain** (`humanify-ag discover --bundle-order`, old = cb64 with the
exp200 binary, new = this index with this branch; per module: a real source
path, a package-only synthetic path, or a leave):

| version | modules | path old -> new | gained a path (from package-only / from a leave) | lost a path | new index-disagrees |
| --- | --- | --- | --- | --- | --- |
| 2.1.121 | 1,483 | 963 -> 1,069 | 107 (75 / 32) | 1 | 8 |
| 2.1.149 | 1,586 | 968 -> 1,072 | 113 (82 / 31) | 9 | 8 |
| 2.1.179 | 1,594 | 955 -> 1,059 | 113 (82 / 31) | 9 | 8 |
| 2.1.181 (control) | 1,584 | 1,047 -> 1,049 | 12 (12 / 0) | 10 | 0 |

The 9 lost on 2.1.179 are opentelemetry core / sdk-metrics files and
`semver/classes/semver.js`: still placed in their package, but their v5
whole-module key no longer picks a single file (more copies now share it).
The 8 new `index-disagrees` are aws-sdk nested-clients / client-signin
modules whose key the index now also holds under another package — a leave,
so the model names them. No module moved to a different path.

## 3. Library score (bun-dev and bun-held, both rules)

PENDING

## 4. Fixtures re-fingerprinted

- `test/e2e/library-index/` — the e2e store's ms index, rebuilt with
  `--replay` (Bun 1.4.0, both slices, names): every table kept its size and
  line count, only the prints moved; id `0242134f…` -> `bfe447b7…`. Its
  README now gives the replay as the rebuild command.
- `crates/humanify-ag-cli/tests/fixtures/runtime-build-esbuild/` — the
  ms@2.1.3 esbuild module shard, rebuilt the way its README says (literal
  shard and registry unchanged).

## 5. Gate

PENDING

## What this could NOT prove

- The eval: not run (scheduled after this lane, against
  `ref-names-111401fe-r1` with `--library-index` the path above).
- The recall gain is a placement count on four CC versions, not a noise
  number; whether the 113 new source paths lower vendor churn across hops is
  the eval's question.
- The 276 GB of new shards sit beside cb64's: freeing either is a separate
  decision.
