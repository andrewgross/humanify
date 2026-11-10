# exp211 — choosing between packages that share a key in library placement

STATUS: IMPLEMENTED, GATED, EVAL SCORED, NOT MERGED (2026-11-10). Branch
`exp211-shared-key-choice`, rebased on main `acdc6b0db`. Code commits:
`73d22527f` (the shared-key choice) and `ec789ff6a` (the vendor-record
manifest walk, a bug already on main that this change exposed). The eval label
is `exp211-ec789ff6`, compared with `ref-names-40921b9f-r1`.

## Totals

- **The target hop moved as expected.** On 2.1.85→2.1.86, vendor noise
  statements fall from 238/242/238/238 (the four reference runs
  r1/r2/r3/s1) to **112**, outside every reference. The other four pairs sit
  inside the reference range.
- **Real change is exact.** novel 5,336 and realLn 556,875 match the
  reference. app:novel, app:realLn, vendor:novel, vendor:realLn and the
  cross-boundary columns are equal too.
- **Vendor totals:** vendorLn −206 (band ±84), vendor:reloc −15,
  vendor:relocSt −4, vendor:mints −6, vendor:treeLn −191. vendorReal is
  inside its band (+15, ±28).
- **Library placement precision rose and no app bytes were added.** On bun-dev
  vendor-precision (71 bundles both binaries scored), 2.16 MB of library code
  moved from the vendor root into a package folder, and 99.87% of it went to
  the right package. CommonJS factory placement precision
  (right / (right + wrong)) rose from 0.98941 to 0.98955. App bytes stayed at
  378 and app files at 4; both were already there on main.
- **Library score (claims) is byte-identical to main** on bun-dev and bun-held
  (structural rule). The discovery claims do not change; only placement does.

## The problem, and what each case turned out to be

Names mode files recognised library code under `vendor/<package>/`. When the
library index holds the same code under more than one package or file,
placement either flipped from version to version or gave up. There were three
known cases:

**(a) lodash vs lodash-es (ESM-inlined lazy modules).** The index holds the
same function bodies under both packages. Discovery's claim propagation
("down", "module", "glue" from index seeds) gives a module to whichever twin
reaches it first, and that depends on the bundle's neighbours, which change
between versions. Lazy modules have no whole-module key in the index, so
the planner took the claim as given. In the reference trees, 4 single modules
and 1 duplicate group on 85→86 changed package between versions (same content
key, `vendor/lodash-es/module-0581b523.js` → `vendor/lodash/module-0581b523.js`).
On 210→211 the count was 9 single modules and 5 duplicate groups.

**(b) opentelemetry / semver on 2.1.179 lost their file path.** exp204 put
this down to the index. It is not the index: each key has ONE row with ONE
file. The bundle carries **two versions of the same package** (semver 7.7.0/2
and 7.7.3+; two @opentelemetry/core copies), and both copies' keys now
answer the same file (`classes/semver.js`). The planner's existing duplicate-path
rule (D1) gives the real path to the better sort key and a synthetic in-package
name to the other. That choice is already deterministic (by content key).
Across versions the CommonJS manifest carry (exp194) keeps it stable: CJS
factory paths were identical on every eval hop, before and after. **No change
was needed here.** A version-tagged real path for the loser, e.g.
`classes/semver@7.7.0.js`, is possible future work; it is not done.

**(c) aws-sdk nested-clients / client-signin became `index-disagrees`.** The
index holds the key under the claimed package AND another one
(`@aws-sdk/nested-clients` re-ships `@aws-sdk/client-sts` modules under
`dist-cjs/submodules/sts/`). The module-answer reader returned "two owners,
no file" for any key two packages hold, and the planner read that as a
disagreement. The same pattern held back far more than the 8 aws modules:
70 of the 72 `index-disagrees` leaves on 2.1.121/149/179/181. Most were ajv
vs its fork `@redocly/ajv`, protobufjs vs `@apollo/protobufjs`, and the
aws-sdk clients.

## What changed (owner: `place::library_layout`, the placement planner)

1. **The answer keeps each owner's files** (`libindex::module_answers`).
   `ModuleAnswer.owners` maps each owning package to its own files.
2. **A claim that is one of the key's owners is not a disagreement.** The
   region moves to that package's own file. A claim outside all owners still
   leaves (`index-disagrees`), and an unclaimed region with several owners
   still stays (`unclaimed`).
3. **The bundle's dependency evidence chooses among owners.** When the claim
   shares the key with other packages, the region goes to the owner with
   strictly the most regions placed on keys that only it holds in this bundle.
   That puts ajv over `@redocly/ajv`, which appears nowhere in the Claude Code
   bundle, and nested-clients over client-sts / client-sso-oidc /
   client-signin. A tie keeps the claim. The count is order-free.
4. **Lazy modules keep the prior's package for a family twin.** The prior's
   split ledger (`fossilModules`: file + statement hashes) gives a map from
   module content key to package (`naming::module_names::prior_library_packages`;
   the content key now has one spelling, `content_key_of_hashes`). If the
   prior filed this exact content under a package of the claim's family
   (`PackageFamilies`; lodash / lodash-es are one), the planner keeps the
   prior's package. Content the prior filed under two packages carries
   nothing. A ledger of another hash era carries nothing.
5. **The carry and the evidence only CHOOSE between packages that already
   hold the code.** Neither ever makes a region move that would otherwise
   stay (tests: `a_carry_never_moves_a_region_that_would_stay`,
   `the_evidence_never_moves_an_unclaimed_shared_key`).

Plumbing: `split_stage::prior_split_ledger_path` is the one resolution of
which prior ledger the run reads; the split stage and the new carry both use
it. `NamingInput` / `EraOptions` carry `prior_library`.
`vendor_names::package_of_vendor_path` moved out of `unpack::bun` so both
readers share it. `docs/responsibility.md` rows for the planner and the
answer reader are updated.

Candidates evaluated and not taken:

- **The neighbouring modules' package** as a rule for lazy modules. The flips
  are themselves a neighbour effect: claims propagate from neighbours, and
  neighbours move between versions.
- **A fixed "CommonJS package over its -es twin" rule.** Lazy (`__esm`)
  modules come from ESM sources, so for lazy modules lodash-es is more likely
  the true source, the opposite of that rule. A fixed rule would also need
  per-unit holder sets, which the planner does not have for lazy modules.
  Without them it could file -es-only code under the CommonJS twin. The carry
  covers every run that has a prior, which includes every eval scored leg,
  self-hop and walk hop.

## The manifest-walk fix (`ec789ff6a`, a bug already on main)

`libdetect::load_manifest` (the vendor-record library detector) looked for
`<dir>/vendor/_bun-modules.json` by walking up **at most 3 directories** from
the FIRST unpacked file. When that file sits deeper, for example
`vendor/ajv/dist/compile/codegen/code.js`, the manifest is never found. Every
factory body then goes to the formatter as app code ("Processing file 1/N",
then `parse error: Expected function name`). The walk is now uncapped and
stops at the first manifest up the chain. Every unpacked file sits under the
output root, so that manifest is the run's own. Test:
`bun_manifest_found_from_a_deeply_nested_first_file` (red, then green).

- **On main** this bug causes 7 of the 10 bun-dev vendor-precision failures
  (hugin-desktop, electron-vite-starter, vscode-dbt-power-user, each on both
  slices, plus node-argus). This branch placed more modules at deep real
  paths, which added shotheadman__mdcz (both slices).
- **Can it reach real Claude Code runs?** `load_manifest` runs on every
  names-mode Bun run, eval and walk included. It starts from the first
  unpacked file in bundle order. In Claude Code that file is the
  image-processor native-module factory, at depth 1–2
  (`vendor/image-processor-node/…`). Every run of `ref-names-40921b9f-r1`
  logs "Skipping 14xx–16xx library files" and "Processing file 1/1". So it
  cannot reach Claude Code today. It would reach it only if Claude Code's
  first factory were placed 3+ folders below `vendor/<pkg>/`, and the old
  code then failed loudly (a parse error), never silently.
- **Main's other 3 failures are a different bug, already on main:**
  just-bash, rusiaaman chat.md and deepmyst mysti (bun slice) fail with
  `stable split failed before any tree was written: wrapper detected (body
  span …) but that block is absent from the estree json`. They failed the
  same way in exp192's main run. bun-kryptokrona fails that way on this
  branch once the manifest is found, and so does main with only the manifest
  fix applied. It is not caused by placement. Not fixed here.

## Gate

| check | worktree (`ec789ff6a`) | clean checkout (`ec789ff6a`) |
| --- | --- | --- |
| shims | PASS | PASS |
| no-private-addresses | PASS | PASS |
| rust:fmt | PASS | PASS |
| rust:clippy | PASS | PASS |
| census:clones (advisory) | REVIEW | REVIEW |
| census:concurrency | PASS | PASS |
| census:slop | PASS | PASS |
| rust:unit | PASS | PASS |
| rust:build | PASS | PASS |
| format-golden | PASS | PASS |
| e2e | PASS (19 pairs, 152 launches = golden) | PASS |

census:clones: production findings are 51, the same as main `acdc6b0db`. The
30 extra test-only groups come from the added `prior_library:` line in
already near-identical `NamingInput` test literals.

## Results

### Eval, `exp211-ec789ff6` vs `ref-names-40921b9f-r1` (5 pairs, names mode, scratch bases, scorer v2)

| KPI | ref r1 | exp211 | delta | band | reading |
| --- | ---: | ---: | ---: | --- | --- |
| novel | 5,336 | 5,336 | = | 0 | exact |
| realLn | 556,875 | 556,875 | = | 0 | exact |
| vendorReal | 14,305 | 14,320 | +15 | ±28 | inside |
| vendorLn | 10,027 | 9,821 | −206 | ±84 | **down** (85→86 lodash) |
| vendor:reloc | 113 | 98 | −15 | — | down |
| vendor:relocSt | 37 | 33 | −4 | — | down |
| vendor:mints | 12 | 6 | −6 | — | down |
| vendor:noise | 41 | 41 | = | — | (85→86: 238 → 112 vendor noise statements per pair, refs 238/242/238/238) |
| noiseLn | 68,179 | 67,145 | −1,034 | ±989 | mostly 197→198 naming noise (6,753 vs the refs' 6,894–7,949): a draw |
| noise | 3,034 | 3,046 | +12 | ±20 | inside |
| reloc | 1,653 | 1,642 | −11 | ±83 | inside |
| mints | 239 | 246 | +7 | ±9 | inside |
| newName | 4,029 | 4,074 | +45 | ±42 | just past its band; app side (refs 4,029–4,071) |
| app:mints | 222 | 238 | +16 | — | app side |
| treeLn | 175,268 | 175,764 | +496 | ±1,183 | inside |

newName and app:mints sit on the app side, which this change does not touch
(it changes vendor placement only; the app-side matching columns are equal:
fnTotal / fnCarried / fnClose / fnAsked / mbCarried). I found no mechanism, so
I read them as draws. All 5 rebases and 5 scored legs exit 0. The warm
self-hop is byte-identical with 0 cache writes; the cold self-hop reads 8
diff lines.

### Stub runs (no LLM; main = `61063385e`, cand = this branch before the rebase)

| hop | family flips main → cand | mixed-package duplicate groups | vendor tree diff (files / lines) |
| --- | --- | --- | --- |
| 2.1.85→86 | 4 → 0 | 1 → 0 | 62 / 502 → 57 / 466 |
| 2.1.210→211 | 9 → 0 | 5 → 1 | 36 / 267 → 20 / 194 |

CommonJS factory paths are identical across hops in both binaries. Footprint
measured per run: 2.1.85 228–273 s / 18.9 GB; 2.1.86 118 s / 9.5 GB;
2.1.210 631 s / 27.9 GB (above the verb's 10–17 GB note); 2.1.211 190–196 s /
16.6 GB.

### Placement on real Claude Code bundles (`humanify-ag discover --bundle-order`, index 4736dcfa)

| version | index-disagrees main → cand | real paths main → cand | existing paths changed |
| --- | --- | --- | --- |
| 2.1.121 | 73 → 2 | 1,069 → 1,136 | 0 |
| 2.1.149 | 72 → 2 | 1,072 → 1,137 | 0 |
| 2.1.179 | 72 → 2 | 1,059 → 1,124 | 0 |
| 2.1.181 | 72 → 2 | 1,049 → 1,114 | 0 |

The discovery report apart from `bundleOrder` is byte-identical. On 2.1.179
the moved modules land in ajv 50 (was 8), nested-clients 19 (was 4) and
protobufjs 32 (was 27). `@redocly/ajv` and `@apollo/protobufjs` gain nothing.

### Library score (structural rule, index 4736dcfa, main `acdc6b0db` vs `73d22527f`)

| set | precision main / cand | recall main / cand | app bytes |
| --- | --- | --- | --- |
| bun-dev-bun | 0.998342 / 0.998342 | 0.864003 / 0.864003 | 0 / 0 |
| bun-dev-node | 0.997439 / 0.997439 | 0.859376 / 0.859376 | 0 / 0 |
| bun-held-bun | 0.999755 / 0.999755 | 0.869671 / 0.869671 | 0 / 0 |
| bun-held-node | 0.997162 / 0.997162 | 0.831513 / 0.831513 | 0 / 0 |

The node sets were already under the 0.998 bar on main; this change does not
touch claims.

### Vendor precision, bun-dev (71 bundles both binaries scored; main `acdc6b0db`, cand `ec789ff6a`)

| measure | main | cand | delta |
| --- | ---: | ---: | ---: |
| ESM era: moved modules / right / wrong-package bytes | 383 / 447,542 / 38,263 | 383 / 447,542 / 38,263 | = |
| ESM era: app files | 0 | 0 | = |
| factory: library bytes at the vendor root | 24,850,404 | 22,694,208 | −2,156,196 |
| factory: right package bytes | 137,143,742 | 139,297,176 | +2,153,434 |
| factory: wrong package bytes | 1,467,972 | 1,470,734 | +2,762 |
| factory: wrong files | 5,264 | 5,351 | +87 |
| factory: app bytes / app files | 378 / 4 | 378 / 4 | = |
| factory placement precision | 0.98941 | 0.98955 | +0.00014 |
| runs failed | 10 | 4 | the manifest fix; the 4 left are the split bug |

The +87 newly "wrong" files, by the folder chosen. They carry 2,762 wrong
bytes in total, a few bytes per file. They are forks and twins whose key the
index also gives the chosen package.

| folder chosen | new wrong files | bundles (bun/node slices) |
| --- | ---: | --- |
| `ajv` | 20 | ts-standard, positron, vue-electron-template, mdcz, cpod, codevisualizer, gistpad |
| `pend` | 9 | vscode-array-index-inlay, vscode-github-actions, gistpad, catalyst, mysti (node) |
| `buffer-crc32` | 9 | the same five as `pend` |
| `natural-compare` | 6 | ts-standard, vscode-array-index-inlay, better-svg |
| `@eggjs/yauzl` | 6 | vscode-github-actions, gistpad, catalyst |
| `vscode-test` | 6 | catalyst, chat.md (node) |
| `css-tree` | 5 | novel-engine, better-svg, mysti (node) |
| `@isaacs/balanced-match` | 5 | vscode-array-index-inlay, better-svg, mysti (node) |
| `@javascript-obfuscator/estraverse` | 4 | ts-standard, vscode-array-index-inlay |
| `@vscode/test-electron` | 4 | helm-intellisense |
| `eventemitter3` | 3 | novel-engine, argus (bun) |
| `@rc-component/pagination` | 2 | prism-browser-community |
| `@modelcontextprotocol/client` | 2 | prism-browser-community |
| `estraverse` | 2 | vue-electron-template |
| `@npmcorp/move` | 2 | gistpad |
| `chalk` | 1 | cross-var (node) |
| `@koa/router` | 1 | vscode-github-actions (node) |

## What this could NOT prove

- **Boot gate: not judged.** All five boot checks failed with `401 OAuth
  access token has been revoked` (an expired login on the host), not a
  pipeline result. The runnable trees were not booted against the Claude API
  in this eval.
- **Bands.** They were measured at `40921b9f7`, not this commit. newName +45
  and app:mints +16 are just past the app-side bands with no mechanism I
  could find; they are read as draws, not proven to be.
- **Scratch bases still flip.** A run with no prior (the eval's scratch base,
  a first walk version) still takes the claim for lodash vs lodash-es. The
  carry makes the NEXT hop agree with it; it does not make two prior-less
  runs agree.
- **Mixed-family modules.** Modules whose claims mix lodash and lodash-es
  inside one group still stay in `src/` as `contested` (210→211 keeps 4
  vendor↔src moves and 1 mixed duplicate group). Folding a family's claims
  before the join is not done.
- **Case (b)** (two versions of one package sharing a real file path) is
  explained, not changed.
- **Wrong-package truth for the newly placed ajv / aws-sdk modules in Claude
  Code** is not checked against `/work/cc-library-truth`. bun-dev vendor
  precision is the only truth-labelled check.

## Proposed CLAUDE.md text

None needed for the gate or eval. In the exp204 paragraph, the reading of the
9 modules that "lost a path" should become: two versions of one package in
the bundle share a real file path; the duplicate-path rule gives it to one of
them, which is not an index ambiguity (exp211). I have not edited CLAUDE.md.
