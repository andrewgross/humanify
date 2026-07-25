# 039 — Naming drift: the leading noise bucket after exp038

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
_Idea → Evidence (table) → Conclusion_; **ceilings measured before builds**;
every hop judged **on its own**.

Read [exp037 FINDINGS.md](../037-noise-source-decomposition/FINDINGS.md) and
[exp038 RESULTS.md](../038-dependency-aware-reorder/RESULTS.md) first.

## Why this experiment exists

exp038 removed reorder as the dominant noise source. The 034 gate now scores the
on-disk diff directly (`layout` block, git lines), and naming is what is left:

| hop     |  churn |   real |    naming | alias | reorder |
| ------- | -----: | -----: | --------: | ----: | ------: |
| 85→86   | 41,550 | 33,954 | **5,740** |    28 |   1,816 |
| 197→198 | 60,854 | 57,984 |       780 |    44 |   1,956 |
| 215→216 | 31,578 | 28,800 |       754 |    74 |   1,950 |
| 118→119 | 38,693 | 37,925 |       456 |    54 |     258 |

**85→86 carries 78% of all naming churn across the four hops.** It is the
"quiet 80s hop, small base" control pair — and the one with the heaviest bundle
reshuffle. Start there; a fix that only works on the calm hops is not a fix.

## Ceiling (measured, not estimated)

`decompose-noise.ts` on 85→86 — how many distinct identifier substitutions
reconcile each noise statement to an existing prior twin:

| slice                              | statements | noiseLn | share |
| ---------------------------------- | ---------: | ------: | ----: |
| **1 rename, SINGLETON hash class** |        131 |  10,000 | 27.0% |
| 1 rename, bucket class (rotation)  |        391 |   1,547 |  4.2% |
| 2 renames                          |        346 |   5,777 | 15.6% |
| 3–5 renames                        |        418 |   7,876 | 21.2% |
| **6+ renames (genuine drift)**     |        380 |  11,868 | 32.0% |

Against exp037's 216 numbers (single-rename 56.8%, singleton 50.8%, 6+ 14.0%),
**85→86 is a harder hop**: a third of its noise is statements that no small set
of renames reconciles.

The recoverable core is the **singleton single-rename slice** — unique structure,
so the matcher knows exactly which prior statement it is, and exactly one name
differs. Note the unit: `noiseLn` charges whole statement mass, so 10,000 noiseLn
is worth far fewer git lines. Size the lever in **git lines** (the `layout`
block) before building.

## The reframing that matters

exp037 assumed the drift was LLM re-naming ("close-match without
prior-alignment"). The trails say otherwise. On 85→86, of 110,001 bindings:

| tier             |  count | kind          |
| ---------------- | -----: | ------------- |
| exact-match      | 77,660 | deterministic |
| binding-cascade  | 13,155 | deterministic |
| close-match      |  7,425 | LLM w/ prior  |
| statement-twin   |  7,058 | deterministic |
| cold LLM         |  3,123 | LLM           |
| minted leftovers |     21 | —             |

**97.2% is named deterministically**, and spot-checking the highest-leverage
drifted roots (`retrieveToolPermissionModeVal → fetchPermissionMode`,
`countActiveTasksWithRemote → getActiveTaskCountVal`, `UL → LRUCache`) shows them
settled by `exact-match`, `statement-twin`, `binding-cascade` and `close-match` —
not by a cold LLM draw.

So the working hypothesis is **not** "the LLM is unstable". It is:

> Two matchers disagree. The FUNCTION-FINGERPRINT matcher pairs a fresh binding
> with prior binding A and transfers A's name; the STATEMENT-hash twin (what the
> noise metric and the reader both use) says the statement corresponds to prior
> binding B. Both are "matches" — to different prior entities.

If that holds, "pin the prior name" is underdetermined: the exact-match tier
already believes it transferred _the_ prior name. The lever would have to be
about which correspondence wins, not about pinning harder.

## Next step — measure, do not build

1. **Attribute every single-rename drift to the tier that produced it.** Join the
   `(freshName, priorName)` pairs `decompose-noise` computes against
   `strategyTrails` from `--diagnostics`. Output: a table of drifted-name count
   and git lines per settling tier. That confirms or kills the hypothesis above.
   (`decompose-noise.ts` computes the pairs but does not export them — it needs
   the same `composeDiff`-style refactor before the join is cheap.)
2. **Size the singleton slice in GIT LINES**, not noiseLn.
3. Only then design. If the hypothesis holds, the candidate levers are about
   correspondence (make the two matchers agree, or let the statement twin
   outrank a weaker fingerprint match), not about LLM stability.

**Do not** retry interchangeable-bucket assignment: the 391-statement / 1,547-ln
rotation slice is the population exp036 already proved irreducible (isomorphic
members, no cross-version identity — see the roadmap and exp037 Finding 1).
