# 059 — RESULTS: the capture is REAL, fully characterised, and NOT YET FIXED

> ## STATUS: **root cause characterised down to the two bindings and the strategy that named them. The remaining question is narrow and named. NO FIX SHIPPED.**
>
> Read this before the [brief](./README.md) — the brief is what was believed
> beforehand, and two of its framings were wrong.

## What the bug actually is

`runtime.js`, in the Bun-unpacked tree for `2.1.197→2.1.198`:

```js
function getFileWriter() {
  if (!logFileWriter) {
    let dirPath = null;                                    // OUTER  (was `e`)
    logFileWriter = createBufferedWriter({
      writeFn: (task) => {
        let debugFilePath = getDebugFilePathVal();
        let dirPath = pathModuleVal.dirname(debugFilePath); // INNER  (was `r`)
        let isDirChanged = dirPath !== dirPath;             // ALWAYS FALSE
        dirPath = dirPath;                                  // NO-OP
```

Two bindings in **nested scopes** were both renamed to `dirPath`. The inner
shadows the outer, so the "has the directory changed since last time?" check is
permanently `false` and the remembered value never updates.

The original is unambiguous:

```
13668:  let e = null;              <- outer, if-block scope
13677:  let r = jae.dirname(n);    <- inner, arrow-function scope
13678:  let o = e !== r;
13679:  e = r;
```

## Who did it

From the `--diagnostics` strategy trail of the reproduction:

```
oldName=e   loc=13668:10   via=['llm']
oldName=r   loc=13677:14   via=['llm']
```

**Both renames came from the LLM naming path, in the same run.** 99 bindings in
this file received the name `dirPath` (mostly via `exact-match` prior transfer);
that is normal and harmless for bindings in unrelated scopes. Exactly one pair
is nested, and that pair is the bug.

## The narrow remaining question

`getRenameRejection` should reject whichever of the two is applied second:

- outer→`dirPath` after inner→`dirPath` must trip `shadows-child`
- inner→`dirPath` after outer→`dirPath` must trip `target-visible`

**Both orderings were tested against the real guard on this exact code shape and
both were correctly rejected.** So the guard logic is right; something about how
the LLM path applies a batch of renames means the second application does not
see the first.

That is the whole remaining question: **why does the second rename's guard not
observe the first rename?**

### The sharpest hypothesis: BOTH guards fail open on empty `referencePaths`

Both guards depend on the same thing, and neither has a fallback:

```ts
// wouldRenameShadowInChildScope
const allPaths = [...binding.referencePaths, ...binding.constantViolations];
for (const refPath of allPaths) { ... }      // empty  =>  loop body never runs
return false;                                //        =>  NO REJECTION

// wouldCaptureOuterReference
return outer.referencePaths.some(inside)     // empty  =>  false
    || outer.constantViolations.some(inside);//        =>  NO REJECTION
```

If the OUTER binding's `referencePaths` is incomplete or empty at rename time,
**both guards pass in either order**, and no race is required. That fits every
observation:

- both renames applied despite the guard being logically correct
- a clean fixture cannot reproduce it (a fresh crawl has complete references)
- draw-dependence is fully explained by "the model has to propose the same name
  twice"; the guard failure is then deterministic, which matches the
  bit-identical token position across independent runs

`applyLlmRename` DOES route through `attemptValidatedRename` (verified), and the
Babel cache is cleared BEFORE naming, not during — so neither "the LLM path
skips the guard" nor "the cache was reset mid-pass" explains it.

### REFUTED 2026-08-03 — the guards had complete data and passed anyway

The experiment below was built (`refCount` on every recorded attempt, captured
BEFORE the rename so it is the number the GUARDS saw) and run. It fired on run 1:

```
loc=13668:10  oldName=e   {"strategy":"llm","outcome":"applied","newName":"dirPath","refCount":2}
loc=13677:14  oldName=r   {"strategy":"llm","outcome":"applied","newName":"dirPath","refCount":3}
```

The outer binding had **2 references** — the read at 13678 and the write at
13679, exactly what the source shows. Nothing was empty. **The fail-open
hypothesis is dead**: both guards had complete reference data and still applied.

That also rules out, for the same reason, anything that works by starving the
guards of references — a stale crawl, a binding registered without references,
a reference list cleared by an earlier pass.

WHAT SURVIVES: the guards ran with correct inputs and returned "no rejection".
On a consistent scope tree that is impossible — both orderings reject when
tested directly. So the two renames were evaluated against scope state in which
the other rename was not visible, WITHOUT the reference lists being wrong.

NEXT SUSPECT, and it is now the only one left standing: **scope-object
identity**. `wouldRenameShadowInChildScope` walks `refPath.scope` upward and
tests `refScope.bindings[newName]`; `wouldCaptureOuterReference` calls
`scope.parent?.getBinding(newName)`. Both read `bindings` maps off scope
OBJECTS. `fastRenameBinding` patches exactly one such map — the one belonging
to the scope it was handed. If the scope object handed to the second rename is
a DIFFERENT object for the same lexical scope, its map still holds the old name
and both guards see a tree in which the first rename never happened.

### MEASURED 2026-08-03 — the two renames came from DIFFERENT SCOPE-TREE ERAS

Recorded `scopeUid` (Babel's per-Scope-object id, assigned in construction
order) and `scopeBlock` for every applied LLM rename. Reproduced on run 1:

```
outer `e`   scopeUid 577865   block 341066:342264
inner `r`   scopeUid 278209   block 341123:341437
```

Two facts, both measured:

1. **They are genuinely lexically nested.** 341123–341437 is strictly inside
   341066–342264. The guards SHOULD have seen each other.
2. **The inner scope object was constructed ~300,000 scopes BEFORE its own
   enclosing scope.** In a single crawl a parent is always constructed before
   its child, so these objects cannot come from one crawl.

Across all 6,332 renames carrying a uid, the values cluster into exactly two
groups with a **151,687 gap** between them:

```
era A   274,542 .. 426,128     <- inner `r` (278209)
era B   577,815 .. 597,814     <- outer `e` (577865)
```

So the naming pass renames through **two distinct scope-tree eras**, and this
capture is a pair that straddles them. That is exactly the condition the
identity hypothesis needs: `fastRenameBinding` patches the `bindings` map of
the object it is handed, and a guard consulting an object from the other era
sees a tree in which that rename never happened — with every reference list
intact, which is what `refCount` 2 and 3 already showed.

**Still not the complete mechanism.** What is measured is that the two renames
used different-era scope objects and that the scopes are nested. What is NOT
yet shown is which object the FAILING guard actually walked to, or the order of
the two renames.

### MECHANISM FOUND — the cache clear's safety argument covers hashing, not the guards

The two eras have a source, and it is in the code with a comment that names the
hazard and then reasons past it. `src/rename/plugin.ts`:

```
 885   buildUnifiedGraph(...)              <- graph captures scope objects   ERA A
 906   prior-version matching              <- fills Babel-cache tombstones
 965   clearBabelCacheAfterPriorMatch(...) <- "induces scope re-crawls"
 970   runRenamePass(...)                  <- fresh path.scope accesses      ERA B
```

The clear exists for a real reason (exp031/exp032: prior-AST tombstones make
V8 re-hash the ephemeron table on nearly every insert — an O(n²) naming hang).
Its own doc comment states the consequence exactly:

> Clearing Babel's cache is safe for later hashing (the pre-generate structural
> invariant) because slot placeholders key by declaration node, **which survives
> the scope re-crawls this clear induces**.

That argument is correct **for hashing** — slot placeholders key by declaration
NODE, and nodes survive a re-crawl. It does not cover the rename guards, which
read `bindings` maps off Scope **OBJECTS**, and a re-crawl produces NEW objects
for the same lexical scopes. Patching `scope.bindings` on one object has no
effect on the other.

So a rename applied through a graph-held (era A) scope is invisible to a guard
walking a freshly-crawled (era B) tree, and vice versa — with every reference
list intact, which is exactly what `refCount` 2 and 3 showed.

**It is gated on a prior version** (`if (!priorVersionCode) return;`), which is
why every observed occurrence is a `--prior-version` run.

#### Status of each claim

- MEASURED: two uid clusters 151,687 apart; the capture pair straddles them;
  both guards had complete reference lists.
- FROM CODE: the clear runs between graph build and naming, is prior-gated, and
  its own comment says it induces scope re-crawls.
- INFERRED, coherent but not yet directly demonstrated: graph-held scopes are
  era A and post-clear accesses are era B, so patching one leaves the other
  stale.

#### REPRODUCED IN A UNIT TEST — and it corrected the mechanism again

`src/rename/scope-era.test.ts` reproduces the capture deterministically in
~30ms, emitting `let changed = dirPath !== dirPath`. It is `it.skip`-ped
because it asserts the CORRECT behaviour and therefore fails on current main;
remove `.skip` when the bug is fixed, and do not weaken the assertions.

Writing it corrected the mechanism a second time. My inference was:

> a re-crawl produces a STALE tree, so the guard misses the other rename

**That is wrong.** A fresh crawl reads the MUTATED ast, so a tree built AFTER a
rename sees it perfectly. The first version of this test proved that by
passing when it should have failed.

The real hazard is the reverse, and ORDER is the whole thing:

1. an old scope object is RETAINED across the clear — which is exactly what the
   unified graph does, since it captured scopes at build time (line 885);
2. the naming pass crawls fresh (era B) and holds those scopes;
3. a rename lands through the RETAINED era-A object — mutating the AST and era
   A's bindings map, but not era B's;
4. a guard walking era B was crawled BEFORE that rename and is never told.

So it is not that either tree is inherently stale. It is that **two live trees
exist over one AST and a write through one does not propagate to the other.**

#### The falsifiable prediction that would close it

**Run WITHOUT `--prior-version`: no clear, one era, and this capture class
cannot occur.** If a no-prior run ever produces it, this mechanism is wrong.

#### Candidate fixes — NOT evaluated, do not pick one from this list alone

- **Do not** simply remove the clear: it prevents the exp031/exp032 O(n²)
  naming hang. That is why it is there.
- Re-derive the graph's scope references after the clear, so naming has one
  tree.
- Have `applyLlmRename` resolve the scope from the CURRENT program scope rather
  than using a graph-held one.
- Note the clear cannot simply move earlier: the tombstones it removes are
  created by prior matching (906), which runs after the graph build (885).

### The two probes that remain, if the prediction does not settle it

1. **Record rename sequence.** The trail has no ordering. Add a monotonic
   counter to each attempt; then it is knowable which of the two was second and
   therefore which guard was the one that should have rejected.
2. **Instrument the walk itself.** In `wouldRenameShadowInChildScope`, record
   the uid of each scope walked and whether it held the target name. If the
   walk visits an era-B arrow scope while the rename landed on era A's, the
   mechanism is proven and the fix is about scope-tree consistency across eras
   — NOT about the guard predicates, which are correct.

Find where the second era comes from: `clearBabelCacheAfterPriorMatch` runs
BEFORE naming (checked), so something inside the naming pass is re-crawling.

### The one-line experiment that settled the previous hypothesis

Log, for each applied LLM rename, `binding.referencePaths.length` alongside the
scope block's `start`/`end`. Then reproduce and look at the two renames at
`13668:10` and `13677:14`. If the outer binding shows **0 references** while the
source plainly references it twice inside the arrow, the hypothesis is
confirmed and the fix is about keeping `referencePaths` accurate — NOT about
adding a guard rule.

If instead both show correct reference counts, the hypothesis is dead and the
next suspect is scope-object identity: whether `binding.scope` handed to
`applyLlmRename` is the same object the other rename mutated.

**Do not add a rejection rule to `getRenameRejection`.** Twelve shapes already
reject correctly; a rule that rejects MORE costs names, and exp044 spent +3,742
lines learning that.

## Reproduction

Deterministic enough to work with: **fired on run 2 of 14, twice in a row**, and
the token position and both slot numbers were **identical** across independent
runs (`308757`, `$4431`→`$4434`). The location is fixed; only whether the model
proposes the colliding name varies.

```bash
experiments/059-rename-capture/repro.sh 14
```

## Corrections to the brief

1. **"A capture shows as `x !== x` in the emitted text" — false as a search
   strategy.** The final file has 45 self-comparisons and the pre-rename source
   has the same 45, identically distributed: `x !== x` is the standard NaN idiom
   (`value !== value`, lodash's `eq`). The capture is invisible by that method
   because the file on disk is not the checked artifact — see 2.
2. **The preserved `runtime.js` was the WRONG artifact.** Reconcile, the
   deferred sweep and the family permutation all replace the output after
   validation. The checked code had 16,384,801 tokens; the written file had
   16,120,630. Diffing the file reported a divergence at token 145 — a
   variable-declaration merge, unrelated — and cost a full investigation cycle.
   Fixed: `<file>.validated` is now preserved and is the artifact that pairs
   with `.original`.

   **Always verify `tokens(.validated) == the diagnostic's "of N tokens each"`
   before trusting any diff.** That single check caught this.

3. **The suspect list was too narrow.** The brief pointed at the prior-version
   transfer path; both renames came from the **LLM** path.

## What is now in place

Every one of these was built during this arc, and the diagnosis used all of them:

- the cold re-parse invariant catches it and marks the run failed
- `run.sh` refuses to publish the pair's KPIs as clean
- the diagnostic names the diverging token and both contexts
- the detail survives into the committed run status
- `.original` and `.validated` are preserved for post-hoc diffing
- the `--diagnostics` strategy trail names the binding, location and strategy

## What is NOT done

- **no fix**
- no failing unit test that reproduces the guard bypass (the isolated
  two-rename sequence does NOT reproduce it — that is a finding, and it means
  the bypass needs whatever additional state the real pipeline carries)
- the bug is still live on `main`
