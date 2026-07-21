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
- The **bulk** of the 22% alias churn needs a different lever: **#4**
  (export-set-keyed alias inheritance — keep the importer's alias stable even
  when the target file moves/renames) targets the file-rename component B can't.
  Measure #4's ceiling the same deterministic way before choosing.

Run: `NODE_OPTIONS=--max-old-space-size=14336 npx tsx experiments/033-naming-noise/b-ceiling.ts [priorVer] [newVer]`
(add `WRITE_TREES=/some/dir` to dump both split trees for a direct diff).
