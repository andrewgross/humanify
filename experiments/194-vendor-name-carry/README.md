# exp194 — an unchanged module keeps the file and identifier an earlier pair carried

STATUS: MERGED to main (2026-11-07). The exact structural-hash carry now carries
the prior entry's file and identifier with its name. Single-hop rerun:
vendor churn on walk hop 2.1.211 -> 2.1.212 21,160 -> 42 lines. Eval
(names, scratch, cold): novel/realLn byte-equal, every pair exit 0, boots
OK, warm self-hop byte-identical, cold self-hop 8 -> 2 lines (the 6 lines
gone were this defect). Not fixed: the lodash / lodash-es folder swap
(234 lines on 2.1.210 -> 2.1.211), a separate placement mechanism.

## The defect

Names-mode walk `/work/walk-e259eca9` (main `e259eca91`, cb64 index,
2.1.210..2.1.216). On 2.1.211 -> 2.1.212,
`vendor/react-error-codes/lib_2132c256.js` became
`vendor/react-error-codes/lib_dc968b00.js`: 10,552 lines, byte-identical
contents, only the name changed. That one rename charged 21,104 changed
lines (83% of that hop's noise, 16% of the whole walk's diff).

## Cause

Two hops, two different carries:

1. **2.1.210 -> 2.1.211:** the module's minified input changed, so its
   structural hash moved (`2132c256…` -> `dc968b00…`). The exact-hash carry
   missed, and the content pairing (finding #90) paired it with the prior
   module. That pairing keeps the prior's name, file AND identifier, so
   2.1.211 recorded `structuralHash dc968b00…` with
   `fileName …/lib_2132c256.js` and `runtimeIdentifier lib_2132c256`.
2. **2.1.211 -> 2.1.212:** the module did not change. The hash
   `dc968b00…` is in the prior manifest, so the EXACT structural-hash carry
   (`modules::name_cjs_factories`) took it — but that carry carried the
   **name only**. The file and identifier were derived from the record's
   own hash again (`stable_stem` = `lib_<hash8>`), so they became
   `lib_dc968b00`.

So the content pairing's carry lasted exactly one hop. None of the guesses
in the brief applied (not a names-mode library folder, not a size cap, not
a model-named-only rule, and the hash did NOT change on the hop that
renamed the file). In the usual case the prior's file and identifier ARE
the hash's spelling, which is why only modules that a pair carried the hop
before were hit.

The same shape hit app text assets (`src/_assets/*`): a content pair keeps
an asset's identifier only, and the next hop's exact match re-derived it.

## Census (all 6 walk hops)

Byte-identical vendor files that changed path, and manifest entries whose
structural hash was unchanged but whose file or identifier changed:

| hop                | identical files renamed | lines charged | unchanged-hash entries that lost file/identifier                                                               |
| ------------------ | ----------------------: | ------------: | -------------------------------------------------------------------------------------------------------------- |
| 2.1.210 -> 2.1.211 |                       7 |           234 | 0 (the 7 are lodash / lodash-es folder swaps of ESM-inlined `module-*.js` files, another mechanism, see below) |
| 2.1.211 -> 2.1.212 |                       1 |        21,104 | 4: react-error-codes (file + identifier), 3 app text assets (identifier)                                       |
| 2.1.212 -> 2.1.213 |                       0 |             0 | 0                                                                                                              |
| 2.1.213 -> 2.1.214 |                       0 |             0 | 1: app text asset `environment` (identifier)                                                                   |
| 2.1.214 -> 2.1.215 |                       0 |             0 | 0                                                                                                              |
| 2.1.215 -> 2.1.216 |                       0 |             0 | 0                                                                                                              |
| **total**          |                   **8** |    **21,338** | **5** (21,104 lines of it this lane's shape, plus ~2 reference lines per asset identifier)                     |

Not fixed here: the 7 lodash / lodash-es files on 2.1.210 -> 2.1.211 (234
lines). They are ESM-inlined library code that names mode files into a
package folder; the index picks `lodash` on one hop and `lodash-es` on the
next for the same body. That is the library placement's package choice, not
the CommonJS-module carry.

## Fix (at the owner)

`docs/responsibility.md` row "Which prior vendor module is this fresh one,
when the exact structural hash missed?" — `modules::vendor_pairing` — plus
the exact carry in `modules::name_cjs_factories`. One path, no new matcher:

- `vendor_names::load_prior_vendor_names` reads each entry's `fileName` and
  `runtimeIdentifier` with its name (`CarriedName::identity`, built by
  `vendor_pairing::prior_identity`: the file only when it is a reusable
  `vendor/…` path).
- `name_cjs_factories` sets that as the record's `carried` identity — the
  same field a content or declared pair fills — so `FileNameChooser`
  reserves the prior path and `IdentifierPlanner::plan` reuses the prior
  identifier. Still no model call; the result is still a pure function of
  the input and the prior manifest.
- An unchanged app text asset keeps the prior asset's identifier (its file
  stays named from its text).
- A stale-era prior (re-keyed by content) carries no file or identifier, as
  before.

Tests (red first, then green), `crates/humanify-ag-core/src/unpack_test.rs`:
`an_unchanged_module_keeps_the_file_and_identifier_an_earlier_pair_carried`
(three hops: pair, then unchanged; the walk's exact shape) and
`an_unchanged_app_text_asset_keeps_the_identifier_an_earlier_pair_carried`.
`loads_prior_vendor_names_from_a_prior_tree` now expects the file and
identifier beside the name.

## Proof

### (a) the single hop rerun

2.1.211 -> 2.1.212 rerun with the candidate binary, the walk's own config
(`runs/2.1.212.toml`, cb64 index, cold, prior =
`trees/2.1.211/.humanify/humanified.js`), scripts in `/work/exp194-hop/`.
Changed lines against the walk's 2.1.211 tree (`git diff --no-index
--no-renames`):

| tree                    | react-error-codes file                              | vendor/ changed lines | src/ changed lines | unchanged-hash entries that lost file/identifier |
| ----------------------- | --------------------------------------------------- | --------------------: | -----------------: | -----------------------------------------------: |
| walk (main `e259eca91`) | `lib_dc968b00.js` (renamed)                         |      21,160 (7 files) |             30,451 |                                                4 |
| exp194                  | `lib_2132c256.js` (kept, byte-identical to 2.1.211) |          42 (4 files) |             30,503 |                                                0 |

Vendor churn on the hop: -21,118 lines. The src/ figure is a cold model
draw on both sides (the app is renamed by the model live); its ±50 is draw.

### (b) gate

`npm run check`, run bare under the heavy lock:

| check         | worktree (`2c21b7083`) | clean checkout (branch tip) | clean checkout (merge commit)           |
| ------------- | ---------------------- | --------------------------- | --------------------------------------- |
| all 13 stages | PASS 13/13             | PASS 13/13 (`0b3375ad3`)    | run after merging (see the lane report) |

### (c) eval

`npm run eval -- score exp194-vendor-name-carry` (binary `981fd5fe7b2c`,
built from `2c21b7083`; names mode, scratch bases, cold, cb64 index), run
alone under the heavy lock. Compared with `leaderboard.ts
ref-names-0357696e-r1 exp192-veto-all-rules exp194-vendor-name-carry`
(both label directories copied read-only from
`/work/exp192-veto-all-rules/experiments/034-eval-harness/results/` into
this worktree's results folder; exp192 is main's pipeline behaviour), and
for the movements, by diffing the label's own trees in
`/tmp/eval-work/<label>/` file by file.

Totals (names-mode bands from `noise-bands.json` `byMode.names`):

| KPI                     | ref-names r1 | exp192 (main) |                    exp194 |  exp194 - main |                band |
| ----------------------- | -----------: | ------------: | ------------------------: | -------------: | ------------------: |
| novel                   |        4,188 |         4,188 |                     4,188 | 0 (byte-equal) |                   0 |
| realLn                  |      416,377 |       416,377 |                   416,377 | 0 (byte-equal) |                   0 |
| treeLn                  |      146,767 |       145,644 |                   145,504 |           -140 |              ±2,116 |
| noise                   |        3,123 |         2,739 |                     2,726 |            -13 |                ±397 |
| noiseLn                 |       65,539 |        50,065 |                    49,733 |           -332 |             ±15,709 |
| vendorLn                |        1,579 |         1,591 |                     1,587 |             -4 |                  ±8 |
| vendorReal              |       10,832 |         9,770 |                     9,784 |            +14 |                 ±96 |
| vendor:vendorReal       |       10,696 |         9,634 |                     9,648 |            +14 |                 ±96 |
| mints                   |          211 |           215 |                       215 |              0 |                 ±22 |
| cold self-hop (2.1.216) |            8 |             8 |                     **2** |             -6 | reference range 8-8 |
| warm self-hop           |    identical |     identical | identical, 0 cache writes |                |                     |
| pairs exit 0 / boots OK |          4/4 |           4/4 |                       4/4 |                |                     |

Reading it:

- The eval rebuilds every base from scratch with NO prior, so on its four
  scored hops no module can have been paired by content the hop before:
  the defect's two-hop shape does not occur there, and every KPI holds
  inside its band, as expected. The one place it does occur is the
  SELF-HOP (2.1.216 re-run with the scored 2.1.216 tree as its prior,
  which content-paired against the 2.1.215 base). There, exp192's 8 lines
  were this defect except for one model re-draw: three app text assets
  lost their carried identifiers and were re-derived (`lib_33a05707`,
  `lib_5716f3d5`, `lib_5734ec5f` -> fresh spellings), which changed their
  three manifest lines and the four app lines that use them
  (`user-config-manager.js`, `chart-resources-docs.js`); the remaining
  change was `skill-doctor.js` (a class name the model re-drew). exp194's
  self-hop is that re-draw only (1 file, 2 lines). "Below the reference
  range" is the fix: the range was recorded with the defect in it and
  should be re-measured.
- vendorReal +14 vs main (inside its ±96 band) is cold draw, not this
  change: the vendor git-line totals per pair are equal or +2
  (2.1.86: 484 vs 484; 2.1.198: 9,429 vs 9,431), and the files that differ
  are (a) model-named re-export shims (`lib_82619232` / `lib_ee387c94`
  groups) that the two cold BASE rebuilds filed under different
  model-chosen folders (`@opentelemetry/api` vs `@aws-sdk/smithy-client`),
  and (b) highlight.js grammar bodies whose model-named locals differ by
  ±4 lines per file. Neither is reachable by a carry: the shims' groups
  changed size (no exact carry) and body locals are named by the model.
