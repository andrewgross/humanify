# exp170 — every change-vs-noise KPI split into app and vendor

## STATUS (2026-10-31)

- **Harness change, scorer only.** No pipeline or eval was run; every number
  below is a re-score of trees already under `/tmp/eval-work/<label>/`.
  Branch `exp170-app-vendor-split` (built on exp168, rebased onto main
  `3b47449ed`).
- **Decision implemented (Andrew, 2026-10-31):** every change-vs-noise KPI is
  reported separately for app code and vendor code. Assuming vendor
  identification is good, many readers will ignore the vendor side of the
  diff, so the app numbers must stand on their own, with no vendor
  contribution, and code that crosses the boundary must be visible.
- **What landed:** the card's additive `churn.surfaces` block (owner
  `experiments/034-eval-harness/surfaces.ts`), the `split.*` summary totals,
  the `app:` / `vendor:` / `cross:` KPI keys (`kpis.ts SPLIT_KPIS`), the
  summary's app | vendor | cross | total table with each pair's moves listed,
  the leaderboard's APP / VENDOR / CROSS-BOUNDARY tables, measured bands for
  every new key, and `rescore.ts` (re-score a label's trees, prove the old
  fields byte-identical, check every partition).
- **Proof (section 4):** 59 cards across 17 labels re-score with every old
  field byte-identical (the three refs ×4 pairs, exp164, exp166 ×2, and 11
  other labels ×4); 24 older cards (6 labels) differ from their stored copy
  only in the documented 2026-10-06 vendor re-pairing / 2026-10-02 clean
  fields, and main's unchanged scorer reproduces this branch's output minus
  the new block byte-for-byte on all six; every partition adds up on all 85
  re-scored cards; a planted src->vendor move is caught exactly, in unit
  tests and on a copy of a real tree; all 15 existing bands reproduce
  exactly and 28 new ones are measured.
- **Not done here:** CLAUDE.md's eval section (an agent may not edit it; the
  proposed text is in the lane report).

## 1. The audit — what each KPI covered before the split

The bundle `.humanify/humanified.js` holds every statement that is not a CJS
library factory — app code, and library code the bundler inlined as ES
modules (35,903 on exp164's 2.1.216; CJS factory bodies never enter it) —
each filed into a tree file by the split ledger's `order`: 35,800 into src/
and 103 into vendor/ on that hop. The tree scorers each look at ONE tree and never at the other.

| KPI (card field)                                                             | scored on                                   | surface it covered                                                                                                       |
| ---------------------------------------------------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| clean / noise / novel (`statements.*`)                                       | the bundle                                  | **both, mixed**: every bundle statement, including the ones filed in vendor/                                             |
| noiseLn / realLn (`lines.*`)                                                 | the bundle                                  | **both, mixed** (as above)                                                                                               |
| reloc / newName (`relocations.*`)                                            | ledger `nameToFiles[name][0]`               | **both, mixed**: names homed in src/ and in vendor/, plus names whose home crossed                                       |
| relocSt (`tree.relocatedStatements`)                                         | ledger, 1:1 hashes                          | **both, mixed**, including src/->vendor/ moves (11 on exp164)                                                            |
| filesRenamed / renamedPairs / topMoves (`tree.*`, context)                   | ledger                                      | **both, mixed** (exp164's renamedPairs are all `src/… => vendor/…`)                                                      |
| mints (`determinism.mintedLeftovers`)                                        | the pipeline's census of the shipped bundle | **both, mixed**; on exp164 at least 450 of 501 sit in bundle code filed in vendor/                                       |
| treeLn, cleanLn, cleanReal, reorderLn, nameOnly, buildMeta, spell (`layout`) | src/ tree (`composeDiff`)                   | **src only** — but charges the src half of every cross-boundary move (removal: real or whole-file)                       |
| vendorLn / vendorReal (`vendor`)                                             | vendor/ tree (vendor scorer)                | **vendor only** — but charges the vendor half of every move (truly added: real); 352 of exp164's 506 vendorReal (exp168) |
| %det / %llm / closeMatch (`determinism.functions`)                           | the pipeline's stats                        | the whole run. Not a change-vs-noise KPI; not split                                                                      |
| self-hop diff lines (`<v>-self-hop.json`, not a card field)                  | the whole emitted tree                      | the whole tree. Not split (a determinism gate, not a KPI)                                                                |

So no headline KPI was app-only: the statement and name columns mixed the
bundle code the pipeline files into vendor/ in with src/, and the tree columns
were each single-surface but double-charged every move.

## 2. The split

`churn.surfaces`, per pair. **Every split is a partition of a frozen field:
app + vendor + crossBoundary = old.** Nothing existing changes.

| axis         | unit                 | app / vendor                                                      | crossBoundary                                                          |
| ------------ | -------------------- | ----------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `statements` | bundle statement     | the file it is filed in (fresh ledger)                            | matched (clean/noise) and every prior copy was on the other surface    |
| `names`      | fresh name           | its home `nameToFiles[name][0]`                                   | its home changed surface                                               |
| `tree`       | 1:1 ledger statement | its fresh file                                                    | prior and fresh file on different surfaces                             |
| `mints`      | census name          | forced by binding counts (k − other side's count)                 | — (`unattributed`: what the counts do not force)                       |
| `lines`      | git line, both trees | src/ = `layout` minus its take; vendor/ = `vendor` minus its take | statements that left one surface and arrived on the other, both halves |

How the tree-line pairing works (`boundaryLines`): every src/ file that
differs and every vendor/ file the vendor scorer charged as real is split into
top-level statements (the composition's own `statementsOf`, identifier-blind
`statementHash`). Per hash, the instances one surface lost on net and the
other gained on net are paired in file order. Each paired statement's lines
are taken out of the file's charge on BOTH sides — src/'s `real` or
`fileAddRemove`, vendor/'s `real` — capped at what that file was actually
charged (`capped` reports what a cap refused), and charged once, to
`crossBoundary`. Two precision guards: one-line statements are never paired
(`var x = () => {};` is one hash in unrelated code; on exp164 such one-liners
"paired" a src no-op with an unrelated vendor no-op five times; `refused`
reports their lines), and an unparseable vendor file is counted
(`unparsedVendorFiles`), never silently skipped. No line is counted here: the
statement lines are the statement owners', the file charges `composeFile`'s
and the vendor scorer's rows.

## 3. The three hops, split (2.1.215 -> 2.1.216)

Re-scored from the trees (exp161's cards are gone; its stats block is read
from the run's diag, whose `coverage` is byte-identical to the stats file —
checked on exp164). exp166 is `exp166-names-hop2` (`210ed0558`, the
committed result; hop 1 is noted below). Bands are the default-mode refs'
(section 5); names mode has no measured band of its own.

TOTALS (every surface; the frozen fields):

| KPI                 | default (exp161) | names exp164  | **exp166 (hop 2)** |
| ------------------- | ---------------- | ------------- | ------------------ |
| treeLn+vendor churn | 25,648           | 26,785        | 26,009             |
| noiseLn             | 3,304            | 5,990         | 4,919              |
| novel / realLn      | 986 / 122,066    | 986 / 122,066 | 986 / 122,066      |
| mints               | 81               | 501           | 54                 |
| vendorReal (old)    | 120              | 506           | 289                |

APP — everything outside vendor/, no vendor contribution:

| KPI               | default (exp161) | names exp164 | **exp166 (hop 2)** | band  |
| ----------------- | ---------------- | ------------ | ------------------ | ----- |
| app:treeLn        | 25,345           | 25,762       | 25,471             | ±16   |
| app:cleanLn       | 24,049           | 24,466       | 24,175             | ±16   |
| app:noise (stmts) | 281              | 312          | 286                | ±17   |
| app:noiseLn       | 3,304            | 5,013        | 4,645              | ±1376 |
| app:novel         | 986              | 986          | **982**            | 0     |
| app:realLn        | 122,066          | 122,066      | **121,995**        | 0     |
| app:cleanReal     | 18,709           | 18,814       | 18,781             | ±74   |
| app:reloc         | 337              | 329          | 331                | ±35   |
| app:relocSt       | 102              | 102          | 102                | 0     |
| app:newName       | 753              | 754          | 750                | ±13   |
| app:mints         | 81               | 49           | 52                 | ±34   |

VENDOR — vendor/ (statement rows: bundle code filed into vendor/):

| KPI                           | default (exp161) | names exp164 | **exp166 (hop 2)** | band |
| ----------------------------- | ---------------- | ------------ | ------------------ | ---- |
| vendor:treeLn                 | 303              | 499          | 406                | ±87  |
| vendor:vendorLn (noise)       | 183              | 255          | 183                | ±55  |
| vendor:vendorReal (moves out) | 120              | 244          | 223                | ±32  |
| bundle statements in vendor/  | 0                | 86           | 1,462              | 0    |
| vendor:noise / noiseLn        | 0 / 0            | 29 / 716     | 6 / 274            | 0    |
| vendor:novel / realLn         | 0 / 0            | 0 / 0        | **4 / 71**         | 0    |
| vendor:newName                | 0                | 32           | 11                 | 0    |
| vendor:mints                  | 0                | 450          | 2                  | 0    |

CROSS-BOUNDARY — moved src/<->vendor/, charged once:

| KPI                         | default (exp161) | names exp164    | **exp166 (hop 2)** |
| --------------------------- | ---------------- | --------------- | ------------------ |
| cross:treeLn (git lines)    | 0                | 524 (262 + 262) | 132 (66 + 66)      |
| tree statements moved       | 0                | 16 to vendor    | 7 to vendor        |
| bundle statements crossed   | 0                | 17              | 0                  |
| cross:relocSt / cross:reloc | 0 / 0            | 11 / 9          | 7 / 12             |
| one-line lines refused      | 0                | 30              | 14                 |
| mints unattributed          | 0                | 2               | 0                  |

What the split shows that the totals did not:

- **exp164's vendorReal 506 was 244 of vendor change.** 262 lines are the
  vendor half of 16 statements moved src/ -> vendor/ (temporary-credentials-
  provider -> @azure/core-rest-pipeline, axios-error -> zod-to-json-schema,
  …; every move is listed in the summary), and their src half (262) was
  whole-file removals in src/. exp168 attributed 352 lines to the moved
  files; the 90 left in vendor:vendorReal are those files' require headers,
  blank lines and the 30 refused one-line statement lines — content the
  pairing cannot prove moved, so it stays charged where it was.
- **exp164's mints were vendor-side.** At least 450 of the 501 sit in bundle
  code filed in vendor/ (the jose/zod junk of exp164/165); the app side read
  49, inside the band of default's 81. exp166 removed it: vendor:mints 2.
- **exp166 moves 4 new statements (71 lines) out of the app view.**
  `novel`/`realLn` stay byte-equal in total, but app:novel reads 982 and
  app:realLn 121,995: four statements new at 2.1.216 were filed straight into
  vendor/ (`@anthropic-ai/claude-agent-sdk`, `@anthropic-ai/sandbox-runtime`
  modules). Those hold columns have a band of 0, so a reader of the app view
  alone would see 71 lines of real change disappear. Whether that is right
  depends on whether the placement is right — the split makes it visible
  instead of deciding it.
- **exp166 files 1,462 bundle statements into vendor/** (exp164: 86 scored,
  default: 0), 1,452 of them byte-clean against the base, which filed them
  there too. That is ESM-inlined library code the index now claims; its
  churn is vendor-side (noise 6 statements / 274 lines) and stays out of the
  app columns.
- exp166 hop 1 (`exp166-names-hop`, `421a5e817`) reads the same tree columns
  within a few lines but mints 1,482 = app 1,201 / vendor 268 /
  unattributed 13 — its mint regression was app-side, not vendor junk.

## 4. Proof

| check                                                                                                                 | result                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| --------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| old fields byte-identical (new card minus `churn.surfaces` == stored card bytes)                                      | **59 / 59 cards**: ref-scratch-0f338ffa-r1/r2/r3 (4 pairs each), exp164-names-hop, exp166-names-hop, exp166-names-hop2, and candidate-0ebb63b3 / 5d4b2d9a / 5d970212 / c5ac0e98 / e214db7a / e475b2a0 / f616f33b-scratch, exp095-steps01-d9e990e3, exp157-vendstab-names, exp158-names-eval, fix-minified-esbuild-68b090e3 (4 pairs each)                                                                                                                                  |
| older cards that differ from their stored copy                                                                        | 24 cards, 6 labels (control-843826be-scratch, candidate-3480c2fa-scratch, control-843826be-8gpu, candidate-8af0574f-8gpu, rust-relaxed-default-843826be, main-2026-09-18): ONLY `vendor.noise/real/relocated` (the 2026-10-06 re-pairing they were never re-derived for) and, on the two oldest, the 2026-10-02 clean fields. Main's unchanged scorer (origin/main worktree) reproduces this branch's output minus `surfaces` byte-for-byte on the 2.1.216 pair of all six |
| every partition adds up (app + vendor + cross = old, per field; mints + unattributed; tree lines incl. fileAddRemove) | **85 / 85** re-scored cards (`rescore.ts partitionFailures`)                                                                                                                                                                                                                                                                                                                                                                                                               |
| planted cross-boundary move, unit (`surfaces.test.ts`)                                                                | a 5-line statement moved src/auth.js -> new vendor file: charged twice before (src real 5 + vendor real 6), now app real = only the genuine new code, vendor real = only the genuine new library + 1 trailing line, cross 10 = 5 + 5; a whole moved file comes out of `fileAddRemove`; a one-line `var x = () => {};` "move" is refused (the test is red with the guard off)                                                                                               |
| planted move on a REAL tree                                                                                           | a hard-link copy of ref r1's 2.1.216 tree with a 22-line statement moved from `src/add-or-update-remote-control.js` into a new `vendor/planted-pkg/` file: cross 44 = 22 + 22, 1 statement, the move listed; app real 20,133 -> 20,111, vendor real 169 -> 147; partitions add up (the unplanted r1 card reads cross 0)                                                                                                                                                    |
| bands                                                                                                                 | all 15 existing bands reproduce exactly from the re-scored refs; 28 new keys written (section 5)                                                                                                                                                                                                                                                                                                                                                                           |
| gate                                                                                                                  | `npm run check` — see the lane report                                                                                                                                                                                                                                                                                                                                                                                                                                      |

## 5. Bands (three same-commit default-mode refs, re-scored)

`noise-bands.json` gains every `app:` / `vendor:` / `cross:` key, derived the
way the 2026-10-06 change did (same trees, scorer only; every pre-existing
band byte-equal). App bands equal their parents except `app:mints` ±34
(`mints:unattributed` ±1). `vendor:treeLn` ±87; `vendor:vendorLn` ±55 and
`vendor:vendorReal` ±32 as before. Every statement-axis vendor band and every
cross band is **0** — default mode files no bundle code into vendor/ and
moves nothing across, so those zeros are a claim about default mode only.
Names mode (exp164/166) has no repeat on record: its vendor-side and cross
columns are unbanded in practice, and the leaderboard's `~0` there means
"default mode never moves this", not "names mode is stable".

## What this does NOT look at (rule 8)

- **Content pairing is statement-level and identifier-blind.** One-line
  statements are refused (they collide across unrelated code), so a genuine
  one-line move stays charged on both sides (`refused`, 30 lines on exp164).
  A multi-line generic shape could still pair falsely; none was seen in the
  listed moves, which are printed for exactly that reason.
- **Headers and separators of a moved file stay charged** to the surface
  that owns the file: the require lines and blank lines of a new vendor file
  holding moved statements are vendor real.
- **The cap trusts the file's charge.** A moved statement that composeDiff's
  tier 3 paired with an edit in the same file is charged less than its lines;
  the take is capped at the file's charge (`capped`, 0 on every hop here).
- **mints are attributed by name.** The census lists names without
  positions; a name both surfaces declare is split only as far as counts
  force it, the rest `unattributed` (0–13 per pair here). The census reads
  the shipped text in the naming stage, not `humanified.js` byte-for-byte; a
  name the bundle never declares goes to `unattributed`.
- **%det / %llm and the self-hop** are run- or tree-wide and not split.
- **"vendor" is the pipeline's own placement.** The split reports what the
  library placement filed where; it cannot tell a correct claim from a wrong
  one (exp166's four new statements in vendor/ are exactly that question).

## Files

- `experiments/034-eval-harness/surfaces.ts` — the owner (one row added to
  `docs/responsibility.md`); `surfaces.test.ts` — its tests.
- `analyze.ts` (one parse, one classification, the `surfaces` block),
  `kpis.ts` (`Scorecard.churn.surfaces`, `split.*` totals, `SPLIT_KPIS`),
  `summarize.ts` (`flattenSurfaces`, the app | vendor | cross table, the
  moves), `leaderboard.ts` (APP / VENDOR / CROSS-BOUNDARY tables),
  `noise-bands.ts` (bands cover the split keys), `noise-bands.json`.
- `experiments/170-app-vendor-split/rescore.ts` — re-score a label's trees:

  ```bash
  npx tsx experiments/170-app-vendor-split/rescore.ts <label> --cards <dir with its cards>
  npx tsx experiments/170-app-vendor-split/rescore.ts <label> --stats-from-diag --pairs 2.1.215->2.1.216
  npx tsx experiments/034-eval-harness/summarize.ts <label>
  ```
