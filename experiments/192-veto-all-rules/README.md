# exp192: `library.discovery.veto_support` defaults to `all-rules`

**STATUS (2026-11-07): MERGED as the default.** Andrew approved the flip on 2026-11-06, conditional on this verdict. The isolation veto now counts support from every rule, not only module support. The old behaviour stays available as `veto_support = "without-module"`. Evidence: exp189 (`experiments/189-dropped-seeds/`, branch exp189-dropped-seeds). That run attributed 73% of the ">= 50% cover but unclaimed" library bytes on bun-dev to the isolation veto: 7.0 MB right against 60 KB wrong.

## Library score (cb64 index, structural rule; byte-equal to exp189-allrules-\*)

| set           | precision | recall | vs control (exp188-ctl) |
| ------------- | --------- | ------ | ----------------------- |
| bun-dev-bun   | 99.834%   | 86.40% | P +0.004, R +2.47       |
| bun-dev-node  | 99.744%   | 85.94% | P =, R +2.79            |
| bun-held-bun  | 99.976%   | 86.97% | P +0.001, R +1.31       |
| bun-held-node | 99.716%   | 83.15% | P +0.005, R +1.28       |
| cc            | 100%      | 81.90% | unchanged               |

App bytes claimed: 0 on every set. The two node-target sets were already under the 99.8% bar before this change and stay under it.

## Placement in names mode (bun-dev vendor precision, `npm run eval -- vendor-precision bun-dev`)

Both sides: 71 bundles scored, the same 10 stub-run failures on both sides (pre-existing).

|                                           | main (f8bbb20ae)        | all-rules               |
| ----------------------------------------- | ----------------------- | ----------------------- |
| app bytes under vendor/ (era / factory)   | 0 / 378                 | 0 / 378 (unchanged)     |
| era bytes filed right / wrong package     | 432,098 / 38,263        | 447,542 / 38,263        |
| factory bytes filed right / wrong package | 136,942,686 / 1,466,805 | 137,179,008 / 1,467,972 |

All new placement is right-package except 1.2 KB.

## Eval (names mode, scratch bases, cold; vs ref-names-0357696e-r1)

- **Hold columns:** novel 4,188 / realLn 416,377, byte-equal (`=`).
- **Run health:** all 12 pairs exit 0. Boots OK. The cold self-hop is 8 ln, inside 8-8, and the warm self-hop is byte-identical.
- **Totals:** treeLn 146,767 -> 145,644, noise 3,123 -> 2,739, noiseLn 65,539 -> 50,065. All are inside the names-mode bands, which are wide and still bimodal (see CLAUDE.md).
- **APP:** app:novel and app:realLn are equal; app:noise 3,080 -> 2,695.
- **vendor:vendorReal 10,696 -> 9,634 (-1,062), a `=` column.**
  - The whole move sits on 2.1.197->198, in `vendor/highlight.js/lib/languages/*`: 7,357 -> 6,299 changed lines.
  - In the reference, each language file's `__commonJS((a, b) => …)` wrapper drew different LLM names on the two sides. For example, bash.js had `(unusedParam, module)` against `(initBashLanguage, moduleContext)`, so the diff charged the whole file as dependency change.
  - With all-rules, highlight.js is claimed on both sides, its wrapper reads the same, and only the real highlight.js 11 edits remain (e.g. `concatenateSourceStrings(…)` -> `regex.concat(…)`).
  - So the -1,062 is naming noise that had been counted as real dependency change. It is not real change dropped.

## Gate

13/13 in the worktree and from a clean checkout (see merge commit).
