# exp037 — Noise-source decomposition: FINDINGS

**Status: the "isomorphic floor" is refuted. The residual noise is not
interchangeable-bucket rotation, and the largest avoidable on-disk churn is
invisible to the eval metric entirely.**

Measured this session on the real 215→216 artifacts under
`/tmp/eval-work/c1-diffobj` (216-off = main's behavior, family-permute toggled
off; 215-rebased = prior). Tools in this directory; all numbers reproduce.

---

## Step 1 — what the noiseLn metric actually measures (this reframes everything)

`experiments/034-eval-harness/analyze.ts::churn` matches statements **by
identifier-blind `statementHash`** (`src/split/statement-hash.ts` masks _every_
identifier — bound, referenced, and property names — keeping structure, literals,
operators). A fresh top-level statement is:

- **clean** iff its exact rendered text (names included) equals **some** prior
  statement of the same hash,
- **noise** (`namingNoiseLines += stmt.lines`) iff its hash exists in the prior
  but **no** prior twin reproduces its text,
- **novel** (real change) iff its hash is absent from the prior.

Consequences that drive everything below:

1. **Position-blind.** A permutation of names among same-hash siblings that still
   reproduces the prior text-set is _already clean_. Pure bucket rotation costs
   nothing. (This is why every positional/assignment attempt failed — they were
   optimizing something the metric doesn't even penalize.)
2. **Whole-statement granularity.** A 998-line statement counts **998** noise
   lines if _one_ identifier drifts. noiseLn is not a line-diff; it is
   statement-mass gated on exact-text reproduction.
3. **Order-blind & relocation-blind.** Statements that are byte-identical but
   emitted in a different file position, or moved to another file, are _not_
   noiseLn — even though git shows them as churn.

---

## Finding 1 — the residual noiseLn is echo-amplified ROOT drift, not buckets

Decomposition of the 5,981 noiseLn on 216 (`decompose-noise.ts`), by the minimum
number of distinct identifier substitutions that reconcile each noise statement to
an existing prior twin:

| reconcilable by…                | statements | noiseLn | share |
| ------------------------------- | ---------: | ------: | ----: |
| **1 rename**                    |        191 |   3,399 | 56.8% |
| ‣ in a **singleton** hash-class |         37 |   3,036 | 50.8% |
| ‣ in a bucket (rotation)        |        154 |     363 |  6.1% |
| **2 renames**                   |         47 |   1,563 | 26.1% |
| 3–5 renames                     |         55 |     183 |  3.1% |
| **6+ renames (genuine drift)**  |         71 |     836 | 14.0% |

**Interchangeable-bucket rotation — the thing exp036 spent itself on — is 6.1%.**
57% is statements a _single_ rename fixes, and 51% of _all_ noise is single-rename
statements in **singleton** classes (unique structure ⇒ the matcher knows exactly
which prior statement it is, zero assignment ambiguity).

Where the one drifted name lives (`drift-mechanism.ts`): **47 statements / 2,968
ln (87% of the single-rename mass) are ECHOES** — big `lazyInitializer` modules
that count as noise purely because they reference **one** top-level binding whose
name drifted. `idx→i` taints a **998-line** statement; `grepOptions→__s` taints
**1,060**. The lever is pinning the **root binding**, which heals every echo — not
per-statement or per-bucket assignment.

Concentration (`root-rename-leverage.ts`): **80% of the single-rename noise is 8
root-rename pairs.**

## Finding 2 — direction confound: much "recoverable" noise is desirable or a rebasing artifact

Single-rename noise (3,399 ln) split by drift direction:

| direction    |    ln | meaning                                                         |
| ------------ | ----: | --------------------------------------------------------------- |
| fresh-better | 1,474 | reverting would **degrade** the name or **re-mint** — do NOT    |
| fresh-worse  |   492 | fresh regressed a good prior name → pin-to-prior is a pure win  |
| instability  | 1,433 | two comparable names, same binding → pin-to-prior for stability |

The top two "fresh-better" cases dominate that column and are **measurement
artifacts of rebasing**: `grepOptions→__s` (1,061 ln) is the merged **below-floor
guard working correctly** — it refuses to inherit the prior's _minted_ `__s` and
lets the LLM give the real name `grepOptions`. In a real walk that name is
established once and inherited thereafter; the rebased prior artificially
reintroduces the mint. So the honest "cleanly pin-to-prior" naming slice is
**≈1,925 ln (instability + fresh-worse)**, not 3,399.

## Finding 3 — the disk-vs-AST gap: REORDER churn is 2.4× the measured noise and the metric is blind to it

The user commits and reviews the split **src tree**. Its real git diff for
215→216 is **68,768 lines** (507 files) — 11× noiseLn — because it also carries
real change, relocation, and **statement reordering** that noiseLn's hash-matching
never counts.

`reorder-churn.ts` (LCS over byte-identical statements per file): **857 statements
/ 14,388 fresh-side lines are byte-identical in both versions but emitted out of
order.** Verified by canonicalizing statement order (`canon-check.ts`):

| file                 | bundle-order churn | canonical-order churn | reorder |
| -------------------- | -----------------: | --------------------: | ------: |
| mouse-action.js      |              3,260 |                **42** | **99%** |
| quote-transformer.js |              4,407 |                   851 |     80% |
| segmenter.js         |              3,499 |                 2,161 |     38% |

`mouse-action.js` barely changed — 3,218 of its 3,260 churned lines are pure
reshuffle of unchanged code. **Reorder churn (~14k fresh-side ln) is the single
largest avoidable chunk of on-disk noise, larger than the entire noiseLn the eval
optimizes, and no prior experiment measured or targeted it.**

Root cause: `src/split/stable-split.ts` emits within-file statements in **fresh
bundle order** (sorted by source `.start` offset). When upstream reshuffles the
bundle (215→216 did, like the known 85→86 35% shuffle), files full of unchanged
statements churn. The prior file order is already recorded in the ledger's
`order[]` — it is used for reconstruction verification, never to stabilize
emission.

---

## The two levers (both real, different machinery, different risk)

**Lever A — echo-root name pinning (the user's explicit target, eval-visible).**
When a module binding's name drifted from a _real_ (non-mint) prior name and the
prior name is free/adoptable, pin it. Heals big echo statements. Sized ≈1,925 ln
of clean recovery on 216 (more once the 2-rename class is included). Open
mechanism question the user raised — _why_ do matched roots drift? (LLM re-naming
without prior-alignment; multi-declarator module bindings the matcher tracks
weakly). Needs a `--diagnostics` trace to confirm before building. Precision risk:
must not touch fresh-better/real-change drift (direction gate required).

**Lever B — stable within-file emit order (disk-visible, metric-blind, biggest).**
Order fresh statements within each file to match their prior-file order (by hash),
appending novel statements near their bundle neighbors. Eliminates ~14k ln of
on-disk reorder churn the eval can't see. Machinery mostly exists (`order[]`
ledger + hash-keyed file inheritance). Risk: reordering top-level statements can
change execution order — but statements upstream _itself_ reshuffled between
versions are empirically order-independent, and the boot gate + concat-equivalence
invariant backstop it. A safe subset (pure declarations only) de-risks further.

**Recommendation:** Lever B first. It is the largest, most directly answers the
user's "layout on disk vs AST" framing, is eval-metric-independent (so it can't be
gamed by the proxy), and its safety has a clean empirical argument + existing
guardrails. Lever A is the natural complement and targets the number the eval
tracks, but carries a direction-precision hazard and an unresolved mechanism
question. The eval also needs a **within-file-order KPI** added regardless, or
Lever B's win stays invisible to the harness.
