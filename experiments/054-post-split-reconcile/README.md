# 054 — Post-split reconcile: does file identity crack the naming floor?

> ## STATUS: COMPLETE — SHIPPED (pending merge). **Read [RESULTS.md](./RESULTS.md), not this file.**
>
> The hypothesis held. Draw-pinned four-pair gate PASSED on every hop:
> **−5,026 git lines, 0 created**, all four landing on the number predicted from
> the mechanism before the run.
>
> **Claims in this brief that did not survive:**
>
> - **"Target = LOCAL-DRIFT, 2,920 git lines over 4 hops."** The reachable
>   population is not a subset of that bucket and is larger than it: 4,514 lines
>   classify as local drift, because the naming bucket only counts name churn
>   inside hash-matched statements.
> - **"Expect [the every-occurrence-on-a-diff-covered-line gate] to remove most
>   of the population."** It is the top skip reason on 85→86 (283) and
>   near-invisible on the calm hops (8 / 10 / 10). `consumer-single-hunk` and
>   `decl-not-clean` are what actually bound them.
> - **The brief lists the consumer tier among the safety gates without
>   qualification.** It is the one tier that produced WRONG renames here, and
>   only post-split: it fires precisely when a declaration changed, and for an
>   import binding that means a different MODULE. Five cross-module misfires,
>   all gated by the new `skipImportDeclarations`.
> - **"Say explicitly which [ordering hazard] you chose and why."** The answer
>   given first — post-emit, pre-write — was WRONG by 922 git lines, because
>   `finishSplitOutput` rewrites the tree afterwards. The pass runs after it.

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md).
**Read first:** [`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md)
(eleven rules), [`docs/matching-cascades.md`](../../docs/matching-cascades.md)
(what every tier compares), [`docs/naming-pipeline.md`](../../docs/naming-pipeline.md)
(execution order).

---

## Standing position, cold, four gate hops

Total reviewer-facing churn **153,492 git lines**. Noise **9,763**:

| bucket                                        |     lines | whose                                              |
| --------------------------------------------- | --------: | -------------------------------------------------- |
| TARGET-CHANGED (bundle init-list permutation) |     3,374 | upstream's — irreducible                           |
| **LOCAL-DRIFT (the LLM naming floor)**        | **2,920** | **this experiment's target**                       |
| `index.js` require ordering                   |     2,046 | 65% upstream, 35% ours (054 predecessor, declined) |
| private fields `#x`                           |       468 | ours, unclaimed                                    |
| ALIAS-ONLY (tree-wide alias widening)         |       370 | ours                                               |
| MOVED-DECL (consumer-side relocation)         |       300 | ours                                               |
| alias headers                                 |       220 | ours                                               |
| reorder                                       |        54 | ours (was 6,148 before 049/050)                    |

85→86 is the shuffle pair and carries **5,780 of the 7,714 src noise**. Report it
**separately from the three calm hops, always** — averaging hides both.

## The idea

`diff-reconcile.ts` already exploits the one signal the matcher cannot have. Its
own docstring says it:

> Every upstream matching mechanism … works per-function or per-binding, **blind
> to where a binding sits in the rendered file**. The plaintext diff … carries a
> signal none of them see: LCS alignment anchored on identical neighboring lines.

It works — 517 / 397 / 328 renames on the three calm hops. But it diffs the
**bundle**: one flat ~800k-line file, pre-split.

Every measurement in 051–053 diffed the **split tree**, and that is where its
discrimination comes from: `composeDiff` matches files by PATH first, so a
statement is compared against ~20 candidates in the same file instead of ~60,000
in a bundle. File scoping is what makes a weak key (`statementHash`, all
identifier names masked) usable at all.

**Nothing feeds that back.** The order is name → split → stop. The split assigns
files using hash inheritance, name votes and content anchors — identity evidence
computed _after_ naming — and the pipeline then throws it away.

**Hypothesis:** running the existing reconcile tiers over the SPLIT TREE instead
of (or in addition to) the bundle recovers drifted names that bundle-scoped LCS
alignment cannot reach, because the candidate pool is 3,000× smaller and the
neighbour context is a real file rather than an arbitrary window.

## Task 0 — the ceiling. No code. Trees already on disk.

**Everything needed is in `/work/exp050-cold` (cold, post-050, four pairs).** No
pipeline run. No LLM call. Reuse `experiments/051-naming-residual/line-ledger.ts`,
which already isolates the LOCAL-DRIFT population and reconciles 7,440 of 7,440.

Compute, per hop, separating 85→86 from the calm three:

1. **Drifted bindings in same-path files with a same-hash twin.** Of the
   LOCAL-DRIFT lines, how many sit in a statement whose `statementHash` occurs
   exactly once on each side of the SAME file path? That is the population a
   post-split pass could even see.
2. **How many survive reconcile's own gates**, which are not optional:
   - **Every occurrence of the binding must sit on a diff-covered line.** This is
     the load-bearing one. A binding with occurrences on unchanged lines cannot
     be renamed — doing so CREATES hunks where there were none, trading noise for
     noise. Expect this to remove most of the population.
   - Unanimity: all proposals for a binding agree on one prior name.
   - The declaration sits in a clean aligned rename-noise pair.
   - Positions resolving to property names / object keys / free identifiers taint
     their whole hunk.
3. **Cap per file at what `git diff` prints.** The decomposition over-charges —
   measured 29% on the alias population in 051, where the ledger said 336 and
   `diff -u` printed ≤256. Report the git-capped number as the ceiling.

**Decision rule, fixed before the measurement:** if the git-capped surviving mass
is under **~500 lines across four hops**, stamp STATUS and close. That is the
same threshold 051 used and it exists because the harness cannot resolve smaller
(rule 11).

## Task 1 — read the survivors

Whatever survives Task 0, **read twenty of them by hand** before believing the
number. Rule 1 has refuted seven hypotheses in this arc, including two that came
from a brief's own stated premise. For each: is it genuinely the same binding, or
is it two different things in a file that happens to hold one statement of that
shape?

Specifically check for the three classes 051 found hiding inside "naming":

- **cross-module references** — same shape, different `require` target;
- **upstream permutation** — the bundle's lazy-init prologues, same dependency
  multiset in a different order;
- **moved declarations** — same exported binding, new home file.

None of those are renames, and all three read like renames.

## Task 2 — build, only if something survives

State the ceiling in git lines, capped per file, **before writing code**.

Reuse `runPriorDiffReconciliation` and its tiers rather than writing a second
implementation — the gates above are the accumulated safety argument and
re-deriving them will lose one. The change is _where the diff comes from_, not
what the tiers decide.

Behind a kill switch (`HUMANIFY_NO_POST_SPLIT_RECONCILE=1`), TDD red-first.

### The ordering problem you must solve first

Reconcile currently runs at phase 3.3, **before** the split (3.5). A post-split
pass runs after file assignment, which means:

- the split ledger has already been written — a rename after it invalidates
  `emitNames`/`emitHashes` unless both are updated **in lockstep**. 050 nearly
  shipped a bug here: `recordEmittedLayout` OVERWRITES `emitHashes`, so writing
  one without the other leaves the arrays describing different permutations and
  the NEXT release mis-aligns silently instead of falling back;
- renaming after emit means re-emitting, or editing the emitted text;
- the boot gate must still pass — `index.js` load order is SEMANTIC.

Say explicitly which of these you chose and why.

## What to avoid — paid for in previous experiments

1. **Do not loosen "every occurrence on a diff-covered line."** It is the gate
   that stops the pass creating the noise it removes.
2. **Do not remove the corpus gate to reach 85→86.** It abstains below 50% line
   alignment because on a reshuffled bundle an aligned pair is coincidence. That
   hop carries 77% of the naming mass and is the tempting target; exp044 measured
   its biggest naming slice as **87.4% irreducible permutation**. The census
   confirms reconcile is currently inert there: `2.1.86`'s tier list contains no
   `reconcile-*` entry at all.
3. **Do not build a second same-hash pairing rule.** `composeDiff` pairs same-hash
   statements FIFO, and 051 showed 96.9% of naming instances had exactly one
   candidate anyway — the pairing was never the problem. `{ pairing: "corroborated" }`
   exists there if you want the comparison.
4. **Do not trust a spelling as an identity.** exp052 keyed a measurement on name
   spellings and read 99.6% where the truth was unknowable, because `error`,
   `options`, `length` sit on thousands of mechanically pinned locals. Key on the
   binding, or on the input bundle's `(functionId, minified name)`.
5. **Do not reason about magnitude from the largest example** (rule 2) or trust a
   predicate whose name sounds like your claim (rule 3). Both have been paid
   three times each in this arc.
6. **Never rename TO a minified name.** Goal is zero minted tokens.

## Gate — only if something ships

`experiments/041-content-anchor/gate-verdict.sh`, every hop judged **on its own**:

1. `noiseLn` AND git-line noise **down on every hop**.
2. `novel` and `realLn` **unmoved** — necessary, NOT sufficient: they come from
   `analyze.ts`'s hash classifier and cannot see a pairing error.
3. Zero NEW pure-rename violations (count the delta; `runtime.js` flakes on
   control legs too).
4. **Boot gate green ×4, both legs.** `bun` is at `~/.bun/bin` and is NOT on
   PATH — without it `run.sh` SILENTLY prints "BOOT GATE SKIPPED" and criterion 4
   is lost with no error.
5. **Self-hop judged PINNED** (`049/pin-selfhop.sh`), never from a cold A/B —
   049 read 16 → 326 cold and it was entirely draw variance.
6. **118→119 is the canary.** 050 cut its reorder to zero and its git-line noise
   rose 198 anyway; a hop can move both ways at once.
7. Confirm bundle-neutrality **draw-pinned** before reading bundle-level columns
   — they move ±2,800 on their own.

### The resolution problem, stated plainly

The `src/` per-hop draw band is **±2,800 git lines** (exp048, rule 11). A cold
A/B cannot see an effect below it and **will still print a confident sign**. If
the ceiling comes in under that, the ONLY valid instrument is a draw-pinned run:
legitimate here because the pass is deterministic and sits downstream of every
prompt — but **prove the pinning worked** (the second leg must write ~0 cache
entries) and state what the pinned run cannot see (multi-hop feedback, where this
release's output becomes the next release's prior).

## Cache policy

**Cold for any number that becomes a KPI.** `run.sh` defaults to no cache since
exp047; `EVAL_LLM_CACHE=<dir>` opts back in and announces itself as not
gate-valid. The cache IS legitimate for a deterministic surface (049 and 050 both
used it correctly) — but prove it: **the second leg must write ~0 entries**, and
a cold candidate needs a COLD control.

## Commands

```bash
# ceiling, no pipeline run — trees are already on disk
npx tsx experiments/051-naming-residual/line-ledger.ts \
  /work/exp050-cold/2.1.215-rebased/src /work/exp050-cold/2.1.216/src "215->216"

# single-pair probe while iterating
EVAL_PAIRS="215->216" experiments/034-eval-harness/run.sh <label> /work

# full cold 4-pair gate (~1hr), both legs
experiments/050-aligner-precision/cold-ab.sh /work

# verdict
npx tsx experiments/034-eval-harness/leaderboard.ts baseline-main <label>
experiments/041-content-anchor/gate-verdict.sh <control> <candidate>

# checks — pre-commit biome is STRICTER than `npm run check`
npm run check
npx biome check <file>
```

`WORKDIR IS /work`, not `/tmp/eval-work`.

## Prior results this experiment must not re-derive

| finding                                                                                                               | where                              |
| --------------------------------------------------------------------------------------------------------------------- | ---------------------------------- |
| naming is 84.7% upstream permutation + LLM floor; 1,138 reducible over 4 hops                                         | `051/RESULTS.md`                   |
| the LLM re-rolls **35.6%** of its own decisions on identical input; ~2/3 of naming instability never reaches the diff | `052/README.md`                    |
| cascade census: `enclosingStatement` is the #2 tier at 21.1%; shingle resolves 0.1%                                   | `053/README.md`                    |
| ambiguous-function identity recovery: **10 of 1,420** — the residue is isomorphic                                     | `034/ceiling-identity-recovery.ts` |
| alias reservation cost **+3,742 lines** and left displacements unchanged                                              | `044`                              |
| reorder closed 6,148 → 54                                                                                             | `049`, `050`                       |
| vendor closed 36,201 → 2,101                                                                                          | `046`, `047`                       |

**Nothing before exp034 was gated on four real version pairs.** Read those dirs
for mechanism, not magnitude. Dirs 033–045 carry **no STATUS block** — 039 and
044 still state numbers their own results corrected (rule 9).
