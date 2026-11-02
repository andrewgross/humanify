# exp177 — index the versions apps actually install (exp167 recall lever #2)

STATUS: DONE (2026-11-02). Branch `exp177-installed-versions` off main `5a48564bd`. **No `crates/` change**: an
index-side DELTA build plus harness scripts. The delta index is built and scored; it is **not** installed under
`/work/library-index/` and `pairs.json` is **not** repointed (Andrew's call). Judged with main's matcher (exp176's
whole-seed change is not in it), scorer 14 (v3), `npm run eval -- library … --rule structural`.

## Totals first

| set (main's matcher)       |                            P before → after |          R before → after | F0.5 before → after | named bytes added | wrong bytes change |     app claimed |
| -------------------------- | ------------------------------------------: | ------------------------: | ------------------: | ----------------: | -----------------: | --------------: |
| bun-dev-bun                |                         99.825 → **99.826** | 81.59 → **82.27** (+0.68) |     95.554 → 95.741 |          +712,342 |               +273 |           0 → 0 |
| bun-dev-node               |                         99.742 → **99.745** | 82.70 → **83.43** (+0.73) |     95.795 → 95.992 |          +800,404 |               −825 |           0 → 0 |
| bun-held-bun (held out)    |                         99.974 → **99.975** | 83.59 → **84.88** (+1.29) |     96.203 → 96.541 |          +336,435 |                  0 |           0 → 0 |
| bun-held-node (held out)   |                         99.708 → **99.719** | 80.99 → **82.19** (+1.20) |     95.302 → 95.640 |          +338,771 |             −1,377 |           0 → 0 |
| cc (620-unit sample)       |                           100.000 → 100.000 |             81.90 → 81.90 |     95.768 → 95.768 |                 0 |                  0 | 0 → 0 (0 units) |
| terser / esbuild remin set | not judged (see "What could not be proved") |                           |                     |                   |                    |                 |

- **Recall up on every Bun set, precision up or equal on every set, app claimed 0.** bun-dev +1.51 MB named over
  both targets (exp167 sized the lever at ~2.7 MB of claims gain); the held-out split gains MORE in points (+1.2–1.3)
  than the dev split (+0.7), so the rule is not fitted to dev. Wrong-family bytes: +273 / −825 / 0 / −1,377 B.
- **Precision bar (99.8%)**: the two `-bun` sets and cc stay above it; the two `-node` sets were under it before
  (99.742 / 99.708) and are still under it, but went UP (+0.003 / +0.011).
- **One thing to read before adopting** (not app-claimed, but app code): on bun-dev the `self` verdict (the app's
  own package claimed as itself) rose from 0.29 MB to **4.15 MB, all in one app, `cli_just-bash`, both targets**.
  just-bash is not a pinned package; it entered as an OWNER because a newly indexed version of `agents`
  (Cloudflare's SDK, `agents@0.14.5` / `0.16.2`) depends on it, and the index labels a dependency's rows with the
  dependency (`just-bash@3.0.1`). So a CLI that is itself a published package now has its own code claimed as a
  library. The scorer counts `self` apart from app-claimed by design (scorer 14); it also moved 1.29 MB of
  `selfOther` (re2js etc. vendored inside just-bash's dist) to `self`. bun-held's `self`/`selfOther` are unchanged.
  The base index already does this for every dependency owner (12,023 owners vs 9,896 pinned packages); the delta
  adds 189 owners.
- **Index growth**: 65,839 → **75,148 versions (+9,309, +14.1%)**, same 9,896 packages, 12,023 → 12,212 owners;
  46.16 → **50.69 GB (+4.53 GB, +9.8%)**; new index id `8462c502258b9321e1c18121495528a5bbf1ccb7fd4678fd17abe0aa8f494eff`
  at `/work/exp177-data/ix/full-bun-names-iv-asof20261025/` (with a `build.json` sidecar).
- **Build time**: download fetch 6.0 h (rate-limited API, one request per 2.2 s, background); shard phase 3.25 h
  of wall on 30 cores in two passes (1.81 h for the first 7,799 versions, 1.45 h for the remaining 3,239 plus the
  reuse check); merge **4.70 h** (16,931 s, `--merge-jobs 30`) at **103.6 GB** peak RSS (cb64's own merge: 100 min /
  98.7 GB on a quieter box). 1,729 of the 11,038 selected versions failed to install (`no-shard`, all npm
  `ETARGET` under `--before`: the documented as-of limit for lockstep monorepo releases), so 9,309 built.

## The rule (fixed before any score was read)

`scripts/select_versions.py`, per package ALREADY in the base index (never a new package):

1. admissible = stable versions published ≤ the index's as-of (2026-10-25T23:59:59.999Z) in the index's own
   registry snapshot (`/work/exp161-data/registry`), minus every version the base index holds or refused;
2. a **compatibility line** (the major for ≥ 1.0, the 0.minor for 0.x: what a caret range resolves within) is in
   use when its versions hold **≥ 1%** of the package's weekly downloads and **≥ 200/week** → add the line's TOP
   (what `^x.y.z` resolves to on a fresh install);
3. a single version holding **≥ 5%** and **≥ 200/week** is added on its own (lockfile-held patches, exact pins).

Signal: npm's per-version download counts (`api.npmjs.org/versions/<pkg>/last-week`, `scripts/fetch_downloads.py`,
fetched 2026-11-01 for all 9,897 packages). It counts installs in the wild — the versions dependency resolution
actually picks — and reads nothing of the judged bundles. The thresholds were written into the script before the
sizing grid was printed, and the grid was used only to check the build cost (~11k versions); no score was
computed for any other setting. Added: **11,038 versions of 4,851 packages** (5,152 line tops, 5,886 single
versions); by publish year 2,352 from 2026, 2,372 from 2025, 1,506 from 2024, the rest older (old majors).

Why this shape: exp167's version-skew examples (eslint-plugin-jsdoc 50.8.0 vs 61+ indexed, azure-devops-node-api
11.2.0 vs 12.5+, tippy.js 2.6.0, i18n-iso-countries 3.7.8, vsce 2.32.0) are each the TOP of an old major — exactly
what a caret range still in use resolves to. The existing pin rule (year-last-12 + top of the last 4 majors) skips
any major older than that and any 0.minor line of a fast releaser (lucide-react) outside the last year.

**Leakage check.** No bun-dev, bun-held or cc version, report or truth label is an input. After selection (not
before), five exp167 dev examples were looked up in the plan to confirm the rule behaves as designed:
eslint-plugin-jsdoc 50.8.0, azure-devops-node-api 11.2.0, i18n-iso-countries 3.7.8 and lucide-react 0.545.0 were
picked (all four built); tippy.js 2.6.0 and @vscode/vsce were not (their old lines are under the floors). Nothing was changed after that look.

**As-of discipline.** No version published after 2026-10-25 is selected (rule 1), and the builder re-checks it
(`asof::select_versions`). Every dependency installs as of its dependent's publish time (`npm install --before`).
One limit, stated: the DOWNLOAD signal itself is the week before the fetch (2026-10-25..31), one week after the
as-of; npm keeps no per-version history, so this rule cannot be replayed for an old as-of. For a historical index
the line rule would need another popularity source (or run without the download filter: 32,721 line tops,
three times the build).

## Where the gain comes from (bun-dev, claimed span bytes per package, delta − base)

azure-devops-node-api (old major 11), eslint-plugin-jsdoc (old majors), @antfu/eslint-config,
@octokit/plugin-rest-endpoint-methods, @antfu/ni, node-notifier, eslint-plugin-unicorn, i18n-iso-countries (plus
just-bash's self claims, above). Top bundles: gistpad and catalyst (~296 KB each per target), vscode-array-index-inlay
(126–144 KB), vscode-github-actions (82 KB). lucide-react does NOT gain: exp167 put most of its misses in
"print differs" (ESM tree-shaking), which another version cannot fix. (Claimed span bytes overlap; they rank
packages, the scorer's numbers above are the bytes.)

## Gate

| check                                                                                             | result                                                                                         |
| ------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| builder identity: main's `humanify-index` re-merges exp161's 60-version pilot on the copied store | **id 94e1839c… reproduced** (same binary, flags, store as cb64)                                |
| base index ⊂ delta index (every cb64 version still indexed)                                       | yes (65,839 of 65,839)                                                                         |
| library score, main's matcher, scorer 14 — bun-dev, bun-held, cc                                  | table above: P up or equal everywhere, app claimed 0                                           |
| `npm run check`                                                                                   | ALL 13 STAGES PASSED (bare, this tree; and from a clean checkout of the commit)                |
| neutrality                                                                                        | not applicable: no pipeline code changed, and a new index is meant to change names-mode output |

## What could not be proved

- **No terser/esbuild re-minified set was judged.** Every remin corpus (remin, remin2-4) was deleted on 2026-10-25
  (docs/design/library-index.md §10), and the full index is Bun-only (two Bun slices): a terser- or esbuild-built
  bundle reads an esbuild slice the index does not have, so the delta (Bun shards + literals) cannot reach it.
  Rebuilding remin4 would judge nothing this change touches.
- **Names mode / the eval's KPIs were not run.** The scorer judges claims; names-mode placement and identifier
  restoration on the delta index (and the cross-version eval) were not measured.
- **The `-node` sets stay under the 99.8% bar**, as before; this change neither causes nor fixes that.
- 1,729 selected versions could not be built (ETARGET); their packages keep only their older pins.

## To make it the default index (NOT done — Andrew's call)

1. Install: `mv /work/exp177-data/ix/full-bun-names-iv-asof20261025 /work/library-index/8462c502…` (a plain
   directory under its id, `build.json` included). Keep cb64 beside it.
2. Repoint `experiments/034-eval-harness/pairs.json` `libraryIndex` to it. That changes every names-mode launch,
   so re-score the names-mode reference (`ref-names-*` r1-r3) on the new index and re-derive
   `noise-bands.json` `byMode.names` and the names self-hop range; labels on cb64 and on the delta must not be
   mixed on the leaderboard.
3. Decide on the just-bash case first: either accept `self` claims for apps that are themselves indexed owners,
   or exclude owners that are only reachable as dependencies of delta versions (a selection-side rule).
4. Update docs/design/library-index.md §10 (the recommended index, how to reproduce: `scripts/build-delta.sh`).
5. Re-running the rule later: re-fetch downloads, re-run `select_versions.py` against the NEW base manifest, build
   only the new pins into a `cp -al` copy of `/work/exp177-data/store` — shards are reused by key; the merge is
   the cost (~4.7 h, ~104 GB).

## Incidents

- npm's downloads API rate-limits after ~20 requests (HTTP 429); the fetcher was made single-threaded at one
  request per 2.2 s (6 h for 9,897 packages).
- `build_chunks.py --wave-lock` crashed (`wait4` with no child) when another lane was already waiting on
  `/work/heavy.lock` at the moment it took the lock: it drained before starting any chunk. Fixed (skip the wait
  when nothing runs); the delta build instead ran as ONE heavy job holding both locks, because yielding each wave
  to a permanently non-empty queue would starve it.
- A throwaway merge of the first 7,799 versions alone was killed after 37 min (not needed: the shards were the
  point); the full merge ran once.
- The exp152 Bun timing shim points at a deleted script; Bun 1.4.0 was called directly (the shard key records the
  bundler version, not its path — proven by the pilot id).

## Files

- `scripts/fetch_downloads.py` — npm per-version download counts, paced, resumable.
- `scripts/select_versions.py` — the rule; writes `delta.pins` and `plan.json` (per version: line, reason,
  downloads, share, publish time; totals; a sizing grid).
- `scripts/build-delta.sh` — the delta build (shards into a hard-link store copy + one merge with cb64's flags).
- `data/delta.pins` (11,038) and `data/plan.json` (with `built` per version: 9,309 built, 1,729 no-shard).
- Not committed: downloads (`/work/exp177-data/downloads/`), store/work/index (`/work/exp177-data/`), score cards
  (`experiments/096-library-matching-score/results/exp177-*`, gitignored; bun-held is private, summaries only).

Reproduce:

```bash
D=/work/exp177-data; S=experiments/177-installed-versions/scripts
python3 -I $S/fetch_downloads.py $D/packages.txt $D/downloads 2.2
python3 -I $S/select_versions.py $D/registry /work/library-index/cb64c1f5…/manifest.json $D/downloads $D/sel-final
cp -al /work/exp161-data/store $D/store; ln -s /work/exp161-data/registry $D/registry
flock /work/index-build.lock flock /work/heavy.lock $S/build-delta.sh $D target/release/humanify-index $D/sel-final/delta.pins
npm run eval -- library exp177-iv-bun-dev --index $D/ix/full-bun-names-iv-asof20261025 --rule structural --corpus bun-dev
```
