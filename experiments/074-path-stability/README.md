# 074 — deterministic paths for modules that do not match

> **STATUS (2026-08-15): BUILT — but the brief's premise was WRONG and
> was corrected before code. Validation run in flight.**
>
> **Path churn is not mint randomness.** Matched modules inherit paths
> perfectly (2,182 matched, ZERO moved). Measured decomposition of the
> 4,592 churned require lines on 85→86:
>
> | cause | lines |
> | --- | ---: |
> | sit in genuinely new files | 473 |
> | target exists in prior (alias/name churn, not placement) | 543 |
> | target absent from prior — real path instability | 3,576 |
> | …of which **ONE module** causes | **3,204 (90%)** |
>
> That module grew 17→18 statements: overlap 0.75 against its prior
> self with a single import edge, so the existing tier could license it
> neither way. It minted fresh, landed elsewhere, and all 3,204
> importers churned — **while its stem was identical in both runs**.
>
> **Fix shipped: a stem-corroborated match tier** (stem unique among
> unmatched modules on BOTH sides + overlap ≥ 0.7), priced before
> building:
>
> | threshold | pairs | churned lines held |
> | ---: | ---: | ---: |
> | 0.5 | 45 | 3,244 |
> | **0.7** | **44** | **3,204** |
> | 0.8 | 43 | **0** (the module that matters sits at 0.75) |
>
> Verified on the real bundle: inherited paths 3,092 → **3,136**.
> Mispairing between content-twins is harmless by construction.
>
> **Folder quality (same increment):** consensus-then-collapse takes
> 809 → **280 folders**, median 4 files/folder, root held at 1,085
> (collapse alone reaches 225 but inflates the root 1,013 → 1,590 —
> order matters). Eager zone renamed to `src/index.js`: it IS the entry
> file. ~1,000 genuinely-shared files stay flat and counted.
>
> **Carry-forward finding:** folder rules only bite on a FIRST run —
> with a prior ledger, 3,136 of 3,273 modules inherit paths (correctly;
> that IS the stability property), so a relayout to 280 folders
> requires scoring without a prior ledger, or accepting that the folder
> shape arrives once and then freezes.


> **This is a BRIEF — a hypothesis.** Its sizing came free from
> exp073's ceiling decomposition (2026-08-15).

## The finding that motivates it

Under fossil layout, **path churn dwarfs name churn**:

| class | churned lines (85→86) |
| --- | ---: |
| require lines pointing at a path absent from the prior tree | **5,728** (26% of all imports) |
| body name churn a name-carry could reach | 944 |
| genuine change | 308 |

And it is extraordinarily concentrated: **179 distinct targets** cause
all 5,728 lines (~32 each), and **one universally-imported helper
(`access-property.js`) accounts for 3,272**.

Path CARRY already works — of 2,182 provably-matched modules, **zero
moved**. The damage comes from modules that do NOT match: ambiguous
twins and genuinely-new modules mint a fresh path each run, and every
importer of a re-minted module churns a require line.

## Hypothesis

Minted paths for unmatched modules can be made deterministic — derived
from content and graph position rather than from discovery order — so
an unmatched module lands on the same path every run, and its
importers stop churning. This needs no naming change and no
classification.

## Design constraints

- **Never positional.** exp035/036 measured positional NAME assignment
  at +50,606 lines; the same trap applies to paths. Derive from
  content (fingerprint), the module's importers, and its stem — never
  from the order the walk discovered it.
- Twins are indistinguishable by content BY CONSTRUCTION, so their
  paths must be derived from something stable that DOES differ: their
  importer set. Two identical modules imported from different places
  are different modules for placement purposes.
- Matched modules keep inheriting verbatim; this only governs the
  unmatched remainder.
- LOG every minted path with its derivation input (rule 11).

## Success criterion (fixed now)

Cold scored run (combined with exp073 and the folder work, one run for
the stack): `novel`/`realLn` byte-exact, boot ×4, cache +0, self-hop
≤ 1 ask. Success = require-line churn on 85→86 falls well below 5,728,
and hidden name-only churn falls below exp070-r1's 1,926.

## Validation run exp074-r1 (cold, 4 pairs) — RESULT

**Gates: all pass.** `novel` 4,188 / `realLn` 416,377 byte-exact; boot
OK ×4; cache +0 on every leg; self-hop 124 lines (inside the 72–182
band) with 1 ask. Emitted tree: 3,274 files, **292 folders**.

**Per-pair hidden name-only churn (055 ledger), vs the fossil-only
baseline (exp070-r1) and the pre-fossil layout:**

| pair | pre-fossil | exp070-r1 (fossil) | **exp074-r1 (stack)** |
| --- | ---: | ---: | ---: |
| 85→86 | ~1,480 | 1,926 | **1,718** |
| 118→119 | 240–260 | — | **704** |
| 197→198 | 904–1,010 | — | **976** |
| 215→216 | 502–556 | — | **648** |

**Require-line churn on 85→86: 4,592 → 1,620 (−65%)** — the
stem-corroborated tier delivered its predicted 3,204 lines.

**Honest reading: the stack is NOT yet a win on the hidden-churn
axis.** Every calm pair reads WORSE than the pre-fossil layout
(118→119 nearly 3× worse), and the on-disk totals show why: `treeLn`
+48,194 and `relocSt` 567 vs 1. A tree of 3,274 small files spreads the
same churn across far more files and multiplies the per-file import
surface; the path fix removed the largest single class but not the
structural cost. `noise` (−173) and `noiseLn` (−2,086) improved, and
the hold columns are exact, so nothing is broken — but the layout's
value case rests on structure, not on this metric.

## Diagnosis of the remaining cost (2026-08-15, post-run)

Three hypotheses tested on the run's own trees; two refuted:

1. **"The extra hidden churn is import/export plumbing"** — REFUTED.
   Decomposing the name-only class by line type: require/alias **0**,
   export plumbing **0**, code bodies **1,718** (old layout: 0 / 0 /
   1,476). Import churn is real but lands in the EDITED class (a
   changed require line also changes a path string, so it is not
   "name-only").
2. **"Small files weaken the post-split repair pass"** — REFUTED, and
   backwards: the repair restored **494 names across 222 files** under
   fossil layout vs **215 across 101** before. Small files help it.
3. **"More code goes to the LLM"** — REFUTED: fossil layout made FEWER
   calls (1,413 vs 1,551) and named FEWER identifiers fresh (3,234 vs
   3,341).

**Measurement caveat that matters:** hidden-churn numbers are NOT
comparable across layouts. `composeDiff` pairs statements per file, so
cutting the tree into 3,274 files instead of 1,528 moves mass between
its REAL and NOISE columns (85→86: REAL 24,119 → 20,605 while
classified noise 5,630 → 10,538). The 1,476 → 1,718 comparison has
different denominators on each side.

**The layout-independent signal — and the real remaining cost:**

| pair | total churn | statements that CHANGED FILE |
| --- | ---: | ---: |
| 85→86 | 50,000 | 244 |
| 118→119 | 45,273 | 29 |
| 197→198 | 63,772 | 133 |
| 215→216 | 33,471 | 161 |
| **total** | **192,516** (ref: 144,322) | **567** (ref: 1) |

So the cost is **placement instability at statement level**, not
naming: 567 statements changed file between two fossil trees where the
old layout moved 1. Every such statement drags its lines into the diff
twice. That is the next lever, and it is the same class the path work
just fixed one level up (modules), now needed one level down
(statements within a module).

### Attribution attempt for the 567 moved statements — CONFOUNDED, not concluded

Tried to attribute them by mapping statement hash → emitted file in both
trees. The result is not usable: statement hashes are NOT unique (the
documented collision property for short generic statements), so a
hash→file map keeps only the first occurrence and the per-module tallies
exceeded the total (2,209 + 714 attributed against 352 distinct hashes).
Recording the failure rather than the number.

Ruled out cheaply: collision-suffixed paths (`foo-2.js`) are NOT the
driver — 9 such files in the fresh tree, 1 in the prior.

The right instrument is module-SCOPED statement identity (a statement
keyed by its module plus its position within it), which the fossil map
already has and this ad-hoc script did not use. Next session's first
diagnostic.

### The 567 moved statements, properly attributed (unique-line method)

The hash method was confounded; unique substantial lines are not.
Across the two fossil trees (85→86): **182,607 unique lines present in
both, 97.51% in the SAME file, 2.49% (4,540 lines) moved.**

The moves are legible and fall into exactly two fixable classes:

| lines | move | class |
| ---: | --- | --- |
| 492 | `redact-url.js` → `redact-url/redact-url.js` | FOLDER churn (name kept, folder re-inferred) |
| 312 | `is-task-active.js` → `usage-stats-schema.js` | FILE RENAME (stem drifted with content) |
| 256 | `parse-command-val/manage-marketplaces.js` → `load-marketplaces/manage-marketplaces.js` | FOLDER churn |
| 153 | `render-server-auth-component.js` → `format-reconnect-result-2.js` | FILE RENAME |

**CORRECTION (same session): the "already-matched modules" reading was
WRONG.** Checked against the ledgers: every one of these modules is an
UNMATCHED fresh mint, so inheritance cannot apply and the fix sketched
here does not exist. The real cause is that the stem-corroborated tier
declines them:

| module | stem unique both sides? | content overlap | tier needs |
| --- | --- | ---: | --- |
| `redact-url` | yes (1 candidate each side) | **0.42** | ≥ 0.70 |
| `manage-marketplaces` | yes (1 candidate each side) | **0.33** | ≥ 0.70 |
| `usage-stats-schema` | no prior with this stem | — | genuinely new |

These files really did change a third to a half of their content, yet
they are plainly the same file: same unique name, exactly one candidate
on each side. **The open design question** — for Andrew, not a
threshold to fiddle — is whether "this stem appears exactly once among
unmatched modules on BOTH sides" is sufficient evidence on its own
(process of elimination: nothing else can claim it), or whether a
heavily-rewritten file should earn a fresh identity. Stakes: ~750 of
the 4,540 moved lines. Note exp074 already measured that moving the
threshold the OTHER way (0.8) destroys the entire 3,204-line win.
