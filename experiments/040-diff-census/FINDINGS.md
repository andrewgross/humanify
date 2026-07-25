# 040 — Diff-noise census: what the reviewed lines actually are

Conventions: every hop judged **on its own**; ranked by measured lines, not by
how noisy something feels.

`line-census.ts` classifies every line of the real `git diff` between two split
trees by what produced it. It reads git's own output rather than re-deriving a
diff, so the categories describe exactly the lines a human sees.

## Per-hop census

| hop     | diff lines |  body | require:rm | require:add | accessor:rm | accessor:add | setter | alias |
| ------- | ---------: | ----: | ---------: | ----------: | ----------: | -----------: | -----: | ----: |
| 85→86   |     50,662 | 89.7% |  **1,933** |   **1,900** |         719 |          551 |    108 |    28 |
| 118→119 |     37,821 | 93.9% |        102 |       1,137 |         107 |          897 |      6 |    54 |
| 197→198 |     52,380 | 93.7% |        788 |       1,240 |         357 |          859 |     34 |    44 |
| 215→216 |     31,709 | 94.2% |        286 |         687 |         189 |          582 |     12 |    74 |

## Finding 1 — `diff-composition` counts import churn as REAL change

`composeDiff` matches statements by structural hash. A `const x = require("…")`
header present on only one side has no counterpart, so it is charged as **real
change**. It only lands in the `alias` bucket when the same PATH exists on both
sides with a different alias.

That is a real blind spot: on 85→86 the census finds **1,933 require lines
removed and 1,900 added**. Near-balanced add/remove is the signature of churn,
not growth — compare 118→119, a large feature drop, where requires are 1,137
added against only 102 removed. So some part of that 3,833 lines is sitting in
the "real change" column and the 89.7%-real headline for that hop is optimistic.

## Finding 2 — `relocatedStatements: 0` does not mean nothing moved

The 034 tree metric only compares statements whose hash is **unique on both
sides**. On these trees that is 11,452 of 19,966 statements (85→86) and 21,797 of
35,903 (215→216) — so **39–43% of statements are invisible to it**. Reporting
"0 relocated" from it, as I did earlier, overstates the result.

Measured order-independently from `nameToFiles`, restricted to names with exactly
one home file on both sides (a name like `key` is declared in 40+ files, and
counting any change to that set inflates the number ~2.5×):

| hop     | single-home names | **moved home file** |
| ------- | ----------------: | ------------------: |
| 85→86   |            28,521 |      **75** (0.26%) |
| 215→216 |            52,050 |      **77** (0.15%) |

Small in percentage, but each move rewrites the `require` header in every
importer plus the accessor line in both files.

**It is NOT, however, what drives Finding 1's import churn — tested and
refuted.** Of the 1,929 removed require paths on 85→86, only **16** are
explained by a known single-home move. Two candidate causes are now ruled out:

| hypothesis                                        | test                                                         | verdict                   |
| ------------------------------------------------- | ------------------------------------------------------------ | ------------------------- |
| relocation of single-home bindings                | match removed→added path pairs against known moves           | **16 of 1,929 — refuted** |
| same-hash vendor shims swapping their `-N` suffix | `vendor/noop/lib_eb5345cb{,-2}.js` proxy targets in 85 vs 86 | **stable — refuted**      |

The vendor check is worth keeping: those two files are NOT content-duplicates,
they are distinct re-export shims that share a structural hash and are told apart
only by a `-2` suffix (the documented "same hash, different library" hazard). They
happened to stay stable here, but the churn pattern that led there is real — the
SAME path is both added and removed across different files in near-equal numbers
(`+34/-31` for one shim, `+28/-32` for its sibling), i.e. the set of files
importing a given module is churning.

**Open question, and the next thing to test:** names declared in SEVERAL files.
The strict measure above deliberately excludes them, yet they are exactly the
`initModule*`/`noop*` family that makes up the ambiguous-hash population. If a
cross-file reference starts resolving to a different declaring file, the importer's
require header churns with no statement having moved.

Two shapes stand out:

- **Swaps between sibling files.** On 85→86, `initModuleVar` moved
  `custom-map.js → map-entry-manager.js` while `initializeZT8` moved the other
  way. Both are same-shaped lazy-init statements, so the assignment tiers cannot
  tell them apart — the interchangeable-bucket problem, on the file axis.
- **Group migrations.** On 215→216, `analyzeEventStats`, `processMessageStats`
  and `toolIdToNameMap` all moved `message-stats.js → branch-fetcher.js`
  together, and `hasLocalTool` left `api-query.js` — which is exactly one of the
  accessor removals visible at the top of that file's diff.

## Finding 3 — header churn is over-weighted in PERCEIVED noise

`api-query.js` on 215→216 (the file that prompted this): 1,164 diff lines, of
which the tools say ~97% is genuine new upstream code — verified by reading it
(`areModelsSame`, `isModelInTriedModels`, refusal-chain routing). Only 56 lines
are emitter-generated.

But those 56 sit in the **sorted accessor and require block at the top of the
file**, so they are the first thing a reviewer sees, and they read as a wall of
near-identical churned lines. Low line count, high review cost. Worth fixing
above its line-count rank.

(One naming-quality item in the new code: `function doNothing3()` — a minted
name on genuinely new code.)

## Ranked, for the worst hop (85→86, 50,662 lines)

| rank | mechanism                    | lines | note                                                                         |
| ---- | ---------------------------- | ----: | ---------------------------------------------------------------------------- |
| 1    | naming churn (body)          | 5,740 | exp039; 32% of it is 6+-rename genuine drift                                 |
| 2    | **require add/remove churn** | 3,833 | new — cause OPEN (relocation + vendor-swap both refuted), mis-scored as real |
| 3    | reorder (body)               | 1,816 | exp038 residue                                                               |
| 4    | accessor add/remove + setter | 1,378 | relocation + rename driven                                                   |
| 5    | require alias                |    28 | exp038 Task D already cut this                                               |

## Next

1. Teach `composeDiff` to score require add/remove as its own category instead of
   folding it into real change — the metric currently flatters itself.
2. Attack **file-assignment stability for ambiguous statements** (Findings 1+2).
   The swap case suggests the same isomorphic-bucket limit as naming, but the
   group-migration case may have a real fix.
3. Re-rank after that: naming (exp039) and assignment stability are now
   comparable in size on the worst hop.
