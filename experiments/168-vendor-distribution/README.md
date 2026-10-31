# exp168 — where vendor code goes, how it is named, what its churn is made of

## STATUS (2026-10-31)

- **Measurement only.** Read-only over existing eval trees; no pipeline or eval
  was run. Branch `exp168-vendor-distribution`.
- **Covered:** the 2.1.215→2.1.216 hop of `exp161-names-hop-default` (names OFF,
  the baseline), `exp161-names-hop` (names ON, before exp164) and
  `exp164-names-hop` (names ON, current main), with the three same-commit
  references `ref-scratch-0f338ffa-r1/r2/r3` scored by the same script for
  bands.
- **Generator:** `vendor_distribution.ts` (one script, one workdir → all
  tables; `--compare` builds the comparison). Re-run it on exp166's hop when
  that lands:
  `npx tsx experiments/168-vendor-distribution/vendor_distribution.ts /tmp/eval-work/exp166-names-hop --json out.json`.
- **Per-run outputs:** `results/*-2.1.216.md`; the comparison:
  `results/compare-2.1.216.md`.
- Harness additions (card-inert): `lib/diff.ts` `changedLineTexts` (the
  changed lines' text from the one owner) and `vendor-churn.ts` `fileRows`
  (the scorer's per-file rows). Both tested; neither changes a card field.

## Run validity

| run                         | exit / errors                                    | warm self-hop                      | cards                                                                                                                                  |
| --------------------------- | ------------------------------------------------ | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| exp164-names-hop            | exit 0, 0 errors (`run-status.json`)             | identical, 0 files, 0 cache writes | `experiments/034-eval-harness/results/exp164-names-hop/`                                                                               |
| exp161-names-hop            | exit 0 per `experiments/161-names-hop/README.md` | identical per that README          | **gone** — written to `/work/exp161-hop/…/results/`, a worktree since removed. Trees + both self-hop trees still in `/tmp/eval-work/`. |
| exp161-names-hop-default    | same                                             | same                               | same                                                                                                                                   |
| ref-scratch-0f338ffa-r1/2/3 | exit 0                                           | —                                  | committed                                                                                                                              |

The exp161 vendor numbers below are therefore re-derived from the trees with
the eval's own scorer. That scorer was checked first against cards that do
exist: it reproduces exp164's `vendor` block exactly (761 / noise 255 / real
506 / manifest 183 / nameOnly 72) and all three refs' 2.1.216 blocks (1,917 /
1,879 / 1,917 churn; 1,771 / 1,739 / 1,773 noise).

## How each mechanism is read (and the checks that it can fire)

- **Vendor factories** are the bundle's CommonJS modules (`vendor/_bun-modules.json`).
  _Inherited from the prior by hash_ = the name ladder's first rung
  (`modules::name_cjs_factories`, carry-over by structural hash): reconstructed as
  "the prior record has the same (structural hash, ordinal) at the same path".
  It reads **1,595 on all three runs**; exp164's run log prints `1595
carry-over, … 22 fallback, 29 app text asset, 1 index` — exact. The label
  the record keeps on a carry is the ORIGINAL rung's (index / model / declared
  / fallback), which is how the "origin" column below is read.
- _Index claim vs unclaimed-but-clean (exp157)_: the planner
  (`place::library_layout`, run by `unpack::bun::place_factories_from_index`)
  moves a factory on a single-package claim, or, with no claim, on a clean
  index owner+file answer. The claim is re-folded here from the run's
  `libraryDiscovery.claims` over the factory's bundle span. The bundle span
  needs the record entry paired to the raw factory: **the record's order is
  NOT the bundle's** (it drifts from entry ~6; order agreed for 1,417/1,618),
  so entries are paired by a rename- and relink-invariant shape of the
  factory function (1,578/1,618 paired in names mode, 1,614/1,618 default; the
  40 unpaired names-mode files are listed as "claim unknown").
  For CARRIED factories the decision was taken on 2.1.215 (no diagnostics
  were written for the rebase), so the claimed/unclaimed split of carried
  factories is read with 2.1.216's claims — a proxy, labelled as such.
- **Era (app) statements in vendor/** come from the placement trail
  (`placementTrails`, `place::tiers`): tier `library-index` = the planner's
  move of a claimed module region; any `fossil:*` tier = the statement went
  where its fossil match put it (the jose class, exp165). Region claims
  (claimed vs exp157-unclaimed) are in `eraRestoration.regions`, exp164 only.
- **Naming, factory bodies:** every binding of the emitted factory function
  is zipped (source order) against the same binding in the raw bundle's
  factory. Same name = left as the bundler wrote it. If the file's bytes equal
  the prior's file at the same path (`finish::vendor_inherit`), the name was
  carried: equal to the prior bundle's raw name = still a minifier name,
  otherwise the prior run's name; if this run's restoration
  (`libraryDiscovery.restorationApplications`) answered that factory, it is
  counted as restored.
- **Naming, app bindings:** the strategy trail's terminal step, mapped to a
  file through `eraRestoration.bodySpans` (exp164; 185,860 / 199,916 mapped
  trails find their final name in the file they map to — a wrong
  coordinate system reads ~1, measured while building this). exp161's
  diagnostics predate `bodySpans`: its names-mode app bindings cannot be split
  src vs vendor (reported as "all files"); in default mode no app statement
  is in vendor/, so every binding is src.
- **Mint-bearing churn lines:** a changed vendor line, in a file holding app
  statements, that contains a name the file declares AND the run's minted
  census lists. Factory bodies are outside the census by design.

## 1. Where code goes — 2.1.216 tree (exp164, names ON, current main)

Totals first (`.humanify/` excluded; lines = `wc -l`):

| area    | files | lines   | bytes      | share of bytes |
| ------- | ----- | ------- | ---------- | -------------- |
| TOTAL   | 6,503 | 903,201 | 48,165,430 | 100%           |
| src/    | 4,845 | 874,152 | 41,726,511 | 86.6%          |
| vendor/ | 1,654 | 24,054  | 6,172,122  | 12.8%          |
| other   | 4     | 4,995   | 266,797    | 0.6%           |

Vendor files are one to a few lines of minified text, so **bytes are the
unit** for vendor; the vendor record alone is 13,659 of the 24,054 lines.

vendor/ by placing mechanism (exclusive, sums to the vendor total):

| mechanism                                                   | files | bytes     | share |
| ----------------------------------------------------------- | ----- | --------- | ----- |
| TOTAL                                                       | 1,654 | 6,172,122 | 100%  |
| inherited from the prior by hash (factory carry-over)       | 1,595 | 5,581,706 | 90.4% |
| origin: index, claimed (by 2.1.216's claims)                | 797   | 3,699,126 | 59.9% |
| origin: index, unclaimed — the exp157 rule                  | 546   | 1,066,210 | 17.3% |
| origin: model-named                                         | 209   | 517,882   | 8.4%  |
| origin: index, claim unknown (factory not paired)           | 39    | 278,100   | 4.5%  |
| origin: declared identity / lib\_\* fallback                | 4     | 20,388    | 0.3%  |
| index claim moved by package (this run)                     | 18    | 22,353    | 0.4%  |
| app regions, library-index tier, every unit claimed         | 17    | 20,387    |       |
| whole factory                                               | 1     | 1,966     |       |
| unclaimed-but-clean moved on owner+file (exp157), this run  | 0     | 0         | 0%    |
| whole-module factory, named this run (model)                | 22    | 20,508    | 0.3%  |
| root `lib_*` fallback, this run                             | 0     | 0         | 0%    |
| app statements filed into vendor/ by placement (jose class) | 18    | 105,255   | 1.7%  |
| the vendor record (`_bun-modules.json`)                     | 1     | 442,300   | 7.2%  |
| other                                                       | 0     | 0         | 0%    |

The 18 jose-class files hold 51 of the 103 app statements in vendor/, placed
by fossil tiers (`fossil:unique-signature` mostly, path carried from the
prior by hash); they are jose (4 files, 2,072 lines) and zod (3 files, 691
lines) chiefly — the exp164/165 junk.

Per package (top by bytes; full top-25 + long tail in `results/`):
highlight.js 18.4%, the vendor root 7.2% (the record), @grpc/grpc-js 6.1%,
node-forge 5.8%, @opentelemetry/otlp-transformer 4.0%, @mixmark-io/domino
3.6%, @opentelemetry/sdk-metrics 3.0%, acorn 2.5%, react 2.2%,
google-auth-library 2.1% … long tail 300 packages, 24.8%.

**Vendor-root loose files:** names mode 3 (the record + two carried `lib_*`
fallbacks, 930 bytes). Default mode: **594 loose files, 2.13 MB, 40.6% of
vendor bytes** — every module the model named sits flat at the vendor root.

## 2. How identifiers are named — exp164 (names ON)

vendor/ factory bodies (53,160 bindings):

| name source                                                          | bindings | share |
| -------------------------------------------------------------------- | -------- | ----- |
| TOTAL                                                                | 53,160   | 100%  |
| restored from the library index (answered this run; bytes inherited) | 27,572   | 51.9% |
| restored from the library index (this run only)                      | 110      | 0.2%  |
| left as bundled (this bundle's minifier name)                        | 11,084   | 20.9% |
| carried from the prior, still the prior bundle's minifier name       | 10,602   | 19.9% |
| factory not paired to the raw bundle (unattributed)                  | 3,792    | 7.1%  |
| model-named / close match                                            | 0        | 0%    |

So in names mode **52% of vendor-body bindings carry an index name, 41% are
still minified** (half this bundle's, half the prior's that the file
inheritance kept). Nothing in a factory body is model-named or close-matched:
those steps do not run there. In default mode 99% are minified (34,742 as
bundled + 17,949 the prior's), 0 restored.

vendor/ app statements (496 bindings): carried by exact match 339 (68%),
left minified 89 (18%), post-split reconcile 42 (8%), model 26 (5%).

src/ for contrast (200,798 bindings): exact match 91.5%, close match 4.3%,
model 2.5%, library-import rule 0.7%, left minified 0.6%, reconcile 0.3%.
Left-minified is **0.6% in src/ vs 18% for app code inside vendor/ vs 41% for
vendor bodies**.

## 3. vendor/ churn, 2.1.215→2.1.216 — exp164

| bucket (exclusive; the eval's vendor scorer) | lines | of which mint-bearing | per-pair band (3 refs) |
| -------------------------------------------- | ----- | --------------------- | ---------------------- |
| TOTAL churn                                  | 761   | 122                   | ±38                    |
| real: truly added                            | 472   | 98                    | ±0                     |
| real: changed in place / paired              | 34    | 8                     | ±6                     |
| real: truly removed                          | 0     | 0                     | ±0                     |
| manifest                                     | 183   | 0                     | ±2                     |
| name re-rolls (nameOnly + freeReroll)        | 72    | 16                    | ±32                    |
| moved / relocated (path draws)               | 0     | 0                     | ±2                     |
| other (unparsed)                             | 0     | 0                     | ±0                     |
| = vendorReal                                 | 506   |                       | ±6 (4-pair: ±32)       |
| = vendorLn (noise)                           | 255   |                       | ±34 (4-pair: ±55)      |

Mint-bearing lines are a cut ACROSS buckets (they overlap them; carve them
out and real falls to 400, re-rolls to 56).

**352 of the 472 "truly added" lines are not new library code.** They are 10
new `vendor/<pkg>/module-*.js` files holding app statements the index placed
this hop (31 statements, all tier `library-index`; 11 of them had a prior
file in `src/`). The vendor scorer looks only at vendor/, so a src→vendor
move reads as real dependency change here and as a removal on the src side
(the exp164 card's `renamedPairs` show the same files as `src/… => vendor/…`).
The other 120 lines are 23 genuinely new ajv factory files — identical to
default mode.

Path stability: 1,612 files kept their path byte-identical, 5 kept the path
and changed (34 ln), 3 kept the path name-only (72 ln), 33 new (472 ln), 0
moved, 0 removed. Identifier stability for identical code: factory files
57,727 / 57,727 bindings same name; app files in vendor 728 same / 12
different.

## 4. Comparison across the three runs (2.1.216; Δ vs names OFF)

Band = max−min of the same number across the three same-commit references
(default mode, commit 0f338ffa: it measures run-to-run draw, not the effect of
a mode change). `~0 (±b)` = inside it. Full table: `results/compare-2.1.216.md`.

| metric                                   | names OFF | exp161 names | exp164 names | Δ exp161 | Δ exp164 | band   |
| ---------------------------------------- | --------- | ------------ | ------------ | -------- | -------- | ------ |
| vendor/ bytes                            | 5,261,327 | 6,160,211    | 6,172,122    | +898,884 | +910,795 | ±2,591 |
| vendor/ lines (src/ moves the opposite)  | 20,468    | 24,054       | 24,054       | +3,586   | +3,586   | ±0     |
| carry-over factories (files)             | 1,595     | 1,595        | 1,595        | ~0 (±1)  | ~0 (±1)  | ±1     |
| app statements filed into vendor (bytes) | 0         | 102,284      | 105,255      | +102,284 | +105,255 | ±0     |
| index claim moved this run (bytes)       | 0         | 21,545       | 22,353       | +21,545  | +22,353  | ±0     |
| body names restored from index           | 0         | 27,328       | 27,682       | +27,328  | +27,682  | ±0     |
| body names left minified (both kinds)    | 52,691    | 22,040       | 21,686       | −30,651  | −31,005  | ±300   |
| app names left minified, all files       | 1,120     | 1,325        | 1,391        | +205     | +271     | ±22    |
| app names carried by exact match         | 186,618   | 186,166      | 186,142      | −452     | −476     | ±3     |
| app names model-named                    | 4,992     | 5,027        | 5,080        | +35      | +88      | ±5     |
| vendor churn                             | 303       | 699          | 761          | +396     | +458     | ±38    |
| vendorReal                               | 120       | 502          | 506          | +382     | +386     | ±6     |
| vendorLn (noise)                         | 183       | 197          | 255          | ~0 (±34) | +72      | ±34    |
| manifest                                 | 183       | 183          | 183          | ~0 (±2)  | ~0 (±2)  | ±2     |
| name re-rolls                            | 0         | 14           | 72           | ~0 (±32) | +72      | ±32    |
| moved / relocated                        | 0         | 0            | 0            | ~0 (±2)  | ~0 (±2)  | ±2     |
| truly added                              | 120       | 472          | 472          | +352     | +352     | ±0     |
| mint-bearing churn lines                 | 0         | 148          | 122          | +148     | +122     | ±0     |
| files kept path byte-identical           | 1,595     | 1,614        | 1,612        | +19      | +17      | ±9     |

Read: names mode moves ~0.9 MB more into vendor/ (restored names are longer,
and 103 app statements move there), restores 52% of vendor-body bindings,
and costs +386 vendorReal on this pair — of which 352 are the src→vendor
moves above, not dependency change. exp164 vs exp161: vendor re-rolls +58
(14 → 72) is outside the ±32 band; mint-bearing lines 148 → 122.

## What this does NOT look at (rule 8)

- **The vendor scorer never looks at src/.** Code that crosses the boundary
  is booked as real change on one side and removal on the other — 352 of
  exp164's 506 vendorReal lines. Neither surface pairs them.
- **Decisions taken on the base (2.1.215) are not observed:** the rebase
  wrote no diagnostics. 1,595/1,618 factories were carried at 2.1.216, so the
  claimed-vs-exp157 split for them is 2.1.216's claims standing in for
  2.1.215's.
- **40 names-mode factory files (3,792 bindings, 278 KB) are unpaired** to
  the raw bundle by shape (4 in default mode) and go unattributed.
- **Restoration in factory bodies is counted per FILE answered,** not per
  binding answered: a renamed binding in an inherited file the index also
  answered counts as restored even if the prior's restoration named it.
  The run log's "47,462 identifiers restored" counts occurrences, a different
  unit from bindings.
- **Mint-bearing lines** use the fresh run's census on both sides (the base
  has no census), and only in files holding app statements.
- **exp161 app bindings** cannot be split src vs vendor (no `bodySpans`).
- **One pair (215→216).** The bands are per-pair spreads of three default-mode
  repeats; names mode has no measured band of its own.
- The vendor record is 7% of vendor bytes and 57% of vendor lines; its churn
  (183) is identical in every run and is counted as noise.
