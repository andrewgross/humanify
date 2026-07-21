# 033 — naming-noise measurement harness + Lever B ceiling

Reproducible measurement for the cross-version naming-noise levers
(`docs/plan-naming-noise-levers.md`). Persists what was throwaway scratchpad
tooling so any branch can be measured against the benchmark hops.

## Benchmark anchors (all persistent, outside the repo)

- Inputs (minified/decompiled): `~/Development/claude-code-versions/inputs/claude-code-2.1.<v>/…`
- Priors (humanified + split ledger): `~/Development/unpacked-claude-code/versions/claude-code-2.1.<v>/.humanify/`
- Archive corpus (per-version git history): `~/Development/unpacked-claude-code/claude-code-history.git`
- Hard hops: 207→208, 209→210, 185→186, 202→203, 197→198, 215→216. Quiet
  controls: 213→214, 214→215.

## Tools

- `view-diff.sh <base-tree> <new-tree>` — scoped, `-I`-filtered unified diff
  between two split trees (same scoping as `build-history-repo.sh`).
- `classify-tokens.py` — token-level diff classifier (robust to the packed
  tool-registry line): splits changed tokens into rename vs structural, lists
  distinct churned bindings.
- `alias-classify.py <new-tree-root>` — splits naming churn into **require-alias
  churn** (split/file-relocation — Lever B/#4) vs **true binding rename**
  (Lever A). Reads a diff on stdin.
- `b-ceiling.ts` — **deterministic Lever B ceiling** (below).
- `diagnose-relocations.ts` — **split-relocation tier attribution**: replicates
  `assignWithPrior` verbatim with per-statement tier tracking (self-checked
  against the real `ledger.order`), then buckets every relocating matched
  binding by the tier that placed it. Drives `docs/plan-split-assignment-stability.md`.
- `diagnose-v2.ts` — **promoted-identity ceiling** (identity preempts a wrong
  name-vote): pure + role-gated relocation reduction, regression count, and
  `WRITE_TREES` tree dump for a real diff. Caches the oracle map.
- `diagnose-v3.ts` — **B2 (distrust-generic-votes) measurement** — shows the
  cheap no-map shortcut backfires (net −1,389 on 215→216).

## Measurement notes

- **Raw line churn is swamped by the ~20k-line LLM nondeterminism floor** (two
  identical rebuilds differ by that much — temp is already 0; it's concurrent
  batch serving). Prefer metrics that are stable run-to-run: distinct
  require-alias churn, module-binding mint count, and — for the split — the
  deterministic `b-ceiling.ts` (no LLM at all).
- For Lever A (function-local naming, which lives IN the nondeterministic
  bucket), the right measure is whether a branch's **run1/run2 variance drops**
  vs fix #1's — did the hints make naming more deterministic — not the raw hop
  churn.

## Lever B ceiling result (2026-07-21, oracle map, 215→216)

`b-ceiling.ts` runs the split (deterministic when a prior ledger is present — no
namer/LLM) on the fixed 216 humanified output with `priorMatchMap` OFF vs ON.
The map is an **oracle**: match the two FINAL humanified outputs (215⇄216) with
the production matcher → `{216-name → 215-name}` for every renamed binding
(3,203 entries). Best-case, so a true ceiling.

```
without B:  residueLocality=1355  viaIdentity=0
with B:     residueLocality=1147  viaIdentity=208     ← rescued, 0 regressions
file relocation of matched bindings vs 215: 2781 → 2657 (net 124 kept in prior file)
concrete diff impact (withB vs noB, only the map differs): 145 files, ~3,056 lines
```

**Verdict.** B is real and **safe** — it moves 208 of the 1,355 locality-residue
statements (15%) back to their prior file with **zero regressions** (the
unique+unanimous gate holds), for **~3,056 lines of relocation churn removed** on
215→216. But **85% of the relocation churn is beyond B's reach** — it's
file-level restructuring (clusters splitting/merging/renaming), not name-flipped
binding drift. So:

- Wiring B into production (populate `priorMatchMap` before AST release — see the
  seam in `commands/unified.ts`) buys a modest, deterministic, safe ~3k-line win.

Run: `NODE_OPTIONS=--max-old-space-size=14336 npx tsx experiments/033-naming-noise/b-ceiling.ts [priorVer] [newVer]`
(add `WRITE_TREES=/some/dir` to dump both split trees for a direct diff).

## Lever #4 ceiling result (2026-07-21, 215→216) — DEAD END, corrects the plan

`hash4-ceiling.py` fuzzy-matches every 216 exporting file to its best-overlap
215 file (Jaccard ≥ 0.5 on export names), weighted by importer `require`-refs:

```
stable (same path):                1492 files   39,597 refs (100%) — no churn
relocated/renamed (recognizable):     0 files        0 refs (0%)   ← #4 ceiling
new/restructured:                     4 files       49 refs (0%)
```

**Files do not move or rename paths across versions** — every 216 file's
best-overlap prior is the file at the _same path_. So #4 (re-key the importer's
alias by the target file's export-set identity) has **nothing to inherit** — my
earlier "#4 targets the file-rename component" hypothesis was **wrong**, and the
measurement kills it.

**What the alias churn actually is:** individual _bindings_ relocating between
stable-path files (`DEFAULT_MODEL` moving from `task-serializer.js` to
`auth-manager.js` pulls every importer's alias with it). That is B's domain, not
#4's. But B only rescues the relocating bindings that fall to **locality
residue** (the 208 above); the rest are bindings the split's name-vote / hash
tiers assign to a _different_ file than prior — a deeper split-clustering
stability problem, addressed by neither B (identity tier) nor #4 (alias naming).

**Net for the 22% alias bucket:** B is the only cheap, safe lever and it caps at
~3k lines; the remainder needs split-assignment stability work (the name-vote /
hash / clustering tiers), which is a larger, separate investigation — not a
quick win. Weigh that against the 62% Lever-A bucket (A1/A2), which is where the
leverage is.

Run: `python3 experiments/033-naming-noise/hash4-ceiling.py <tree-215-src> <tree-216-src>`

## Split-assignment stability result (2026-07-21, `diagnose-*.ts`, 215→216)

The "deeper split-clustering stability problem" the #4 section flagged is now
diagnosed — full writeup: `docs/plan-split-assignment-stability.md`.

- 2,781 / 3,065 matched (renamed) top-level bindings relocate. By deciding tier:
  **name-all-same 78.6%** (2,185), hash 10.9%, residue-novote 4.5%,
  residue-conflict 3.3%, name-ordinal 2.7%.
- **99% of the name-all-same relocations are name COLLISIONS** — the binding was
  renamed _to_ a generic/minted name that already exists (unanimously) in the
  prior ledger as a different binding (there are ~6,340 such single-file
  "collision-magnet" names), so the name-vote confidently teleports it to that
  other binding's file. **Lever B (identity, fires only when `votes.size===0`)
  reaches just 4.5% of relocations.**
- **Fix = promote the identity tier to PREEMPT the collision vote** (gated:
  matched + unanimous prior home + role-agrees + non-generic new name). Measured
  oracle ceiling: role-gated **2,162 / 2,781 relocations fixed (78%), ~18,833
  lines, ≤1 regression**; pure **2,691 (97%), 0 regressions**. Needs `priorMatchMap`
  wired (the seam in `commands/unified.ts`).
- The cheap no-map shortcut (distrust generic votes, `diagnose-v3.ts`)
  **backfires**: net −1,389 (it discards 6,326 legitimate generic keeps to catch
  the collisions) — the matcher's identity map is required, not optional.

## PRODUCTION-MAP REALITY — the part-4 ceiling is an oracle artifact (2026-07-21)

Levers B (fill) and A (preempt) were WIRED (`prior-match-map.ts`,
`prior-transfer.ts`, `plugin.ts`, `unified.ts`; branch `feat/naming-noise-followups`)
and the map dumped from a real 215→216 run (`writePriorMatchMapDebug`, `-vv`).
The production map holds **5 useful entries**, not thousands. Measured:

- Of **22,802 matched module bindings, ~all PIN** — they inherit their prior name
  AND (via the name-vote) their prior file. **A matched binding does not relocate;
  it is stable.** Only 5 (+28 Bun temps) flip.
- The real churn population is **1,020 novel names** (216 names absent from 215 —
  new/changed code). Of those, only **285 have ANY recoverable prior identity even
  in the best-case oracle** (`oracle-coverage.ts`), 280 with a unanimous home. That
  280 is the absolute A/C ceiling; production's first-pass match captures far less.
- **Same-name file relocations: 216 (0.40%)** — bindings that KEPT their name but
  moved files. This is the actual require-alias-churn driver, and **B/A cannot
  touch it** (`final===prior` → dropped from the map by design). It is the
  non-unique-name / ordinal-tier's domain — a separate lever.
- Part-4's oracle map has 3,203 entries but **2,923 of them re-point a SAME-name
  216 binding to a _different_ 215 binding** (final↔final name reuse). Applying
  those would MISFILE, not fix — so the "2,162 relocations / 18,833 lines" is both
  unreachable in production AND largely wrong. `diagnose-v2`'s ≤1-regression count
  used the oracle's own circular home definition and could not see it.

**Verdict: B/A are correct + safe but LOW-YIELD in production (~5 today, ~280
oracle ceiling for significant extra capture work).** The matched population pins;
the flip population is genuinely-new code with no prior identity. The real 22%
alias bucket is the 216 same-name relocations, which needs an ordinal/positional
lever, not the identity tier.

Run: `npx tsx experiments/033-naming-noise/oracle-coverage.ts` (ceiling) and
`prod-map-measure.ts <runDir>` (ON/OFF on the real sidecar map).
