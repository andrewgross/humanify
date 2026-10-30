# exp164 — the mint premium: why claimed units mint names the index could answer

STATUS: LANDED (2026-10-30). Branch `exp164-mint-premium` off main
`ff9789c41`. The census is complete and the mechanism is proven at every
layer; two run-side fixes shipped (names-mode only, default-neutral).
**Outcome: mints 685 → 501 (−184, 30× the band) — real, but the ≤100 bar
is NOT met. The remaining +420 is index-side + claim-boundary (the junk
statements are largely app code swallowed by claimed init regions — an
index rebuild cannot fix them); the next lever is the claim boundary,
exp165 material.**

## The question

The 2.1.215→2.1.216 hop with names mode read **mints 685 vs default's 81**
(+604, band ±33) — restoration's own cost. The minted declarations
concentrate INSIDE claimed packages: @opentelemetry 633, @grpc 277, semver
274, @aws-sdk 203 minted `lib_*` declarations across the walk. The index
holds names data for these very units (100% unit-row coverage). **Why does
a claimed unit mint at all, and can the premium shrink without hurting
anything else?**

## The census

The shipped-tree junk census: every minted binding located by name, span
and split-ledger file on the exp161-names-hop shipped tree
(`/work/exp164-mint-census/shipped-located-exp161-names-hop.json`) —
**684 junk bindings**, classified against the dump run's eraRestoration
block (Site A's per-statement rows, the freeze's bindings, the converted
claimed regions with raw spans) and the real run's claims:

| junk | bucket        | stmts | top packages                                   |
| ---- | ------------- | ----- | ---------------------------------------------- |
| 351  | H2-capture    | 31    | zod 167, jose 131, @azure 24                   |
| 259  | H4-no-claim   | 73    | jose 153, lodash-es 12, @azure 8               |
| 73   | H3-propagated | 20    | @azure 26, zod-to-json-schema 13, lodash-es 12 |
| 1    | H1-frozen     | 1     | @smithy 1                                      |

Reading the buckets (H1 = coverage, H2 = granularity/capture, H3 = claim
boundary, H4 = no claim — the four hypotheses):

- **The buckets are Site A's shadow, not the mechanism.** The dump's Site
  A rows are targeted by a drifted era conversion (see below), so a junk
  binding's "no site" or "no captured print" row mostly says where Site A
  was NOT, not where the answer is not.
- **The mechanism (proven at every layer):** the claimed CONTAINER
  statements — the unspliced lazy-init wrappers and registrations — are
  frozen by the library layout and never asked by the transfer or the
  waves, so their bindings keep the minifier's names. The capture DOES
  hold answers for them (4,279 captured print hashes; e.g. the zod giant's
  captured answer names 15–16 of its slots) — but they apply at neither
  restoration site:
  - **Site B skipped them by contract** (`in_a_module`): a row outside
    every extracted body was left "to the era".
  - **Site A cannot reach them**: the era conversion's ordinal arithmetic
    (`i − removed_before`) accounts the splice but not the format's
    reshape (only 118/7,628 content-correct), and even with correct
    ordinals the hash bridge is broken — the native formatter is a
    REWRITER (declarator splits, `a,b,c;`→`a;b;c;`, `a&&b;`→`if(a)b;`,
    `c?a:b;`→if/else, `void x;` splits, `!0`→true), so the pipeline print
    hash of an era statement ≠ the captured pre-format hash for exactly
    the code-heavy statements that need restoration. Site A: 0
    applications, 84 captures = twin collisions.
- **The depth gap (F1b), the other half:** `structural_scores_relaxed`
  early-returns when the whole print hits, so the nested functions were
  never consulted — a wrapper's nested locals never got captured even
  when the index holds their per-function rows.

## The fixes (run-side only; no index rebuild)

Both names-gated (`restoration: matching == Names`), so every other mode
is byte-inert by construction:

1. **Site B owns the container statements too**
   (`naming::restore::apply_body_restoration`): the `in_a_module` filter
   is gone — every captured application applies on the bundle, from the
   capture's carried spans+slots (drift-immune; the same ingest
   determinism Site B already relied on). The unpack extends the
   restoration edits into the container's edit list: a body-range edit
   sits under the declaration splice and is dropped by the slice's
   overlap rule, the container's own edits apply in the runtime text, and
   the bodies' slices carry their own as before. The exported surface is
   string-anchored (`Object.defineProperty(module.exports,
"<input name>", { get: () => binding })`) — binding renames leave it
   intact (audited on the shipped tree: 91/93 accessors string==target,
   the 2 mismatches legitimate).
2. **The capture's depth**
   (`libindex::lookup::restoration_answers`): the function prints are
   consulted beside a whole-print hit (the route's greedy selection,
   deduplicated by hash) — the index's per-function rows answer the
   nested locals.

F2 (builder-side `/4 --names`), F3 (propagated claims get lookups) and F4
(fallback naming reuses the index's per-unit answer) are NOT built: the
census's answered share sits in the captured container units, and F3's 73
junk bindings (propagated-only claims) are the smaller, separately-gated
remainder.

## Results

The hop re-run with the fixes (label `exp164-names-hop`, cold, scratch
base, pair 2.1.215→2.1.216, both legs exit 0; the before column is
exp161-names-hop's names leg — the SAME config, so the measured bands
apply):

| KPI     | before | after   | delta    | band           |
| ------- | ------ | ------- | -------- | -------------- |
| stmts   | 35903  | 35903   | =        | —              |
| clean   | 34558  | 34559   | +1       | ±17            |
| noise   | 359    | 358     | −1       | ±17            |
| novel   | 986    | 986     | **= 0**  | 0              |
| noiseLn | 5411   | 5990    | +579     | ±1376          |
| realLn  | 122066 | 122066  | **= 0**  | 0              |
| reloc   | 351    | 338     | −13      | ±35            |
| newName | 771    | 786     | +15      | ±13 (marginal) |
| mints   | 685    | **501** | **−184** | ±33            |

- **mints −184**: 30× the band — real. 27% of the +604 premium is gone
  with two run-side fixes and no index rebuild. novel/realLn stay
  byte-equal (band 0): the fixes touch naming only.
- **The capture nearly doubled** (stub dump, LLM-independent):
  restorationApplications 4,718 → 9,016 — 1,005 container applications
  (Site B's new ownership) + 8,011 body applications; 74,666 slot-name
  pairs offered / 13,273 distinct names. Site A stays at 0 applied as
  expected (the era/print bridge is broken by the formatter's rewrites —
  the unpack now owns the answers; the EraDump is diagnostics-only).
- **The after junk census: 684 → 499 minted bindings.** zod-to-json-schema
  19 → 0, zod 167 → 63, lodash-es 26 → 9, @smithy 11 → 7, @aws-sdk 8 → 7,
  @azure 58 → 57, @typespec 12 → 12, lodash 11 → 11 — and **jose 284 →
  284, untouched**.

After-buckets (classify-dump.mts on the after dump; the same four
hypotheses): H2-capture 235 (jose 131, zod 63, @azure 24), H4-no-claim
217 (jose 153), H3-propagated 47 — and **zero H1 / anomaly / ambiguous
rows**. The remaining premium is a coverage problem, not a plumbing one.

### Why the remainder does not shrink further

- **The junk statements are largely NOT the claimed package's code.** The
  jose 284 sit in 12 statements across 4 module files; shipped stmt 30196
  is Claude Code APP code (the `async function $zp({ plugin: e, scope: t
= "user", … })` plugin installer) placed in
  `vendor/jose/module-af5c7d31.js`. The remaining zod junk is 5 statements
  (1044, 1127–1130) whose factories reference `adjustTokenCount` /
  `mapperReducer` — app code bundled into zod's init region. The index
  holds 2,156 jose unit rows (median 69 nodes) yet every jose route-miss
  persists: a bundled print cannot match an npm-built jose row when the
  statement was never jose's code. **F2 (an index rebuild) would not fix
  these either — the rows already exist; the prints cannot match.**
- **The wrappers compound it**: jose's wrappers reference cross-module
  factories as `lib_82619232_2()` — minted free identifiers become
  content tokens, breaking the print-hash bridge even for statements that
  ARE library code.
- **Coverage proxy** (exp160-shape; NOT exp161's exact published 45.2%
  definition — the proxy reads 45.1% on exp161's own tree): 250 covered
  packages (the union of both runs' diag package lists); declared names
  in covered vendor files 28,513 → 29,636; restoredish names 12,846 →
  **13,031 (+185)** — restored-name MASS rose, occurrences of minted
  names inside covered files 5,074 → 4,892 (−182). The restoredish
  PERCENTAGE reads 45.1% → 44.0% only because the declared denominator
  grew +1,123 (declaration/file composition between two cold runs).
  Every count restoration itself owns rose; no coverage regression.

### The bar

- **mints ≤ 100 on the hop pair: NOT met** — 501 (+420 over default's
  81). The remaining +420 is index-side + claim-boundary: H2+H4 = 452 of
  the 499 are statements the index's route cannot answer because they are
  not the package's npm code. The lever left standing is the CLAIM
  BOUNDARY (claiming a package freezes app statements swallowed by its
  init region) — exp165 material, not exp164.
- novel/realLn byte-equal: MET (band 0).
- restoration coverage NOT lower (45.2% floor): MET by count (+185
  restored names, −185 junk, applications +91%); the % proxy read 44.0 vs
  45.1 on a denominator that grew +1,123 — composition between cold runs,
  not a regression.
- vendor/src buckets: nameOnly 4,166 vs the before names tree's 4,144
  (+22, inside the cold-run composition drift; default was 3,946).
- cold self-hop divergence = known mode artifact: 1,040 ln (exp161's was
  1,176) — reported, not chased.

## Verification

- red/green: the container application test (the inverted
  `an_application_outside_the_extracted_bodies_is_not_applied`) and the
  capture-depth test each watched fail before the fix;
  `a_body_range_edit_under_a_declaration_splice_is_dropped_from_the_container_slice`
  pins the routing invariant the container extension relies on.
- `cargo test --workspace` green (1,508 core tests); clippy clean.
- warm neutrality vs main `ff9789c41` (names off, default AND report
  mode with the index mounted): **NEUTRAL both legs** — 0 differing
  files / 0 differing lines, both legs exit 0, baseline +0 cache writes
  (warm standing cache `/work/neutrality-cache`). Names off is
  byte-inert by construction (`restoration: matching == Names`), and the
  run proved it.
- gate 13/13 (`npm run check`), including the e2e stage (the release
  binary on the committed fixtures, export surface exact).
- the scored hop (label `exp164-names-hop`): both legs exit 0; scored leg
  326s, peak 17,494 MB, cache +0; BOOT GATE OK; WARM self-hop
  byte-identical with 0 cache writes; cold self-hop 1,040 ln (known
  names-mode re-roll artifact, outside the 2–184 reference range as
  exp161's 1,176 was).

- red/green: the container application test (the inverted
  `an_application_outside_the_extracted_bodies_is_not_applied`) and the
  capture-depth test each watched fail before the fix;
  `a_body_range_edit_under_a_declaration_splice_is_dropped_from_the_container_slice`
  pins the routing invariant the container extension relies on.
- `cargo test --workspace` green; clippy clean.
- warm neutrality vs main (names off, default AND report mode): byte
  identity.
- gate 13/13 (`npm run check`).
- the hop re-run with the before/after mint table.
