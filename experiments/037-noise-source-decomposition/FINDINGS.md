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

## Finding 4 — require-alias drift: one local-variable name poisons an import alias tree-wide

Found by the user reviewing the real 215→216 tree: `hook-metadata.js` churned on

    -const fileModTime = require("../../uri-validator/lsp-search/file-mod-time.js");
    +const lspSearchFileModTime = require("../../uri-validator/lsp-search/file-mod-time.js");

plus every `fileModTime.X` reference in the file. The imported PATH is unchanged —
this is pure alias churn, 100% noise.

**Mechanism.** `nsCandidates` (`cjs-emit.ts`) offers the bare basename first
(`fileModTime`), then widens up the path (`lspSearchFileModTime`), taking the
first candidate `nsNameIsFree` accepts. That predicate's `inSource` check rules
out any identifier appearing **anywhere in the whole bundle**, including nested
locals in unrelated files. In 216 the LLM named one function-local
`let fileModTime = await getFileModificationTime(normalizedPath)` — an identifier
occurring **0× in the 215 bundle, 2× in 216** — so the bare alias became unfree
tree-wide and every importer widened.

**The check is over-broad.** The real hazard is a local shadowing the alias _in
the file where the alias is declared_. The colliding local lives in
`completion/files/bash-command-generator.js`, which does **not import** that
module at all — so 148 lines churned across 28 files to prevent an impossible
collision. The alias choice is also not prior-aware.

**Cost on 215→216** (`alias-drift.ts`): 3 naming draws → 73 alias renames → **312
reference lines across 67 files**, all pure noise. All three follow the identical
0→2-3 occurrence pattern.

| alias drift                                  | lines | files |
| -------------------------------------------- | ----: | ----: |
| `fileModTime → lspSearchFileModTime`         |   148 |    28 |
| `apiRetry → assistantMessagesApiRetry`       |   144 |    38 |
| `memoryExtractor → userInputMemoryExtractor` |    20 |     7 |

**Fixes (complementary).** (1) _Per-file freeness_ — scope `inSource` to the file
where the alias is declared; prevents all three cases with no correctness loss.
(2) _Prior-aware alias_ — record aliases in the split ledger and keep the prior's
alias when still legal; guarantees stability even on a genuine same-file
collision. Note `claimed` is global too (one alias per path tree-wide), which is
a deliberate readability choice and can stay.

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

---

## Lever B — BUILT (2026-07-24)

Implemented on branch `exp037-noise-decomposition`.

- `alignEmissionOrder` / `alignFileStatements` (`src/split/stable-split.ts`): order
  each file's statements to the prior file's emission order, matched by
  `statementHash`. Statements never move between files (assignment and the
  hash/file pairing every inheritance tier reads are untouched); the ledger's
  `hashes[]` carries the aligned order so the next release chains off it.
- The **runnable** emitter (`src/split/cjs-emit.ts::orderedIndexesByFile`) replays
  that aligned order — the shipped tree, not just the byte-slice review tree, is
  what git sees, so the win had to land here. Concat-equivalence relaxed to a
  code-preserving multiset (reorder is legal; nothing lost/duplicated/mangled).
- `HUMANIFY_NO_EMIT_ALIGN=1` toggles it off (A/B + kill switch).

### The decisive safety lesson — move only what is provably load-order-independent

The first cut reordered every statement to prior order. It was a **pure reorder**
(0 content mismatches) and cut reorder churn ~88% on the review tree — **but the
runnable tree crashed on boot**: `defineModuleExports(m, {...})` ran before
`var m = {}` was assigned (`TypeError: Properties can only be defined on Objects`).
A side-effectful statement reads, at load time, a binding another statement
assigns at load time — their relative order is load-bearing. The "98.9%
declarations are safe" estimate was wrong: a `var m = {}` is only safe to move if
nothing else reads `m` at load time.

The safe algorithm (`alignFileStatements`): **function declarations move freely;
every other statement keeps its bundle order.** A `FunctionDeclaration` is hoisted
and initialized before any statement runs, so its textual position has zero
runtime effect and it may cross anything. All load-order _data_ dependencies are
between non-function statements (a function assigns and reads nothing at load
time), so keeping the non-functions in bundle order preserves every dependency by
construction. Concretely: align all statements to the prior, then restore the
non-functions to bundle order in the slots the aligned order gave them, leaving
functions at their prior-matched positions.

- A _functions-only-among-function-positions_ first attempt was too timid (only
  reordered functions among the slots functions already sat in) — **12%** reorder
  churn cut on 215→216. Letting hoisted functions cross non-functions lifted it to
  **45%** (14,388 → 7,929 ln), boot-clean.

### Validated results (clean same-naming A/B: ON vs `HUMANIFY_NO_EMIT_ALIGN=1` OFF, both full pipeline)

| pair                | boots | pure reorder | reorder churn OFF→ON | on-disk git churn OFF→ON | self-hop                       |
| ------------------- | ----- | ------------ | -------------------- | ------------------------ | ------------------------------ |
| 2.1.215→216 (quiet) | ✅    | ✅ 0 mism    | 14,388→7,929 (−45%)  | 68,768→48,698 (−29%)     | 0                              |
| 2.1.85→86 (shuffle) | ✅    | ✅ 0 mism    | 15,840→9,648 (−39%)  | 80,012→60,444 (−24%)     | 44 (pre-existing naming flake) |

`npm run check` green (1523 unit + 33 fp); committed `1b46fdb`. The shuffle pair is
where prior positional attempts died (+401), so passing it is the key result. The
85→86 self-hop 44 is the documented pre-existing naming draw-flake (ON==OFF; Lever
B is emit-order only, never touches naming).

**v1 full 4-pair sweep** (on-disk git churn OFF→ON, all boot, all pure reorders):
215→216 −29%, 85→86 −24%, 197→198 −13%, **118→119 +2.3% (regression)**; aggregate
−18.4%.

### v2 — the unambiguous-hash precision guard (commit `41d0b7d`)

The one v1 regression was a PRECISION failure, not a safety one: a statement
claimed its prior position by structural hash even when that hash was
**ambiguous**. Same-shaped stubs (`noop`s, tiny getters that differ only in their
names) got FIFO-paired, so their text teleported to a guessed position and
manufactured churn — on the one hop (118→119, a feature drop) that had almost
nothing to reorder (645 ln baseline).

Guard: a statement may claim a prior position only when its hash occurs **exactly
once on each side**. Ambiguous statements anchor to their predecessor, exactly
like novel ones — precision over recall, the same rule the inheritance tiers use.

| pair            | boots | pure reorder | git churn OFF→ON             | v1 → v2               |
| --------------- | ----- | ------------ | ---------------------------- | --------------------- |
| 215→216         | ✅    | ✅ 0 mism    | 68,894→46,832 (−32%)         | −29% → **−32%**       |
| 85→86 (shuffle) | ✅    | ✅ 0 mism    | 80,012→60,814 (−24%)         | −24% → −24%           |
| 197→198         | ✅    | ✅ 0 mism    | 75,680→62,420 (−18%)         | −13% → **−18%**       |
| 118→119         | ✅    | ✅ 0 mism    | 38,895→38,421 (−1.2%)        | **+2.3% ✗ → −1.2% ✓** |
| **aggregate**   | 4/4   | 4/4          | **263,481→208,487 (−20.9%)** | −18.5% → **−20.9%**   |

Reorder-churn proxy under v2: 215→216 −63%, 85→86 −38%, 197→198 −9%, 118→119
−19% — v1's proxy regressions on BOTH 118→119 (+91%) and 197→198 (+29%) became
reductions.

Pure upside: the regression is gone and the big win is intact. Both boot, both
pure reorders (0 content-mismatch). The 86 `runtime.js` rename-invariant violation
is the documented pre-existing draw-flake — present in v1-ON, v1-OFF and v2 alike.

- The residual ~55% is non-function statements (`var`/expression) that carry
  load-order dependencies. A **dependency-aware v2** — compute each top-level
  statement's load-time (assigns, reads) sets and allow any reorder that preserves
  the read-after-assign edges — could safely recover much of it. Clear follow-up;
  functions-anywhere is the safe, shipping floor.
