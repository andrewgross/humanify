# Round 3 — stable factory identifiers at the unpacker (2026-07-08)

Fix under test: commit `f7d82d0` — the Bun unpack adapter rewrites every
factory-var reference (runtime.js AND other factories' bodies) to the
sanitized extracted-file name, a pure function of module content
(banner/url/`lib_<structuralHash8>` cascade). Byte-precise edits from the
classification AST's binding, applied through the same splice that
removes declarations; capture-guarded per reference site.

**Methodology change from rounds 1–2:** BOTH legs re-ran (fresh v119 +
incremental v120), because the fix changes the unpacked input and its
value is that each version derives the same identifiers independently —
no transfer involved, fresh legs agree by construction.

## Results

| metric               | phase-6 | round 1 | round 2 | round 3 | cumulative |
| -------------------- | ------- | ------- | ------- | ------- | ---------- |
| diff lines           | 143,106 | 72,157  | 63,684  | 51,148  | −64%       |
| noise hunks          | 22,998  | 10,128  | 8,745   | 6,206   | −73%       |
| noise share of hunks | 92.0%   | 83.8%   | 82.1%   | 76.2%   | −15.8pp    |
| genuine-change hunks | 2,001   | 1,953   | 1,903   | 1,936   | ~stable    |
| rename occurrences   | 47,761  | 18,848  | 16,423  | 11,596  | −76%       |
| distinct bindings    | 12,299  | 5,533   | 4,862   | 3,479   | −72%       |

Buckets (occurrences):

| bucket          | round 2        | round 3       | delta |
| --------------- | -------------- | ------------- | ----- |
| transfer-gap    | 10,761 (65.5%) | 7,283 (62.8%) | −32%  |
| asymmetric      | 1,619 (9.9%)   | 1,414 (12.2%) | −13%  |
| minifier-reroll | 4,043 (24.6%)  | 2,899 (25.0%) | −28%  |

- **`QH→dH` (474 occ) and the whole factory-var family are gone** — both
  versions now emit identical `lib_*` identifiers with zero transfer.
- **Transfer-gap −32% is the hash un-poisoning:** functions whose bodies
  reference factory vars used to serialize `I=QH` vs `I=dH` (free
  identifiers are hash content), knocking them out of exact-match. They
  now hash identically and reuse prior names. Corroborated by the LLM
  bill: incremental-leg tokens 7.0M → 5.9M, time 8m27s → 7m47s.
- Run health: parse OK both legs, invariant clean, genuine-change stable.

## Remaining top contributors → tracked work

- `serializeWithErrorHandling → serializeJson` (423): FOURTH different
  name in four runs — close-match LLM naming instability.
- `v6→X6` (114), `$_→w_` (111), `vH→T_` (104), `J_→W_` (96): megafunction
  truncation (500-line LLM cap).
- `y6→C6` (98), `HK→qK` (57): wrapper-scope class invisibility.
- `g→F` (62), `s→o` (57), `C→E` (56): residual token churn in functions
  neither leg fully named (mostly inside the truncated megafunctions —
  no transfer pairs exist, so the deferred retry can't reach them).
- `aggregateStats → q` (53, asymmetric): v119 named it, v120 didn't —
  coverage asymmetry worth an autopsy alongside the truncation work.
