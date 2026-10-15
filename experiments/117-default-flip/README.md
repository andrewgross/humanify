# exp117: flip the proven default-off library keys on

## STATUS

- **DONE, branch `exp117-default-flip`, NOT merged.** Main `0b2b136b` (exp113: k2-all default index) is merged in.
- **Shipped by this branch** in `DiscoveryOptions::recommended()`:
  - `context_presence = 1.5`
  - `context_presence_slack = 16384`
  - `presence_fence = true`
  - `declared_max_bytes = 65536`

  The test that pins the defaults was changed first and failed (red), then passed (green):
  `library_config_test.rs`, `the_default_presence_keys_and_declared_cap_are_on`. `humanify.example.toml` was
  regenerated and `docs/library-matching-config.md` updated. Library matching itself stays `off` by default.
- **Left out: `shadow_ties`.** It is the one key whose combination lowered precision on a set: test2-esbuild
  P 99.4091 -> 99.4062, +1,035 wrong-family bytes. It is also the most valuable key (see the trade-off below), so
  the call is the coordinator's.
- **Before/after:** precision and F0.5 are never lower on any of the 19 sets across six corpora (dev, apps,
  bundlers, pop, test, test2). App-claimed bytes are 0 everywhere. All runs are structural, scorer 7, on k2-all.
- **Confirmed:** at the commit, the defaults with no `--pipeline-set` reproduce the `cpf+dmb` labels' set summaries
  exactly (dev and bundlers: `exp117-k2all-head*`).
- **Gate:** ALL 13 STAGES PASSED, bare, at `0803d8e4`.
- **Neutrality, warm** (2.1.215 -> 2.1.216, `--priors /work/exp050-cold --cache /work/neutrality-cache`, under
  `flock /work/neutrality.lock`), `0803d8e4` vs main `0b2b136b`:
  - default run (matching off): **NEUTRAL, 0 files / 0 lines, cache +0 / +0, exit 0 / 0.**
  - report mode (`--pipeline-set library.matching=report --pipeline-set library.index=/work/exp113-data/ix/k2-all`):
    **NEUTRAL, 0 / 0, +0 / +0, exit 0 / 0.** `diff -rq` of the two trees lists NOTHING. Even
    `.humanify/library-report.json` is byte-identical: on Claude Code 2.1.216 the four keys move no claim. The
    scored `cc` set is also unchanged under every variant. The report does carry discovery (Bun slice, seeds,
    packages).
  - The index is k2-all, the new default, not k2-l12m4. The Bun slices are the same.
- **Optional @redocly/ajv fork pair:** NOT done as a families change; see the section on it below.
- **Integrity note:** test2 summaries decided two things here. Ablations on test2-esbuild (summary only) named
  `shadow_ties` as the cause of the drop. Test2 webpack/rollup also showed `declared_max_bytes`' marginal cost. No
  bundle was inspected. But the frozen judge was used for a key selection, so treat test2 as partly spent for
  key-level decisions.

## Before/after (before = main `0b2b136b` shipped defaults on k2-all = exp113's labels; after = this branch's defaults)

P / R / F0.5 in %, structural, scorer 7. Before labels: `exp113-k2all{,-apps,-bundlers}`, `pop-exp113-k2all`,
`test-exp113-k2all`, `test2-exp113-k2all`. After labels: `*exp117-k2all-cpfdmb*` (with `--pipeline-set`, at
`0b2b136b`), which are identical to `exp117-k2all-head*` (the committed defaults, at `0803d8e4`).

| corpus | set | before P / R / F0.5 | after P / R / F0.5 | dF0.5 | wrong-family bytes | app |
|---|---|---|---|---|---|---|
| dev | cc | 100.00 / 81.90 / 95.77 | **100.00 / 81.90 / 95.77** | +0.00 | 0 -> 0 | 0 |
| dev | esbuild-app | 95.70 / 65.45 / 87.60 | **96.21 / 65.45 / 87.94** | +0.34 | 144,685 -> 126,841 | 0 |
| dev | heldout | 99.09 / 66.67 / 90.31 | **99.16 / 66.66 / 90.35** | +0.04 | 185,893 -> 172,403 | 0 |
| dev | wrangler | 95.99 / 18.52 / 52.26 | **95.99 / 18.52 / 52.26** | +0.00 | 5,652 -> 5,652 | 0 |
| apps | apps-esbuild | 98.53 / 89.70 / 96.63 | **98.64 / 89.69 / 96.71** | +0.08 | 342,936 -> 316,272 | 0 |
| apps | apps-rollup | 97.89 / 76.45 / 92.69 | **97.95 / 76.45 / 92.74** | +0.05 | 1,614,146 -> 1,563,690 | 0 |
| apps | apps-webpack | 97.93 / 72.06 / 91.37 | **97.93 / 72.06 / 91.37** | +0.00 | 1,134,415 -> 1,132,161 | 0 |
| bundlers | dev-rollup | 98.52 / 75.61 / 92.90 | **98.52 / 75.61 / 92.90** | +0.00 | 24,259 -> 24,259 | 0 |
| bundlers | dev-webpack | 97.80 / 61.64 / 87.53 | **97.80 / 61.64 / 87.53** | +0.00 | 25,422 -> 25,422 | 0 |
| pop | pop-bun | 91.32 / 76.75 / 87.98 | **94.84 / 76.90 / 90.61** | +2.63 | 7,384,297 -> 4,237,605 | 0 |
| pop | pop-esbuild | 90.47 / 79.00 / 87.92 | **93.41 / 79.40 / 90.23** | +2.31 | 8,445,300 -> 5,683,553 | 0 |
| test | test-bun | 95.44 / 66.31 / 87.73 | **97.10 / 66.31 / 88.85** | +1.11 | 2,068,971 -> 1,295,108 | 0 |
| test | test-esbuild | 95.86 / 75.87 / 91.06 | **96.34 / 75.91 / 91.42** | +0.36 | 2,259,605 -> 1,987,853 | 0 |
| test | test-rollup | 97.09 / 67.94 / 89.42 | **97.40 / 67.92 / 89.62** | +0.21 | 2,219,836 -> 1,974,781 | 0 |
| test | test-webpack | 98.30 / 70.04 / 90.96 | **98.80 / 70.04 / 91.30** | +0.34 | 1,806,431 -> 1,266,915 | 0 |
| test2 | test2-bun | 98.52 / 77.65 / 93.50 | **98.62 / 77.65 / 93.57** | +0.07 | 485,321 -> 450,923 | 0 |
| test2 | test2-esbuild | 99.41 / 74.17 / 93.07 | **99.41 / 74.16 / 93.08** | +0.00 | 190,108 -> 188,458 | 0 |
| test2 | test2-rollup | 94.65 / 72.98 / 89.34 | **94.84 / 72.78 / 89.42** | +0.08 | 2,438,214 -> 2,339,455 | 0 |
| test2 | test2-webpack | 98.72 / 68.92 / 90.86 | **98.78 / 68.85 / 90.88** | +0.02 | 1,519,110 -> 1,447,918 | 0 |

## Which keys: deltas vs the shipped defaults on k2-all (dP / dF0.5, points)

`cpf` = context_presence 1.5 + slack 16384 + presence_fence; `dmb` = declared_max_bytes 65536; all four = cpf + dmb
+ shadow_ties.

| set | shipped P / F0.5 | cpf dP / dF0.5 | cpf+dmb (NEW DEFAULT) dP / dF0.5 | all four (+shadow_ties) dP / dF0.5 |
|---|---|---|---|---|
| cc | 100.00 / 95.77 | +0.000 / +0.000 | +0.000 / +0.000 | +0.000 / +0.000 |
| esbuild-app | 95.70 / 87.60 | +0.510 / +0.342 | +0.510 / +0.342 | +0.550 / +0.377 |
| heldout | 99.09 / 90.31 | +0.065 / +0.041 | +0.065 / +0.041 | +0.152 / +0.108 |
| wrangler | 95.99 / 52.26 | +0.000 / +0.000 | +0.000 / +0.000 | +0.000 / +0.000 |
| apps-esbuild | 98.53 / 96.63 | +0.112 / +0.084 | +0.112 / +0.084 | +0.201 / +0.157 |
| apps-rollup | 97.89 / 92.69 | +0.065 / +0.045 | +0.065 / +0.045 | +0.606 / +0.583 |
| apps-webpack | 97.93 / 91.37 | +0.004 / +0.002 | +0.004 / +0.002 | +0.674 / +0.626 |
| dev-rollup | 98.52 / 92.90 | +0.000 / +0.000 | +0.000 / +0.000 | +1.348 / +1.506 |
| dev-webpack | 97.80 / 87.53 | +0.000 / +0.000 | +0.000 / +0.000 | +1.845 / +1.580 |
| pop-bun | 91.32 / 87.98 | +0.141 / +0.104 | +3.518 / +2.631 | +3.714 / +2.809 |
| pop-esbuild | 90.47 / 87.92 | +0.212 / +0.183 | +2.940 / +2.308 | +3.115 / +2.476 |
| test-bun | 95.44 / 87.73 | +0.207 / +0.140 | +1.656 / +1.114 | +1.788 / +1.222 |
| test-esbuild | 95.86 / 91.06 | +0.082 / +0.061 | +0.481 / +0.358 | +0.560 / +0.421 |
| test-rollup | 97.09 / 89.42 | +0.033 / +0.016 | +0.312 / +0.206 | +0.787 / +0.638 |
| test-webpack | 98.30 / 90.96 | +0.010 / +0.006 | +0.500 / +0.342 | +0.774 / +0.592 |
| test2-bun | 98.52 / 93.50 | +0.104 / +0.075 | +0.104 / +0.075 | +0.186 / +0.146 |
| test2-esbuild | 99.41 / 93.07 | +0.005 / +0.002 | +0.005 / +0.002 | -0.003 / +0.010 |
| test2-rollup | 94.65 / 89.34 | +0.205 / +0.145 | +0.192 / +0.076 | +1.474 / +1.392 |
| test2-webpack | 98.72 / 90.86 | +0.059 / +0.036 | +0.058 / +0.016 | +0.406 / +0.348 |

- **cpf alone** is clean on all 19 sets.
- **cpf + dmb** (the new default) is clean on all 19 sets against the shipped defaults. `dmb` carries the large
  pop/test gains: pop-bun +2.5, test-bun +1.0 F0.5 over cpf. Its MARGINAL cost on top of cpf is recall on two test2
  sets: test2-rollup F0.5 -0.07 (P -0.013, R -0.20) and test2-webpack F0.5 -0.02 (P -0.001, R -0.06). In both,
  wrong-family bytes also fall (-104 and -141), so the loss is right claims seeded by a declared manifest inside
  a >64 KB statement. On the old step4-wr index, `dmb` alone also cost dev apps-webpack F0.5 -0.09 (recall). That
  set is flat on k2-all. If the coordinator reads the rule as "no key with any marginal drop", drop `dmb` too:
  it is one line in `recommended()`.
- **shadow_ties** (all four vs cpf+dmb) adds +1.5 F0.5 on dev-rollup/dev-webpack and +1.3 on test2-rollup. It is
  what removes exp113's @redocly/ajv fork claims: dev-bundlers wrong bytes go 24,259 -> 2,114 (rollup) and
  25,422 -> 4,139 (webpack), and only all-four moves them. It also adds +0.5-0.6 on apps-rollup/apps-webpack.
  Its one cost is test2-esbuild: P -0.0029 points, +1,035 wrong-family bytes. F0.5 still rises (+0.010) there.
  Under the stated rule (any precision drop is real; leave out the key) it stays off. **Recommendation for the
  coordinator:** this 1,035 B can't be diagnosed on test2. Its counterpart on every dev/test set is a gain. A
  follow-up lane could look for the same mechanism on dev/pop (shadow_ties moving a seed to a rival that is wrong)
  and gate the key on that.

## Attribution of the test2-esbuild drop (summary-only ablations, test2-esbuild set)

| keys | P | R | F0.5 | wrong-family B |
|---|---|---|---|---|
| shipped | 99.4091 | 74.1673 | 93.0738 | 190,108 |
| shadow_ties | 99.4063 | 74.2188 | 93.0881 | 191,143 |
| cpf | 99.4142 | 74.1611 | 93.0754 | 188,458 |
| dmb | 99.4091 | 74.1673 | 93.0738 | 190,108 |
| st + cpf | 99.4062 | 74.2069 | 93.0843 | 191,143 |
| all four | 99.4062 | 74.2069 | 93.0843 | 191,143 |

These ran on k2-l12m4. exp113 showed that Bun/esbuild sets score byte-for-byte the same on k2-l12m4 and k2-all, and
the k2-all all-four row reproduces 191,143 exactly. Labels: `test2-exp117-{st,cpf,dmb,p3}-k2-structural`.

## Phase 1 (superseded): k2-l12m4 + step4-wr-v3, before exp113 landed

The first pass scored the same candidates against the then-shipped defaults (main `126dd3a9`) on the old index pair.
The coordinator then moved the default index to k2-all, and every decision above was re-made there. Phase-1 findings,
for the record:
- all four keys: the only drop was the same test2-esbuild 1,035 B.
- cpf + dmb: dropped F0.5 on test2-rollup (-0.16), test2-webpack (-0.07) and apps-webpack (-0.07).
- dmb alone: dropped apps-webpack (-0.09), test2-rollup (-0.17) and test2-webpack (-0.05).
- cpf alone: tiny recall-only drops on test-webpack (-0.03) and test2-webpack (-0.02).

On step4-wr, nothing but all-four was clean except for the test2-esbuild drop. On k2-all, webpack/rollup recall
roughly doubles and the cpf / cpf+dmb drops are gone. Labels: `*exp117-{base,flip4,flip3,dmb,cpf}-*`, `-k2`/`-wr`
and the test2 `-bun-` variants. Tables: below.

<details><summary>Phase-1 tables (base = shipped defaults at 126dd3a9)</summary>

All four keys:

| corpus | set | P base -> flip4 | R | F0.5 | wrong B | app B |
|---|---|---|---|---|---|---|
| dev | cc | 100.00 -> 100.00 (+0.00) | 81.90 -> 81.90 (+0.00) | 95.77 -> 95.77 (+0.00) | 0 -> 0 | 0 -> 0 |
| dev | esbuild-app | 95.69 -> 96.24 (+0.55) | 65.36 -> 65.39 (+0.03) | 87.57 -> 87.94 (+0.38) | 144,685 -> 125,529 | 0 -> 0 |
| dev | heldout | 99.09 -> 99.24 (+0.15) | 66.33 -> 66.35 (+0.02) | 90.18 -> 90.29 (+0.11) | 185,893 -> 154,754 | 0 -> 0 |
| dev | wrangler | 95.99 -> 95.99 (+0.00) | 18.52 -> 18.52 (+0.00) | 52.26 -> 52.26 (+0.00) | 5,652 -> 5,652 | 0 -> 0 |
| apps | apps-esbuild | 98.53 -> 98.73 (+0.20) | 89.70 -> 89.71 (+0.01) | 96.63 -> 96.78 (+0.16) | 342,936 -> 295,542 | 0 -> 0 |
| apps | apps-rollup | 96.29 -> 97.60 (+1.31) | 35.47 -> 35.94 (+0.47) | 71.70 -> 72.67 (+0.97) | 1,338,573 -> 866,309 | 0 -> 0 |
| apps | apps-webpack | 97.34 -> 97.68 (+0.34) | 46.65 -> 46.44 (-0.22) | 79.96 -> 80.02 (+0.06) | 950,176 -> 821,247 | 0 -> 0 |
| bundlers | dev-rollup | 99.84 -> 99.84 (+0.00) | 71.14 -> 71.14 (+0.00) | 92.39 -> 92.39 (+0.00) | 2,452 -> 2,452 | 0 -> 0 |
| bundlers | dev-webpack | 99.59 -> 99.62 (+0.03) | 59.24 -> 59.26 (+0.02) | 87.65 -> 87.68 (+0.03) | 4,510 -> 4,131 | 0 -> 0 |
| pop | pop-bun | 91.32 -> 95.03 (+3.71) | 76.75 -> 77.03 (+0.28) | 87.98 -> 90.79 (+2.81) | 7,384,297 -> 4,075,571 | 0 -> 0 |
| pop | pop-esbuild | 90.47 -> 93.59 (+3.12) | 79.00 -> 79.54 (+0.54) | 87.92 -> 90.40 (+2.48) | 8,445,300 -> 5,532,133 | 0 -> 0 |
| test | test-bun | 95.44 -> 97.23 (+1.79) | 66.31 -> 66.36 (+0.05) | 87.73 -> 88.95 (+1.22) | 2,068,971 -> 1,235,745 | 0 -> 0 |
| test | test-esbuild | 95.86 -> 96.42 (+0.56) | 75.86 -> 75.92 (+0.06) | 91.06 -> 91.48 (+0.42) | 2,259,608 -> 1,944,389 | 0 -> 0 |
| test | test-rollup | 94.62 -> 95.43 (+0.81) | 35.59 -> 35.70 (+0.11) | 71.05 -> 71.50 (+0.45) | 2,208,539 -> 1,865,925 | 0 -> 0 |
| test | test-webpack | 97.95 -> 99.03 (+1.07) | 39.02 -> 39.03 (+0.01) | 75.23 -> 75.74 (+0.51) | 1,220,313 -> 574,025 | 0 -> 0 |
| test2 | test2-bun | 98.52 -> 98.71 (+0.19) | 77.65 -> 77.70 (+0.04) | 93.50 -> 93.64 (+0.15) | 485,321 -> 423,818 | 0 -> 0 |
| test2 | test2-esbuild | 99.41 -> 99.41 (-0.00) | 74.17 -> 74.21 (+0.04) | 93.07 -> 93.08 (+0.01) | 190,108 -> 191,143 | 0 -> 0 |
| test2 | test2-rollup | 96.69 -> 97.11 (+0.42) | 44.87 -> 44.89 (+0.02) | 78.55 -> 78.78 (+0.23) | 907,207 -> 789,642 | 0 -> 0 |
| test2 | test2-webpack | 98.29 -> 98.62 (+0.33) | 31.76 -> 31.73 (-0.03) | 69.27 -> 69.37 (+0.10) | 943,878 -> 758,293 | 0 -> 0 |

cpf + dmb:

| corpus | set | P base -> flip3 | R | F0.5 | wrong B | app B |
|---|---|---|---|---|---|---|
| dev | cc | 100.00 -> 100.00 (+0.00) | 81.90 -> 81.90 (+0.00) | 95.77 -> 95.77 (+0.00) | 0 -> 0 | 0 -> 0 |
| dev | esbuild-app | 95.69 -> 96.20 (+0.51) | 65.36 -> 65.36 (-0.00) | 87.57 -> 87.91 (+0.34) | 144,685 -> 126,841 | 0 -> 0 |
| dev | heldout | 99.09 -> 99.15 (+0.07) | 66.33 -> 66.32 (-0.01) | 90.18 -> 90.22 (+0.04) | 185,893 -> 172,403 | 0 -> 0 |
| dev | wrangler | 95.99 -> 95.99 (+0.00) | 18.52 -> 18.52 (+0.00) | 52.26 -> 52.26 (+0.00) | 5,652 -> 5,652 | 0 -> 0 |
| apps | apps-esbuild | 98.53 -> 98.64 (+0.11) | 89.70 -> 89.69 (-0.01) | 96.63 -> 96.71 (+0.08) | 342,936 -> 316,272 | 0 -> 0 |
| apps | apps-rollup | 96.29 -> 96.29 (+0.01) | 35.47 -> 35.46 (-0.01) | 71.70 -> 71.70 (-0.00) | 1,338,573 -> 1,335,903 | 0 -> 0 |
| apps | apps-webpack | 97.34 -> 97.36 (+0.02) | 46.65 -> 46.51 (-0.15) | 79.96 -> 79.89 (-0.07) | 950,176 -> 940,131 | 0 -> 0 |
| bundlers | dev-rollup | 99.84 -> 99.84 (+0.00) | 71.14 -> 71.14 (+0.00) | 92.39 -> 92.39 (+0.00) | 2,452 -> 2,452 | 0 -> 0 |
| bundlers | dev-webpack | 99.59 -> 99.59 (+0.00) | 59.24 -> 59.24 (+0.00) | 87.65 -> 87.65 (+0.00) | 4,510 -> 4,510 | 0 -> 0 |
| pop | pop-bun | 91.32 -> 94.84 (+3.52) | 76.75 -> 76.90 (+0.15) | 87.98 -> 90.61 (+2.63) | 7,384,297 -> 4,237,605 | 0 -> 0 |
| pop | pop-esbuild | 90.47 -> 93.41 (+2.94) | 79.00 -> 79.40 (+0.40) | 87.92 -> 90.23 (+2.31) | 8,445,300 -> 5,683,553 | 0 -> 0 |
| test | test-bun | 95.44 -> 97.10 (+1.66) | 66.31 -> 66.31 (-0.00) | 87.73 -> 88.85 (+1.11) | 2,068,971 -> 1,295,108 | 0 -> 0 |
| test | test-esbuild | 95.86 -> 96.34 (+0.48) | 75.86 -> 75.90 (+0.04) | 91.06 -> 91.42 (+0.36) | 2,259,608 -> 1,987,856 | 0 -> 0 |
| test | test-rollup | 94.62 -> 95.14 (+0.52) | 35.59 -> 35.59 (+0.00) | 71.05 -> 71.28 (+0.23) | 2,208,539 -> 1,983,695 | 0 -> 0 |
| test | test-webpack | 97.95 -> 98.82 (+0.87) | 39.02 -> 38.98 (-0.04) | 75.23 -> 75.61 (+0.38) | 1,220,313 -> 693,365 | 0 -> 0 |
| test2 | test2-bun | 98.52 -> 98.62 (+0.10) | 77.65 -> 77.65 (+0.00) | 93.50 -> 93.57 (+0.07) | 485,321 -> 450,923 | 0 -> 0 |
| test2 | test2-esbuild | 99.41 -> 99.41 (+0.01) | 74.17 -> 74.16 (-0.01) | 93.07 -> 93.08 (+0.00) | 190,108 -> 188,458 | 0 -> 0 |
| test2 | test2-rollup | 96.69 -> 96.61 (-0.08) | 44.87 -> 44.67 (-0.20) | 78.55 -> 78.38 (-0.16) | 907,207 -> 926,200 | 0 -> 0 |
| test2 | test2-webpack | 98.29 -> 98.29 (+0.01) | 31.76 -> 31.68 (-0.08) | 69.27 -> 69.20 (-0.07) | 943,878 -> 937,793 | 0 -> 0 |

dmb alone (test2-bun in `test2-exp117-dmb-bun-k2-structural`: identical to base):

| corpus | set | P base -> dmb | R | F0.5 | wrong B | app B |
|---|---|---|---|---|---|---|
| dev | cc | 100.00 -> 100.00 (+0.00) | 81.90 -> 81.90 (+0.00) | 95.77 -> 95.77 (+0.00) | 0 -> 0 | 0 -> 0 |
| dev | esbuild-app | 95.69 -> 95.69 (+0.00) | 65.36 -> 65.36 (+0.00) | 87.57 -> 87.57 (+0.00) | 144,685 -> 144,685 | 0 -> 0 |
| dev | heldout | 99.09 -> 99.09 (+0.00) | 66.33 -> 66.33 (+0.00) | 90.18 -> 90.18 (+0.00) | 185,893 -> 185,893 | 0 -> 0 |
| dev | wrangler | 95.99 -> 95.99 (+0.00) | 18.52 -> 18.52 (+0.00) | 52.26 -> 52.26 (+0.00) | 5,652 -> 5,652 | 0 -> 0 |
| apps | apps-esbuild | 98.53 -> 98.53 (+0.00) | 89.70 -> 89.70 (+0.00) | 96.63 -> 96.63 (+0.00) | 342,936 -> 342,936 | 0 -> 0 |
| apps | apps-rollup | 96.29 -> 96.29 (+0.00) | 35.47 -> 35.47 (+0.00) | 71.70 -> 71.70 (+0.00) | 1,338,573 -> 1,338,573 | 0 -> 0 |
| apps | apps-webpack | 97.34 -> 97.33 (-0.01) | 46.65 -> 46.51 (-0.14) | 79.96 -> 79.87 (-0.09) | 950,176 -> 950,176 | 0 -> 0 |
| bundlers | dev-rollup | 99.84 -> 99.84 (+0.00) | 71.14 -> 71.14 (+0.00) | 92.39 -> 92.39 (+0.00) | 2,452 -> 2,452 | 0 -> 0 |
| bundlers | dev-webpack | 99.59 -> 99.59 (+0.00) | 59.24 -> 59.24 (+0.00) | 87.65 -> 87.65 (+0.00) | 4,510 -> 4,510 | 0 -> 0 |
| pop | pop-bun | 91.32 -> 94.68 (+3.37) | 76.75 -> 76.90 (+0.15) | 87.98 -> 90.50 (+2.52) | 7,384,297 -> 4,368,859 | 0 -> 0 |
| pop | pop-esbuild | 90.47 -> 93.19 (+2.72) | 79.00 -> 79.30 (+0.30) | 87.92 -> 90.04 (+2.12) | 8,445,300 -> 5,880,601 | 0 -> 0 |
| test | test-bun | 95.44 -> 96.88 (+1.44) | 66.31 -> 66.31 (-0.00) | 87.73 -> 88.70 (+0.97) | 2,068,971 -> 1,393,520 | 0 -> 0 |
| test | test-esbuild | 95.86 -> 96.26 (+0.40) | 75.86 -> 75.89 (+0.03) | 91.06 -> 91.35 (+0.30) | 2,259,608 -> 2,034,410 | 0 -> 0 |
| test | test-rollup | 94.62 -> 95.13 (+0.51) | 35.59 -> 35.59 (+0.00) | 71.05 -> 71.28 (+0.23) | 2,208,539 -> 1,988,797 | 0 -> 0 |
| test | test-webpack | 97.95 -> 98.83 (+0.87) | 39.02 -> 39.02 (+0.00) | 75.23 -> 75.64 (+0.41) | 1,220,313 -> 693,365 | 0 -> 0 |
| test2 | test2-esbuild | 99.41 -> 99.41 (+0.00) | 74.17 -> 74.17 (+0.00) | 93.07 -> 93.07 (+0.00) | 190,108 -> 190,108 | 0 -> 0 |
| test2 | test2-rollup | 96.69 -> 96.60 (-0.09) | 44.87 -> 44.67 (-0.20) | 78.55 -> 78.38 (-0.17) | 907,207 -> 929,327 | 0 -> 0 |
| test2 | test2-webpack | 98.29 -> 98.28 (-0.00) | 31.76 -> 31.70 (-0.06) | 69.27 -> 69.21 (-0.05) | 943,878 -> 943,737 | 0 -> 0 |

cpf alone (test2-bun in `test2-exp117-cpf-bun-k2-structural`: P 98.62, F0.5 93.57):

| corpus | set | P base -> cpf | R | F0.5 | wrong B | app B |
|---|---|---|---|---|---|---|
| dev | cc | 100.00 -> 100.00 (+0.00) | 81.90 -> 81.90 (+0.00) | 95.77 -> 95.77 (+0.00) | 0 -> 0 | 0 -> 0 |
| dev | esbuild-app | 95.69 -> 96.20 (+0.51) | 65.36 -> 65.36 (-0.00) | 87.57 -> 87.91 (+0.34) | 144,685 -> 126,841 | 0 -> 0 |
| dev | heldout | 99.09 -> 99.15 (+0.07) | 66.33 -> 66.32 (-0.01) | 90.18 -> 90.22 (+0.04) | 185,893 -> 172,403 | 0 -> 0 |
| dev | wrangler | 95.99 -> 95.99 (+0.00) | 18.52 -> 18.52 (+0.00) | 52.26 -> 52.26 (+0.00) | 5,652 -> 5,652 | 0 -> 0 |
| apps | apps-esbuild | 98.53 -> 98.64 (+0.11) | 89.70 -> 89.69 (-0.01) | 96.63 -> 96.71 (+0.08) | 342,936 -> 316,272 | 0 -> 0 |
| apps | apps-rollup | 96.29 -> 96.29 (+0.01) | 35.47 -> 35.46 (-0.01) | 71.70 -> 71.70 (-0.00) | 1,338,573 -> 1,335,903 | 0 -> 0 |
| apps | apps-webpack | 97.34 -> 97.37 (+0.03) | 46.65 -> 46.65 (-0.00) | 79.96 -> 79.98 (+0.01) | 950,176 -> 940,131 | 0 -> 0 |
| bundlers | dev-rollup | 99.84 -> 99.84 (+0.00) | 71.14 -> 71.14 (+0.00) | 92.39 -> 92.39 (+0.00) | 2,452 -> 2,452 | 0 -> 0 |
| bundlers | dev-webpack | 99.59 -> 99.59 (+0.00) | 59.24 -> 59.24 (+0.00) | 87.65 -> 87.65 (+0.00) | 4,510 -> 4,510 | 0 -> 0 |
| pop | pop-bun | 91.32 -> 91.46 (+0.14) | 76.75 -> 76.75 (-0.00) | 87.98 -> 88.08 (+0.10) | 7,384,297 -> 7,253,043 | 0 -> 0 |
| pop | pop-esbuild | 90.47 -> 90.69 (+0.21) | 79.00 -> 79.10 (+0.10) | 87.92 -> 88.10 (+0.18) | 8,445,300 -> 8,248,252 | 0 -> 0 |
| test | test-bun | 95.44 -> 95.65 (+0.21) | 66.31 -> 66.31 (-0.00) | 87.73 -> 87.87 (+0.14) | 2,068,971 -> 1,970,559 | 0 -> 0 |
| test | test-esbuild | 95.86 -> 95.94 (+0.08) | 75.86 -> 75.86 (+0.00) | 91.06 -> 91.12 (+0.06) | 2,259,608 -> 2,213,054 | 0 -> 0 |
| test | test-rollup | 94.62 -> 94.63 (+0.01) | 35.59 -> 35.59 (+0.00) | 71.05 -> 71.05 (+0.01) | 2,208,539 -> 2,203,437 | 0 -> 0 |
| test | test-webpack | 97.95 -> 97.95 (-0.00) | 39.02 -> 38.98 (-0.04) | 75.23 -> 75.20 (-0.03) | 1,220,313 -> 1,220,313 | 0 -> 0 |
| test2 | test2-esbuild | 99.41 -> 99.41 (+0.01) | 74.17 -> 74.16 (-0.01) | 93.07 -> 93.08 (+0.00) | 190,108 -> 188,458 | 0 -> 0 |
| test2 | test2-rollup | 96.69 -> 96.70 (+0.01) | 44.87 -> 44.87 (-0.00) | 78.55 -> 78.55 (+0.00) | 907,207 -> 904,080 | 0 -> 0 |
| test2 | test2-webpack | 98.29 -> 98.30 (+0.01) | 31.76 -> 31.74 (-0.02) | 69.27 -> 69.25 (-0.02) | 943,878 -> 937,934 | 0 -> 0 |

</details>

## @redocly/ajv over ajv (exp113's dev-bundlers fork claim): not fixed by families here

The derived families (`crates/humanify-core/src/libindex/families-derived.json`, exp104's `derive_families.py`) do
not join `@redocly/ajv` and `ajv` because `@redocly/ajv` is not in the universe the file was derived over. That
universe is the names the truths, origins and reports mentioned at exp104 time, and the registry cache
(`/work/exp104-data/registry`) has no entry for it. The generic fix is to re-derive the file over a universe that
includes every package of the default index. The fork rule (same repository name, GitHub records it as a fork) would
then join pairs like this one. That is NOT a clean change for this lane: the scorer's `family()` reads the SAME file
(exp106 made it one file), so re-deriving it changes the truth judgement of every label. It needs its own lane with a
full re-score. `shadow_ties` already removes most of those bytes (above).

## Reproduce

```bash
IX=/work/exp113-data/ix/k2-all
CPF="--pipeline-set library.discovery.context_presence=1.5 --pipeline-set library.discovery.context_presence_slack=16384 --pipeline-set library.discovery.presence_fence=true"
DMB="--pipeline-set library.discovery.declared_max_bytes=65536"
ST="--pipeline-set library.discovery.shadow_ties=true"
C="--index $IX --rule structural --cpus 0-11 --jobs 4"
npm run eval -- library exp117-k2all-cpfdmb $C $CPF $DMB
npm run eval -- library exp117-k2all-cpfdmb-apps $C --corpus apps $CPF $DMB
npm run eval -- library exp117-k2all-cpfdmb-bundlers $C --corpus bundlers $CPF $DMB
npm run eval -- library pop-exp117-k2all-cpfdmb $C --corpus pop $CPF $DMB
npm run eval -- library test-exp117-k2all-cpfdmb $C --corpus test $CPF $DMB
npm run eval -- library test2-exp117-k2all-cpfdmb $C --corpus test2 $CPF $DMB
# same for cpf ($CPF) and flip4 ($ST $CPF $DMB); at 0803d8e4 with no --pipeline-set = exp117-k2all-head*
# neutrality (under flock /work/neutrality.lock):
npm run eval -- neutrality origin/main 2.1.215:2.1.216 --priors /work/exp050-cold --cache /work/neutrality-cache
npm run eval -- neutrality origin/main 2.1.215:2.1.216 --priors /work/exp050-cold --cache /work/neutrality-cache \
  --pipeline-set library.matching=report --pipeline-set library.index=/work/exp113-data/ix/k2-all
```
