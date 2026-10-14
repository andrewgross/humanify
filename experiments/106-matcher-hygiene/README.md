# exp106: matcher hygiene (banner names, helpers, small seeds, payloads, families, guards) + scorer 5

## STATUS (2026-10-14)

EXECUTED on branch `exp106-matcher-hygiene` (main `dcf66970` merged in: exp103 index /3, exp105 webpack/rollup,
exp101). Every matcher change is a `[library]` key, DEFAULT OFF; the default `humanify discover` claims are
unchanged (`exp106-ctl2-structural` reproduces `exp102-final-structural` card for card). Scorer bumped to
`exp096-score/5` (two truth corrections + F0.5). Gate: ALL 13 STAGES PASSED at every commit.

**Totals first: the recommended DEFAULT** (judged on `c2-l12m4-v3`, the campaign's candidate index, dev; and on
the frozen TEST split's summaries with `step4-wr-v3`, the only /3 index holding the webpack and rollup slices the
test corpus needs; c2 has none, so a c2 test run refuses every rollup/webpack bundle):

```toml
[library.lookup]
agreement = false             # the STRUCTURAL rule: dev held-out F0.5 85.0 vs agreement 75.2 (c2)
[library.discovery]
evidence_conflict = true
literal_conflict = true
contested_cover = 0.05
specific_rivals = true        # exp106: makes evidence_conflict / contested_cover safe on bigger indexes
banner_names = "known"        # exp106
derived_families = true       # exp106
payload_min_bytes = 4096      # exp106: worth +11.4 F0.5 held-out on call2k, -0.05 on c2
```

| index / corpus | set            | P / R / **F0.5** / F1: none       | exp102 guards (G)                 | **recommended**                   |
| -------------- | -------------- | --------------------------------- | --------------------------------- | --------------------------------- |
| c2 dev         | held-out 67    | 98.67 / 54.24 / **84.78** / 70.00 | 99.22 / 54.10 / **85.04** / 70.02 | 99.25 / 54.05 / **85.03** / 69.99 |
| c2 dev         | task-master-ai | 98.35 / 52.50 / **83.73** / 68.46 | 98.99 / 43.51 / **78.87** / 60.45 | 99.13 / 52.30 / **84.07** / 68.47 |
| c2 dev         | wrangler       | 95.54 / 16.52 / **48.83** / 28.17 | 95.67 / 12.71 / **41.51** / 22.45 | 95.54 / 16.52 / **48.83** / 28.17 |
| c2 dev         | Claude Code    | 100 / 76.65 / **94.26** / 86.78   | same                              | same                              |
| call2k dev     | held-out 67    | 81.23 / 59.72 / **75.77** / 68.84 | (exp101) 81.56 / 59.52 / 68.82 F1 | 98.94 / 59.58 / **87.39** / 74.38 |
| call2k dev     | task-master-ai | 93.94 / 52.61 / **81.19** / 67.45 | (exp101) 94.09 / 46.09 / 61.87 F1 | 94.74 / 52.42 / **81.57** / 67.49 |
| step4-wr TEST  | test-bun       | 52.82 / 14.11 / **34.11** / 22.27 | 55.28 / 13.46 / **34.10** / 21.65 | 55.82 / 13.53 / **34.35** / 21.79 |
| step4-wr TEST  | test-esbuild   | 51.39 / 14.28 / **33.81** / 22.34 | 51.81 / 14.25 / **33.93** / 22.35 | 51.86 / 14.32 / **34.02** / 22.44 |
| step4-wr TEST  | test-webpack   | 85.44 / 34.04 / **65.62** / 48.68 | 85.49 / 33.99 / **65.61** / 48.64 | 85.43 / 33.84 / **65.47** / 48.48 |
| step4-wr TEST  | test-rollup    | 75.76 / 28.67 / **57.03** / 41.60 | 76.72 / 28.52 / **57.34** / 41.59 | 76.79 / 28.49 / **57.34** / 41.56 |

App claimed: 0 on every dev set of every label; on test, webpack 3,962 B and rollup 16 B under EVERY
configuration (exp105's open item, exp107's lane). No set passes the 99.8% precision bar except Claude Code.

Read it this way: exp102's three guards as merged (G) are UNSAFE on bigger indexes — on c2, `evidence_conflict`
takes task-master F0.5 83.7 -> 78.9 and `contested_cover` takes wrangler 48.8 -> 41.5 — and `specific_rivals`
removes both losses while keeping the precision. The other exp106 keys each add a little on dev and test
(banner names, derived families), and `payload_min_bytes` is the one large lever, on the large index.

## What was built (all `[library.discovery]` unless noted; default off)

| key                                                     | rule                                                                                                                                                                                                                                                                                                                                                       | verdict                                            |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| `banner_names = "known"`                                | with an index, a `/*!` banner seeds only a package the index knows (owners + manifest packuments/packages), resolved to npm's spelling: exact, lower case, or the ONE known `@s/n` spelled `s-n` (`noble-curves` -> `@noble/curves`). An unknown name is no seed.                                                                                          | **ON**                                             |
| `derived_families = true`                               | the matcher's "one family?" (lookup agreement, isolation veto, containment, conflict guards, queue) joins packages by the npm-metadata links, PAIRWISE, restricted to the index's own packages. The links file is ONE artifact: `crates/humanify-core/src/libindex/families-derived.json`, read by the Rust matcher and by the scorer's `families_lib.py`. | **ON**                                             |
| `specific_rivals = true`                                | `evidence_conflict` / `contested_cover` read another family's prints only where the lookup's specificity rule would (a sponge holding a package's code in a far bigger module is no rival).                                                                                                                                                                | **ON** (with the guards)                           |
| `payload_min_bytes = 4096`                              | a PROPAGATED unit of >= N bytes, >= 90% string-literal bytes (a base64 wasm blob, a data table), is refused unless the claimed family's own index evidence holds it (a print of it, or its long literal).                                                                                                                                                  | **ON** for any index bigger than step4 (see below) |
| `library.lookup.literal_margin_by_family = true`        | the agreement rule's literal margin is read against the best file of another FAMILY, not another package.                                                                                                                                                                                                                                                  | **ON** (agreement)                                 |
| `helper_families = N`                                   | an index print held by >= N families is emitted helper code; a unit such prints cover half of is no seed / no propagated claim.                                                                                                                                                                                                                            | off (N=4 costs right claims, N=12 is ~neutral)     |
| ~~`self_support_max_nodes`~~, ~~`lone_seed_max_nodes`~~ | small-seed corroboration, two forms                                                                                                                                                                                                                                                                                                                        | REFUTED, deleted                                   |

Tests (red first): `libdetect/discovery_test.rs` (`banner_names_known_keeps_only_names_the_index_knows_resolved_to_npm_spelling`,
`helper_prints_held_by_many_families_seed_and_carry_nothing`,
`a_data_payload_is_not_claimed_by_propagation_without_its_own_evidence`,
`specific_rivals_only_a_sponge_s_unspecific_print_is_no_conflict`), `libindex/families_test.rs`
(`derived_links_join_packages_pairwise_never_through_a_scope`), `libindex/lookup_test.rs`
(`the_literal_margin_by_family_ignores_a_sibling_holding_the_same_literals`), config keys in
`humanify-cli/src/library_config_test.rs`.

## All labels (scorer 5; P / R / F0.5 / F1)

Dev, `c2-l12m4-v3` (merged binary):

| label                                       | held-out                | task-master-ai          | wrangler                | Claude Code           |
| ------------------------------------------- | ----------------------- | ----------------------- | ----------------------- | --------------------- |
| `exp106-c2-none-structural`                 | 98.67/54.24/84.78/70.00 | 98.35/52.50/83.73/68.46 | 95.54/16.52/48.83/28.17 | 100/76.65/94.26/86.78 |
| `exp106-c2-ec-structural`                   | 98.76/54.11/–/69.91     | 98.08/43.51/–/60.28     | 96.10/14.19/–/24.72     | =                     |
| `exp106-c2-lc-structural`                   | 99.01/54.24/–/70.08     | 98.36/52.50/–/68.46     | 95.54/16.52/–/28.17     | =                     |
| `exp106-c2-cc-structural`                   | 98.80/54.23/–/70.02     | 99.09/52.45/–/68.59     | 94.29/12.71/–/22.41     | =                     |
| `exp106-c2-ecsr-structural`                 | 98.75/54.21/–/70.00     | 98.37/52.37/–/68.35     | 95.54/16.52/–/28.17     | =                     |
| `exp106-c2-ccsr-structural`                 | 98.80/54.23/–/70.02     | 99.09/52.45/–/68.59     | 95.54/16.52/–/28.17     | =                     |
| `exp106-c2-G-structural`                    | 99.22/54.10/85.04/70.02 | 98.99/43.51/78.87/60.45 | 95.67/12.71/41.51/22.45 | =                     |
| `exp106-c2-Gsr-structural`                  | 99.21/54.20/85.08/70.11 | 99.13/52.37/84.11/68.53 | 95.54/16.52/48.83/28.17 | =                     |
| `exp106-c2-pay-structural`                  | 99.00/53.90/84.81/69.80 | 98.35/52.42/83.68/68.39 | = none                  | =                     |
| `exp106-c2-rec-structural2` (recommended)   | 99.25/54.05/85.03/69.99 | 99.13/52.30/84.07/68.47 | 95.54/16.52/48.83/28.17 | =                     |
| `exp106-c2-none-agreement`                  | 95.13/38.08/73.20/54.39 | 99.27/34.37/72.05/51.06 | 100/10.13/36.03/18.39   | 100/63.39/89.64/77.59 |
| `exp106-c2-G-agreement`                     | 96.52/37.94/73.75/54.47 | 99.31/34.33/72.04/51.02 | 100/6.42/25.53/12.06    | =                     |
| `exp106-c2-m-agreement` (margin by family)  | 95.17/38.45/73.49/54.77 | 99.30/36.27/73.69/53.14 | 100/10.97/38.11/19.77   | 100/64.32/90.01/78.28 |
| `exp106-c2-rec-agreement`                   | 96.73/39.82/75.23/56.41 | 99.35/36.19/73.64/53.05 | 100/10.97/38.11/19.77   | 100/64.45/90.07/78.39 |
| `exp106-c2-m0-agreement` (literal_margin=0) | 98.45/47.11/80.83/63.73 | 98.36/38.32/74.90/55.16 | 98.46/12.50/41.44/22.18 | 100/69.16/91.81/81.77 |

Dev, `call2k-l12m4-v3`:

| label                       | held-out                | task-master-ai          | wrangler                | Claude Code             |
| --------------------------- | ----------------------- | ----------------------- | ----------------------- | ----------------------- |
| `exp106-k2-none-structural` | 81.23/59.72/75.77/68.84 | 93.94/52.61/81.19/67.45 | 99.03/20.68/56.33/34.21 | 100/75.18/93.81/85.83   |
| `exp106-k2-pay-structural`  | 98.68/59.39/87.14/74.15 | 93.93/52.53/81.14/67.38 | =                       | =                       |
| `exp106-k2-rec-structural`  | 98.94/59.58/87.39/74.38 | 94.74/52.42/81.57/67.49 | =                       | =                       |
| `exp101-call2k-agreement`   | 95.17/38.18/73.29/54.50 | 91.80/30.09/65.10/45.32 | 100/13.45/43.73/23.71   | 99.90/47.96/82.12/64.81 |
| `exp106-k2-m-agreement`     | 95.20/38.58/73.60/54.91 | 92.16/31.59/66.61/47.05 | 100/14.56/46.00/25.41   | 99.91/48.89/82.66/65.65 |
| `exp106-k2-m0-agreement`    | 98.37/47.76/81.17/64.30 | 92.97/34.34/69.30/50.15 | 98.80/16.15/48.82/27.76 | 100/52.47/84.66/68.83   |
| `exp106-k2-ls12-agreement`  | 95.17/38.20/73.30/54.52 | =                       | 100/13.66/–/24.04       | =                       |
| `exp106-k2-cov8-agreement`  | 95.40/40.15/74.81/56.52 | =                       | =                       | =                       |
| `exp106-k2-rec-agreement`   | 96.36/40.16/75.29/56.70 | 92.38/31.51/66.63/46.99 | 100/14.56/46.00/25.41   | 99.91/48.89/82.66/65.65 |

TEST (`step4-wr-v3`; summaries only), agreement:

| label                           | test-bun          | test-esbuild      | test-webpack                      | test-rollup       |
| ------------------------------- | ----------------- | ----------------- | --------------------------------- | ----------------- |
| `test-exp106-wr-none-agreement` | 58.68/12.95/34.38 | 58.94/11.75/32.68 | 95.49/23.04/58.63 (app 611)       | 87.08/15.68/45.58 |
| `test-exp106-wr-G-agreement`    | 61.59/10.84/31.80 | 59.39/11.75/32.79 | 97.54/22.81/58.92 (app 611)       | 91.93/15.24/45.83 |
| `test-exp106-wr-rec-agreement`  | 61.97/11.00/32.16 | 59.67/11.93/33.15 | 97.53/22.73/58.82 (app 611)       | 92.60/15.27/46.00 |
| `test-exp106-wr-m0-agreement`   | 57.94/13.92/35.49 | 60.14/12.78/34.54 | 95.36/26.48/62.72 (app **3,118**) | 88.43/19.33/51.57 |

step4 (pre-merge binary, /2 index; claims are byte-identical to /3 per exp103), structural, dev and TEST
(per-guard): `evidence_conflict` alone costs test-bun recall 14.06 -> 13.48 and, under agreement, 12.89 -> 10.81
(the guard set's whole agreement loss); `literal_conflict` (test-bun F1 22.20 -> 22.42) and `contested_cover`
(22.54) alone are clean; banner + families (`hyg`) 22.29; all of it with G (`best`) 21.68 vs G 21.57.

## Item by item (dev, judged claim by claim with `claims.py diff`, scorer 5)

1. **Banner names that are not packages.** step4, structural, held-out: removed 6,859 B wrong (`noble-curves`
   4,567 + `Runge-Kutta` 1,866 + `noble-ciphers`/`noble-hashes`/`OpenPGP.js`), lost 2,258 B right (`markdown-it`:
   real, but not in the step4 index's known names). Agreement: -20,026 B wrong (Runge-Kutta's MODULE fill 13 KB).
   Resolution to npm spelling needs the scoped name in the index's known set; step4 does not know `@noble/curves`.
2. **Transpiler / bundler helpers.** Commonness from the index alone cannot see TypeScript's `__disposeResources`:
   its prints are held by ONE family in step4 (quickjs-wasi) and two in call2k (quickjs-wasi, puppeteer-core). The
   tslib helpers (`__extends`, `__createBinding`) ARE visible: their function prints are held by 12-72 families.
   `helper_families`: N=4 removed 22.5 KB right for 2.4 KB wrong (popular code bundled into many packages' builds
   — balanced-match, yargs, acorn — is "common" too); N=12 removed 784 B wrong (`@anthropic-ai/sdk`/`zod` over
   tslib) for 526 B right; N=24 smaller still. Kept, off. The generic answer for the rare helpers is an index that
   holds tslib / @babel/runtime themselves (then the helper is ambiguous and the veto's ambiguity rule applies).
3. **Small seeds that propagate.** Two generic forms, both REFUTED on dev and deleted:
   - self-support floor (a seed under N nodes may not count its own propagation as veto support): N=300 cost
     735 KB right held-out for 48.6 KB wrong; N=1000 786 KB right.
   - lone-seed floor (a seed under N nodes with no other seed of its family in the bundle is dropped): N=300 cost
     31 KB right AND ADDED 66 KB wrong (dropping a lone seed let MODULE fill an `@angular/compiler` module as
     `prettier`, 72 KB).
     The domino case (one 480 B seed that is Angular's own `parseStyle` copied into `@mixmark-io/domino`) is a
     copy, indistinguishable from a real single-function package by size or loneliness.
4. **MODULE into rollup `?commonjs-*` proxies.** Not reproducible on dev: no dev rollup truth carries a
   `?commonjs-*` source, and a self-built rollup probe (CJS entry + ms + semver) draws no claim at all. The scorer
   side is fixed (scorer 5 `virtual`: test-rollup app claimed 76 -> 0 at step4f, 248 -> 16 on exp105's label).
   The remaining test app claims belong to exp107 (zero app claims).
5. **Matcher families from metadata.** step4 structural held-out +46.2 KB right (micromark monorepo members,
   inquirer, mimic-fn ~ mimic-function, @isaacs/balanced-match), 98 B wrong added, 89 B wrong removed.
6. **Truth bugs (scorer 5).** `chained`: dev relabels 0 sources; test-esbuild 1,520 sources -> app claimed 13,806
   -> 0 (step4f) and 1,311,851 -> 0 on `test-exp101-c2-structural`, whose precision rises 74.29 -> 77.46 (most of
   those bytes were RIGHT `@angular/compiler` claims once the truth stopped calling them app). `virtual`: see 4.
   With both rules off, scorer 5 reproduces scorer 4 (`scorer5/verify_s4.py`, 70/70 cards x 2 labels). Re-scoring
   every committed dev label changed no card beyond the schema and the new `f05` keys.

## The coordinator's call2k / c2 findings

- **@biomejs/wasm-nodejs over @baszalmstra/rattler (3.9 MB, call2k):** `payload_min_bytes=4096` removes it:
  call2k structural held-out P 81.23 -> 98.68, F1 68.84 -> 74.15, for -0.33 pt recall. The cost is the same on
  every index: @sentry/node-core's own 44 KB base64 worker script (89.6 KB + 12.7 KB right, MODULE/DOWN) has the
  same shape and no matching long literal in the index. On c2 (no biome case) the guard is net negative (removes
  102 KB right, 55.7 KB wrong that `literal_conflict` already removes).
- **Propagation into a dependency (got over @szmarczak/http-timer, dotenv-expand over @noble/curves):** not
  addressed; a consumer/dependency direction rule needs package dependency metadata in the index.
- **Agreement dilution (CC agreement recall 63.4 on c2 -> 48.0 on call2k):** `literal_margin_by_family` recovers
  0.9 pt on call2k CC (48.0 -> 48.9) and helps c2 too (CC 63.39 -> 64.32, task-master 34.37 -> 36.27, wrangler
  +0.84, held-out +0.37, wrong unchanged). It is not the main cause: `literal_margin=0` gives 52.5,
  `literal_cover=0.8` nothing. A `literal_margin` of 0 gives the biggest agreement gain on EVERY index (c2 held-out F0.5 73.2 -> 80.8 with
  precision UP, 95.1 -> 98.5; call2k CC 48.0 -> 52.5) and on test (F0.5 up on all four sets) — but it lets the app
  claims on test-webpack rise 611 -> 3,118 B, so it is NOT recommended until exp107 closes app claims. `literal_score`
  12 and `literal_cover` 0.8 barely move. The rest of the call2k CC drop is unexplained here.
- **exp102's guards on c2:** `evidence_conflict` alone takes task-master structural recall 52.50 -> 43.51 (ajv
  seeds outweighed by another family's print: 417 KB right) and wrangler 16.52 -> 14.19; `contested_cover` alone
  takes wrangler 16.52 -> 12.71 (`@aws-sdk/client-sts`/`-sso`/`-ssooidc` DOWN). `specific_rivals` makes both safe:
  `exp106-c2-Gsr-structural` task-master 52.37, wrangler 16.52, held-out precision 99.21 (G 99.22).

## Instruments

- `claims.py wrong|diff|rows`: every claim judged by the CURRENT scorer (dev only).
- `probe.py`: a claim's index evidence (whole and function prints' owners).
- `scorer5/chained.py`, `scorer5/verify_s4.py`: the chained layer, and the rules-off identity check.
- Labels: `experiments/096-library-matching-score/results/exp106-*` and `test-exp106-*` (summaries only for test).
