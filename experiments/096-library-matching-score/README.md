# exp096: a library-matching score we can track over time

## STATUS (2026-10-14)

- **A second corpus: exp100's FROZEN test split (`corpus-test.json`, `--corpus test`).** It holds 55 new apps
  chosen by rule (none was in exp095's 360-app selection corpus or the 67 held-out apps), built with Bun,
  esbuild, webpack and rollup: 191 bundles in the sets `test-bun`, `test-esbuild`, `test-webpack` and
  `test-rollup`. This dev corpus (`corpus.json`) and every committed label are unchanged. A test label commits
  per-set aggregates only, and the leaderboard refuses to mix it with dev labels. Tuning lanes never run or read
  it. See `experiments/100-test-split/README.md`.
- **Scorer 3: precision / recall / F1 (2026-10-14, Andrew: "more standard").** Same bytes, same judge: precision =
  named ÷ (named + wrong family + app claimed), recall unchanged, F1 = 2PR ÷ (P + R). The wrong-family and
  wrong-name shares are gone; the bar is precision ≥ 99.8% (app claimed stays a hard 0). Every committed label was
  re-scored from its reports: cards are byte-identical to scorer 2's apart from the metric fields, and precision
  equals 1 − scorer 2's wrong-family share on every set that claimed anything (no set had app claims). A set
  that claimed nothing (wrangler on the ix-v2 labels) reads precision 0 and F1 0, the usual convention. Older numbers in this file and in
  docs/matching-architecture.md quote wrong-family shares: read precision as 100% minus that.
- **Scorer 2: the truth is ORIGIN-AWARE (branch `exp096-score-truth`).** A source map credits a vendored or
  pre-bundled copy to the package that SHIPS it (zod inside `@anthropic-ai/claude-agent-sdk`'s `sdk.mjs`), while the
  matcher names its ORIGIN. The truth now carries both: `origins.py` reads, from the frozen map's own paths and
  `sourcesContent`, a vendored directory (`third_party/`, `vendor/`, `vendored/`, ...) or a bundler module marker
  (esbuild `// node_modules/x/...`, rolldown `//#region`, webpack). A claim is right if it names the shipper OR the
  origin; `namedAsOrigin` and every old share (`*ShipperOnly`) stay on the card. Every label was re-scored from its
  raw reports: the shipper-only columns reproduce scorer 1 to the digit on all 9 labels × 4 sets, and the
  determinism check passes again (70/70).

  - **Hand check before trusting it** (`handcheck.json`, 78 distinct wrong-family claims from 100 byte-weighted
    draws over both `integ2` labels, each read against the shipped source): **49 copies, 29 real errors.** The new
    truth credits 44 of the 49 copies and **none of the 29 errors**. It misses 5 copies that leave no marker
    (openpgp's rollup bundle of tweetnacl, `@ai-sdk/provider-utils`' `src/to-json-schema/zod3-to-json-schema`,
    `@sentry/node-core`'s `proxy/` copy of agent-base, balanced-match republished as `@isaacs/balanced-match`).
    By bytes the sample is 97% copies (733 KB caught, 3 KB missed, 21 KB errors).
  - **Index evidence is NOT used** (measured): a whole-unit hash in the index under the claimed package covers only
    ~40 KB of the 573 KB the markers leave unexplained, it is the matcher's own evidence, and it cannot tell an
    origin from a consumer that bundled the shipper (delaunator named `d3-delaunay`, `string-width` named `boxen`).
  - **Wrong-family share, old -> new truth** (recall old -> new in brackets):

    | label                      | Claude Code | held-out apps                   | esbuild app                     | wrangler    |
    | -------------------------- | ----------- | ------------------------------- | ------------------------------- | ----------- |
    | `integ2-agreement`         | 0.0 -> 0.0% | 12.8 -> **7.0%** (11.8 -> 12.6) | 12.4 -> **3.8%** (18.4 -> 20.2) | 0 -> 0%     |
    | `integ2-structural`        | 0.0 -> 0.0% | 15.5 -> **4.9%** (18.1 -> 20.4) | 20.9 -> **8.3%** (29.1 -> 33.7) | 6.0 -> 6.0% |
    | `ix-v2-agreement`          | 0.0 -> 0.0% | 12.8 -> 7.0%                    | 13.5 -> 12.3%                   | floor       |
    | `ix-v2-structural`         | 0.0 -> 0.0% | 15.5 -> 4.9%                    | 13.5 -> 12.6%                   | floor       |
    | `ix-v2-ccexact-agreement`  | 0.2 -> 0.2% | 12.7 -> 7.0%                    | 13.6 -> 12.3%                   | floor       |
    | `ix-v2-ccexact-structural` | 0.0 -> 0.0% | 15.6 -> 5.2%                    | 13.5 -> 12.6%                   | floor       |

    `integ-*` equal `integ2-*`; Claude Code is unchanged by construction (its truth is the hand-labelled sample).
    **Still not at the ~0.2% bar.** What is left is mostly real: sponge index rows (prettier's bundled
    `index.mjs` named on @google/genai's own code, 158 KB structural; a `Runge-Kutta` package on cytoscape, 125 KB
    agreement), consumers named for their dependency (d3-delaunay, boxen, got, tinyglobby, axios, @inquirer/core),
    `content-type` on MCP SDK code, `@license` as a package name. Next lever: the sponge filter for unit rows
    (exp095 SYNTHESIS §5).

- **The esbuild-app "drop" was mostly the truth; the real loss is a STALE INDEX (`lazy_modules.py`).** Under the
  origin-aware truth, main's lazy-module unpack RAISED task-master-ai's structural recall 32.1% -> 33.7% and cut
  wrong family 12.6% -> 8.3%. Byte by byte (`ix-v2-structural` -> `integ2-structural`):
  - **gained 287 KB**: 210 KB of ajv / zod copies inside claude-agent-sdk's `sdk.mjs`, now split into lazy-body
    statements and named by their origin. Scorer 1 counted exactly these as wrong family: hence 13.5% -> 20.9%.
  - **lost 216 KB**: 169 KB had been named through ONE whole lazy-wrapper unit (`u$r=L(()=>{N6r="aws...",...})`)
    whose hash the index holds; the run now cuts that wrapper into lazy-body statements (`N6r="aws...",v6r=...`),
    and the index (built 2026-10-12, before main's 68b090e3 taught the unit model esbuild 0.28's guarded `__esm`)
    has only the wrapper (`d-file`/`d-absent` 117 KB, too small 29 KB). The rest is knock-on: hoisted functions
    whose neighbours went unnamed now hit the isolation veto (28 KB `e-veto`).
  - **Why a rebuild would not fix it today:** `UNITS_VERSION` is still `"2"` (`libindex/units.rs`), so the index
    builder's shard key (`vendor-content-key/4+units/2`) is unchanged and the store REUSES every stale esbuild
    shard. The unit model changed under a constant fingerprint version.
  - **Proposed fix (not made here: no crates/ change in this lane):** bump `UNITS_VERSION` whenever the statement
    unit model changes (68b090e3 did), and rebuild the esbuild slice. **Validated on 7 packages** (the top lost
    shippers: @opentelemetry/semantic-conventions, @aws-sdk/nested-clients, @smithy/core, marked, mime,
    @aws-sdk/core, @opentelemetry/api): rebuilt with main's `humanify-index` into a fresh store, the whole-unit
    hashes of their units in the CURRENT run went from 66 KB (17%) found in v2 to **347 KB (89%)** found in the
    rebuilt index. **Predicted effect:** task-master-ai structural recall +3.4 to +4.4 points (33.7% -> ~37-38%:
    the 169 KB wrapper-named bytes back, plus up to 47 KB of veto/size knock-on), wrong family unchanged; the
    agreement label similarly; wrangler (549 lazy modules) should gain too. Bun inputs are unaffected (their
    numbers did not move at the merge). The ~41 KB of lazy-body `d-*` bytes in other packages are lazy-in-app but
    eager-in-index (`var x=v` hoisted to `var x;` + `x=v`): a later binding-init normal form, not this fix.
- **Eval verb registered:** `npm run eval -- library <label> --index <dir> --rule agreement|structural` and
  `npm run eval -- library-leaderboard <labels>` (flags validated upfront, `test/eval-library-args.test.ts`; the
  runner's ambient `EXP096_PYTHON` / `EXP096_DATA` reads are gone, and `run.sh` joined the measurement-owners
  ratchet against `${VAR:-}` reads).

- **State.** DONE (instrument + first labels). Branch `exp096-library-matching-score` (from `exp095-match-misses`),
  not merged, not pushed. One Rust change: a `humanify discover` verb (commit `486b06b9`, gate ALL 12 STAGES
  PASSED). The pipeline's output is unchanged: the two new request fields default off.
- **Totals first.** The score is deterministic. Two runs of one configuration gave byte-identical reports, cards
  and summary (70 of 70), so **the noise band is zero and any movement between labels is real**. No label claims
  any app code. First labels (recall = share of library statement bytes named with the right package or its
  family):

  | label                      | index             | rule       | Claude Code 2.1.216 | held-out apps (67) | task-master-ai (esbuild) | wrangler | app code claimed |
  | -------------------------- | ----------------- | ---------- | ------------------- | ------------------ | ------------------------ | -------- | ---------------- |
  | `ix-v2-agreement`          | v2 `b00fa735`     | agreement  | **23.0%**           | **11.8%**          | 20.1%                    | 0%       | 0                |
  | `ix-v2-structural`         | v2                | structural | 37.1%               | 18.1%              | 31.7%                    | 0%       | 0                |
  | `ix-v2-ccexact-agreement`  | v2 + CC exact (C) | agreement  | 55.6%               | 12.1%              | 20.0%                    | 0%       | 0                |
  | `ix-v2-ccexact-structural` | v2 + CC exact (C) | structural | 81.7%               | 18.5%              | 31.7%                    | 0%       | 0                |

  - **Wrong names.** Claude Code is at 0.0% wrong family on both rules, and 0.2% with the merged index under
    agreement: one 9-byte sample unit, `@typespec/ts-http-runtime` named `@azure/identity`.
    The held-out apps are at 12.8% (agreement) and 15.5% (structural) of claimed library bytes. The esbuild app
    is at 13.5%. None of them meets the ~0.2% bar. exp095 read these by hand: almost all are vendored copies,
    which the source map credits to the package that ships the copy. The top ones are web-streams-polyfill inside
    @google/genai and zod inside claude-agent-sdk.
  - **The lane numbers are reproduced through the pipeline's own path.** Claude Code 37.1% / 23.0% and
    held-out 18.1% / 11.8% are the exp095 index-v2 lane's numbers. Each miss class matches the misses lane within
    a point. On Claude Code the structural run makes the same 2,808 claims as the lane's, with the same seed, grown and vetoed bytes.
  - **The real delta on the leaderboard:** adding index C's 3,238 builds (Claude Code's exact library versions,
    built with Bun `--target node` and `--target bun`) lifts Claude Code from 23.0% to 55.6% under agreement, and
    from 37.1% to 81.7% under structural. Not indexed falls from 33.8% to 0%, and hash differs falls from 18.2% to
    10.4%. **This is fitted, not held out.** Index C's package list was chosen from Claude Code's own readers and
    its labelled sample. The held-out apps barely move (+0.2 / +0.4 points), and their wrong-family share stays
    flat.
  - **Two gaps the score exposes that are not matching:**
    - **Detection.** The pipeline's detector calls 60 of the 67 held-out Bun bundles, and the esbuild app,
      something other than what built them. Bun writes no `// @bun` pragma for `--target node`. With no
      `--bundler` flag the pipeline would read no index slice for them. The runner passes the corpus's known
      bundler (the pipeline's `--bundler`), and each card records the detector's own verdict.
    - **Unpack.** On this branch, minified wrangler and 2 held-out apps hit the adapter's one-file floor, so no
      discovery runs at all. They score 0 with the miss class `no-discovery`. Main's 68b090e3 (finding #96)
      should lift wrangler. A label on a branch carrying that fix will show it.
  - **After merging main (2026-10-13, labels `integ-agreement` / `integ-structural`, commit `132ae635`, same
    index and corpus).** Claude Code and the held-out aggregate are unchanged to the printed digit; no label claims
    app code. Main's esbuild fixes (findings #96: the module helper by shape; the guarded `__esm` as an init
    helper) move exactly the esbuild-built inputs:

    | input                    | agreement      | structural     | why                                                                                       |
    | ------------------------ | -------------- | -------------- | ----------------------------------------------------------------------------------------- |
    | wrangler (re-minified)   | 0% -> **6.6%** | 0% -> **7.5%** | unpacks now (368 factories, 549 lazy modules); was the one-file floor                     |
    | task-master-ai (esbuild) | 20.1% -> 18.4% | 31.7% -> 29.1% | its 1,997 `__esm` modules are now lazy modules: 17,646 -> 23,995 statements, fewer claims |
    | held-out wrangler, vite  | +0.02 pt / 0   | +0.02 pt / 0   | one or two more runtime helpers by shape; same claims                                     |

    The detector's own verdict moves on 59 cards (the runner passes `--bundler`, so nothing scored follows it):
    54 held-out bundles `unknown` -> `bun`; wrangler, held-out wrangler, todometer and task-master-ai `unknown` ->
    `esbuild`; held-out renovate `bun` -> `unknown`. **Fixed by `fix/detect-bun-target-node` (merged, labels `integ2-*`, commit `5205b0f0`):** renovate's prologue holds Bun's own `__export` registrar (`configurable` + `set`) AND a later esbuild-shaped one; main read esbuild on ANY esbuild-shaped registrar, which contradicted Bun's shape -> `unknown`. The fix reads the FIRST registrar of the runtime prologue: the detection census is 108/108 (88 Bun, 20 esbuild: this corpus, the e2e fixtures, the 8 eval-pair Claude Code inputs). Every scored field of `integ2-*` equals `integ-*`. `manifest_sha` now hashes the corpus manifest's canonical spelling: the commit
    hook had re-spelled `corpus.json`, so the file's bytes no longer matched the labels' `corpusSha256`.

  - **CLOSED (see the top of STATUS: a truth artifact + a stale index): a matcher gap the unpack fix exposed.** Once main recognised task-master-ai's
    1,997 lazy esbuild (`__esm`) modules, its recall fell 31.7% -> 29.1% (structural; 20.1% -> 18.4% agreement)
    and its wrong-family share ROSE 13.5% -> 20.9% (structural). The unpack is more correct; the matching is not
    ready for it: discovery now sees lazy-module init-body statements (`LazyBody` units, 17,646 -> 23,995
    statements) that the index's units do not line up with. Investigate lazy-module statements vs index units
    before reading esbuild numbers.
  - **OPEN (config sweep):** `--library-discovery-units` is not one of the six operational flags
    (`docs/config-consolidation.md`); it must become a config setting in the next sweep step.
  - **Run cost:** 70 bundles in about 80 s at 4 in flight. Claude Code takes 11 s and 1.4 GB. Peak memory is
    2.4 GB. Most of a small bundle's time goes to loading the index (440 MB on disk).

## 1. What is scored (the card)

One card per (label, bundle), in `results/<label>/cards/<id>.json`. One `summary.json` per label sums each set
(held-out = every app's bytes together). Totals come first:

| field                                                     | meaning                                                                                                                                                                                                                                                                                                                                                                                          |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `recall`                                                  | library statement bytes claimed as the right package or a member of its family ÷ library statement bytes                                                                                                                                                                                                                                                                                         |
| `appClaimedBytes`                                         | bytes of APP code (mapped outside `node_modules`) claimed as any package. **The hard bar: must be 0.** `summary.bars.appBarPassed`                                                                                                                                                                                                                                                               |
| `precision`                                               | scorer 3: named ÷ (named + wrong-family + app claimed) bytes, judged against the **99.8% bar** (the ~0.2% wrong-name bar of exp095's scale lane). Claims on the app's own package (`self*`), unmapped and foreign bytes are not judged                                                                                                                                                           |
| `f1`                                                      | 2PR ÷ (P + R)                                                                                                                                                                                                                                                                                                                                                                                    |
| `wrongFamilyBytes`                                        | claimed library bytes named as ANOTHER family (precision's main false positive)                                                                                                                                                                                                                                                                                                                  |
| `missBytes`                                               | every library unit NOT named gets ONE cause. This is the misses lane's classifier (`decompose.classify`), imported unchanged: `a` not indexed, `b` version missing, `c` too small, `d-file`/`d-variant`/`d-absent`/`d-nofile` hash differs, `e-veto`/`e-agree`/`e-support`/`e-spec`/`e-ambig`/`e-other` a guard refused it, `f` indexed as another package, plus `no-discovery` and `unit-model` |
| `indexSeeds`                                              | the same precision / recall / F1 for index seeds alone (no fixpoint)                                                                                                                                                                                                                                                                                                                             |
| `namedAsShipperBytes` / `namedAsOriginBytes`, `copyBytes` | scorer 2: the named bytes split by HOW they are right (the shipping package's family, or only the origin's), and the library bytes that are a recognisable copy (have an origin). Sets also carry `selfOriginBytes`: the app's own pre-bundled copies named by their origin                                                                                                                      |
| `*ShipperOnly`                                            | `precision`, `recall`, `f1` under the OLD shipper-only truth (scorer 1, reproduced to the digit)                                                                                                                                                                                                                                                                                                 |
| `selfOther` (in `claimedBytes`)                           | the app's OWN package's code (e.g. vite's pre-bundled parse5) named as another package. Reported apart from app code, and not counted in the bar                                                                                                                                                                                                                                                 |
| `exploratory.names` (**EXPLORATORY**)                     | for units claimed through an exact whole-unit hash: how many, their bytes, the local bindings they declare, and the share of those bindings whose index rows hold the truth's exact version. See §5                                                                                                                                                                                              |
| `detected`, `discoveryRan`                                | the detector's own verdict beside the toolchain used, and whether discovery ran                                                                                                                                                                                                                                                                                                                  |
| `timing.json` (per label)                                 | wall seconds and peak memory per run (index load included), plus the index id and size (`pipeline.json`). This is NOT part of the deterministic cards                                                                                                                                                                                                                                            |

Claude Code byte figures are **stratified ratio estimates** from the labelled sample (strata sizes from the
negatives lane). Its counts are sample units: `appClaimedSampleUnits`, `claimedSampleUnits`.

## 2. The corpus (`corpus.json`, 70 bundles, content-addressed)

The inputs are NOT committed: Claude Code is not ours to publish, and the rest is 1.1 GB. They are frozen under
`/work/exp096-data/corpus/<id>/`, and `corpus.json` records each file's sha256 and origin. `run.sh` refuses a
corpus whose hashes moved (`corpus.py verify`). The ground truth is frozen alongside
(`/work/exp096-data/truth/<id>.truth.json.gz`) and its hash is recorded too. The installs it was read from may go
away.

| set           | bundles | bundler / build                                                                                                                                                                      | ground truth                                                                                                                                                                                                                                         |
| ------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cc`          | 1       | Claude Code 2.1.216, the `bun build --compile` binary's extracted entry (Bun `--target bun`)                                                                                         | the exp095 negatives lane's **620-statement stratified random sample** (seed 95, strata S0–S3, blind second pass 124/124 agreed). Library units carry their package, and their version when Claude Code's readers or the exact-version specs know it |
| `heldout`     | 67      | the exp095 held-out apps (no selection saw them): Bun 1.4.0 `--minify --target node` + source map                                                                                    | source map. A unit's label is the package holding the majority of its mapped bytes (`node_modules/<pkg>`; the app's own package = `<self>`; anything else = app). Version comes from the `package.json` beside the mapped file                       |
| `wrangler`    | 1       | wrangler's published `cli.js` re-minified with esbuild (exp095 eval)                                                                                                                 | the outer and inner maps composed. Version comes from the pnpm path                                                                                                                                                                                  |
| `esbuild-app` | 1       | **new**: held-out task-master-ai rebuilt with esbuild 0.28.2 `--bundle --minify --platform=node --format=esm --sourcemap` (the index's esbuild recipe, same install as its Bun twin) | source map. esbuild CHAINS packages' own maps, so `https://…` and `webpack://…` sources (entities, xterm: 223 KB) are labelled `<foreign>`: library code whose package the path cannot name, counted as neither app nor library                      |

**Limits of the Claude Code truth.** The sample has about 108 library units and about 511 app units, so the
recall estimate is wide (exp095 bootstrap: e.g. 37.1% (26–50)). "0 app claimed" on Claude Code means 0 of the
~511 sampled app units. That is a sample bar, not a proof over the 13 MB of app code. The 1,647 CommonJS factory
modules (5.05 MB) are outside the sampled population, as they are outside discovery's statement scope. Forks (the
ink fork, the yoga port, a commander-modelled formatter) are labelled app.

## 3. Determinism and the noise band

There is no LLM and no sampling in discovery. `determinism.py ix-v2-agreement ix-v2-agreement-repeat` compared
two full runs of one configuration (same binary, index, rule and corpus): **70/70 raw reports, 70/70 cards and the
summary are byte-identical.** `bands.json` records this: band 0, so every delta is real. One exception: the
`timing.json` wall clock is not deterministic, and it is kept out of the cards. Rule 11's lesson still applies to
a CHANGE: a delta can come from the denominator moving, e.g. a unit-model change that re-cuts statements.
`libraryBytes` is on every card, so check it before reading recall.

## 4. What this score does NOT look at (rule 8)

- **Bundled CommonJS modules (vendor/ files).** Only statement units are scored, i.e. the code the bundler hoisted
  out of module boundaries. A Factory unit is "external" to discovery, and whole-module rows (`modules.*.jsonl`,
  vendor_content_key) are not scored. On Claude Code that is 5.05 MB, outside the 15.3 MB scored.
- **Names.** Nothing is renamed. The names column is a proxy (§5).
- **Detection.** The runner passes the corpus's known bundler. A detector regression shows only in the
  `bundlerDetected` count, never in recall.
- **Versions.** "Right" means the right package (or family). A claim naming the right package from the wrong
  version range is not judged.
- **What the truth cannot see.** A copy is seen only through a vendored directory or a bundler's module marker
  (`origins.py`). A copy with neither (rollup output, a copy under the shipper's own `src/`, a republished fork)
  is still credited to the shipper alone: 5 of 49 copies in the hand check. Pre-bundled copies inside the app's own
  package (`selfOther`) are reported apart. Chained-map sources are `<foreign>`. Bun's own map misattributions are noted in exp095 misses §2.1.
- **The corpus is near best case for the index's recipe.** The held-out apps were built with Bun `--target node`,
  exactly like the index, and the esbuild app was built with the index's esbuild flags. Only Claude Code
  (`--target bun`) and wrangler (re-minified) differ from the recipe. Claude Code is one app of one vendor.
- **Index C is fitted to Claude Code** (its readers and its sample chose the packages). The `ccexact` labels'
  Claude Code recall is not held-out evidence.
- **Not done here:** class-d sub-classes (the misses lane's `diffd.py` rebuilds index counterparts), and bootstrap
  intervals.
- **The `e-agree` attribution** under the agreement rule uses the misses lane's Python emulation of the structural
  rule, which reproduced in-run seeds 59/59 and 235/235. It does not use a second structural run.

## 5. The exploratory names column (for a future "names from the index" step)

For every claim that is right (or same family) and whose WHOLE unit hash is in the index under that family:
`exactUnits`, `exactBytes`, `bindings` (the symbols declared in those units), and `bindingsSameVersion` (bindings
in units whose matching index rows include the truth's exact version). The index builds that version's npm source,
so its source names would be the author's names, apart from a bundler's collision renames. The index stores no
names today, so **no name is compared**. These are upper bounds on what an index carrying names could supply.

| label                     | set      | exact units | bindings | same-version share |
| ------------------------- | -------- | ----------- | -------- | ------------------ |
| `ix-v2-agreement`         | held-out | 6,453       | 65,037   | 97.7%              |
| `ix-v2-agreement`         | CC       | 526         | 4,662    | 91.7% (sample)     |
| `ix-v2-ccexact-agreement` | CC       | 1,639       | 15,025   | 98.0% (sample)     |

## 6. How to run

```bash
# one label = one (commit, index, rule); builds target/release/humanify from HEAD (crates/ must be clean)
experiments/096-library-matching-score/run.sh <label> --index <index dir> --rule agreement|structural \
    [--commit <ref>] [--jobs 4] [--cpus 0-15] [--force-mixed]
P=/work/exp095-library-fingerprints/scale/.venv/bin/python   # numpy/scipy for exp095's family reader
$P -I experiments/096-library-matching-score/leaderboard.py ix-v2-agreement <label>...
$P -I experiments/096-library-matching-score/determinism.py <label> <label-repeat>   # re-prove band 0
$P -I experiments/096-library-matching-score/corpus.py verify
```

The instrument is `humanify discover <bundle> --library-index <dir> --rule … [--bundler …] --unit-prints --out …`.
It takes the same adapter path a `--diagnostics --library-index` pipeline run takes, stopped before naming.
`run.sh` refuses: a dirty `crates/`, a moved corpus, a label that already holds another binary / index / rule /
corpus (`--force-mixed`), and an unreadable index. The leaderboard refuses labels on different corpus manifests or
scorer versions.

**Registered:** `npm run eval -- library <label> ...` runs `run.sh`, `npm run eval -- library-leaderboard <labels>`
runs `leaderboard.py` (`scripts/eval.ts`, with what each proves and cannot prove). The origin layer is frozen once:
`corpus.py origins` (writes `<DATA>/truth/<id>.origins.json.gz` and `origins.json`; `verify` checks both).

## 7. Files

- `corpus.json`: the manifest (committed). `corpus.py`: freeze/verify. `lib096.py`: shared paths, hashing and
  JSON spelling. It imports exp095 misses' `truth.py`, `mmlib.py` and `decompose.py` unchanged.
- `run.sh` / `run.py`: build, run, score. `score.py`: the cards. `leaderboard.py`, `determinism.py`,
  `bands.json`.
- `merge_index.py`: a lab union of two index directories (how `v2+ccexact` was made: v2 + the negatives lane's
  index C, `/work/exp096-data/indexes/`).
- `origins.py`, `origins.json`: the origin layer (scorer 2) and its frozen shas. `handcheck.json`: the hand check's
  verdicts (names and numbers only), drawn by `handcheck_draw.py`. `lazy_modules.py`: the esbuild-app lazy-module
  decomposition and the rebuilt-index coverage check.
- `results/<label>/`: `pipeline.json` (binary sha256, build commit, index id and size, rule, corpus sha),
  `summary.json`, `timing.json`, `cards/`.
- Bulk, not committed: `/work/exp096-data/` (corpus, truth, raw reports, index cache, the merged index).
