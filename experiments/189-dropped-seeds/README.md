# exp189: which stage drops library statements whose matched-function cover is >= 50%

## STATUS (2026-11-06): DIAGNOSED. Instrument only (a seed trail in the `discover` report); no rule changed.

- **Totals first.** exp188's ">= 50% cover, unclaimed" bucket on bun-dev is **9,682 KB right / 875 KB wrong**
  (bun-dev-bun 4,842 / 315, bun-dev-node 4,840 / 560, the same totals exp188 printed). **Every byte of it is now
  charged to the stage that dropped it; none is unexplained** (no unit lacks a trail row).
- **The isolation veto owns most of it: 7,028 KB right / 60 KB wrong (73% of the right bytes).** Then the factory
  tie fence (1,123 / 638), the factory whole-seed rule (1,087 / 14), the lookup's position window (407 / 29), and
  small remainders (context presence, shadow ties, evidence conflict, helper prints).
- **exp188's open example is answered.** The pre-built `@microsoft/applicationinsights-*` statements inside
  `@vscode/extension-telemetry` (exp188's 30-50% bucket, not this one) are dropped by the **factory whole-seed rule**
  (exp132/exp176, `factory_whole_seed` + `factory_whole_cover = 0.95`): each is a one-statement CommonJS module
  that function prints cover 30-50% of, with no whole print held. exp188 opened the cover floor, the package cap,
  evidence conflict and the helper floor, but not this rule.
- **No fix was built.** The stage that owns the bucket, the isolation veto, is the lever exp143 / exp144 / exp147
  already worked: exp144's `veto_support = "shown"` was skipped by Andrew on 2026-10-24, and exp147 found no
  generic guard for its residual errors. A new veto rule here would redo that branch, not add a clean one. The
  second stage (tie fence) is 64% right / 36% wrong, not separable by a threshold; the third (whole-seed) has its
  threshold measured and chosen in exp176. What was run instead: the veto's EXISTING relaxation,
  `veto_support = "all-rules"` (a registry key since exp143, config only), scored on the Bun corpora and cc for
  the record (section 3): **recall +2.5 / +2.8 points on bun-dev and +1.3 / +1.3 on bun-held, precision up or
  equal on every set (held-out included), app 0, cc unchanged.** Not flipped (default unchanged); Andrew's call.

## 1. The instrument: the seed trail

`Discovery::seed_trail` (crates/humanify-ag-core/src/libdetect/discovery.rs): per index seed the lookup proposed,
what became of it. It is collected on every indexed run (set differences; no extra lookup) and written to the
report only by the `discover` verb with `--unit-prints` (`seedTrail`). Stages, in pipeline order:

| where              | stages                                                                                                                                       |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| lookup guards      | `unsupported` (the position window refused the pick; a second `picks` pass, verb only), `evidenceConflict`, `helperFamilies`, `helperPrints` |
| presence rules     | `contextPresence`, `copyPresence` / `copyPresenceMoved`, `shadowTies` / `shadowTiesMoved`, `factoryTieFence`, `factoryWholeSeed`             |
| after the fixpoint | `vetoed`, `settledOut` (another seed on the unit won), `external`, `ownPackage`, `claimedOther`, `claimed`                                   |

**It changes nothing:** the 90 bun-dev reports of the instrumented binary are byte-equal to `exp188-ctl-bun-dev`'s
(main `f8bbb20a`) once the `seedTrail` field is removed (90 / 90). **The counters fire:** whole-corpus row counts
are claimed 155,957, vetoed 8,735, settledOut 8,609, unsupported 5,221, factoryTieFence 2,738, claimedOther 963,
shadowTiesMoved 356, shadowTies 259, factoryWholeSeed 75, copyPresenceMoved 73, contextPresence 49, helperPrints
48, evidenceConflict 1. Stages with no row on bun-dev: `copyPresence` (drop), `helperFamilies` (off by default),
`external`, `ownPackage` (no own package found in these bundles), `unclaimed`. The unit test
`the_seed_trail_names_the_stage_that_dropped_each_index_seed` plants a whole-seed drop, a copy-presence drop, a veto
and a kept seed and reads each stage back.

`attribute.py` joins the census population (exp188's `census.unclaimed_units`, refactored out of `census_map` so
there is one copy; the refactored census reproduces exp188's JSON byte for byte on `exp188-ctl-bun-dev`) to the
trail by span and charges each unit ONE stage: the last that touched it in pipeline order. A `settledOut` row is
charged to the winning seed's own fate when that seed has a row.

## 2. Attribution (bun-dev; KB of whole units; verdict = the census's best package, as in exp188)

| stage            | bun-dev-bun right |   wrong |  other | share of right | bun-dev-node right |   wrong |  other | share of right | top right packages (both targets)                                                          |
| ---------------- | ----------------: | ------: | -----: | -------------: | -----------------: | ------: | -----: | -------------: | ------------------------------------------------------------------------------------------ |
| vetoed           |             3,756 |      28 |     20 |          77.6% |              3,272 |      32 |     26 |          67.6% | highlight.js 1,559, lucide-react 1,499, @ant-design/icons-svg 1,412, zod 345, css-tree 317 |
| factoryTieFence  |               214 |     200 |      0 |           4.4% |                909 |     438 |     19 |          18.8% | tr46 290, prompts 228, encoding-sniffer 110, @eslint/css-tree 103                          |
| factoryWholeSeed |               642 |       7 |      0 |          13.3% |                445 |       7 |      0 |           9.2% | ajv 590, jquery 191, libsodium-wrappers 109, azure-devops-node-api 75                      |
| unsupported      |               199 |      15 |      3 |           4.1% |                208 |      14 |      5 |           4.3% | @ant-design/icons 208                                                                      |
| contextPresence  |                 0 |      51 |     24 |              - |                  0 |      38 |     24 |              - | (none right)                                                                               |
| shadowTies       |                 9 |      13 |      0 |           0.2% |                  7 |      30 |      0 |           0.1% | chalk 14                                                                                   |
| evidenceConflict |                20 |       0 |      0 |           0.4% |                  - |       - |      - |              - | @modelcontextprotocol/sdk 20                                                               |
| helperPrints     |                 1 |       1 |      0 |           0.0% |                  1 |       0 |      0 |           0.0% | dompurify 1                                                                                |
| **total**        |         **4,842** | **315** | **47** |                |          **4,840** | **560** | **74** |                |                                                                                            |

Judged instead by the package the run itself proposed (the trail row's), the veto rows read 3,770 / 14 (bun) and
3,280 / 24 (node), the tie fence 322 / 92 and 1,007 / 340.

The vetoed bytes, by shape (both targets): whole-print hits 5,932 right / 55 wrong, function-cover hits 1,095 / 5;
factory bodies 5,863 / 47, top-level statements 1,148 / 8, lazy bodies 16 / 3; units >= 1 KB 5,356 / 14, under 1 KB
1,671 / 45. These are leaf tables with no claimed same-family neighbour: highlight.js language definitions,
lucide / ant-design icon nodes, zod locale tables. That is the case the veto was made for (a fork that keeps a
library's leaf tables, docs/design/library-discovery.md §7), and on bun-dev it is 99.2% right code.

## 3. The veto's existing relaxation, scored (config only)

`library.discovery.veto_support = "all-rules"` at main `f8bbb20a`, cb64 index, structural rule, against
`exp188-ctl-*` (the default at the same commit). Labels `exp189-allrules-{bun-dev,bun-held,cc}`.

| set                      | precision ctl -> all-rules | recall ctl -> all-rules | F0.5 ctl -> all-rules | named KB (delta) | wrong-family KB | app |
| ------------------------ | -------------------------- | ----------------------- | --------------------- | ---------------- | --------------- | --- |
| bun-dev-bun              | 99.830 -> 99.834           | 83.93 -> 86.40 (+2.47)  | 96.19 -> 96.82        | +2,583           | 149.9 -> 150.3  | 0   |
| bun-dev-node             | 99.744 -> 99.744           | 83.15 -> 85.94 (+2.79)  | 95.92 -> 96.64        | +3,053           | 234.2 -> 241.8  | 0   |
| bun-held-bun (held-out)  | 99.975 -> 99.976           | 85.66 -> 86.97 (+1.31)  | 96.74 -> 97.07        | +343             | 5.6 -> 5.6      | 0   |
| bun-held-node (held-out) | 99.711 -> 99.716           | 81.87 -> 83.15 (+1.29)  | 95.55 -> 95.90        | +360             | 66.5 -> 66.4    | 0   |
| cc (sample)              | 100.000 -> 100.000         | 81.90 -> 81.90          | 95.77 -> 95.77        | 0                | 0 -> 0          | 0   |

Band 0 (exp096 `bands.json`), so every delta is real. Precision rises or holds on every set, held-out included;
app claimed stays 0. bun-dev-node and bun-held-node were under the 99.8% bar before and stay under it (their wrong
bytes are the existing ones). bun-dev's new wrong bytes (+0.4 KB bun, +7.6 KB node) come from 5 bundles, each
beside tens of KB of new right bytes; 6 other bundles lose wrong bytes (a claim that now holds its neighbour
keeps it from a wrong family). It recovers about 2.6-3.1 MB of bun-dev's 7.0 MB vetoed right bytes: the rest
has no MODULE fill to stand on either.

**This is not flipped here.** exp143 / exp144 measured the same key on the older corpora (since deleted from
this box) and it failed the strict bar there twice (apps-rollup P -0.040 from one 29 KB copied-parser seed;
remin3-esbuild +49 B app, a truth artifact); exp144's `shown` fixed the first and Andrew skipped the flip on
2026-10-24. On the corpora that exist today the key passes: precision up on all four Bun sets, held-out
included. Whether that reopens the flip is Andrew's call; a flip would also need the pipeline-side checks exp176
ran (names-mode placement, warm neutrality, the eval), which were not run here.

## What this could not prove

- **The fix.** None was built: see STATUS. The veto relaxations already measured (exp143 `all-rules`, exp144
  `shown`) remain Andrew's call; section 3 adds their Bun-corpus numbers.
- **bun-held was not diagnosed** (a held-out judge): totals of the scored variant only.
- **One stage per unit.** A unit two stages would each have dropped is charged only the later one; removing that
  stage alone may not claim it (the earlier stage's reading stands behind it only when its row is the charged
  one).
- **The verdicts are a ceiling.** As in exp188, a byte counted right is right if claimed as the census's (or the
  run's) package; growth and later guards of a run without the stage can change both sides.
- **The trail reads the final pass only** for presence and the fixpoint (the targeted round re-runs both); the
  lookup guards' rows cover both lookups.

## Files

- `crates/humanify-ag-core/src/libdetect/discovery.rs`: `SeedDrop`, `Discovery::seed_trail`, `note_step` (presence),
  `unsupported_picks`, `fixpoint_trail`, `SeedTrailRow` / `seed_trail_rows`, `IndexLookup::trail`;
  `unpack/bun.rs` writes `seedTrail` with the unit rows. Test in `discovery_test.rs`.
- `experiments/189-dropped-seeds/trail-run.py`: runs a working-tree binary over a corpus with exp096's own
  per-bundle runner into a diagnosis label (`exp189-trail-bun-dev`, raw reports under the corpus data dir).
- `attribute.py`: the attribution (JSON: per set, stage -> verdict -> bytes; top packages; examples).
- `run-variants.sh`: the scored `all-rules` labels (`flock /work/heavy.lock .../run-variants.sh <commit>`).
- `experiments/188-partial-claims/census.py`: `unclaimed_units` extracted (exp188's branch is merged into this one
  for it).
