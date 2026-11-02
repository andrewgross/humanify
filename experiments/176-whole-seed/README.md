# exp176 — the whole-seed rule's cover exception (exp167 recall lever #1)

STATUS: BUILT, judged (2026-11-02), branch `exp176-whole-seed` off main `5a48564bd`. Default ON at
`library.discovery.factory_whole_cover = 0.95`. Library score judged on bun-dev (dev), bun-held (held-out, one
shot) and cc. The re-minified terser/esbuild sets were NOT judged: their frozen corpora are not on this box
(see "What was not proven"). Pipeline effect on Claude Code: none — discovery claims identical on all eight eval
versions, warm neutrality NEUTRAL in names mode on 2.1.215→2.1.216; the cold eval label was NOT run (see "The
pipeline effect"). Names-mode vendor placement on bun-dev: +1.81 MB placed under the right package, +159 B in a wrong
package's folder, app bytes in vendor/ unchanged. Gate 13/13 from a clean checkout of `4cad27670`.

## Totals first

| set (scorer 14, index cb64c1f5) | named bytes before → after |    added right | wrong family | app claimed | precision before → after | recall before → after |
| ------------------------------- | -------------------------: | -------------: | -----------: | ----------: | -----------------------: | --------------------: |
| bun-dev-bun                     |    85,450,834 → 87,903,712 | **+2,452,878** | 149,866 (+0) |       0 → 0 |          99.825 → 99.830 |       81.593 → 83.935 |
| bun-dev-node                    |    90,622,311 → 91,115,401 |   **+493,090** | 234,218 (+0) |       0 → 0 |          99.742 → 99.744 |       82.702 → 83.152 |
| bun-held-bun (held-out)         |    21,833,276 → 22,372,270 |   **+538,994** |   5,631 (+0) |       0 → 0 |          99.974 → 99.975 |       83.591 → 85.655 |
| bun-held-node (held-out)        |    22,717,588 → 22,964,433 |   **+246,845** |  66,464 (+0) |       0 → 0 |          99.708 → 99.711 |       80.986 → 81.866 |
| cc 2.1.216 (sample)             |      1,819,438 → 1,819,438 |             +0 |       0 (+0) |       0 → 0 |        100.000 → 100.000 |       81.903 → 81.903 |

**+3.73 MB named right across the four Bun sets, zero wrong-family bytes and zero app bytes added anywhere.**
Precision rises on every set (it can only rise when right bytes grow and wrong bytes do not). bun-held-node sits
under the 99.8% bar before and after (99.708 → 99.711: the existing wrong bytes, unchanged). The base labels
reproduce exp167's numbers exactly (bun-dev-bun P 99.825 / R 81.593, bun-dev-node P 99.742 / R 82.702).

The carrier stays refused: with the rule fully off (exp167's toggle) playwright-core's pre-bundled `utilsBundle.js`
in `vscode-github-actions` (bun target) is claimed as @vue/compiler-sfc / @mastra/\* on 48 spans (34.8 KB). Under
this rule, at every threshold measured (0.5 to 0.98), 46 of those spans stay unclaimed; the other 2 are claimed as
@vue/compiler-sfc by main already (the base label), unchanged.

## What was built

The factory whole-seed rule (exp132, `factory_whole_seed`) keeps a family's index seeds in a bundled CommonJS module
only when one of them is a WHOLE statement the family's index holds (or a declared / banner seed). exp167 measured
it refusing 4.04 MB on bun-dev, mostly UMD/IIFE libraries whose whole statement print drifts while their function
prints match (libsodium, xterm, jquery, sanitize-html, azure-devops-node-api, the MCP SDK).

New: the module is ALSO backed for a family whose index prints cover at least `factory_whole_cover` of the
**module's** nodes, summed over its body statements (per statement: the whole print, else the largest
non-overlapping function prints at the 20-node row floor — `family_cover`, the scoring every other cover guard
uses), **counting only prints the family holds in an index module the lookup's specificity rule reads** (the same
filter `whole_held` applies). That last clause is how the carrier case stays out: code a family holds only inside a
far bigger file (a pre-bundled carrier's dist) is not that family's file, so it adds no cover.

- `crates/humanify-core/src/libdetect/discovery.rs`: `DiscoveryOptions::factory_whole_cover` (on in
  `recommended()` at 0.95), `factory_whole` reads it, `Containment::covered_nodes` (new) and
  `Containment::specific` (the specificity test, now shared by `whole_held`, `copy_share` and `covered_nodes`;
  three copies became one).
- `crates/humanify-cli`: ONE key `library.discovery.factory_whole_cover` (float 0..1, 0 = no exception), the
  example config regenerated, `docs/library-matching-config.md` and `docs/responsibility.md` (the Containment
  row) updated.
- Tests (red first, then green): `factory_whole_cover_backs_a_module_its_family_s_prints_nearly_cover` (a
  one-statement module covered → kept; a function plus one-line statements → kept; a one-statement module whose
  covered print is an inner helper → refused; a covered helper statement beside a bigger statement of the module's
  own → refused; the cover is what refuses those two (a 0.05 bar takes both); a function held only in a far bigger
  index module adds no cover); `the_factory_whole_cover_key_reads_from_the_file` (default 0.95, range checked).

### Reuse, and what earlier lessons it avoids

- Carriers: recognised by the existing specificity rule (index module > 4× + 4 KB of the bundle's module), exactly
  as `whole_held` already does. No new carrier test, no carrier indexing (exp148 refuted that), no tie vote
  (exp149), no veto relaxation (exp143/144/147): the isolation veto, tie fence, shadow ties and copy presence all
  still run before this rule, unchanged.
- Cover: `family_cover`, the one cover scorer `covers` / `outweighed` / `contested_thin` already use.

## Design: why the MODULE's cover, not "single-statement modules only"

The brief asked for single-statement modules only. That was built first (arm A, commit `0188af370`) and measured;
it reaches only a quarter of the lever, because the lever's own named targets are not single-statement modules.
Census (`census.py`, exp167's main-binary reports, bun-dev, the 4.04 MB exp167 classes `guard.presence`):

| module shape       | bytes refused | top packages                                                                         |
| ------------------ | ------------: | ------------------------------------------------------------------------------------ |
| 1 body statement   |       1.11 MB | @xterm/xterm 487K, jquery 191K, @microsoft/1ds-post-js 151K, libsodium-wrappers 110K |
| 2 body statements  |       0.94 MB | **libsodium 843K** (`"use strict"`-style prologue + the IIFE), tree-sitter-wasm 92K  |
| 3+ body statements |       1.99 MB | **azure-devops-node-api 1.05M**, MCP client/server 590K, **sanitize-html 260K**      |

libsodium's module is 2 statements with module cover 1.000; sanitize-html's is 5 statements with module cover
0.999. A one-line prologue or `module.exports = X` beside a UMD body is not evidence against it. The module-level
cover keeps the rule exactly where it was made to protect — a shared helper or an inlined copy covers PART of a
module (the test's N module: one statement fully covered, a third of the module) — and is identical to arm A on a
one-statement module.

Measured, same commit family, bun-dev, every threshold (bytes named right added; wrong-family and app bytes added
were **0 at every point of both arms**):

| threshold | arm A (1 statement only) bun / node | module cover (shipped) bun / node |
| --------: | ----------------------------------: | --------------------------------: |
|      0.50 |                +1,010,379 / +99,688 |             +2,771,340 / +640,600 |
|      0.65 |                                   – |             +2,722,593 / +640,600 |
|      0.80 |                  +961,927 / +95,558 |             +2,693,510 / +619,573 |
|      0.90 |                        +866,366 / 0 |             +2,565,894 / +493,090 |
|      0.95 |                        +756,454 / 0 |         **+2,452,878 / +493,090** |
|      0.98 |                                   – |             +2,107,309 / +158,423 |

## How the threshold was derived (0.95)

The measured dev runs cannot place the threshold: no threshold from 0.5 up adds a single wrong byte, so the wrong
side never shows (rule 11: a gate cannot resolve what it never sees). The only evidence on the wrong side is the
census's first-order view: every unclaimed CommonJS module on bun-dev, the family the structural rule picks for
its largest statement, and that family's module cover, judged against truth with NO other guard applied. By cover
bin, the share of those modules' bytes that are a WRONG family (multi-statement modules; one-statement modules
show no wrong bytes in any bin):

| module cover | < 0.5 | 0.5–0.8 | 0.8–0.9 | 0.9–0.95 | 0.95–0.98 | 0.98–0.99 | 0.99–1.0 |
| -----------: | ----: | ------: | ------: | -------: | --------: | --------: | -------: |
|  wrong share |   21% |      7% |     24% |      13% |    **5%** |      1.6% |     0.1% |

(right / wrong KB per bin: 200/54, 2,347/165, 708/229, 470/70, 1,470/75, 640/11, 2,880/4.) 0.95 is the lowest bin
edge above which the first-order wrong share falls to 5% or less (an order of magnitude under the 13–24% just
below it); the other guards remove the rest, as the measured zero shows. Its measured cost against 0.9 is 113 KB on
bun-dev-bun (4% of the gain), 0 on bun-dev-node. Precision first: the dev data allow 0.5 (+3.41 MB), the one piece
of wrong-side evidence says 0.95. The threshold was fixed on bun-dev BEFORE the held-out run, which was run once.

## The pipeline effect

Discovery claims on every Claude Code version the eval walks (2.1.85, 86, 118, 119, 197, 198, 215, 216), base vs
candidate, same binary, names-mode config (`humanify discover --bundle-order`): **identical on all eight** — the same claims (3,636–3,649 per version, 0 added, 0 removed) and byte-identical
`bundleOrder` (the names-mode placement answers). No Claude Code module in the walk is a UMD/IIFE factory the rule
refused, so the rule cannot reach the eval's output on these inputs.

**Warm neutrality, names mode (the default), 2.1.215→2.1.216** (`npm run eval -- neutrality 5a48564bd 2.1.215:2.1.216
--cache /work/neutrality-cache --priors /work/exp050-cold`, candidate `c0a6d3557`, crates = `4cad27670`):
**NEUTRAL** — 0 differing files, 0 differing lines, exit codes 0 / 0, cache writes candidate +11,155, **baseline +0**.

**Why no cold eval label.** The brief asked for one names-mode label against `ref-names-0357696e-r1`, on the
expectation that output changes. It does not change on these inputs: the claims and placement answers above are
identical on every version the eval walks, and the warm neutrality run shows byte-identical output. A cold label
would differ from the reference only by the LLM's re-rolls; it cannot resolve an effect of zero and would still
print a sign (rule 11; CLAUDE.md: "for a change that is supposed to alter NOTHING … the eval is the wrong
instrument"). novel/realLn cannot move: no claim moves. If a cold label is still wanted for the record, it is
`npm run eval -- score <label>` at `4cad27670`, ~1 h+, alone under the heavy lock.

**Names-mode vendor placement on bun-dev** (exp166's instrument, `npm run eval -- vendor-precision bun-dev <bin>
<label>`, main `5a48564bd` vs candidate `4cad27670`; 71 of 90 bundles judged — the same 10 stub runs fail on BOTH
binaries with `stable split failed before any tree was written: wrapper detected … absent from the estree json`, a
main defect this change does not touch, and 9 more have no truth-labelled vendor bytes):

| quantity (factory placement)          |        main |   candidate |          Δ |
| ------------------------------------- | ----------: | ----------: | ---------: |
| library bytes under the right package | 135,132,767 | 136,942,686 | +1,809,919 |
| library bytes left at `vendor/` root  |  26,862,705 |  25,052,627 | −1,810,078 |
| library bytes under a WRONG package   |   1,466,646 |   1,466,805 |       +159 |
| app bytes under vendor/ (bar 0 new)   |         378 |         378 |         +0 |
| era (exact) placement, every column   |           – |           – |  identical |

The +159 B sit in six newly placed files (libsodium, sanitize-html, mousetrap ×2, @xterm/addon-fit, map-stream,
remove-markdown; ~27 B each, consistent with wrapper/boundary bytes the truth labels as a neighbour — not
hand-read). The 378 app bytes are main's, unchanged.

**Instrument fix on the way:** `experiments/166-restore-or-ask/stub-run.mts` still wrote `library.names_unlocked`,
refused since exp171, so every bun-dev vendor-precision run failed on current main; the key is dropped (commit
`c0a6d3557`).

## Gate

`npm run check` from a clean checkout of `4cad27670` (`git worktree add --detach /work/exp176-clean`, fresh
`npm ci --ignore-scripts`): **ALL 13 STAGES PASSED** — typecheck, lint, rust:fmt, rust:clippy, knip, knip:prod,
census:clones, census:concurrency, unit, rust:unit, rust:build, rust:format-golden, e2e (19 fixture pairs).

## What was not proven

- **The re-minified terser / esbuild sets** (remin, remin2-4): their frozen corpora (`/work/exp126-data`,
  `/work/exp140-data`, `/work/exp141-data`, `/work/exp146-data`) are not on this box. The rule only touches cut
  CommonJS factories whose family's prints cover ≥ 95% of the module; terser rewrites statements so that cover is
  rarely reached, but that is an argument, not a measurement. Judge on remin4 (and the remin2 judge) where the data
  lives before relying on it for terser output.
- **cc's CommonJS factories** (5.05 MB) sit outside the cc sample (rule 8); the claims on cc 2.1.216 are byte-equal
  anyway (22,674 claims, 8,233,138 bytes, before and after).
- The multi-statement wrong side at high cover was read first-order only; no measured run reached it.
- Vendor placement on the 10 bun-dev bundles whose stub run fails on main (three of them are bundles this rule
  gains on: dbt-power-user, mysti, hugin-desktop).
- A cold names-mode eval label (not run; see above).

## Files

- `census.py` — the shape census (exp167's reports and truth; imports exp167's census from its branch checkout).
- `data/census-bundev.json` — the census aggregate (bytes by cover bin and statement count, per package). `data/*.summary.json` — every label's scorer summary (per-bundle cards stay in the gitignored `experiments/096-library-matching-score/results/<label>/`). Labels: `bun-dev-exp176-base`,
  `bun-dev-exp176-a{0.5,0.8,0.9,0.95}` (arm A), `bun-dev-exp176-c0.95` (the shipped code, = arm B's 0.95 byte for
  byte), `bun-held-exp176-{base,c0.95}`, `cc-exp176-{base,c0.95}`. Arm B's sweep labels
  `bun-dev-exp176-b{0.5,0.65,0.8,0.9,0.95,0.98}` were scored from the scratch branch `exp176-armB-scratch`
  (`fc0cb83ac`, measurement only).
